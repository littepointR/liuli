#![no_std]
#![no_main]

mod display;
mod hardware;
#[cfg(feature = "enhanced")]
mod heap;
mod touch;
#[cfg(feature = "touch-irq")]
mod touch_irq;
const TOUCH_POLL_MS: u64 = include!(concat!(env!("OUT_DIR"), "/poll.rs"));
#[cfg(feature = "st-hal")]
unsafe extern "C" {
    fn HAL_GetTick() -> u32;
}

use display::{Display, HEIGHT, WIDTH};
use hardware as hw;
use liuli::prelude::*;
#[cfg(feature = "showcase")]
use liuli_widget_demo::showcase::{ShowcaseApp, ShowcaseModel, showcase_fonts};
#[cfg(not(feature = "showcase"))]
use liuli_widget_demo::{Catalog, FONT, Model};

#[cfg(feature = "embassy")]
mod embassy;
#[cfg(feature = "st-hal")]
core::arch::global_asm!(include_str!("boot.s"), diagnostics = const cfg!(feature="diagnostics") as u8, touch_irq = const cfg!(feature="touch-irq") as u8);
static mut STORAGE: Storage = Storage::ZERO;
static mut STRIPE: [u16; 800 * 32] = [0; 800 * 32];
#[cfg(feature = "diagnostics")]
#[used]
#[unsafe(link_section = ".report")]
static mut REPORT: [u32; 16] = [0; 16];

