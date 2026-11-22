use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use liuli::prelude::*;
use liuli::render::InFlightBand;
use liuli_settings::{SettingsApp, SettingsModel, settings_font};
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
