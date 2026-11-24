use crate::hardware as hw;
use embedded_graphics_core::{
    Pixel, draw_target::DrawTarget, geometry::OriginDimensions, pixelcolor::IntoStorage,
};
use liuli::prelude::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 480;
const WORDS: usize = (WIDTH * HEIGHT) as usize;
pub const FIRST: usize = 0xc000_0000;
pub const SECOND: usize = 0xc00c_0000;
pub struct Display {
    front: usize,
    back: usize,
    dirty: Rect,
    repair_dirty_off: usize,
    repair_dirty_len: usize,
    repair_active: bool,
    lit: bool,
}
impl Display {
    pub fn new() -> Result<Self, u32> {
        // Both disjoint buffers have the same contents before the first incremental frame.
        for base in [FIRST, SECOND] {
            hw::clean_invalidate_cache(base, WORDS * 2);
            hw::dma2d_fill(base, WIDTH, HEIGHT, 0, 0);
            hw::clean_invalidate_cache(base, WORDS * 2);
        }
        hw::panel(FIRST)?;
        Ok(Self {
            front: FIRST,
            back: SECOND,
            dirty: Rect::ZERO,
            repair_dirty_off: 0,
            repair_dirty_len: 0,
            repair_active: false,
            lit: false,
        })
    }
    fn wait_repair(&mut self) {
        if self.repair_active {
            hw::dma2d_wait();
            hw::clean_invalidate_cache(self.back + self.repair_dirty_off, self.repair_dirty_len);
            self.repair_active = false;
        }
    }
    pub fn front(&self) -> usize {
        self.front
    }
    /// Lossless SWD snapshot in the unused SDRAM gap before the UI heap.
    /// Each word holds a run length in its upper half and an RGB565 pixel below.
    pub fn snapshot(&self) -> u32 {
        hw::dma2d_wait();
        const CAPTURE: usize = 0xc018_0000;
        const LIMIT: usize = (0xc020_0000 - CAPTURE) / 4;
        let pixels = unsafe { core::slice::from_raw_parts(self.front as *const u16, WORDS) };
        let mut start = 0;
        let mut count = 0;
        while start < pixels.len() {
            let color = pixels[start];
            let mut end = start + 1;
            while end < pixels.len() && end - start < u16::MAX as usize && pixels[end] == color {
                end += 1;
            }
            if count == LIMIT {
                return u32::MAX;
            }
            let run = ((end - start) as u32) << 16 | u32::from(color);
            unsafe { (CAPTURE as *mut u32).add(count).write(run) };
            count += 1;
            start = end;
        }
        hw::clean_cache(CAPTURE, 4 * count);
        (4 * count) as u32
    }
    pub fn submit(&mut self) -> Result<(), u32> {
        self.wait_repair();
        let dirty = self.dirty;
        if dirty.is_empty() {
            return Ok(());
        }
        let dirty_off = dirty.y as usize * WIDTH as usize * 2;
        let dirty_len = dirty.h as usize * WIDTH as usize * 2;
        hw::clean_cache(self.back + dirty_off, dirty_len);
        hw::swap(self.back)?;
        core::mem::swap(&mut self.front, &mut self.back);
        // Liuli damages are relative to the last committed frame. Clean/invalidate the spare
        // rows and launch Chrom-ART 2D DMA2D asynchronously while the CPU starts the next frame.
        hw::clean_invalidate_cache(self.back + dirty_off, dirty_len);
        let rect_off = (dirty.y as usize * WIDTH as usize + dirty.x as usize) * 2;
        hw::dma2d_copy_async(
            self.front + rect_off,
            self.back + rect_off,
            dirty.w,
            dirty.h,
            WIDTH - dirty.w,
        );
        self.repair_dirty_off = dirty_off;
        self.repair_dirty_len = dirty_len;
        self.repair_active = true;
        self.dirty = Rect::ZERO;
        if !self.lit {
            hw::backlight();
            self.lit = true;
        }
        Ok(())
    }
}
impl OriginDimensions for Display {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
impl DrawTarget for Display {
    type Color = Rgb565;
    type Error = u32;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(&mut self, pixels: I) -> Result<(), u32> {
        for Pixel(p, color) in pixels {
            if Rect::new(0, 0, WIDTH, HEIGHT).contains_point(p) {
                self.fill_contiguous(&Rectangle::new(p, Size::new(1, 1)), [color])?;
            }
        }
        Ok(())
    }
    fn fill_contiguous<I: IntoIterator<Item = Rgb565>>(
        &mut self,
        rect: &Rectangle,
        colors: I,
    ) -> Result<(), u32> {
        let r = Rect::new(
            rect.top_left.x,
            rect.top_left.y,
            rect.size.width,
            rect.size.height,
        );
        if r.is_empty() {
            return Ok(());
        }
        if r.intersect(Rect::new(0, 0, WIDTH, HEIGHT)) != r {
            return Err(9);
        }
        self.wait_repair();
        let mut colors = colors.into_iter();
        for y in r.y as usize..r.bottom64() as usize {
            let start = (y * WIDTH as usize + r.x as usize) * 2 + self.back;
            let row = unsafe { core::slice::from_raw_parts_mut(start as *mut u16, r.w as usize) };
            let mut i = 0;
            let len = row.len();
            if (start & 2) != 0 && i < len {
                row[i] = colors.next().ok_or(9u32)?.into_storage();
                i += 1;
            }
            while i + 1 < len {
                let lo = u32::from(colors.next().ok_or(9u32)?.into_storage());
                let hi = u32::from(colors.next().ok_or(9u32)?.into_storage());
                unsafe {
                    (row.as_mut_ptr().add(i) as *mut u32).write(lo | (hi << 16));
                }
                i += 2;
            }
            if i < len {
                row[i] = colors.next().ok_or(9u32)?.into_storage();
            }
        }
        self.dirty = if self.dirty.is_empty() {
            r
        } else {
            self.dirty.union(r)
        };
        Ok(())
    }
}
