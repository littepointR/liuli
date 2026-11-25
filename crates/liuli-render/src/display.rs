use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Dimensions;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::Rectangle;
use liuli_core::color::PixelFormat;
use liuli_core::dither::DitherMode;
use liuli_core::geometry::{Point, Rect, Rotation, Size, rgb565_to_u16, u16_to_rgb565};

use crate::wire::{
    BufferTooSmall, ByteOrder, byte_order_to_pixel_format, encode_rgb565_row,
};

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
    format: PixelFormat,
    dither: DitherMode,
    chunk: &'a mut [u8],
}

impl<'a, B> ByteWindowSink<'a, B> {
    pub fn new(
        bus: B,
        bounds: Rect,
        order: ByteOrder,
        chunk: &'a mut [u8],
    ) -> Result<Self, BufferTooSmall> {
        Self::with_format(
            bus,
            bounds,
            byte_order_to_pixel_format(order),
            DitherMode::None,
            chunk,
        )
    }

    pub fn with_format(
        bus: B,
        bounds: Rect,
        format: PixelFormat,
        dither: DitherMode,
        chunk: &'a mut [u8],
    ) -> Result<Self, BufferTooSmall> {
        if chunk.len() < format.min_chunk_bytes() {
            return Err(BufferTooSmall);
        }
        let order = match format {
            PixelFormat::Rgb565Le | PixelFormat::Bgr565Le => ByteOrder::LittleEndian,
            _ => ByteOrder::BigEndian,
        };
        Ok(Self {
            bus,
            bounds,
            order,
            format,
            dither,
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
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    #[inline]
    pub const fn dither(&self) -> DitherMode {
        self.dither
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
    fn max_row_chunk_words(&self) -> usize {
        match self.format {
            PixelFormat::Rgb565Le
            | PixelFormat::Rgb565Be
            | PixelFormat::Bgr565Le
            | PixelFormat::Bgr565Be => (self.chunk.len() / 2).max(1),
            PixelFormat::Rgb666 | PixelFormat::Rgb888 | PixelFormat::Bgr888 => {
                (self.chunk.len() / 3).max(1)
            }
            PixelFormat::Xrgb8888 | PixelFormat::Argb8888 | PixelFormat::Abgr8888 => {
                (self.chunk.len() / 4).max(1)
            }
            PixelFormat::Gray8 => self.chunk.len().max(1),
            PixelFormat::Gray4 => self.chunk.len().saturating_mul(2).max(2),
            PixelFormat::Mono1Msb | PixelFormat::Mono1Lsb => {
                self.chunk.len().saturating_mul(8).max(8)
            }
        }
    }

    #[inline]
    fn stage_words(&self) -> usize {
        self.max_row_chunk_words().min(MAX_STAGE_WORDS)
    }

    fn write_row_words(&mut self, gx0: i32, gy: i32, words: &[u16]) -> Result<(), B::Error>
    where
        B: ByteWindowBus,
    {
        let max_words = self.max_row_chunk_words();
        let mut off = 0usize;
        for slice in words.chunks(max_words) {
            if let Ok(nbytes) = encode_rgb565_row(
                slice,
                gx0.saturating_add(off as i32),
                gy,
                self.format,
                self.dither,
                self.chunk,
            ) {
                self.bus.write_bytes(&self.chunk[..nbytes])?;
            }
            off += slice.len();
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
            self.write_row_words(vis.x, vis.y.saturating_add(ry as i32), row)?;
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
            self.write_row_words(pt.x, pt.y, &w)?;
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

        for ry in 0..vis_h {
            let gy = vis.y.saturating_add(ry as i32);
            for _ in 0..skip_x {
                if iter.next().is_none() {
                    return Ok(());
                }
            }
            let mut gx = vis.x;
            let mut count = 0usize;
            for _ in 0..vis_w {
                let Some(color) = iter.next() else {
                    if count > 0 {
                        self.write_row_words(gx, gy, &word_buf[..count])?;
                    }
                    return Ok(());
                };
                word_buf[count] = rgb565_to_u16(color);
                count += 1;
                if count == cap {
                    self.write_row_words(gx, gy, &word_buf[..count])?;
                    gx = gx.saturating_add(count as i32);
                    count = 0;
                }
            }
            if count > 0 {
                self.write_row_words(gx, gy, &word_buf[..count])?;
            }
            if ry + 1 < vis_h {
                for _ in 0..tail_x {
                    if iter.next().is_none() {
                        return Ok(());
                    }
                }
            }
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
        for ry in 0..vis.h {
            let gy = vis.y.saturating_add(ry as i32);
            let mut rem = vis.w as usize;
            let mut gx = vis.x;
            while rem > 0 {
                let n = rem.min(cap);
                self.write_row_words(gx, gy, &word_buf[..n])?;
                gx = gx.saturating_add(n as i32);
                rem -= n;
            }
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

/// Rotating [`DisplaySink`] wrapper transforming logical bands into physical panel coordinates.
///
/// Uses an inline `[u16; MAX_PIXELS]` scratch buffer (`#[no_std]`, zero heap allocation) and
/// automatically falls back to multi-row tiling when `MAX_PIXELS < rect.w * rect.h`.
pub struct RotatedDisplaySink<S, const MAX_PIXELS: usize> {
    inner: S,
    physical_size: Size,
    rotation: Rotation,
    scratch: [u16; MAX_PIXELS],
}

impl<S, const MAX_PIXELS: usize> RotatedDisplaySink<S, MAX_PIXELS> {
    #[inline]
    pub const fn new(inner: S, physical_size: Size, rotation: Rotation) -> Self {
        Self {
            inner,
            physical_size,
            rotation,
            scratch: [0; MAX_PIXELS],
        }
    }

    #[inline]
    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    #[inline]
    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
    }

    #[inline]
    pub const fn physical_size(&self) -> Size {
        self.physical_size
    }

    #[inline]
    pub const fn logical_size(&self) -> Size {
        self.rotation.logical_size(self.physical_size)
    }

    #[inline]
    pub fn inner(&self) -> &S {
        &self.inner
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    #[inline]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: DisplaySink, const MAX_PIXELS: usize> DisplaySink for RotatedDisplaySink<S, MAX_PIXELS> {
    type Error = S::Error;

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
        if self.rotation == Rotation::Deg0 {
            return self.inner.flush_rect(rect, pixels);
        }
        let logical = self.logical_size();
        let vis = rect.intersect(Rect::new(0, 0, logical.width, logical.height));
        if vis.is_empty() {
            return Ok(());
        }
        let stride = rect.w as usize;
        let skip_x = (vis.x - rect.x) as usize;
        let skip_y = (vis.y - rect.y) as usize;
        let phys_rect = self.rotation.map_rect_to_physical(vis, self.physical_size);
        let phys_w = phys_rect.w as usize;
        let phys_h = phys_rect.h as usize;
        let total = phys_w.saturating_mul(phys_h);
        if total == 0 || MAX_PIXELS == 0 {
            return Ok(());
        }

        let max_tile_h = (MAX_PIXELS / phys_w).max(1).min(phys_h);
        let mut py_base = 0usize;
        while py_base < phys_h {
            let tile_h = (phys_h - py_base).min(max_tile_h);
            let tile_len = phys_w * tile_h;
            for ty in 0..tile_h {
                let py = phys_rect.y + (py_base + ty) as i32;
                for px_off in 0..phys_w {
                    let px = phys_rect.x + px_off as i32;
                    let lpt = self
                        .rotation
                        .map_point_to_logical(Point::new(px, py), self.physical_size);
                    let src_idx = (skip_y + (lpt.y - vis.y) as usize) * stride
                        + (skip_x + (lpt.x - vis.x) as usize);
                    self.scratch[ty * phys_w + px_off] = pixels.get(src_idx).copied().unwrap_or(0);
                }
            }
            let sub_rect = Rect::new(
                phys_rect.x,
                phys_rect.y + py_base as i32,
                phys_w as u32,
                tile_h as u32,
            );
            self.inner.flush_rect(sub_rect, &self.scratch[..tile_len])?;
            py_base += tile_h;
        }
        Ok(())
    }

    fn finish_frame(&mut self) -> Result<(), Self::Error> {
        self.inner.finish_frame()
    }
}
