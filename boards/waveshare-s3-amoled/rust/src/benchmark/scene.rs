use super::*;
extern crate alloc;

// Fullscreen invalidation emits horizontal bands in display order. Checking the coordinate
// as well as the color catches missing, repeated and misplaced transfers.
struct Scene<'a> {
    frame: &'a mut [u16],
    next: usize,
    compare: bool,
}

impl OriginDimensions for Scene<'_> {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Scene<'_> {
    type Color = Rgb565;
    type Error = ();

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, pixels: I) -> Result<(), ()> {
        for Pixel(pos, color) in pixels {
            let width = WIDTH as usize;
            if pos != Point::new((self.next % width) as i32, (self.next / width) as i32) {
                return Err(());
            }
            let word = self.frame.get_mut(self.next).ok_or(())?;
            if self.compare {
                if *word != color.into_storage() {
                    return Err(());
                }
            } else {
                *word = color.into_storage();
            }
            self.next += 1;
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
                board_blend(row.as_mut_ptr(), row.len(), fg.into_storage(), a.into())
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
            .present_sync_with_pixels(&mut target, scratch, |_| Ok(()), glyphs)
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
