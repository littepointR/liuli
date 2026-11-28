use crate::hardware as hw;
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
    #[cfg(feature = "diagnostics")]
    pub fn front(&self) -> usize {
        self.front
    }
    /// Lossless SWD snapshot in the unused SDRAM gap before the UI heap.
    /// Each word holds a run length in its upper half and an RGB565 pixel below.
    #[cfg(feature = "diagnostics")]
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
impl DisplaySink for Display {
    type Error = u32;

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
        if rect.is_empty() {
            return Ok(());
        }
        if rect.intersect(Rect::new(0, 0, WIDTH, HEIGHT)) != rect {
            return Err(9);
        }
        let stride = rect.w as usize;
        let needed = stride.saturating_mul(rect.h as usize);
        if pixels.len() < needed {
            return Err(9);
        }
        self.wait_repair();
        for ry in 0..rect.h as usize {
            let y = rect.y as usize + ry;
            let start = (y * WIDTH as usize + rect.x as usize) * 2 + self.back;
            let row = unsafe { core::slice::from_raw_parts_mut(start as *mut u16, stride) };
            row.copy_from_slice(&pixels[ry * stride..(ry + 1) * stride]);
        }
        self.dirty = if self.dirty.is_empty() {
            rect
        } else {
            self.dirty.union(rect)
        };
        Ok(())
    }

    fn finish_frame(&mut self) -> Result<(), Self::Error> {
        self.submit()
    }
}
