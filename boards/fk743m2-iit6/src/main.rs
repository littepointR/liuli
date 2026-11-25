#![no_std]
#![no_main]

mod display;
mod hardware;
#[cfg(feature = "enhanced")]
mod heap;
mod touch;

use display::{Display, HEIGHT, WIDTH};
use hardware as hw;
use liuli::prelude::*;
use liuli_widget_demo::{Catalog, FONT, Model};

#[cfg(feature = "embassy")]
mod embassy;
#[cfg(feature = "st-hal")]
core::arch::global_asm!(include_str!("boot.s"));
static mut STORAGE: Storage = Storage::ZERO;
static mut STRIPE: [u16; 800 * 32] = [0; 800 * 32];
#[used]
#[unsafe(link_section = ".report")]
static mut REPORT: [u32; 16] = [0; 16];

fn report(i: usize, v: u32) {
    assert!(i < 16);
    unsafe {
        (&raw mut REPORT).cast::<u32>().add(i).write_volatile(v);
    }
}
fn read_report(i: usize) -> u32 {
    assert!(i < 16);
    unsafe { (&raw const REPORT).cast::<u32>().add(i).read_volatile() }
}
fn fail(code: u32) -> ! {
    report(1, 0xbad00000 | code);
    loop {
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail(0xff)
}
type Ui<'s> = Engine<'s, 'static, Catalog>;

fn present(
    ui: &mut Ui<'_>,
    screen: &mut Display,
    band: &mut [u16],
    frames: &mut u32,
) -> Result<(), u32> {
    let start = hw::cycles();
    let status = ui.present(screen, band).map_err(|_| 20u32)?;
    if status == PresentStatus::Presented {
        *frames = frames.wrapping_add(1);
        report(2, *frames);
        report(3, screen.front() as u32);
        report(4, ui.last_metrics().pixels_sent);
        report(5, hw::cycles().wrapping_sub(start) / hw::CPU_MHZ);
        report(9, hw::status());
    }
    Ok(())
}

async fn run() -> Result<(), u32> {
    report(0, 0x4c554c49);
    report(1, 1);
    hw::clocks()?;
    report(1, 2);
    hw::sdram()?;
    report(1, 3);
    #[cfg(feature = "enhanced")]
    unsafe {
        heap::init();
    }
    let mut screen = Display::new()?;
    let mut touch = touch::Touch::detect().ok();
    report(6, touch.as_ref().map_or(0, |t| t.product));
    let font = Lf4Font::from_bytes(FONT).map_err(|_| 14u32)?;
    // Reset is the only caller; interrupts never access UI state.
    let storage_ptr = &raw mut STORAGE;
    let band_ptr = &raw mut STRIPE;
    let storage = unsafe { &mut *storage_ptr };
    let band = unsafe { &mut *band_ptr };
    let mut ui = Engine::<Catalog>::new_in_place_with_fonts(
        storage,
        Model::for_size(Size::new(WIDTH, HEIGHT)),
        liuli_widget_demo::studio_fonts(font).map_err(|_| 14u32)?,
        Rect::new(0, 0, WIDTH, HEIGHT),
        InstantMs(0),
    )
    .map_err(|_| 15u32)?;
    #[cfg(feature = "enhanced")]
    ui.set_layer_budget(1024 * 1024).map_err(|_| 16u32)?;
    ui.set_band_height(32);
    ui.try_set_focus_ring_visible(true).map_err(|_| 17u32)?;
    let mut frames = 0;
    present(&mut ui, &mut screen, band, &mut frames)?;
    report(1, 4);
    #[cfg(feature = "st-hal")]
    let mut clock = {
        let mut c = MonotonicClock32::new(hw::CPU_MHZ * 1000);
        c.sample(hw::cycles());
        c
    };
    #[cfg(feature = "embassy")]
    let start = embassy_time::Instant::now();
    let mut io_errors = 0u32;
    let mut retry_at = 1000;
    loop {
        #[cfg(feature = "st-hal")]
        let now = clock.sample(hw::cycles());
        // TIM5 continues while the executor sleeps between polls.
        #[cfg(feature = "embassy")]
        let now = InstantMs(start.elapsed().as_millis());
        ui.tick(now).map_err(|_| 18u32)?;
        if touch.is_none() && now.0 >= retry_at {
            touch = touch::Touch::detect().ok();
            report(6, touch.as_ref().map_or(0, |t| t.product));
            retry_at = now.0 + 2000;
        }
        if let Some(t) = touch.as_mut() {
            let mut dispatch_ok = true;
            if !t.poll(|ev| {
                dispatch_ok &= ui.handle_input(ev, now).is_ok();
            }) {
                io_errors = io_errors.saturating_add(1);
                report(7, io_errors);
                touch = None;
                retry_at = now.0 + 1000;
            }
            if !dispatch_ok {
                return Err(19);
            }
        }
        // Optional SWD test input: the host publishes coordinates before the sequence.
        // ACK follows successful presentation. No interrupts or test firmware are required.
        let seq = read_report(10);
        let ack = read_report(13);
        if seq != ack && read_report(11) == 6 {
            report(14, screen.snapshot());
        } else if seq != ack {
            let packed = read_report(12);
            let point = Point::new((packed & 0xffff) as i32, (packed >> 16) as i32);
            let id = PointerId(15);
            let mut event_time = now;
            let ev = match read_report(11) {
                // A complete short tap is timed on the board. USB round trips must
                // never turn a regression tap into a long press.
                5 => {
                    ui.handle_input(RawInput::PointerDown { id, pos: point }, now)
                        .map_err(|_| 19u32)?;
                    present(&mut ui, &mut screen, band, &mut frames)?;
                    hw::delay(16_000);
                    event_time = InstantMs(now.0 + 16);
                    ui.tick(event_time).map_err(|_| 18u32)?;
                    RawInput::PointerUp { id, pos: point }
                }
                1 => RawInput::PointerDown { id, pos: point },
                2 => RawInput::PointerUp { id, pos: point },
                3 => RawInput::PointerMove { id, pos: point },
                4 => RawInput::PointerCancel { id },
                _ => return Err(21),
            };
            ui.handle_input(ev, event_time).map_err(|_| 19u32)?;
        }
        present(&mut ui, &mut screen, band, &mut frames)?;
        report(8, ui.model().page as u32);
        if seq != ack {
            report(13, seq);
        }
        #[cfg(feature = "st-hal")]
        {
            let after = clock.sample(hw::cycles());
            match ui.wakeup(after) {
                NextWakeup::Now => {}
                NextWakeup::At(t) => {
                    let ms = t.0.saturating_sub(after.0).min(4) as u32;
                    if ms > 0 {
                        hw::delay(ms * 1000);
                    }
                }
                NextWakeup::OnEvent => hw::delay(4000),
            }
        }
        #[cfg(feature = "embassy")]
        embassy_time::Timer::after_millis(4).await;
    }
}
#[cfg(feature = "st-hal")]
#[unsafe(no_mangle)]
extern "C" fn firmware_main() -> ! {
    for i in 0..16 {
        report(i, 0);
    }
    // In the polling backend the loop never suspends.
    let mut future = core::pin::pin!(run());
    let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
    let core::task::Poll::Ready(result) = core::future::Future::poll(future.as_mut(), &mut cx)
    else {
        fail(0xfd)
    };
    match result {
        Err(code) => fail(code),
        Ok(()) => fail(0xfe),
    }
}
