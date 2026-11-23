use core::convert::Infallible;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use liuli::prelude::*;
use liuli::render::InFlightBand;
use liuli_settings::{
    PageId, SettingsApp, SettingsModel, TOAST_DURATION_MS, TerminalApp, TerminalModel, app_fonts,
    settings_font, terminal_keys as tk,
};
use liuli_sim::screen::MemoryScreen;

struct CountingAllocator;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ZEROED_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ZEROED_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

struct StaticDmaSink<'a> {
    fb: Box<[u16]>,
    width: u32,
    in_flight: Option<InFlightBand<'a>>,
}

impl<'a> StaticDmaSink<'a> {
    fn new(width: u32, height: u32) -> Self {
        Self {
            fb: vec![0u16; (width * height) as usize].into_boxed_slice(),
            width,
            in_flight: None,
        }
    }
}

impl<'a> DmaSink<'a> for StaticDmaSink<'a> {
    type Error = ();

    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error> {
        if self.in_flight.is_some() {
            return StartOutcome::Busy(band);
        }
        self.in_flight = Some(band.into_in_flight());
        StartOutcome::Started
    }

    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error> {
        let Some(band) = self.in_flight.take() else {
            return PollOutcome::Pending;
        };
        let meta = band.meta();
        let src = band.pixels();
        for ry in 0..meta.rect.h as usize {
            let dst_off =
                (meta.rect.y as usize + ry) * (self.width as usize) + (meta.rect.x as usize);
            let src_off = ry * (meta.rect.w as usize);
            self.fb[dst_off..dst_off + meta.rect.w as usize]
                .copy_from_slice(&src[src_off..src_off + meta.rect.w as usize]);
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
fn runtime_loop_performs_zero_heap_allocations() {
    // Pre-allocate all host buffers outside the counting window.
    let mut storage = Box::new(Storage::new());
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let dma_a: &'static mut [u16] = Box::leak(vec![0u16; DEFAULT_BAND_PIXELS].into_boxed_slice());
    let dma_b: &'static mut [u16] = Box::leak(vec![0u16; DEFAULT_BAND_PIXELS].into_boxed_slice());
    let mut dma = DmaCoordinator::new(dma_a, dma_b);
    let mut sink = StaticDmaSink::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let font = settings_font();
    let rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    ALLOC_CALLS.store(0, Ordering::SeqCst);
    ZEROED_CALLS.store(0, Ordering::SeqCst);
    REALLOC_CALLS.store(0, Ordering::SeqCst);
    COUNTING.store(true, Ordering::SeqCst);

    let result: Result<(), ()> = (|| {
        let mut engine = Engine::<SettingsApp>::new_in_place(
            &mut storage,
            SettingsModel::new(InstantMs(0)),
            font,
            rect,
            InstantMs(0),
        )
        .map_err(|_| ())?;
        engine.set_focus_ring_visible(true);

        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| ())?;

        // Nav to sound + toggle -> 160ms animation
        engine
            .dispatch(RawInput::Nav(NavDir::Next), InstantMs(10), false)
            .map_err(|_| ())?;
        engine
            .dispatch(RawInput::Activate, InstantMs(20), false)
            .map_err(|_| ())?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| ())?;

        let _ = engine.advance(InstantMs(100), false).map_err(|_| ())?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| ())?;
        let _ = engine.advance(InstantMs(180), false).map_err(|_| ())?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| ())?;

        // Nav to theme -> toggle -> nav to reset (triggers scroll)
        engine
            .dispatch(RawInput::Nav(NavDir::Next), InstantMs(190), false)
            .map_err(|_| ())?;
        engine
            .dispatch(RawInput::Activate, InstantMs(200), false)
            .map_err(|_| ())?;
        engine
            .dispatch(RawInput::Nav(NavDir::Next), InstantMs(210), false)
            .map_err(|_| ())?;
        engine
            .dispatch(RawInput::Nav(NavDir::Next), InstantMs(220), false)
            .map_err(|_| ())?;

        // Temperature update while visible
        let _ = engine.advance(InstantMs(1220), false).map_err(|_| ())?;

        // Drive one full DMA transfer frame through DmaCoordinator + StaticDmaSink
        for _ in 0..128 {
            if engine.poll_dma(&mut dma, &mut sink).map_err(|_| ())? == PresentStatus::Presented {
                break;
            }
        }

        // Open and close modal
        engine
            .dispatch(RawInput::Activate, InstantMs(1230), false)
            .map_err(|_| ())?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| ())?;
        engine
            .dispatch(RawInput::Back, InstantMs(1240), false)
            .map_err(|_| ())?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| ())?;
        Ok(())
    })();

    COUNTING.store(false, Ordering::SeqCst);

    let allocs = ALLOC_CALLS.load(Ordering::SeqCst);
    let zeroed = ZEROED_CALLS.load(Ordering::SeqCst);
    let reallocs = REALLOC_CALLS.load(Ordering::SeqCst);

    assert!(result.is_ok(), "engine path returned unexpected error");
    assert_eq!(allocs, 0, "unexpected alloc calls in hot loop: {allocs}");
    assert_eq!(
        zeroed, 0,
        "unexpected alloc_zeroed calls in hot loop: {zeroed}"
    );
    assert_eq!(
        reallocs, 0,
        "unexpected realloc calls in hot loop: {reallocs}"
    );
}

