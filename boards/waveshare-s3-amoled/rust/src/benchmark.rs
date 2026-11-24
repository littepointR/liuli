use super::*;
use liuli_alarms::Msg;
unsafe extern "C" {
    fn board_begin_sample();
    fn board_sample(kind: u32, n: u32, elapsed: i64, pixels: u32);
}
// Replay identical inputs and panel transfers on both backends. The measurement
// starts before submitting the action and ends after the synchronous QSPI write.
pub(super) fn run(
    e: &mut Engine<'_, '_, BoardApp>,
    target: &mut Display<'_>,
    scratch: &mut [u16],
    glyphs: &mut impl FrameGlyphs,
) -> Result<(), ()> {
    let time = || InstantMs(unsafe { board_now_us() } as u64 / 1000);
    present(e, target, scratch, glyphs)?;
    e.try_message(Msg::Open(0), time(), false).map_err(|_| ())?;
    e.try_message(Msg::Edit, time(), false).map_err(|_| ())?;
    present(e, target, scratch, glyphs)?;
    for backend in 0..2 {
        if backend == 0 {
            e.set_sync_blend(liuli::render::raster::blend_solid);
        } else {
            e.set_sync_blend(|row, fg, a| unsafe {
                board_blend(row.as_mut_ptr(), row.len(), fg.into_storage(), a.into())
            });
        }
        for i in 0..32 {
            let start = unsafe {
                board_begin_sample();
                board_now_us()
            };
            e.try_message(Msg::Ask, time(), false).map_err(|_| ())?;
            present(e, target, scratch, glyphs)?;
            let elapsed = unsafe { board_now_us() } - start;
            unsafe {
                board_sample(backend, i, elapsed, e.last_metrics().pixels_sent);
                board_wait();
            }
            e.try_message(Msg::Back, time(), false).map_err(|_| ())?;
            present(e, target, scratch, glyphs)?;
        }
    }
    e.try_message(Msg::Back, time(), false).map_err(|_| ())?;
    e.try_message(Msg::Back, time(), false).map_err(|_| ())?;
    present(e, target, scratch, glyphs)?;
    for i in 0..32 {
        let id = PointerId(0);
        for (pos, down) in [(Point::new(100, 430), true), (Point::new(100, 78), false)] {
            let input = if down {
                RawInput::PointerDown { id, pos }
            } else {
                RawInput::PointerMove { id, pos }
            };
            let start = unsafe {
                board_begin_sample();
                board_now_us()
            };
            e.try_dispatch(input, time(), false).map_err(|_| ())?;
            present(e, target, scratch, glyphs)?;
            if !down {
                unsafe {
                    board_sample(2, i, board_now_us() - start, e.last_metrics().pixels_sent);
                    board_wait();
                }
            }
        }
        e.try_dispatch(
            RawInput::PointerUp {
                id,
                pos: Point::new(100, 78),
            },
            time(),
            false,
        )
        .map_err(|_| ())?;
    }
    complex_text(target)?;
    #[cfg(feature = "sleep-benchmark")]
    {
        if unsafe { board_arm_sleep() } != 0 {
            return Err(());
        }
        let start = unsafe {
            board_begin_sample();
            board_now_us()
        };
        if unsafe { board_sleep_cycle() } != 0 {
            return Err(());
        }
        e.invalidate_screen().map_err(|_| ())?;
        present(e, target, scratch, glyphs)?;
        let pixels = e.last_metrics().pixels_sent;
        if pixels != WIDTH * HEIGHT {
            return Err(());
        }
        unsafe {
            board_sample(4, 0, board_now_us() - start, pixels);
        }
    }
    e.invalidate_screen().map_err(|_| ())?;
    Ok(())
}

unsafe extern "C" {
    #[cfg(feature = "sleep-benchmark")]
    fn board_arm_sleep() -> i32;
    #[cfg(feature = "sleep-benchmark")]
    fn board_sleep_cycle() -> i32;
    fn board_font_frame(requests: u32, repeated: u32, reads: u32, bytes: u32);
}
fn complex_text(target: &mut Display<'_>) -> Result<(), ()> {
    extern crate alloc;
    use liuli::core::geometry::{rgb565_to_u16, u16_to_rgb565};
    use liuli::enhanced::{Editor, ShapingFont, glyphs::raster_glyph, shape_paragraph};
    let fonts = [
        ShapingFont::from_bytes(include_bytes!(env!("LIULI_ARABIC")), 0).ok_or(())?,
        ShapingFont::from_bytes(include_bytes!(env!("LIULI_INDIC")), 0).ok_or(())?,
    ];
    let mut edit = Editor::new("سلام ".into());
    let mut pixels = alloc::vec![0u16;240*48];
    for i in 0..32 {
        let start = unsafe {
            board_begin_sample();
            board_now_us()
        };
        unsafe {
            board_font_frame(0, 0, 0, 0);
        }
        edit.compose("क");
        edit.commit("क्षि");
        pixels.fill(0);
        let mut pen = 4i32;
        let mut failed = false;
        shape_paragraph(edit.text(), &fonts, 24, |idx, g| {
            if raster_glyph(
                &fonts[idx],
                g.id,
                24,
                (pen + g.x_offset, 32 - g.y_offset),
                4096,
                |x, y, a| {
                    if (0..240).contains(&x) && (0..48).contains(&y) {
                        let dst = &mut pixels[(y * 240 + x) as usize];
                        *dst = rgb565_to_u16(liuli::text::blend_rgb565_a4(
                            u16_to_rgb565(65535),
                            u16_to_rgb565(*dst),
                            a,
                        ));
                    }
                },
            )
            .is_err()
            {
                failed = true;
            }
            pen += g.advance;
        });
        if failed {
            return Err(());
        }
        for (i, band) in pixels.chunks_exact(240 * 8).enumerate() {
            target.fill_contiguous(
                &Rectangle::new(Point::new(0, (i * 8) as i32), Size::new(240, 8)),
                band.iter().copied().map(u16_to_rgb565),
            )?;
        }
        unsafe {
            board_sample(3, i, board_now_us() - start, 240 * 48);
            board_wait();
        }
        edit.backspace();
    }
    Ok(())
}
