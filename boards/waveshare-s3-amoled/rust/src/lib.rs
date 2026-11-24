#![no_std]
use core::panic::PanicInfo;
use embedded_graphics_core::{
    Pixel, draw_target::DrawTarget, geometry::OriginDimensions, pixelcolor::IntoStorage,
};
use liuli::prelude::*;
#[cfg(any(not(feature = "widget-demo"), feature = "benchmark"))]
mod application;
#[cfg(all(feature = "widget-demo", not(feature = "benchmark")))]
mod application {
    pub use liuli_widget_demo::Catalog as BoardApp;
    pub fn model(height: u16) -> liuli_widget_demo::Model {
        liuli_widget_demo::Model::new(height)
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
    fn board_wait();
    fn board_wait_ms(ms: u32);
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
impl OriginDimensions for Display<'_> {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
impl DrawTarget for Display<'_> {
    type Color = Rgb565;
    type Error = ();
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, it: I) -> Result<(), ()> {
        for Pixel(pos, c) in it {
            if Rect::new(0, 0, WIDTH, HEIGHT).contains_point(pos) {
                self.fill_contiguous(&Rectangle::new(pos, Size::new(1, 1)), [c])?;
            }
        }
        Ok(())
    }
    fn fill_contiguous<I: IntoIterator<Item = Rgb565>>(
        &mut self,
        rect: &Rectangle,
        colors: I,
    ) -> Result<(), ()> {
        let count = rect.size.width as usize * rect.size.height as usize;
        let dst = self.wire[self.slot].get_mut(..count).ok_or(())?;
        let mut written = 0;
        for (word, c) in dst.iter_mut().zip(colors) {
            *word = c.into_storage().to_be();
            written += 1;
        }
        if written != count {
            return Err(());
        }
        // `board_pixels` waits for any previous band DMA before reusing `self.wire[self.slot]`.
        let res = unsafe {
            board_pixels(
                rect.top_left.x,
                rect.top_left.y,
                rect.size.width,
                rect.size.height,
                dst.as_ptr(),
            )
        };
        self.slot ^= 1;
        if res == 0 { Ok(()) } else { Err(()) }
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
trait FrameGlyphs: liuli::text::storage::GlyphPixels {
    fn start(&mut self) {}
    fn finish(&self) {}
}
impl FrameGlyphs for liuli::text::storage::InlinePixels {}
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
        .present_sync_with_pixels(
            target,
            scratch,
            |_| unsafe { if board_flush() == 0 { Ok(()) } else { Err(()) } },
            glyphs,
        )
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
    let Ok(font) = Lf4Font::from_bytes(unsafe { core::slice::from_raw_parts(ptr, len) }) else {
        return -1;
    };
    #[cfg(feature = "enhanced")]
    let mut cache = enhanced::cache();
    #[cfg(feature = "enhanced")]
    let Ok(mut glyphs) = enhanced::Reader::new(len, &mut cache) else {
        return -6;
    };
    #[cfg(not(feature = "enhanced"))]
    let mut glyphs = liuli::text::storage::InlinePixels;
    let initial = unsafe { board_now_us() };
    let now = InstantMs(initial as u64 / 1000);
    let storage = unsafe { &mut *(&raw mut STORAGE) };
    let scratch = unsafe { &mut *(&raw mut SCRATCH) };
    let [w0, w1] = unsafe { &mut *(&raw mut WIRE) };
    let mut display = Display {
        wire: [w0, w1],
        slot: 0,
    };
    let Ok(mut engine) = Engine::<BoardApp>::new_in_place(
        storage,
        application::model(HEIGHT as u16),
        font,
        Rect::new(0, 0, WIDTH, HEIGHT),
        now,
    ) else {
        return -2;
    };
    engine.set_band_height(BAND_HEIGHT);
    #[cfg(feature = "enhanced")]
    engine.set_sync_blend(|row, fg, a| unsafe {
        board_blend(row.as_mut_ptr(), row.len(), fg.into_storage(), u32::from(a))
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
    let mut previous = [Contact::default(); 2];
    let mut previous_count = 0;
    #[cfg(feature = "input-trace")]
    let mut trace = input_trace::Recorder::start(now);
    loop {
        let start = unsafe { board_now_us() };
        let now = InstantMs(start as u64 / 1000);
        let mut points = [Contact::default(); 2];
        let status = unsafe { board_touch(points.as_mut_ptr()) };
        let count = status.clamp(0, 2) as usize;
        if engine.try_advance(now, false).is_err() {
            return -3;
        }
        #[cfg(feature = "input-trace")]
        trace.tick(now);
        let mut dispatch = |input| {
            let result = engine.try_dispatch(input, now, false);
            #[cfg(feature = "input-trace")]
            if result.is_ok() {
                trace.input(now, input);
            }
            result
        };
        for old in &previous[..previous_count] {
            if !points[..count].iter().any(|p| p.id == old.id) {
                let id = PointerId(old.id);
                let input = if status < 0 {
                    RawInput::PointerCancel { id }
                } else {
                    RawInput::PointerUp { id, pos: old.pos() }
                };
                if dispatch(input).is_err() {
                    return -4;
                }
            }
        }
        for p in &points[..count] {
            let id = PointerId(p.id);
            let pos = p.pos();
            let input = match previous[..previous_count].iter().find(|old| old.id == p.id) {
                None => RawInput::PointerDown { id, pos },
                Some(old) if old.pos() != pos => RawInput::PointerMove { id, pos },
                _ => continue,
            };
            if dispatch(input).is_err() {
                return -4;
            }
        }
        previous = points;
        previous_count = count;
        match present(&mut engine, &mut display, scratch, &mut glyphs) {
            Ok(PresentStatus::Presented) => unsafe {
                board_frame(engine.last_metrics().pixels_sent, board_now_us() - start)
            },
            Ok(_) => {}
            Err(_) => return -5,
        }
        let after = InstantMs(unsafe { board_now_us() } as u64 / 1000);
        match engine.next_wakeup(after, false) {
            NextWakeup::Now => {}
            NextWakeup::At(t) => {
                let wait_ms = t.0.saturating_sub(after.0).clamp(1, 10) as u32;
                unsafe { board_wait_ms(wait_ms) };
            }
            NextWakeup::OnEvent => unsafe { board_wait() },
        }
    }
}
