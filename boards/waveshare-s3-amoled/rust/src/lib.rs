#![no_std]
use core::panic::PanicInfo;
use liuli::prelude::*;
#[cfg(any(not(feature = "widget-demo"), feature = "benchmark"))]
mod application;
#[cfg(all(feature = "effects-benchmark", not(feature = "benchmark")))]
mod application {
    pub use liuli_widget_demo::Catalog as BoardApp;
    pub fn model(height: u16) -> liuli_widget_demo::Model {
        liuli_widget_demo::Model::new(height)
    }
}
#[cfg(all(
    feature = "widget-demo",
    not(any(feature = "effects-benchmark", feature = "benchmark"))
))]
mod application {
    pub use liuli_widget_demo::showcase::ShowcaseApp as BoardApp;
    pub fn model(height: u16) -> liuli_widget_demo::showcase::ShowcaseModel {
        liuli_widget_demo::showcase::ShowcaseModel::for_size(liuli::prelude::Size::new(
            super::WIDTH,
            height.into(),
        ))
    }
}
use application::BoardApp;

#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(all(feature = "effects-benchmark", not(feature = "benchmark")))]
mod effects_benchmark;
#[cfg(feature = "enhanced")]
mod enhanced;
#[cfg(feature = "input-trace")]
mod input_trace;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 536;
#[cfg(feature = "enhanced")]
const BAND_HEIGHT: u32 = 32;
#[cfg(not(feature = "enhanced"))]
const BAND_HEIGHT: u32 = 8;
const WORDS: usize = WIDTH as usize * BAND_HEIGHT as usize;
static mut STORAGE: Storage = Storage::ZERO;
static mut SCRATCH: [u16; WORDS] = [0; WORDS];
static mut WIRE: [[u16; WORDS]; 2] = [[0; WORDS]; 2];

unsafe extern "C" {
    #[cfg(feature = "sleep-benchmark")]
    fn board_take_wake_report() -> i32;
    fn board_pixels(x: i32, y: i32, w: u32, h: u32, bytes: *const u16) -> i32;
    fn board_flush() -> i32;
    fn board_now_us() -> i64;
    fn board_touch(points: *mut Contact) -> i32;
    #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
    fn board_wait();
    fn board_wait_input(deadline_ms: u32, since_read_ms: u32, active: i32) -> i32;
    #[cfg(feature = "diagnostics")]
    fn board_frame(pixels: u32, elapsed_us: i64);
    #[cfg(feature = "enhanced")]
    fn board_blend(p: *mut u16, len: usize, fg: u16, a: u32);
    fn abort() -> !;
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { abort() }
}
struct Display<'a> {
    wire: [&'a mut [u16]; 2],
    slot: usize,
}
impl DisplaySink for Display<'_> {
    type Error = ();

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), ()> {
        let count = (rect.w as usize).saturating_mul(rect.h as usize);
        if pixels.len() < count {
            return Err(());
        }
        let dst = self.wire[self.slot].get_mut(..count).ok_or(())?;
        encode_rgb565_words(&pixels[..count], dst, ByteOrder::BigEndian).map_err(|_| ())?;
        // `board_pixels` waits for any previous band DMA before reusing `self.wire[self.slot]`.
        let res = unsafe { board_pixels(rect.x, rect.y, rect.w, rect.h, dst.as_ptr()) };
        self.slot ^= 1;
        if res == 0 { Ok(()) } else { Err(()) }
    }

    fn finish_frame(&mut self) -> Result<(), ()> {
        if unsafe { board_flush() } == 0 {
            Ok(())
        } else {
            Err(())
        }
    }
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
struct Contact {
    x: i32,
    y: i32,
    id: u8,
}
impl Contact {
    fn pos(self) -> Point {
        Point::new(self.x, self.y)
    }
}
trait FrameGlyphs: GlyphPixels {
    fn start(&mut self) {}
    fn finish(&self) {}
}
impl FrameGlyphs for InlinePixels {}
fn present(
    e: &mut Engine<'_, '_, BoardApp>,
    target: &mut Display<'_>,
    scratch: &mut [u16],
    glyphs: &mut impl FrameGlyphs,
) -> Result<PresentStatus, ()> {
    let step = core::num::NonZeroU16::new(2).unwrap();
    e.align_damage(step, step).map_err(|_| ())?;
    glyphs.start();
    let result = e
        .present_with_pixels(target, scratch, glyphs)
        .map_err(|_| ());
    glyphs.finish();
    result
}

#[unsafe(no_mangle)]
pub extern "C" fn liuli_static_bytes() -> u32 {
    (size_of::<Storage>() + size_of::<[u16; WORDS]>() + size_of::<[[u16; WORDS]; 2]>()) as u32
}
#[unsafe(no_mangle)]
pub extern "C" fn liuli_benchmark() -> i32 {
    cfg!(feature = "benchmark") as i32
}
#[unsafe(no_mangle)]
pub extern "C" fn liuli_task_stack() -> u32 {
    if cfg!(feature = "enhanced") {
        48 * 1024
    } else {
        16 * 1024
    }
}

