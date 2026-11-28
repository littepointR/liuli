#![no_std]
#![no_main]
use core::hint::black_box;
use liuli::prelude::*;

// Disabled pools must disappear from the actual type layout.
const _: () = {
    use liuli::core::{
        capacity as c,
        effects::Effects,
        part_style::{LayoutExtras, PartStyles},
        time::AnimPool,
        view::SparklinePool,
    };
    assert!(c::MAX_ANIMATIONS != 0 || size_of::<AnimPool>() == 0);
    assert!(c::MAX_EFFECTS != 0 || size_of::<Effects>() == 0);
    assert!(c::MAX_PART_STYLES != 0 || size_of::<PartStyles>() == 0);
    assert!(c::MAX_LAYOUT_EXTRAS != 0 || size_of::<LayoutExtras>() == 0);
    assert!(c::MAX_SPARKLINES != 0 || size_of::<SparklinePool>() == 0);
    assert!(c::METRICS || size_of::<c::FrameCounters>() == 0);
};

static mut STORAGE: Storage = Storage::ZERO;
static mut BAND: [u16; 64 * 8] = [0; 64 * 8];

// Identical bounded allocator for the three allocator-enabled link comparisons.
#[cfg(feature = "heap")]
mod heap {
    use core::alloc::{GlobalAlloc, Layout};
    use core::sync::atomic::{AtomicUsize, Ordering};
    struct Bump;
    #[global_allocator]
    static ALLOC: Bump = Bump;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    static mut POOL: [u8; 4096] = [0; 4096];
    unsafe impl GlobalAlloc for Bump {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let base = (&raw mut POOL).cast::<u8>();
            let mut pos = NEXT.load(Ordering::Relaxed);
            loop {
                let Some(aligned) = (base.addr() + pos).checked_next_multiple_of(layout.align())
                else {
                    return core::ptr::null_mut();
                };
                let start = aligned - base.addr();
                let Some(end) = start.checked_add(layout.size()).filter(|&v| v <= 4096) else {
                    return core::ptr::null_mut();
                };
                match NEXT.compare_exchange_weak(pos, end, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => return unsafe { base.add(start) },
                    Err(actual) => pos = actual,
                }
            }
        }
        unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
    }
}

struct Label;
impl App for Label {
    type Model = ();
    type Msg = ();
    fn update(_: &mut (), _: (), _: InstantMs) -> Change {
        Change::Unchanged
    }
    fn on_action(_: &(), _: UiAction) -> Option<()> {
        None
    }
    fn view(_: &()) -> impl Scene {
        layout(Key(1), NodeProps::new(), move |ui| {
            ui.label(Key(2), NodeProps::new().text(TextRef::Static("A")))
        })
    }
}
struct Sink;
impl DisplaySink for Sink {
    type Error = ();
    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), ()> {
        black_box((rect, pixels));
        Ok(())
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn Reset() -> ! {
    // Link-time footprint probe only. Use the reference firmware for runnable board startup.
    let font = Lf4Font::from_bytes(black_box(include_bytes!(
        "../../../apps/liuli-widget-demo/assets/heading.lf4"
    )))
    .unwrap();
    let mut e = Engine::<Label>::new_in_place_with_provider(
        unsafe { &mut *(&raw mut STORAGE) },
        (),
        FontSet::single(font),
        Rect::new(0, 0, 64, 32),
        InstantMs(0),
    )
    .unwrap();
    #[cfg(feature = "dict")]
    black_box(&liuli_widgets::ime::DEFAULT_PINYIN_DICT);
    let _ = black_box(e.present(&mut Sink, unsafe { &mut *(&raw mut BAND) }));
    loop {
        let now = InstantMs(black_box(16));
        let _ = black_box(e.tick(now));
        black_box(e.wakeup(now));
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop()
    }
}
