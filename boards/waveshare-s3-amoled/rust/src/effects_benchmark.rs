//! Unattended physical LCD transfers using the same widget demo and input as the desktop.
use super::*;
unsafe extern "C" {
    fn board_begin_sample();
    fn board_sample(kind: u32, index: u32, elapsed: i64, pixels: u32);
}
fn tap(e: &mut Engine<'_, '_, BoardApp>, key: Key, now: InstantMs) -> Result<(), ()> {
    let rect = e.active_tree().node_by_key(key).ok_or(())?.layout_rect;
    let pos = Point::new(rect.x + (rect.w / 2) as i32, rect.y + (rect.h / 2) as i32);
    let id = PointerId(0);
    for input in [
        RawInput::PointerDown { id, pos },
        RawInput::PointerUp { id, pos },
    ] {
        e.handle_input(input, now).map_err(|_| ())?;
    }
    Ok(())
}
pub(super) fn run(
    e: &mut Engine<'_, '_, BoardApp>,
    display: &mut Display<'_>,
    scratch: &mut [u16],
    glyphs: &mut impl FrameGlyphs,
    start: InstantMs,
) -> Result<(), ()> {
    let mut time = start;
    let page = liuli_widget_demo::PAGES
        .iter()
        .position(|p| p.0 == liuli_widget_demo::Page::Depth)
        .ok_or(())?;
    tap(e, Key(100 + page as u32), time)?;
    present(e, display, scratch, glyphs)?;
    // Sample the same 0..180 degree turn, with and without backdrop filtering.
    for kind in [5, 6] {
        if kind == 6 {
            tap(e, Key(307), time)?;
            time = time.saturating_add(640);
            e.tick(time).map_err(|_| ())?;
            tap(e, Key(309), time)?;
            present(e, display, scratch, glyphs)?;
        }
        tap(e, Key(307), time)?;
        for i in 0..16 {
            time = time.saturating_add(40);
            let start = unsafe {
                board_begin_sample();
                board_now_us()
            };
            e.tick(time).map_err(|_| ())?;
            if present(e, display, scratch, glyphs)? != PresentStatus::Presented {
                return Err(());
            }
            unsafe {
                board_sample(
                    kind,
                    i,
                    board_now_us() - start,
                    e.last_metrics().pixels_sent,
                );
                board_wait();
            }
        }
    }
    tap(e, Key(3), time)?;
    present(e, display, scratch, glyphs)?;
    // The UI returns to wall time for normal input after the virtual sequence.
    while (unsafe { board_now_us() } as u64 / 1000) < time.0 {
        unsafe { board_wait() };
    }
    Ok(())
}