/// Called once by the ESP-IDF UI task. The mapped font remains valid for the loop.
/// # Safety
/// The caller must provide readable font memory and exclusive use of the UI statics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn liuli_run(ptr: *const u8, len: usize) -> i32 {
    let mapped = unsafe { core::slice::from_raw_parts(ptr, len) };
    #[cfg(any(
        not(feature = "widget-demo"),
        feature = "benchmark",
        feature = "effects-benchmark"
    ))]
    let Ok(font) = Lf4Font::from_bytes(mapped) else {
        return -1;
    };
    #[cfg(feature = "enhanced")]
    let mut cache = enhanced::cache();
    #[cfg(feature = "enhanced")]
    let Ok(mut glyphs) = enhanced::Reader::new(mapped, &mut cache) else {
        return -6;
    };
    #[cfg(not(feature = "enhanced"))]
    let mut glyphs = InlinePixels;
    let initial = unsafe { board_now_us() };
    let now = InstantMs(initial as u64 / 1000);
    let storage_ptr = &raw mut STORAGE;
    let scratch_ptr = &raw mut SCRATCH;
    let wire_ptr = &raw mut WIRE;
    let storage = unsafe { &mut *storage_ptr };
    let scratch = unsafe { &mut *scratch_ptr };
    let [w0, w1] = unsafe { &mut *wire_ptr };
    let mut display = Display {
        wire: [w0, w1],
        slot: 0,
    };
    #[cfg(all(
        feature = "widget-demo",
        not(any(feature = "effects-benchmark", feature = "benchmark"))
    ))]
    let Ok(fonts) = liuli_widget_demo::showcase::showcase_fonts() else {
        return -1;
    };
    #[cfg(all(feature = "effects-benchmark", not(feature = "benchmark")))]
    let Ok(fonts) = liuli_widget_demo::studio_fonts(font) else {
        return -1;
    };
    #[cfg(not(all(feature = "widget-demo", not(feature = "benchmark"))))]
    let fonts = FontSet::single(font);
    let Ok(mut engine) = Engine::<BoardApp>::new_in_place_with_fonts(
        storage,
        application::model(HEIGHT as u16),
        fonts,
        Rect::new(0, 0, WIDTH, HEIGHT),
        now,
    ) else {
        return -2;
    };
    engine.set_band_height(BAND_HEIGHT);
    #[cfg(feature = "enhanced")]
    engine.set_sync_blend(|row, fg, a| unsafe {
        board_blend(row.as_mut_ptr(), row.len(), rgb565_to_u16(fg), u32::from(a))
    });
    #[cfg(feature = "sleep-benchmark")]
    let run_benchmark = unsafe { board_take_wake_report() } == 0;
    #[cfg(all(feature = "benchmark", not(feature = "sleep-benchmark")))]
    let run_benchmark = true;
    #[cfg(feature = "benchmark")]
    if run_benchmark && benchmark::run(&mut engine, &mut display, scratch, &mut glyphs).is_err() {
        return -7;
    }
    #[cfg(all(feature = "effects-benchmark", not(feature = "benchmark")))]
    if effects_benchmark::run(&mut engine, &mut display, scratch, &mut glyphs, now).is_err() {
        return -8;
    }
    let mut tracker = TouchTracker::<2>::new();
    let mut read_touch = true;
    let mut last_read = now;
    let mut retry_touch = false;
    #[cfg(feature = "input-trace")]
    let mut trace = input_trace::Recorder::start(now);
    loop {
        let start = unsafe { board_now_us() };
        let now = InstantMs(start as u64 / 1000);
        let mut points = [Contact::default(); 2];
        let status = if read_touch {
            last_read = now;
            unsafe { board_touch(points.as_mut_ptr()) }
        } else {
            0
        };
        let count = status.clamp(0, 2) as usize;
        if (cfg!(feature = "input-trace") || engine.wakeup(now) == NextWakeup::Now)
            && engine.tick(now).is_err()
        {
            return -3;
        }
        #[cfg(feature = "input-trace")]
        trace.tick(now);
        let mut contacts = [TouchPoint::new(0, Point::zero()); 2];
        for (dst, src) in contacts.iter_mut().zip(&points[..count]) {
            *dst = TouchPoint::new(src.id, src.pos());
        }
        let mut dispatch = |input| {
            let result = engine.handle_input(input, now);
            #[cfg(feature = "input-trace")]
            if result.is_ok() {
                trace.input(now, input);
            }
            result.map(|_| ()).map_err(|_| ())
        };
        if read_touch {
            retry_touch = status < 0;
        }
        let diff_res = if !read_touch {
            Ok(())
        } else if status < 0 {
            tracker.try_cancel_all(&mut dispatch)
        } else {
            tracker.try_update(&contacts[..count], &mut dispatch)
        };
        if diff_res.is_err() {
            return -4;
        }
        match present(&mut engine, &mut display, scratch, &mut glyphs) {
            #[cfg(feature = "diagnostics")]
            Ok(PresentStatus::Presented) => unsafe {
                board_frame(engine.last_metrics().pixels_sent, board_now_us() - start)
            },
            Ok(_) => {}
            Err(_) => return -5,
        }
        let after = InstantMs(unsafe { board_now_us() } as u64 / 1000);
        let deadline_ms = match engine.wakeup(after) {
            NextWakeup::Now => 0,
            NextWakeup::At(t) => t.0.saturating_sub(after.0).min(u64::from(u32::MAX - 1)) as u32,
            NextWakeup::OnEvent => u32::MAX,
        };
        let active = tracker.active_count() != 0 || retry_touch || cfg!(feature = "input-trace");
        let since_read = after.0.saturating_sub(last_read.0).min(u64::from(u32::MAX)) as u32;
        read_touch = unsafe { board_wait_input(deadline_ms, since_read, i32::from(active)) } != 0;
    }
}
