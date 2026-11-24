//! Embassy owns RCC, TIM5 and the thread-mode executor. The panel is shared with the ST BSP.
use embassy_stm32::{rcc::*, time::Hertz};

fn clocks() -> embassy_stm32::Config {
    let mut cfg = embassy_stm32::Config::default();
    let rcc = &mut cfg.rcc;
    rcc.hse = Some(Hse {
        freq: Hertz(25_000_000),
        mode: HseMode::Oscillator,
    });
    rcc.pll1 = Some(Pll {
        source: PllSource::HSE,
        prediv: PllPreDiv::DIV5,
        mul: PllMul::MUL160,
        divp: Some(PllDiv::DIV2),
        divq: Some(PllDiv::DIV5),
        divr: None,
        fracn: None,
    });
    rcc.pll3 = Some(Pll {
        source: PllSource::HSE,
        prediv: PllPreDiv::DIV25,
        mul: PllMul::MUL330,
        divp: None,
        divq: None,
        divr: Some(PllDiv::DIV10),
        fracn: None,
    });
    rcc.sys = Sysclk::PLL1_P;
    rcc.ahb_pre = AHBPrescaler::DIV2;
    rcc.apb1_pre = APBPrescaler::DIV2;
    rcc.apb2_pre = APBPrescaler::DIV2;
    rcc.apb3_pre = APBPrescaler::DIV2;
    rcc.apb4_pre = APBPrescaler::DIV2;
    rcc.voltage_scale = VoltageScale::Scale1;
    rcc.mux.fmcsel = mux::Fmcsel::PLL1_Q;
    cfg
}

// HAL timeout loops share the Embassy monotonic clock, including during SDRAM startup.
#[unsafe(no_mangle)]
extern "C" fn HAL_GetTick() -> u32 {
    embassy_time::Instant::now().as_millis() as u32
}

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    for i in 0..16 {
        super::report(i, 0);
    }
    let _peripherals = embassy_stm32::init(clocks());
    super::fail(super::run().await.err().unwrap_or(0xfe));
}

#[cortex_m_rt::exception]
unsafe fn HardFault(_frame: &cortex_m_rt::ExceptionFrame) -> ! {
    super::report(15, unsafe { (*cortex_m::peripheral::SCB::PTR).cfsr.read() });
    super::fail(1)
}
