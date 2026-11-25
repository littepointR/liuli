#![no_std]
#![no_main]

mod audit_sink;

use core::arch::global_asm;
use core::hint::black_box;
use core::panic::PanicInfo;

use audit_sink::{AuditByteBus, AuditSink};
use liuli::prelude::*;
use liuli_settings::{PageId, TerminalApp, TerminalModel, TerminalMsg, app_fonts};

#[unsafe(no_mangle)]
static mut LIULI_STORAGE: Storage = Storage::new();
#[unsafe(no_mangle)]
static mut LIULI_DMA_BAND_A: AlignedBand<DEFAULT_BAND_PIXELS> = AlignedBand::zeroed();
#[unsafe(no_mangle)]
static mut LIULI_DMA_BAND_B: AlignedBand<DEFAULT_BAND_PIXELS> = AlignedBand::zeroed();

#[cfg(feature = "startup-probe")]
#[unsafe(no_mangle)]
static mut LIULI_PROBE_DATA: u32 = 0xA55A_5AA5;
#[cfg(feature = "startup-probe")]
#[unsafe(no_mangle)]
static mut LIULI_PROBE_BSS: u32 = 0;

global_asm!(
    r#"
    .syntax unified
    .thumb
    .section .text.Reset,"ax",%progbits
    .global Reset
    .type Reset,%function
    .thumb_func
Reset:
    ldr r0, =_sdata
    ldr r1, =_edata
    ldr r2, =_sidata
1:
    cmp r0, r1
    bcs 2f
    ldr r3, [r2], #4
    str r3, [r0], #4
    b 1b
2:
    ldr r0, =_sbss
    ldr r1, =_ebss
    movs r2, #0
3:
    cmp r0, r1
    bcs 4f
    str r2, [r0], #4
    b 3b
4:
    bl firmware_main
5:
    b 5b
    .size Reset, . - Reset
"#
);

unsafe extern "C" {
    fn Reset() -> !;
}

#[unsafe(link_section = ".vector_table.reset_vector")]
#[unsafe(no_mangle)]
pub static RESET_VECTOR: unsafe extern "C" fn() -> ! = Reset;

#[unsafe(no_mangle)]
pub extern "C" fn firmware_main() -> ! {
    #[cfg(feature = "startup-probe")]
    {
        // SAFETY: `.data` and `.bss` are initialized by `Reset` before entering `firmware_main`.
        let (probe_data, probe_bss) = unsafe {
            let p_d = &raw mut LIULI_PROBE_DATA;
            let p_b = &raw mut LIULI_PROBE_BSS;
            (p_d.read_volatile(), p_b.read_volatile())
        };
        if probe_data != 0xA55A_5AA5 || probe_bss != 0 {
            panic!("startup initialization failed");
        }
        black_box((probe_data, probe_bss));
    }

    // SAFETY: `Reset` initialized RAM and `firmware_main` never returns or re-enters,
    // yielding unique references to these static buffers once.
    let storage_ptr = &raw mut LIULI_STORAGE;
    let band_a_ptr = &raw mut LIULI_DMA_BAND_A;
    let band_b_ptr = &raw mut LIULI_DMA_BAND_B;
    let (storage, band_a, band_b) =
        unsafe { (&mut *storage_ptr, &mut *band_a_ptr, &mut *band_b_ptr) };

    let screen = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut engine = Engine::<TerminalApp>::new_in_place_with_fonts(
        storage,
        TerminalModel::new(InstantMs(0)),
        app_fonts(),
        screen,
        InstantMs(0),
    )
    .expect("init");
    engine.set_focus_ring_visible(true);

    // 1. Synchronous byte-window transport path (full initial Dashboard frame).
    {
        let mut wire_chunk = [0u8; RECOMMENDED_WIRE_CHUNK_BYTES];
        let mut wire_sink = ByteWindowSink::new(
            AuditByteBus::new(),
            screen,
            ByteOrder::BigEndian,
            &mut wire_chunk,
        )
        .expect("wire chunk");
        let sync_status = engine.present(&mut wire_sink, band_a.as_mut_slice());
        let _ = black_box((sync_status, wire_sink.bus().summary()));
        let m = engine.last_metrics();
        let _ = engine.try_with_model(|model| model.record_completed_frame(m, InstantMs(0)));
    }

    // 2. Page switch + non-blocking double-buffered DMA pipeline path across both A & B slots.
    let _ = black_box(engine.send(TerminalMsg::SelectPage(PageId::Settings), InstantMs(16)));
    let _ = black_box(engine.handle_input(RawInput::Nav(NavDir::Next), InstantMs(16)));
    {
        let mut dma = DmaCoordinator::new(band_a.as_mut_slice(), band_b.as_mut_slice());
        let mut sink = AuditSink::new();
        let mut polls = 0u32;
        while polls < 256 {
            polls += 1;
            match engine.poll_dma(&mut dma, &mut sink) {
                Ok(PresentStatus::Presented | PresentStatus::Idle) => break,
                Ok(PresentStatus::Pending) => {}
                Err(err) => {
                    let _ = black_box(err);
                    break;
                }
            }
        }
        let _ = black_box((
            polls,
            sink.slot_mask() == 0b11 && sink.completed_count() >= 2,
            sink.summary(),
            engine.last_metrics(),
        ));
    }

    loop {
        black_box(engine.wakeup(InstantMs(16)));
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        black_box(());
    }
}
