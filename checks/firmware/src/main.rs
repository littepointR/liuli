#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::convert::Infallible;
use core::hint::black_box;
use core::panic::PanicInfo;

use embedded_graphics_core::Pixel;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics_core::primitives::Rectangle;
use liuli::prelude::*;
use liuli_settings::{SettingsApp, SettingsModel, settings_font};

struct SyncCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for SyncCell<T> {}

impl<T> SyncCell<T> {
    const fn new(val: T) -> Self {
        Self(UnsafeCell::new(val))
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn get_mut(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

static STORAGE: SyncCell<Storage> = SyncCell::new(Storage::new());
static DMA_BAND_A: SyncCell<[u16; DEFAULT_BAND_PIXELS]> =
    SyncCell::new([0u16; DEFAULT_BAND_PIXELS]);
static DMA_BAND_B: SyncCell<[u16; DEFAULT_BAND_PIXELS]> =
    SyncCell::new([0u16; DEFAULT_BAND_PIXELS]);

struct NullSpiPanel;

impl OriginDimensions for NullSpiPanel {
    fn size(&self) -> Size {
        Size::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT)
    }
}

impl DrawTarget for NullSpiPanel {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(pt, color) in pixels {
            black_box((pt.x, pt.y, color.into_storage()));
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        black_box(area);
        for c in colors {
            black_box(c.into_storage());
        }
        Ok(())
    }
}

#[unsafe(link_section = ".vector_table.reset_vector")]
#[unsafe(no_mangle)]
pub static RESET_VECTOR: unsafe extern "C" fn() -> ! = Reset;

#[unsafe(no_mangle)]
pub extern "C" fn Reset() -> ! {
    // SAFETY: Single-core bare-metal reset entry with static non-overlapping buffers.
    let storage = unsafe { STORAGE.get_mut() };
    let band_a = unsafe { DMA_BAND_A.get_mut() };
    let band_b = unsafe { DMA_BAND_B.get_mut() };

    let screen = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut engine = Engine::<SettingsApp>::new_in_place(
        storage,
        SettingsModel::new(InstantMs(0)),
        settings_font(),
        screen,
        InstantMs(0),
    )
    .expect("init");
    engine.set_focus_ring_visible(true);

    let mut panel = NullSpiPanel;
    let _ = black_box(engine.present_sync(&mut panel, band_a));
    let _ = black_box(engine.dispatch(RawInput::Nav(NavDir::Next), InstantMs(16), false));
    let _ = black_box(engine.present_sync(&mut panel, band_b));

    loop {
        black_box(engine.next_wakeup(InstantMs(16), false));
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        black_box(());
    }
}
