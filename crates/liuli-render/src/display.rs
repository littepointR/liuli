use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Dimensions;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::Rectangle;
use liuli_core::geometry::{Rect, rgb565_to_u16, u16_to_rgb565};

use crate::wire::{BufferTooSmall, ByteOrder, encode_rgb565};

/// Recommended byte staging chunk size for [`ByteWindowSink`].
pub const RECOMMENDED_WIRE_CHUNK_BYTES: usize = 64;
const MAX_STAGE_WORDS: usize = 32;

/// Bus-agnostic synchronous display output sink receiving contiguous RGB565 band rectangles.
///
/// Suits SPI, QSPI, 8080/6800 parallel buses, SDRAM/LTDC framebuffers, or virtual test targets
/// without going through per-pixel `embedded-graphics` iterators.
pub trait DisplaySink {
    type Error;

    /// Writes a contiguous row-major RGB565 band (`pixels.len() >= rect.w * rect.h`) at `rect`.
    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error>;

    /// Called once after all dirty bands of a non-idle frame have been flushed.
    fn finish_frame(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<S: DisplaySink + ?Sized> DisplaySink for &mut S {
    type Error = S::Error;

    #[inline]
    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
        (**self).flush_rect(rect, pixels)
    }

    #[inline]
    fn finish_frame(&mut self) -> Result<(), Self::Error> {
        (**self).finish_frame()
    }
}

/// Bus-agnostic 8-bit window-addressed display bus (SPI, QSPI, 8-bit 8080/6800).
pub trait ByteWindowBus {
    type Error;

    fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error>;
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Adapts a [`ByteWindowBus`] into both [`DisplaySink`] (zero-copy band slices) and `DrawTarget`.
pub struct ByteWindowSink<'a, B> {
    bus: B,
    bounds: Rect,
    order: ByteOrder,
    chunk: &'a mut [u8],
}

impl<'a, B> ByteWindowSink<'a, B> {
    pub fn new(
        bus: B,
        bounds: Rect,
        order: ByteOrder,
        chunk: &'a mut [u8],
    ) -> Result<Self, BufferTooSmall> {
        if chunk.len() < 2 {
            return Err(BufferTooSmall);
        }
        Ok(Self {
            bus,
            bounds,
            order,
            chunk,
        })
    }

    #[inline]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    #[inline]
    pub const fn order(&self) -> ByteOrder {
        self.order
    }

    #[inline]
    pub fn bus(&self) -> &B {
        &self.bus
    }

    #[inline]
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    #[inline]
    pub fn into_bus(self) -> B {
        self.bus
    }

    #[inline]
    fn stage_words(&self) -> usize {
        (self.chunk.len() / 2).clamp(1, MAX_STAGE_WORDS)
    }

    fn write_word_slice(&mut self, words: &[u16]) -> Result<(), B::Error>
    where
        B: ByteWindowBus,
    {
        if let Ok(nbytes) = encode_rgb565(words, self.chunk, self.order) {
            self.bus.write_bytes(&self.chunk[..nbytes])?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), B::Error>
    where
        B: ByteWindowBus,
    {
        self.bus.flush()
    }
}

impl<B: ByteWindowBus> DisplaySink for ByteWindowSink<'_, B> {
    type Error = B::Error;

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
        let vis = rect.intersect(self.bounds);
        if vis.is_empty() {
            return Ok(());
        }
        self.bus.set_window(vis)?;
        let max_words = (self.chunk.len() / 2).max(1);
        if vis == rect {
            let total = (vis.w as usize).saturating_mul(vis.h as usize);
            let src = &pixels[..total.min(pixels.len())];
            for slice in src.chunks(max_words) {
                self.write_word_slice(slice)?;
            }
            return Ok(());
        }

        let skip_x = (vis.x - rect.x) as usize;
        let skip_y = (vis.y - rect.y) as usize;
        let stride = rect.w as usize;
        let vis_w = vis.w as usize;
        let vis_h = vis.h as usize;

        for ry in 0..vis_h {
            let row_start = (skip_y + ry).saturating_mul(stride).saturating_add(skip_x);
            if row_start >= pixels.len() {
                break;
            }
            let available = (pixels.len() - row_start).min(vis_w);
            let row = &pixels[row_start..row_start + available];
            for slice in row.chunks(max_words) {
                self.write_word_slice(slice)?;
            }
            if available < vis_w {
                break;
            }
        }
        Ok(())
    }

