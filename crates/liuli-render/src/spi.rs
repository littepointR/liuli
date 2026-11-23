use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Dimensions;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::Rectangle;
use liuli_core::geometry::{Rect, rgb565_to_u16};

use crate::wire::{BufferTooSmall, ByteOrder, encode_rgb565};

pub const RECOMMENDED_SPI_CHUNK_BYTES: usize = 64;
const MAX_STAGE_WORDS: usize = 32;

pub trait SpiWindowBus {
    type Error;

    fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error>;
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct SpiWindowTarget<'a, B> {
    bus: B,
    bounds: Rect,
    order: ByteOrder,
    chunk: &'a mut [u8],
}

impl<'a, B> SpiWindowTarget<'a, B> {
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
        (self.chunk.len() / 2).min(MAX_STAGE_WORDS).max(1)
    }

    fn write_word_slice(&mut self, words: &[u16]) -> Result<(), B::Error>
    where
        B: SpiWindowBus,
    {
        if let Ok(nbytes) = encode_rgb565(words, self.chunk, self.order) {
            self.bus.write_bytes(&self.chunk[..nbytes])?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), B::Error>
    where
        B: SpiWindowBus,
    {
        self.bus.flush()
    }
}

impl<B> Dimensions for SpiWindowTarget<'_, B> {
    fn bounding_box(&self) -> Rectangle {
        self.bounds.to_eg()
    }
}

impl<B: SpiWindowBus> DrawTarget for SpiWindowTarget<'_, B> {
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