#[cfg(feature = "diagnostics")]
fn report(i: usize, v: u32) {
    assert!(i < 16);
    unsafe {
        (&raw mut REPORT).cast::<u32>().add(i).write_volatile(v);
    }
}
#[cfg(feature = "diagnostics")]
fn read_report(i: usize) -> u32 {
    assert!(i < 16);
    unsafe { (&raw const REPORT).cast::<u32>().add(i).read_volatile() }
}
macro_rules! record {
    ($i:expr, $value:expr) => {
        #[cfg(feature = "diagnostics")]
        {
            report($i, $value);
        }
    };
}
fn fail(_code: u32) -> ! {
    record!(1, 0xbad00000 | _code);
    loop {
        core::hint::spin_loop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail(0xff)
}
#[cfg(not(feature = "showcase"))]
type Ui<'s> = Engine<'s, 'static, Catalog>;
#[cfg(feature = "showcase")]
type Ui<'s> = Engine<'s, 'static, ShowcaseApp>;

fn present(
    ui: &mut Ui<'_>,
    screen: &mut Display,
    band: &mut [u16],
    _frames: &mut u32,
) -> Result<(), u32> {
    #[cfg(feature = "diagnostics")]
    let start = hw::cycles();
    let _status = ui.present(screen, band).map_err(|_| 20u32)?;
    #[cfg(feature = "diagnostics")]
    if _status == PresentStatus::Presented {
        *_frames = _frames.wrapping_add(1);
        record!(2, *_frames);
        record!(3, screen.front() as u32);
        record!(4, ui.last_metrics().pixels_sent);
        record!(5, hw::cycles().wrapping_sub(start) / hw::CPU_MHZ);
        record!(9, hw::status());
    }
    Ok(())
}

async fn run() -> Result<(), u32> {
    record!(0, 0x4c554c49);
    record!(1, 1);
    hw::clocks()?;
    record!(1, 2);
    hw::sdram()?;
    record!(1, 3);
    #[cfg(feature = "enhanced")]
    unsafe {
        heap::init();
    }
    let mut screen = Display::new()?;
    let mut touch = touch::Touch::detect().ok();
    record!(6, touch.as_ref().map_or(0, |t| t.product));
    // Reset is the only caller; interrupts never access UI state.
    let storage_ptr = &raw mut STORAGE;
    let band_ptr = &raw mut STRIPE;
    let storage = unsafe { &mut *storage_ptr };
    let band = unsafe { &mut *band_ptr };
    #[cfg(not(feature = "showcase"))]
    let mut ui = {
        let font = Lf4Font::from_bytes(FONT).map_err(|_| 14u32)?;
        Engine::<Catalog>::new_in_place_with_fonts(
            storage,
            Model::for_size(Size::new(WIDTH, HEIGHT)),
            liuli_widget_demo::studio_fonts(font).map_err(|_| 14u32)?,
            Rect::new(0, 0, WIDTH, HEIGHT),
            InstantMs(0),
        )
        .map_err(|_| 15u32)?
    };
    #[cfg(feature = "showcase")]
    let mut ui = Engine::<ShowcaseApp>::new_in_place_with_fonts(
        storage,
        ShowcaseModel::new(),
        showcase_fonts().map_err(|_| 14u32)?,
        Rect::new(0, 0, WIDTH, HEIGHT),
        InstantMs(0),
    )
    .map_err(|_| 15u32)?;
    #[cfg(feature = "enhanced")]
    ui.set_layer_budget(1024 * 1024).map_err(|_| 16u32)?;
    ui.set_band_height(32);
    let mut frames = 0;
    present(&mut ui, &mut screen, band, &mut frames)?;
    record!(1, 4);
    #[cfg(feature = "st-hal")]
    let mut clock = {
        let mut c = MonotonicClock32::new(1);
        c.sample(unsafe { HAL_GetTick() });
        c
    };
    #[cfg(feature = "embassy")]
    let start = embassy_time::Instant::now();
    let mut next_poll = 0u64;
    #[cfg(feature = "diagnostics")]
    let mut io_errors = 0u32;
    let mut retry_at = 1000;
    loop {
        #[cfg(feature = "st-hal")]
        let now = clock.sample(unsafe { HAL_GetTick() });
        // TIM5 continues while the executor sleeps between polls.
        #[cfg(feature = "embassy")]
        let now = InstantMs(start.elapsed().as_millis());
        if ui.wakeup(now) == NextWakeup::Now {
            ui.tick(now).map_err(|_| 18u32)?;
        }
        if touch.is_none() && now.0 >= retry_at {
            touch = touch::Touch::detect().ok();
            next_poll = now.0;
            record!(6, touch.as_ref().map_or(0, |t| t.product));
            retry_at = now.0 + 2000;
        }
        #[cfg(feature = "touch-irq")]
        let touch_event = touch_irq::take();
        #[cfg(not(feature = "touch-irq"))]
        let touch_event = false;
        if let Some(t) = touch.as_mut()
            && (now.0 >= next_poll || touch_event)
        {
            let mut dispatch_ok = true;
            if !t.poll(|ev| {
                dispatch_ok &= ui.handle_input(ev, now).is_ok();
            }) {
                #[cfg(feature = "diagnostics")]
                {
                    io_errors = io_errors.saturating_add(1);
                    record!(7, io_errors);
                }
                touch = None;
                retry_at = now.0 + 1000;
            } else {
                next_poll = if cfg!(feature = "touch-irq") && !t.active() {
                    u64::MAX
                } else {
                    now.0 + TOUCH_POLL_MS
                };
            }
            if !dispatch_ok {
                return Err(19);
            }
        }
        #[cfg(feature = "diagnostics")]
        let (seq, ack) = {
            // Optional SWD test input: the host publishes coordinates before the sequence.
            // ACK follows successful presentation. No interrupts or test firmware are required.
            let seq = read_report(10);
            let ack = read_report(13);
            if seq != ack && read_report(11) == 6 {
                record!(14, screen.snapshot());
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
                    7 => RawInput::Nav(NavDir::Next),
                    8 => RawInput::Nav(NavDir::Prev),
                    _ => return Err(21),
                };
                ui.handle_input(ev, event_time).map_err(|_| 19u32)?;
            }
            (seq, ack)
        };
        present(&mut ui, &mut screen, band, &mut frames)?;
        #[cfg(not(feature = "showcase"))]
        record!(8, ui.model().page as u32);
        #[cfg(feature = "showcase")]
        record!(8, u32::from(ui.model().page_index()));
        #[cfg(feature = "diagnostics")]
        if seq != ack {
            record!(13, seq);
        }
        #[cfg(feature = "st-hal")]
        let after = clock.sample(unsafe { HAL_GetTick() });
        #[cfg(feature = "embassy")]
        let after = InstantMs(start.elapsed().as_millis());
        let touch_deadline = if touch.is_some() { next_poll } else { retry_at };
        let mut wait_ms = touch_deadline
            .saturating_sub(after.0)
            .min(u64::from(i32::MAX as u32));
        if cfg!(feature = "diagnostics") {
            wait_ms = wait_ms.min(4);
        }
        match ui.wakeup(after) {
            NextWakeup::Now => wait_ms = 0,
            NextWakeup::At(t) => wait_ms = wait_ms.min(t.0.saturating_sub(after.0)),
            NextWakeup::OnEvent => {}
        }
        #[cfg(feature = "st-hal")]
        {
            let start = unsafe { HAL_GetTick() };
            while unsafe { HAL_GetTick() }.wrapping_sub(start) < wait_ms as u32 {
                #[cfg(feature = "touch-irq")]
                if touch_irq::pending() {
                    break;
                }
                cortex_m::asm::wfi();
            }
        }
        #[cfg(all(feature = "embassy", feature = "touch-irq"))]
        if wait_ms != 0 {
            let _ = embassy_time::with_timeout(
                embassy_time::Duration::from_millis(wait_ms),
                touch_irq::wait(),
            )
            .await;
        }
        #[cfg(all(feature = "embassy", not(feature = "touch-irq")))]
        if wait_ms != 0 {
            embassy_time::Timer::after_millis(wait_ms).await;
        }
    }
}
#[cfg(feature = "st-hal")]
#[unsafe(no_mangle)]
extern "C" fn firmware_main() -> ! {
    #[cfg(feature = "diagnostics")]
    for i in 0..16 {
        record!(i, 0);
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
