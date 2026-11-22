use std::collections::VecDeque;

use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::capacity::{PresentError, PresentStatus};
use liuli_core::damage::DamageSet;
use liuli_core::geometry::Rect;
use liuli_core::paint::{CommandList, PaintCmd, StaticTextLookup};
use liuli_render::{
    BandSlot, DmaCoordinator, DmaSink, FreeBand, InFlightBand, PollOutcome, ReadyBand, StartOutcome,
};
use liuli_text::Lf4Font;

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

struct FakeDmaDriver<'a> {
    in_flight: Option<InFlightBand<'a>>,
    snap_pixels: Vec<u16>,
    fb: Vec<u16>,
    fb_w: u32,
    busy_next: bool,
    reject_next: Option<&'static str>,
    fail_complete: Option<(&'static str, bool)>, // (error, confirm_hw_stopped)
    stale_completions: VecDeque<(BandSlot, u32, u32)>,
    stale_scratch: Vec<u16>,
    seq_log: Vec<(u32, u32, BandSlot, Rect)>,
    ready_to_finish: bool,
}

impl<'a> FakeDmaDriver<'a> {
    fn new(w: u32, h: u32) -> Self {
        Self {
            in_flight: None,
            snap_pixels: Vec::new(),
            fb: vec![0; (w * h) as usize],
            fb_w: w,
            busy_next: false,
            reject_next: None,
            fail_complete: None,
            stale_completions: VecDeque::new(),
            stale_scratch: vec![0u16; 128],
            seq_log: Vec::new(),
            ready_to_finish: false,
        }
    }

    fn verify_in_flight_unchanged(&self) {
        if let Some(band) = &self.in_flight {
            assert_eq!(
                band.pixels(),
                &self.snap_pixels[..],
                "in-flight DMA buffer mutated!"
            );
        }
    }

    fn complete_current(&mut self) {
        self.ready_to_finish = true;
    }
}

impl<'a> DmaSink<'a> for FakeDmaDriver<'a> {
    type Error = &'static str;

    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error> {
        if self.busy_next {
            self.busy_next = false;
            return StartOutcome::Busy(band);
        }
        if let Some(err) = self.reject_next.take() {
            return StartOutcome::Rejected(err, band);
        }
        if self.in_flight.is_some() {
            return StartOutcome::Busy(band);
        }
        let meta = band.meta();
        self.snap_pixels = band.pixels().to_vec();
        self.seq_log
            .push((meta.frame, meta.seq, meta.slot, meta.rect));
        self.in_flight = Some(band.into_in_flight());
        self.ready_to_finish = false;
        StartOutcome::Started
    }

    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error> {
        if let Some((slot, frame, seq)) = self.stale_completions.pop_front() {
            // Leak a small static/owned scratch chunk only in test to simulate a stale IRQ
            // carrying a mismatched (frame, seq, slot) without releasing the real in_flight band.
            let leaked: &'a mut [u16] = Box::leak(self.stale_scratch.clone().into_boxed_slice());
            return PollOutcome::Completed {
                slot,
                frame,
                seq,
                band: FreeBand::new(leaked, slot),
            };
        }
        if !self.ready_to_finish {
            return PollOutcome::Pending;
        }
        let Some(band) = self.in_flight.take() else {
            return PollOutcome::Pending;
        };
        let meta = band.meta();
        if let Some((err, stopped)) = self.fail_complete.take() {
            let returned = if stopped {
                Some(band.confirm_stopped())
            } else {
                // Hardware could not be confirmed stopped: drop/park permanently so safe code never touches it!
                core::mem::forget(band);
                None
            };
            return PollOutcome::Failed {
                slot: meta.slot,
                frame: meta.frame,
                seq: meta.seq,
                error: err,
                band: returned,
            };
        }

        // Copy verified snapshot pixels into simulated panel framebuffer
        let pixels = band.pixels();
        assert_eq!(
            pixels,
            &self.snap_pixels[..],
            "DMA saw mutated pixels before completion!"
        );
        for ry in 0..meta.rect.h as usize {
            let gy = meta.rect.y as usize + ry;
            let dst = gy * (self.fb_w as usize) + meta.rect.x as usize;
            let src = ry * (meta.rect.w as usize);
            self.fb[dst..dst + meta.rect.w as usize]
                .copy_from_slice(&pixels[src..src + meta.rect.w as usize]);
        }

        PollOutcome::Completed {
            slot: meta.slot,
            frame: meta.frame,
            seq: meta.seq,
            band: band.confirm_stopped(),
        }
    }
}