    fn finish_frame(&mut self) -> Result<(), Self::Error> {
        self.bus.flush()
    }
}

impl<B> Dimensions for ByteWindowSink<'_, B> {
    fn bounding_box(&self) -> Rectangle {
        self.bounds.to_eg()
    }
}

impl<B: ByteWindowBus> DrawTarget for ByteWindowSink<'_, B> {
    type Color = Rgb565;
    type Error = B::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(pt, color) in pixels {
            if !self.bounds.contains_point(pt) {
                continue;
            }
            self.bus.set_window(Rect::new(pt.x, pt.y, 1, 1))?;
            let w = [rgb565_to_u16(color)];
            self.write_word_slice(&w)?;
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let area_r = Rect::from_eg(*area);
        let vis = area_r.intersect(self.bounds);
        if vis.is_empty() {
            return Ok(());
        }

        self.bus.set_window(vis)?;

        let src_w = area_r.w as usize;
        let skip_x = (vis.x - area_r.x) as usize;
        let skip_y = (vis.y - area_r.y) as usize;
        let vis_w = vis.w as usize;
        let vis_h = vis.h as usize;
        let tail_x = src_w - skip_x - vis_w;

        let mut iter = colors.into_iter();
        for _ in 0..(skip_y * src_w) {
            if iter.next().is_none() {
                return Ok(());
            }
        }

        let cap = self.stage_words();
        let mut word_buf = [0u16; MAX_STAGE_WORDS];
        let mut count = 0usize;

        for ry in 0..vis_h {
            for _ in 0..skip_x {
                if iter.next().is_none() {
                    if count > 0 {
                        self.write_word_slice(&word_buf[..count])?;
                    }
                    return Ok(());
                }
            }
            for _ in 0..vis_w {
                let Some(color) = iter.next() else {
                    if count > 0 {
                        self.write_word_slice(&word_buf[..count])?;
                    }
                    return Ok(());
                };
                word_buf[count] = rgb565_to_u16(color);
                count += 1;
                if count == cap {
                    self.write_word_slice(&word_buf[..count])?;
                    count = 0;
                }
            }
            if ry + 1 < vis_h {
                for _ in 0..tail_x {
                    if iter.next().is_none() {
                        if count > 0 {
                            self.write_word_slice(&word_buf[..count])?;
                        }
                        return Ok(());
                    }
                }
            }
        }

        if count > 0 {
            self.write_word_slice(&word_buf[..count])?;
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let vis = Rect::from_eg(*area).intersect(self.bounds);
        if vis.is_empty() {
            return Ok(());
        }
        self.bus.set_window(vis)?;
        let cap = self.stage_words();
        let word = rgb565_to_u16(color);
        let word_buf = [word; MAX_STAGE_WORDS];
        let mut remaining = vis.area64();
        while remaining > 0 {
            let n = remaining.min(cap as u64) as usize;
            self.write_word_slice(&word_buf[..n])?;
            remaining -= n as u64;
        }
        Ok(())
    }
}

/// Adapts an existing `embedded-graphics` `DrawTarget<Color = Rgb565>` into a [`DisplaySink`].
pub struct EgDisplaySink<T>(pub T);

impl<T> EgDisplaySink<T> {
    /// Wraps `target` in a [`DisplaySink`] adapter.
    #[inline]
    pub const fn new(target: T) -> Self {
        Self(target)
    }

    #[inline]
    pub fn inner(&self) -> &T {
        &self.0
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.0
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: DrawTarget<Color = Rgb565>> DisplaySink for EgDisplaySink<T> {
    type Error = T::Error;

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
        let target_bb = Rect::from_eg(self.0.bounding_box());
        let phys_rect = rect.translate(target_bb.x, target_bb.y);
        let vis_phys = phys_rect.intersect(target_bb);
        if vis_phys.is_empty() {
            return Ok(());
        }

        let skip_x = (vis_phys.x - phys_rect.x) as usize;
        let skip_y = (vis_phys.y - phys_rect.y) as usize;
        let stride = rect.w as usize;
        let vis_w = vis_phys.w as usize;
        let vis_h = vis_phys.h as usize;

        let color_iter = (0..vis_h).flat_map(|ry| {
            let row_start = (skip_y + ry) * stride + skip_x;
            pixels[row_start..row_start + vis_w]
                .iter()
                .map(|&w| u16_to_rgb565(w))
        });

        self.0.fill_contiguous(&vis_phys.to_eg(), color_iter)
    }
}
