#![no_std]
use core::panic::PanicInfo;
use embedded_graphics_core::{
    Pixel, draw_target::DrawTarget, geometry::OriginDimensions, pixelcolor::IntoStorage,
};
use liuli::prelude::*;
mod application;
use application::BoardApp;

#[cfg(feature = "benchmark")]
mod benchmark;
#[cfg(feature = "enhanced")]
mod enhanced;

const WIDTH: u32 = 240;
const HEIGHT: u32 = 536;
const WORDS: usize = WIDTH as usize * 8;
static mut STORAGE: Storage = Storage::ZERO;
static mut SCRATCH: [u16; WORDS] = [0; WORDS];
static mut WIRE: [u16; WORDS] = [0; WORDS];

unsafe extern "C" {
    #[cfg(feature = "benchmark")]
    fn board_take_wake_report() -> i32;
    fn board_pixels(x: i32, y: i32, w: u32, h: u32, bytes: *const u16) -> i32;
    fn board_now_us() -> i64;
    fn board_touch(points: *mut Contact) -> i32;
    fn board_wait();
    fn board_frame(pixels: u32, elapsed_us: i64);
    fn board_blend(p: *mut u16, len: usize, fg: u16, a: u32);
    fn abort() -> !;
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { abort() }
}
struct Display<'a>(&'a mut [u16]);
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
        let dst = self.0.get_mut(..count).ok_or(())?;
        let mut source = colors.into_iter();
        for word in dst.iter_mut() {
            *word = source.next().ok_or(())?.into_storage().to_be();
        }
        // The blocking board driver completes the transfer before releasing this borrow.
        if unsafe {
            board_pixels(
                rect.top_left.x,
                rect.top_left.y,
                rect.size.width,
                rect.size.height,
                dst.as_ptr(),
            )
        } == 0
        {
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
        .present_sync_with_pixels(target, scratch, |_| Ok(()), glyphs)
        .map_err(|_| ());
    glyphs.finish();
    result
}

#[unsafe(no_mangle)]
pub extern "C" fn liuli_static_bytes() -> u32 {
    (size_of::<Storage>() + 2 * size_of::<[u16; WORDS]>()) as u32
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
    let mut display = Display(unsafe { &mut *(&raw mut WIRE) });
    let Ok(mut engine) = Engine::<BoardApp>::new_in_place(
        storage,
        application::model(HEIGHT as u16),
        font,
        Rect::new(0, 0, WIDTH, HEIGHT),
        now,
    ) else {
        return -2;
    };
    #[cfg(feature = "enhanced")]
    engine.set_sync_blend(|row, fg, a| unsafe {
        board_blend(row.as_mut_ptr(), row.len(), fg.into_storage(), u32::from(a))
    });
    #[cfg(feature = "benchmark")]
    if unsafe { board_take_wake_report() } == 0
        && benchmark::run(&mut engine, &mut display, scratch, &mut glyphs).is_err()
    {
        return -7;
    }
    let mut previous = [Contact::default(); 2];
    let mut previous_count = 0;
    loop {
        let start = unsafe { board_now_us() };
        let now = InstantMs(start as u64 / 1000);
        let mut points = [Contact::default(); 2];
        let status = unsafe { board_touch(points.as_mut_ptr()) };
        let count = status.clamp(0, 2) as usize;
        if engine.try_advance(now, false).is_err() {
            return -3;
        }
        for old in &previous[..previous_count] {
            if !points[..count].iter().any(|p| p.id == old.id) {
                let id = PointerId(old.id);
                let input = if status < 0 {
                    RawInput::PointerCancel { id }
                } else {
                    RawInput::PointerUp { id, pos: old.pos() }
                };
                if engine.try_dispatch(input, now, false).is_err() {
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
            if engine.try_dispatch(input, now, false).is_err() {
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
        unsafe { board_wait() };
    }
}