#[test]
fn dma_ping_pong_preserves_in_flight_and_handles_busy_reject_failure_fault() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 32, 16); // 4 bands of height 4
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(1, 2, 3);
    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(4, 2, 20, 12),
        clip: screen,
        color: Rgb565::new(20, 40, 15),
    })
    .unwrap();

    let mut buf_a = [0u16; 32 * 4];
    let mut buf_b = [0u16; 32 * 4];
    let mut coord = DmaCoordinator::new(&mut buf_a, &mut buf_b);
    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(screen);

    let mut drv = FakeDmaDriver::new(32, 16);

    // 1. Busy on first try_start keeps ReadyBand without losing seq=0
    drv.busy_next = true;
    coord.begin_frame::<&str>(&dmg, screen, 4).unwrap();
    let (st, _) = coord
        .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert!(!coord.has_in_flight());
    assert!(!dmg.is_empty());

    // 2. Next step starts seq=0 on A and renders seq=1 on B into queue
    let (st, _) = coord
        .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert!(coord.has_in_flight());
    drv.verify_in_flight_unchanged();
    assert!(!dmg.is_empty());

    // 3. Stale/duplicate completion with wrong frame/seq is ignored; real in-flight stays protected
    drv.stale_completions.push_back((BandSlot::A, 99, 99));
    let (st_stale, _) = coord
        .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
        .unwrap();
    assert_eq!(st_stale, PresentStatus::Pending);
    assert!(coord.has_in_flight());
    drv.verify_in_flight_unchanged();

    // Complete bands one by one; damage remains non-empty until the 4th band finishes
    for step_idx in 0..3 {
        drv.complete_current();
        let (st, _) = coord
            .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
            .unwrap();
        assert_eq!(
            st,
            PresentStatus::Pending,
            "step {step_idx} ended too early"
        );
        assert!(!dmg.is_empty());
        drv.verify_in_flight_unchanged();
    }

    // Complete 4th (final) band -> Presented + damage cleared!
    drv.complete_current();
    let (st, metrics) = coord
        .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
        .unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert!(dmg.is_empty());
    assert_eq!(metrics.bands, 4);
    assert_eq!(metrics.pixels_sent, 32 * 16);
    assert_eq!(
        drv.seq_log.iter().map(|e| e.1).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );

    // 4. Test Rejected -> marks full screen damage and recovers cleanly
    dmg.mark_full_screen(screen);
    drv.reject_next = Some("bus rejected");
    coord.begin_frame::<&str>(&dmg, screen, 4).unwrap();
    let err = coord.step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font);
    assert_eq!(err, Err(PresentError::Target("bus rejected")));
    assert!(dmg.is_full_screen());
    assert!(!coord.is_active());

    // 5. Partial frame failure (recoverable, stopped=true) discards queued band and retries full frame
    coord.begin_frame::<&str>(&dmg, screen, 4).unwrap();
    let _ = coord
        .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
        .unwrap();
    drv.fail_complete = Some(("crc fail", true));
    drv.complete_current();
    let err2 = coord.step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font);
    assert_eq!(err2, Err(PresentError::Target("crc fail")));
    assert!(dmg.is_full_screen());
    assert!(!coord.is_faulted());

    // Retry whole frame succeeds
    coord.begin_frame::<&str>(&dmg, screen, 4).unwrap();
    loop {
        let (st, _) = coord
            .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
            .unwrap();
        if st == PresentStatus::Presented {
            break;
        }
        drv.complete_current();
    }
    assert!(dmg.is_empty());

    // 6. Unconfirmed stop -> Faulted permanently until explicit recovery
    dmg.mark_full_screen(screen);
    coord.begin_frame::<&str>(&dmg, screen, 4).unwrap();
    let _ = coord
        .step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font)
        .unwrap();
    drv.fail_complete = Some(("hung dma", false));
    drv.complete_current();
    let err3 = coord.step(&mut drv, &cmds, &mut dmg, &StaticTextLookup, &font);
    assert_eq!(err3, Err(PresentError::DmaFaulted));
    assert!(coord.is_faulted());
    assert_eq!(
        coord.begin_frame::<&str>(&dmg, screen, 4),
        Err(PresentError::DmaFaulted)
    );
}
