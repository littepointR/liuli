use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::capacity::{PresentError, PresentStatus};
use liuli_core::damage::DamageSet;
use liuli_core::geometry::Rect;
use liuli_core::paint::{CommandList, PaintCmd, SliceSceneResources};
use liuli_core::time::InstantMs;
use liuli_render::{
    AlignedBand, BandSlot, DmaCacheOps, DmaCoordinator, DmaSink, FreeBand, InFlightBand,
    PollOutcome, ReadyBand, StartOutcome, TeError, TeGate, TeMode, TeWait, present_sync,
};
use liuli_text::{FontSet, Lf4Font};

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct IrqEvent {
    slot: BandSlot,
    frame: u32,
    seq: u32,
    stale: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum CacheOrderStep {
    SetWindow(u32, u32, BandSlot),
    CleanDCache(usize, usize),
    Barrier,
    StartTx(u32, u32, BandSlot),
}

struct RecordingCache {
    log: Vec<CacheOrderStep>,
}

impl RecordingCache {
    fn new() -> Self {
        Self { log: Vec::new() }
    }
}

impl DmaCacheOps for RecordingCache {
    fn clean_dcache_range(&mut self, addr: usize, len_bytes: usize) {
        self.log.push(CacheOrderStep::CleanDCache(addr, len_bytes));
    }

    fn data_sync_barrier(&mut self) {
        self.log.push(CacheOrderStep::Barrier);
    }
}

struct FixedIrqQueue {
    items: [Option<IrqEvent>; 8],
    head: usize,
    tail: usize,
    len: usize,
}

impl FixedIrqQueue {
    const fn new() -> Self {
        Self {
            items: [None; 8],
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    fn push(&mut self, ev: IrqEvent) {
        assert!(self.len < self.items.len(), "IRQ test queue full");
        self.items[self.tail] = Some(ev);
        self.tail = (self.tail + 1) % self.items.len();
        self.len += 1;
    }

    fn pop(&mut self) -> Option<IrqEvent> {
        if self.len == 0 {
            return None;
        }
        let ev = self.items[self.head].take();
        self.head = (self.head + 1) % self.items.len();
        self.len -= 1;
        ev
    }
}

struct FakeDmaDriver<'a> {
    in_flight: Option<InFlightBand<'a>>,
    snap_pixels: Vec<u16>,
    fb: Vec<u16>,
    fb_w: u32,
    busy_next: bool,
    reject_next: Option<&'static str>,
    fail_complete: Option<(&'static str, bool)>,
    irq_queue: FixedIrqQueue,
    stale_ignored: usize,
    seq_log: Vec<(u32, u32, BandSlot, Rect)>,
    cache: RecordingCache,
    te: TeGate,
    now_ms: InstantMs,
}

impl<'a> FakeDmaDriver<'a> {
    fn new(w: u32, h: u32, te_mode: TeMode) -> Self {
        Self {
            in_flight: None,
            snap_pixels: Vec::new(),
            fb: vec![0; (w * h) as usize],
            fb_w: w,
            busy_next: false,
            reject_next: None,
            fail_complete: None,
            irq_queue: FixedIrqQueue::new(),
            stale_ignored: 0,
            seq_log: Vec::new(),
            cache: RecordingCache::new(),
            te: TeGate::new(te_mode),
            now_ms: InstantMs(0),
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
        if let Some(band) = &self.in_flight {
            let meta = band.meta();
            self.irq_queue.push(IrqEvent {
                slot: meta.slot,
                frame: meta.frame,
                seq: meta.seq,
                stale: false,
            });
        }
    }

    fn push_stale_irq(&mut self, slot: BandSlot, frame: u32, seq: u32) {
        self.irq_queue.push(IrqEvent {
            slot,
            frame,
            seq,
            stale: true,
        });
    }
}

impl<'a> DmaSink<'a> for FakeDmaDriver<'a> {
    type Error = TeError<&'static str>;

    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error> {
        if self.busy_next {
            self.busy_next = false;
            return StartOutcome::Busy(band);
        }
        if let Some(err) = self.reject_next.take() {
            return StartOutcome::Rejected(TeError::Driver(err), band);
        }
        if self.in_flight.is_some() {
            return StartOutcome::Busy(band);
        }
        let meta = band.meta();
        match self.te.check_band(meta.frame, meta.seq, self.now_ms) {
            TeWait::Ready => {}
            TeWait::Waiting => return StartOutcome::Busy(band),
            TeWait::TimedOut => return StartOutcome::Rejected(TeError::TimedOut, band),
        }

        // Required BSP ordering: set window -> clean cache over valid bytes -> DSB -> start TX
        self.cache
            .log
            .push(CacheOrderStep::SetWindow(meta.frame, meta.seq, meta.slot));
        let ptr_addr = band.pixels().as_ptr() as usize;
        let len_bytes = band.valid_bytes();
        self.cache.clean_dcache_range(ptr_addr, len_bytes);
        self.cache.data_sync_barrier();
        self.cache
            .log
            .push(CacheOrderStep::StartTx(meta.frame, meta.seq, meta.slot));

        self.snap_pixels = band.pixels().to_vec();
        self.seq_log
            .push((meta.frame, meta.seq, meta.slot, meta.rect));
        self.in_flight = Some(band.into_in_flight());
        StartOutcome::Started
    }

    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error> {
        while let Some(ev) = self.irq_queue.pop() {
            let matches_current = if let Some(band) = &self.in_flight {
                let m = band.meta();
                !ev.stale && ev.frame == m.frame && ev.seq == m.seq && ev.slot == m.slot
            } else {
                false
            };

            if !matches_current {
                // Driver filters stale IRQ without manufacturing a fake FreeBand
                self.stale_ignored += 1;
                continue;
            }

            let band = self.in_flight.take().expect("checked in_flight");
            let meta = band.meta();
            if let Some((err, stopped)) = self.fail_complete.take() {
                let returned = if stopped {
                    Some(band.confirm_stopped())
                } else {
                    let _ = band;
                    None
                };
                return PollOutcome::Failed {
                    slot: meta.slot,
                    frame: meta.frame,
                    seq: meta.seq,
                    error: TeError::Driver(err),
                    band: returned,
                };
            }

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

            return PollOutcome::Completed {
                slot: meta.slot,
                frame: meta.frame,
                seq: meta.seq,
                band: band.confirm_stopped(),
            };
        }
        PollOutcome::Pending
    }
}

#[test]
fn dma_ping_pong_cache_order_recovery_guard_and_te_fault_free_paths() {
    let font = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
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

    let mut aligned_a: AlignedBand<{ 32 * 4 }> = AlignedBand::zeroed();
    let mut aligned_b: AlignedBand<{ 32 * 4 }> = AlignedBand::zeroed();
    assert_eq!(
        (aligned_a.as_slice().as_ptr() as usize) % liuli_render::CACHE_LINE_BYTES,
        0
    );
    assert_eq!(
        (aligned_b.as_slice().as_ptr() as usize) % liuli_render::CACHE_LINE_BYTES,
        0
    );

    let mut coord = DmaCoordinator::new(aligned_a.as_mut_slice(), aligned_b.as_mut_slice());
    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(screen);

    // Non-faulted idle coordinator must reject recover_from_fault!
    let mut rec_idle_a = [0u16; 32 * 4];
    let mut rec_idle_b = [0u16; 32 * 4];
    assert_eq!(
        coord.recover_from_fault::<TeError<&'static str>>(&mut rec_idle_a, &mut rec_idle_b),
        Err(PresentError::DmaBusy)
    );

    let mut drv = FakeDmaDriver::new(32, 16, TeMode::Immediate);

    // 1. Busy on first try_start keeps ReadyBand without losing seq=0
    drv.busy_next = true;
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    let (st, m0) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert!(!coord.has_in_flight());
    assert!(coord.has_queued());
    assert_eq!(m0.bands, 0);
    assert_eq!(m0.pixels_sent, 0);
    assert!(!dmg.is_empty());

    // Recover cannot overwrite an active frame!
    let mut rec_active_a = [0u16; 32 * 4];
    let mut rec_active_b = [0u16; 32 * 4];
    assert_eq!(
        coord.recover_from_fault::<TeError<&'static str>>(&mut rec_active_a, &mut rec_active_b),
        Err(PresentError::DmaBusy)
    );

    // 2. Next step starts seq=0 on A and renders seq=1 on B into queue
    let (st, _) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert!(coord.has_in_flight());
    drv.verify_in_flight_unchanged();
    assert!(!dmg.is_empty());

    // 3. Stale ISR completions (wrong frame/seq/slot) ignored cleanly without releasing A
    drv.push_stale_irq(BandSlot::A, 99, 99);
    drv.push_stale_irq(BandSlot::B, coord.frame_id(), 0);
    let (st_stale, _) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st_stale, PresentStatus::Pending);
    assert_eq!(drv.stale_ignored, 2);
    assert!(coord.has_in_flight());
    drv.verify_in_flight_unchanged();

    for step_idx in 0..3 {
        drv.complete_current();
        let (st, _) = coord
            .step(
                &mut drv,
                &cmds,
                &mut dmg,
                &SliceSceneResources::EMPTY,
                &font,
            )
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
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert!(dmg.is_empty());
    assert_eq!(metrics.bands, 4);
    assert_eq!(metrics.pixels_sent, 32 * 16);
    assert_eq!(
        drv.seq_log.iter().map(|e| e.1).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );

    // Verify exact BSP cache maintenance ordering per started band
    assert_eq!(drv.cache.log.len(), 16); // 4 bands * 4 steps
    for chunk in drv.cache.log.as_chunks::<4>().0 {
        assert!(matches!(chunk[0], CacheOrderStep::SetWindow(..)));
        match chunk[1] {
            CacheOrderStep::CleanDCache(addr, len_bytes) => {
                assert_eq!(addr % liuli_render::CACHE_LINE_BYTES, 0);
                assert_eq!(len_bytes, 32 * 4 * 2);
            }
            _ => panic!("expected CleanDCache step, got {chunk:?}"),
        }
        assert_eq!(chunk[2], CacheOrderStep::Barrier);
        assert!(matches!(chunk[3], CacheOrderStep::StartTx(..)));
    }

    // Compare reconstructed panel against synchronous renderer pixel-for-pixel
    let mut ref_words = vec![0u16; 32 * 16];
    let mut ref_scratch = [0u16; 32 * 4];
    let mut ref_dmg = DamageSet::new();
    ref_dmg.mark_full_screen(screen);
    present_sync(
        &mut SlicePanel {
            w: 32,
            fb: &mut ref_words,
        },
        &mut ref_scratch,
        screen,
        4,
        &cmds,
        &mut ref_dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();
    assert_eq!(drv.fb, ref_words);

    // 4. Test Rejected -> marks full screen damage and recovers cleanly
    dmg.mark_full_screen(screen);
    drv.reject_next = Some("bus rejected");
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    let err = coord.step(
        &mut drv,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    );
    assert_eq!(
        err,
        Err(PresentError::Target(TeError::Driver("bus rejected")))
    );
    assert!(dmg.is_full_screen());
    assert!(!coord.is_active());

    // 5. Partial frame failure (recoverable, stopped=true) discards queued band and retries full frame
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    let _ = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    drv.fail_complete = Some(("crc fail", true));
    drv.complete_current();
    let err2 = coord.step(
        &mut drv,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    );
    assert_eq!(err2, Err(PresentError::Target(TeError::Driver("crc fail"))));
    assert!(dmg.is_full_screen());
    assert!(!coord.is_faulted());

    // Retry whole frame succeeds
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    loop {
        let (st, _) = coord
            .step(
                &mut drv,
                &cmds,
                &mut dmg,
                &SliceSceneResources::EMPTY,
                &font,
            )
            .unwrap();
        if st == PresentStatus::Presented {
            break;
        }
        drv.complete_current();
    }
    assert!(dmg.is_empty());

    // 6. Unconfirmed stop -> Faulted until recover_from_fault with fresh slots succeeds
    dmg.mark_full_screen(screen);
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    let _ = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    drv.fail_complete = Some(("hung dma", false));
    drv.complete_current();
    let err3 = coord.step(
        &mut drv,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    );
    assert_eq!(err3, Err(PresentError::DmaFaulted));
    assert!(coord.is_faulted());
    assert_eq!(
        coord.begin_frame::<TeError<&'static str>>(&dmg, screen, 4),
        Err(PresentError::DmaFaulted)
    );

    // Explicit recovery once hardware is confirmed halted works cleanly
    let mut fresh_a = [0u16; 32 * 4];
    let mut fresh_b = [0u16; 32 * 4];
    assert_eq!(
        coord.recover_from_fault::<TeError<&'static str>>(&mut fresh_a, &mut fresh_b),
        Ok(())
    );
    assert!(!coord.is_faulted());
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    loop {
        let (st, _) = coord
            .step(
                &mut drv,
                &cmds,
                &mut dmg,
                &SliceSceneResources::EMPTY,
                &font,
            )
            .unwrap();
        if st == PresentStatus::Presented {
            break;
        }
        drv.complete_current();
    }
    assert_eq!(drv.fb, ref_words);
}

#[test]
fn coordinator_ignores_mismatched_triple_and_faults_on_wrong_band_slot() {
    struct MisbehavingSink<'a> {
        wrong_triple_buf: Option<FreeBand<'a>>,
        wrong_slot_buf: Option<FreeBand<'a>>,
        in_flight: Option<InFlightBand<'a>>,
        phase: u8,
    }

    impl<'a> DmaSink<'a> for MisbehavingSink<'a> {
        type Error = ();

        fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, ()> {
            self.in_flight = Some(band.into_in_flight());
            StartOutcome::Started
        }

        fn poll_complete(&mut self) -> PollOutcome<'a, ()> {
            match self.phase {
                // Phase 0: deliver wrong frame/seq/slot triple with another valid FreeBand
                0 => {
                    self.phase = 1;
                    let b = self.wrong_triple_buf.take().unwrap();
                    PollOutcome::Completed {
                        slot: BandSlot::B,
                        frame: 999,
                        seq: 999,
                        band: b,
                    }
                }
                // Phase 1: match triple, but hand back a FreeBand whose internal slot is B instead of A!
                1 => {
                    self.phase = 2;
                    let _parked = self.in_flight.take().unwrap();
                    let _ = _parked;
                    let wrong = self.wrong_slot_buf.take().unwrap();
                    PollOutcome::Completed {
                        slot: BandSlot::A,
                        frame: 1,
                        seq: 0,
                        band: wrong,
                    }
                }
                _ => PollOutcome::Pending,
            }
        }
    }

    let font = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
    let screen = Rect::new(0, 0, 16, 8);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    let mut buf_a = [0u16; 64];
    let mut buf_b = [0u16; 64];
    let mut extra_wrong_triple = [0u16; 64];
    let mut extra_wrong_slot = [0u16; 64];

    let mut coord = DmaCoordinator::new(&mut buf_a, &mut buf_b);
    let mut dmg = DamageSet::new();
    dmg.add(Rect::new(0, 0, 16, 4), screen);
    coord.begin_frame::<()>(&dmg, screen, 4).unwrap();

    let mut sink = MisbehavingSink {
        wrong_triple_buf: Some(FreeBand::new(&mut extra_wrong_triple, BandSlot::B)),
        wrong_slot_buf: Some(FreeBand::new(&mut extra_wrong_slot, BandSlot::B)),
        in_flight: None,
        phase: 0,
    };

    // Initial step starts seq=0 on A
    let (st0, _) = coord
        .step(
            &mut sink,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st0, PresentStatus::Pending);

    // Phase 0: wrong triple ignored safely; real in-flight A stays tracked
    let (st1, _) = coord
        .step(
            &mut sink,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st1, PresentStatus::Pending);
    assert!(coord.has_in_flight());
    assert!(!coord.is_faulted());

    // Phase 1: band.slot() == B != exp_slot A => coordinator quarantines fault without corrupting slots!
    let err = coord.step(
        &mut sink,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    );
    assert_eq!(err, Err(PresentError::DmaFaulted));
    assert!(coord.is_faulted());
    assert!(!coord.is_active());
    assert!(dmg.is_full_screen());
}

#[test]
fn te_sync_missing_duplicate_timeout_and_recovery() {
    let font = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
    let screen = Rect::new(0, 0, 32, 16);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(4, 8, 12);
    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(2, 2, 28, 12),
        clip: screen,
        color: Rgb565::new(31, 0, 0),
    })
    .unwrap();

    let mut buf_a = [0u16; 32 * 4];
    let mut buf_b = [0u16; 32 * 4];
    let mut coord = DmaCoordinator::new(&mut buf_a, &mut buf_b);
    let mut drv = FakeDmaDriver::new(32, 16, TeMode::WaitForTe { timeout_ms: 20 });

    // Stray TE pulse while idle must NOT satisfy the upcoming frame
    drv.te.on_te_edge();
    assert_eq!(drv.te.check_band(1, 0, InstantMs(100)), TeWait::Waiting);
    drv.te.reset();

    // Frame 1: wait for TE across multiple polls -> ReadyBand preserved, metrics == 0
    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(screen);
    drv.now_ms = InstantMs(100);
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();

    let (st, m) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert_eq!(m.bands, 0);
    assert_eq!(m.pixels_sent, 0);
    assert!(!coord.has_in_flight());
    assert!(coord.has_queued());

    // Poll at t=110 with missing edge -> still Pending, 0 pixels sent
    drv.now_ms = InstantMs(110);
    let (st, m) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert_eq!(m.bands, 0);
    assert_eq!(m.pixels_sent, 0);

    // Duplicate TE edges before t=115 satisfy the initial wait cleanly
    drv.te.on_te_edge();
    drv.te.on_te_edge();
    drv.now_ms = InstantMs(115);
    let (st, m) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st, PresentStatus::Pending);
    assert!(coord.has_in_flight());
    assert_eq!(m.bands, 1);

    // Mid-frame stray TE edge and advancing past 20 ms do NOT block/timeout seq 1..3
    drv.te.on_te_edge();
    drv.now_ms = InstantMs(160);
    loop {
        drv.complete_current();
        let (st, _) = coord
            .step(
                &mut drv,
                &cmds,
                &mut dmg,
                &SliceSceneResources::EMPTY,
                &font,
            )
            .unwrap();
        if st == PresentStatus::Presented {
            break;
        }
    }
    assert!(dmg.is_empty());

    // Frame 2: missing TE edge causes explicit timeout and preserves full-screen damage
    dmg.mark_full_screen(screen);
    drv.now_ms = InstantMs(200);
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    let _ = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();

    drv.now_ms = InstantMs(219);
    let (st_before_to, m_before_to) = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    assert_eq!(st_before_to, PresentStatus::Pending);
    assert_eq!(m_before_to.bands, 0);

    drv.now_ms = InstantMs(220);
    let to_err = coord.step(
        &mut drv,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    );
    assert_eq!(to_err, Err(PresentError::Target(TeError::TimedOut)));
    assert!(!coord.is_active());
    assert!(!coord.is_faulted());
    assert!(dmg.is_full_screen());

    // Recovery after timeout: retry at t=250, deliver TE at t=255 -> Presented + pixel exact
    drv.now_ms = InstantMs(250);
    coord
        .begin_frame::<TeError<&'static str>>(&dmg, screen, 4)
        .unwrap();
    let _ = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    drv.te.on_te_edge();
    drv.now_ms = InstantMs(255);
    let _ = coord
        .step(
            &mut drv,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
        )
        .unwrap();
    loop {
        drv.complete_current();
        let (st, _) = coord
            .step(
                &mut drv,
                &cmds,
                &mut dmg,
                &SliceSceneResources::EMPTY,
                &font,
            )
            .unwrap();
        if st == PresentStatus::Presented {
            break;
        }
    }
    assert!(dmg.is_empty());

    let mut sync_words = vec![0u16; 32 * 16];
    let mut sync_scratch = [0u16; 32 * 4];
    let mut sync_dmg = DamageSet::new();
    sync_dmg.mark_full_screen(screen);
    present_sync(
        &mut SlicePanel {
            w: 32,
            fb: &mut sync_words,
        },
        &mut sync_scratch,
        screen,
        4,
        &cmds,
        &mut sync_dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();
    assert_eq!(drv.fb, sync_words);
}

struct SlicePanel<'a> {
    w: u32,
    fb: &'a mut [u16],
}

impl embedded_graphics::geometry::OriginDimensions for SlicePanel<'_> {
    fn size(&self) -> embedded_graphics::geometry::Size {
        embedded_graphics::geometry::Size::new(self.w, (self.fb.len() as u32) / self.w)
    }
}

impl embedded_graphics::draw_target::DrawTarget for SlicePanel<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        use embedded_graphics::pixelcolor::IntoStorage;
        for embedded_graphics::Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 {
                let idx = (p.y as usize) * (self.w as usize) + (p.x as usize);
                if idx < self.fb.len() {
                    self.fb[idx] = c.into_storage();
                }
            }
        }
        Ok(())
    }
}
