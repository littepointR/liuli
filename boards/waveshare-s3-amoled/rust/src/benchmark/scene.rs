use super::*;
extern crate alloc;

// Fullscreen invalidation emits horizontal bands in display order. Checking the coordinate
// as well as the color catches missing, repeated and misplaced transfers.
struct Scene<'a> {
    frame: &'a mut [u16],
    next: usize,
    compare: bool,
}

impl DisplaySink for Scene<'_> {
    type Error = ();

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), ()> {
        let width = WIDTH as usize;
        let stride = rect.w as usize;
        if pixels.len() < stride.saturating_mul(rect.h as usize) {
            return Err(());
        }
        for ry in 0..rect.h as usize {
            let y = rect.y as usize + ry;
            for rx in 0..stride {
                let x = rect.x as usize + rx;
                if y * width + x != self.next {
                    return Err(());
                }
                let word = self.frame.get_mut(self.next).ok_or(())?;
                let px = pixels[ry * stride + rx];
                if self.compare {
                    if *word != px {
                        return Err(());
                    }
                } else {
                    *word = px;
                }
                self.next += 1;
            }
        }
        Ok(())
    }
}

pub(super) fn check(
    engine: &mut Engine<'_, '_, BoardApp>,
    scratch: &mut [u16],
    glyphs: &mut impl FrameGlyphs,
) -> Result<(), ()> {
    let mut frame = alloc::vec![0u16; (WIDTH * HEIGHT) as usize];
    for compare in [false, true] {
        if compare {
            engine.set_sync_blend(|row, fg, a| unsafe {
                board_blend(row.as_mut_ptr(), row.len(), rgb565_to_u16(fg), a.into())
            });
        } else {
            engine.set_sync_blend(liuli::render::raster::blend_solid);
        }
        let mut target = Scene {
            frame: &mut frame,
            next: 0,
            compare,
        };
        engine.invalidate_screen().map_err(|_| ())?;
        glyphs.start();
        engine
            .present_with_pixels(&mut target, scratch, glyphs)
            .map_err(|_| ())?;
        glyphs.finish();
        if target.next != (WIDTH * HEIGHT) as usize {
            return Err(());
        }
    }
    unsafe { board_scene_verified(WIDTH * HEIGHT) };
    Ok(())
}

unsafe extern "C" {
    fn board_scene_verified(pixels: u32);
}