struct CountingSpiBus {
    windows: u32,
    bytes: u32,
    flushes: u32,
}

impl CountingSpiBus {
    const fn new() -> Self {
        Self {
            windows: 0,
            bytes: 0,
            flushes: 0,
        }
    }
}

impl SpiWindowBus for CountingSpiBus {
    type Error = Infallible;

    fn set_window(&mut self, _rect: Rect) -> Result<(), Self::Error> {
        self.windows = self.windows.saturating_add(1);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.bytes = self.bytes.saturating_add(bytes.len() as u32);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.flushes = self.flushes.saturating_add(1);
        Ok(())
    }
}

#[test]
fn terminal_three_page_full_lifecycle_performs_zero_heap_allocations() {
    // Pre-allocate all host buffers outside the counting window.
    let mut storage = Box::new(Storage::new());
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut band_a = Box::new(AlignedBand::<DEFAULT_BAND_PIXELS>::zeroed());
    let mut band_b = Box::new(AlignedBand::<DEFAULT_BAND_PIXELS>::zeroed());
    let mut dma = DmaCoordinator::new(band_a.as_mut_slice(), band_b.as_mut_slice());
    let mut sink = StaticDmaSink::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut spi_chunk = [0u8; RECOMMENDED_SPI_CHUNK_BYTES];
    let fonts = app_fonts();
    let rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    ALLOC_CALLS.store(0, Ordering::SeqCst);
    ZEROED_CALLS.store(0, Ordering::SeqCst);
    REALLOC_CALLS.store(0, Ordering::SeqCst);
    COUNTING.store(true, Ordering::SeqCst);

    let result: Result<(u32, u32, u32), &'static str> = (|| {
        let mut spi_target = SpiWindowTarget::new(
            CountingSpiBus::new(),
            rect,
            ByteOrder::BigEndian,
            &mut spi_chunk,
        )
        .map_err(|_| "spi target init")?;

        let mut engine = Engine::<TerminalApp>::new_in_place_with_fonts(
            &mut storage,
            TerminalModel::new(InstantMs(0)),
            fonts,
            rect,
            InstantMs(0),
        )
        .map_err(|_| "engine init")?;
        engine
            .try_set_focus_ring_visible(true)
            .map_err(|_| "focus ring")?;

        // 1. Initial frame via SpiWindowTarget + flush
        if engine
            .present_sync_with(&mut spi_target, &mut scratch, SpiWindowTarget::flush)
            .map_err(|_| "spi present")?
            != PresentStatus::Presented
        {
            return Err("expected Presented on initial SPI frame");
        }
        let m0 = engine.last_metrics();

        // 2. Telemetry ticks + sync MemoryScreen present
        let _ = engine
            .try_advance(InstantMs(1000), false)
            .map_err(|_| "adv 1000")?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| "mem present 1000")?;
        let _ = engine
            .try_advance(InstantMs(2000), false)
            .map_err(|_| "adv 2000")?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| "mem present 2000")?;

        // 3. Long-press repeat on Dashboard '-' button (adjusts load_bias / load_pct downward)
        engine
            .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(2010), false)
            .map_err(|_| "nav dec")?;
        if engine.focus().focused_key() != Some(tk::DASH_BTN_DEC) {
            return Err("expected DASH_BTN_DEC");
        }
        let bias_before_hold = engine.model().load_bias;
        engine
            .try_dispatch(RawInput::KeyDown(HoldKey::Activate), InstantMs(2020), false)
            .map_err(|_| "hold down")?;
        let _ = engine
            .try_advance(InstantMs(2420), false)
            .map_err(|_| "hold delay")?;
        let _ = engine
            .try_advance(InstantMs(2900), false)
            .map_err(|_| "hold repeat")?;
        engine
            .try_dispatch(RawInput::KeyUp(HoldKey::Activate), InstantMs(2910), false)
            .map_err(|_| "hold up")?;
        if engine.model().load_bias >= bias_before_hold {
            return Err("expected load_bias decreased after long press");
        }

        // 4. Switch to Settings tab (x=160, y=14), toggle Dark Theme (EaseOutCubic), nav to Reset
        engine
            .try_dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(160, 14),
                },
                InstantMs(2920),
                false,
            )
            .map_err(|_| "tab settings down")?;
        engine
            .try_dispatch(
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(160, 14),
                },
                InstantMs(2930),
                false,
            )
            .map_err(|_| "tab settings up")?;
        if engine.model().active_page != PageId::Settings {
            return Err("expected PageId::Settings");
        }

        let mut now_ms = 2940u64;
        for _ in 0..16 {
            if engine.focus().focused_key() == Some(tk::SET_TOGGLE_THEME) {
                break;
            }
            now_ms += 10;
            engine
                .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(now_ms), false)
                .map_err(|_| "nav to theme")?;
        }
        if engine.focus().focused_key() != Some(tk::SET_TOGGLE_THEME) {
            return Err("did not reach SET_TOGGLE_THEME");
        }
        now_ms += 10;
        engine
            .try_dispatch(RawInput::Activate, InstantMs(now_ms), false)
            .map_err(|_| "toggle dark theme")?;
        now_ms += 80;
        let _ = engine
            .try_advance(InstantMs(now_ms), false)
            .map_err(|_| "anim mid")?;
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| "present anim mid")?;
        now_ms += 80;
        let _ = engine
            .try_advance(InstantMs(now_ms), false)
            .map_err(|_| "anim end")?;

        for _ in 0..8 {
            if engine.focus().focused_key() == Some(tk::SET_BTN_RESET) {
                break;
            }
            now_ms += 10;
            engine
                .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(now_ms), false)
                .map_err(|_| "nav to reset")?;
        }
        if engine.focus().focused_key() != Some(tk::SET_BTN_RESET) {
            return Err("did not reach SET_BTN_RESET");
        }

        // 5. Switch to Diagnostics tab (x=280, y=14) -> record metrics & render Small12
        let m_snap = engine.last_metrics();
        engine
            .try_with_model(|m| m.record_completed_frame(m_snap, InstantMs(now_ms)))
            .map_err(|_| "record frame")?;
        now_ms += 10;
        engine
            .try_dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(280, 14),
                },
                InstantMs(now_ms),
                false,
            )
            .map_err(|_| "tab diag down")?;
        now_ms += 10;
        engine
            .try_dispatch(
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(280, 14),
                },
                InstantMs(now_ms),
                false,
            )
            .map_err(|_| "tab diag up")?;
        if engine.model().active_page != PageId::Diagnostics {
            return Err("expected PageId::Diagnostics");
        }
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| "present diagnostics")?;

        // 6. Switch back to Settings tab -> scroll/focus restored -> open modal -> confirm -> Toast
        now_ms += 10;
        engine
            .try_dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(160, 14),
                },
                InstantMs(now_ms),
                false,
            )
            .map_err(|_| "tab set back down")?;
        now_ms += 10;
        engine
            .try_dispatch(
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(160, 14),
                },
                InstantMs(now_ms),
                false,
            )
            .map_err(|_| "tab set back up")?;
        if engine.focus().focused_key() != Some(tk::SET_BTN_RESET) {
            return Err("expected SET_BTN_RESET restored on return");
        }

        now_ms += 10;
        engine
            .try_dispatch(RawInput::Activate, InstantMs(now_ms), false)
            .map_err(|_| "open reset modal")?;
        now_ms += 10;
        engine
            .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(now_ms), false)
            .map_err(|_| "nav modal confirm")?;
        if engine.focus().focused_key() != Some(tk::BTN_CONFIRM) {
            return Err("expected BTN_CONFIRM");
        }
        now_ms += 10;
        engine
            .try_dispatch(RawInput::Activate, InstantMs(now_ms), false)
            .map_err(|_| "confirm reset")?;
        if engine.model().toast.is_none() {
            return Err("expected toast after reset");
        }
        let _ = engine
            .present_sync(&mut screen, &mut scratch)
            .map_err(|_| "present toast")?;

        // Expire toast and drive full DMA frame (while asserting Busy frame lock)
        now_ms += TOAST_DURATION_MS + 10;
        let _ = engine
            .try_advance(InstantMs(now_ms), false)
            .map_err(|_| "expire toast")?;
        if engine.model().toast.is_some() {
            return Err("expected toast expired");
        }

        let mut saw_busy = false;
        let mut presented_dma = false;
        for _ in 0..128 {
            match engine
                .poll_dma(&mut dma, &mut sink)
                .map_err(|_| "poll dma")?
            {
                PresentStatus::Pending => {
                    if !engine.is_dma_busy() {
                        return Err("expected is_dma_busy while Pending");
                    }
                    saw_busy = true;
                    if !matches!(
                        engine.try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(now_ms), false),
                        Err(TransactionError::Busy(_))
                    ) {
                        return Err("expected Busy on try_dispatch during DMA");
                    }
                }
                PresentStatus::Presented => {
                    presented_dma = true;
                    break;
                }
                PresentStatus::Idle => break,
            }
        }
        if !presented_dma || !saw_busy || engine.is_dma_busy() {
            return Err("DMA transfer did not exercise Busy -> Presented cleanly");
        }

        Ok((m0.bands, spi_target.bus().windows, spi_target.bus().flushes))
    })();

    COUNTING.store(false, Ordering::SeqCst);

    let allocs = ALLOC_CALLS.load(Ordering::SeqCst);
    let zeroed = ZEROED_CALLS.load(Ordering::SeqCst);
    let reallocs = REALLOC_CALLS.load(Ordering::SeqCst);

    let (init_bands, spi_windows, spi_flushes) =
        result.unwrap_or_else(|msg| panic!("terminal lifecycle failed: {msg}"));
    assert_eq!(init_bands, 30);
    assert_eq!(spi_windows, 30);
    assert_eq!(spi_flushes, 1);
    assert_eq!(allocs, 0, "unexpected alloc calls in hot loop: {allocs}");
    assert_eq!(
        zeroed, 0,
        "unexpected alloc_zeroed calls in hot loop: {zeroed}"
    );
    assert_eq!(
        reallocs, 0,
        "unexpected realloc calls in hot loop: {reallocs}"
    );
}
