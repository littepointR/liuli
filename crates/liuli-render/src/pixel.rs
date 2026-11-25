use liuli_core::color::{
    PixelFormat, Rgb888, Rgba8888, rgb565_u16_to_argb8888, rgb565_u16_to_rgb888,
    rgb888_to_rgb565_round,
};
use liuli_core::geometry::{Point, Rect, Rgb565, rgb565_to_u16};
use liuli_text::{blend_rgb565_a4_u16, blend_rgb565_a8_u16};

use crate::wire::BufferTooSmall;

/// Abstraction over a surface's native pixel representation (`RGB565` or `ARGB8888`).
pub trait SurfacePixel: Copy + Eq + Default + 'static {
    const FORMAT: PixelFormat;
    const BYTES_PER_PIXEL: usize;

    fn from_rgb565(c: Rgb565) -> Self;
    fn from_rgb565_u16(raw: u16) -> Self;
    fn from_rgb888(c: Rgb888) -> Self;
    fn from_rgba8888(c: Rgba8888) -> Self;
    fn to_rgb565_u16(self) -> u16;
    fn to_rgba8888(self) -> Rgba8888;
    fn blend_a4(self, fg: Self, alpha4: u8) -> Self;
    fn blend_a8(self, fg: Self, alpha8: u8) -> Self;
    fn write_le(self, dst: &mut [u8]);
}

/// Packed little-endian RGB565 surface pixel (`2` bytes per pixel).
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct PixelRgb565(pub u16);

impl PixelRgb565 {
    #[inline]
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl SurfacePixel for PixelRgb565 {
    const FORMAT: PixelFormat = PixelFormat::Rgb565Le;
    const BYTES_PER_PIXEL: usize = 2;

    #[inline]
    fn from_rgb565(c: Rgb565) -> Self {
        Self(rgb565_to_u16(c))
    }

    #[inline]
    fn from_rgb565_u16(raw: u16) -> Self {
        Self(raw)
    }

    #[inline]
    fn from_rgb888(c: Rgb888) -> Self {
        Self(rgb565_to_u16(rgb888_to_rgb565_round(c.r, c.g, c.b)))
    }

    #[inline]
    fn from_rgba8888(c: Rgba8888) -> Self {
        Self(rgb565_to_u16(rgb888_to_rgb565_round(c.r, c.g, c.b)))
    }

    #[inline]
    fn to_rgb565_u16(self) -> u16 {
        self.0
    }

    #[inline]
    fn to_rgba8888(self) -> Rgba8888 {
        let c = rgb565_u16_to_rgb888(self.0);
        Rgba8888::opaque(c.r, c.g, c.b)
    }

    #[inline]
    fn blend_a4(self, fg: Self, alpha4: u8) -> Self {
        Self(blend_rgb565_a4_u16(fg.0, self.0, alpha4))
    }

    #[inline]
    fn blend_a8(self, fg: Self, alpha8: u8) -> Self {
        Self(blend_rgb565_a8_u16(fg.0, self.0, alpha8))
    }

    #[inline]
    fn write_le(self, dst: &mut [u8]) {
        let [b0, b1] = self.0.to_le_bytes();
        dst[0] = b0;
        dst[1] = b1;
    }
}

/// Straight-alpha 32-bit `ARGB8888` pixel (`0xAARRGGBB`, wire LE `[B, G, R, A]`, `4` bytes per pixel).
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct PixelArgb8888(pub u32);

impl PixelArgb8888 {
    pub const TRANSPARENT: Self = Self(0);

    #[inline]
    pub const fn new(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    #[inline]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(0xFF, r, g, b)
    }

    #[inline]
    pub const fn a(self) -> u8 {
        (self.0 >> 24) as u8
    }

    #[inline]
    pub const fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }

    #[inline]
    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }

    #[inline]
    pub const fn b(self) -> u8 {
        self.0 as u8
    }
}

impl SurfacePixel for PixelArgb8888 {
    const FORMAT: PixelFormat = PixelFormat::Argb8888;
    const BYTES_PER_PIXEL: usize = 4;

    #[inline]
    fn from_rgb565(c: Rgb565) -> Self {
        Self::from_rgb565_u16(rgb565_to_u16(c))
    }

    #[inline]
    fn from_rgb565_u16(raw: u16) -> Self {
        Self(rgb565_u16_to_argb8888(raw, 0xFF))
    }

    #[inline]
    fn from_rgb888(c: Rgb888) -> Self {
        Self::new(0xFF, c.r, c.g, c.b)
    }

    #[inline]
    fn from_rgba8888(c: Rgba8888) -> Self {
        Self::new(c.a, c.r, c.g, c.b)
    }

    #[inline]
    fn to_rgb565_u16(self) -> u16 {
        rgb565_to_u16(rgb888_to_rgb565_round(self.r(), self.g(), self.b()))
    }

    #[inline]
    fn to_rgba8888(self) -> Rgba8888 {
        Rgba8888::new(self.r(), self.g(), self.b(), self.a())
    }

    #[inline]
    fn blend_a4(self, fg: Self, alpha4: u8) -> Self {
        self.blend_a8(fg, alpha4.min(15) * 17)
    }

    #[inline]
    fn blend_a8(self, fg: Self, alpha8: u8) -> Self {
        let sa = (u32::from(fg.a()) * u32::from(alpha8) + 127) / 255;
        if sa == 0 {
            return self;
        }
        if sa >= 255 {
            return Self::new(255, fg.r(), fg.g(), fg.b());
        }
        let da = u32::from(self.a());
        let inv = 255 - sa;
        if da == 255 {
            let rr = (u32::from(fg.r()) * sa + u32::from(self.r()) * inv + 127) / 255;
            let rg = (u32::from(fg.g()) * sa + u32::from(self.g()) * inv + 127) / 255;
            let rb = (u32::from(fg.b()) * sa + u32::from(self.b()) * inv + 127) / 255;
            return Self::new(255, rr as u8, rg as u8, rb as u8);
        }
        if da == 0 {
            return Self::new(sa as u8, fg.r(), fg.g(), fg.b());
        }
        let w_src = sa * 255;
        let w_dst = da * inv;
        let denom = w_src + w_dst;
        let out_a = ((denom + 127) / 255) as u8;
        let half = denom >> 1;
        let rr = (u32::from(fg.r()) * w_src + u32::from(self.r()) * w_dst + half) / denom;
        let rg = (u32::from(fg.g()) * w_src + u32::from(self.g()) * w_dst + half) / denom;
        let rb = (u32::from(fg.b()) * w_src + u32::from(self.b()) * w_dst + half) / denom;
        Self::new(out_a, rr as u8, rg as u8, rb as u8)
    }

    #[inline]
    fn write_le(self, dst: &mut [u8]) {
        dst[..4].copy_from_slice(&self.0.to_le_bytes());
    }
}

/// Typed pixel-buffer target for off-screen / ARGB8888 / RGB565 compositing bands.
pub struct TypedBandTarget<'a, P: SurfacePixel> {
    pixels: &'a mut [P],
    band_rect: Rect,
}

impl<'a, P: SurfacePixel> TypedBandTarget<'a, P> {
    pub fn new(pixels: &'a mut [P], band_rect: Rect) -> Option<Self> {
        let len = usize::try_from(band_rect.w)
            .ok()?
            .checked_mul(usize::try_from(band_rect.h).ok()?)?;
        if len == 0 || pixels.len() < len {
            return None;
        }
        Some(Self {
            pixels: &mut pixels[..len],
            band_rect,
        })
    }

    #[inline]
    pub const fn band_rect(&self) -> Rect {
        self.band_rect
    }

    #[inline]
    pub const fn width(&self) -> u32 {
        self.band_rect.w
    }

    #[inline]
    pub const fn height(&self) -> u32 {
        self.band_rect.h
    }

    #[inline]
    pub fn pixels(&self) -> &[P] {
        self.pixels
    }

    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [P] {
        self.pixels
    }

    #[inline]
    pub fn clear(&mut self, color: P) {
        self.pixels.fill(color);
    }

    #[inline]
    fn local_index(&self, gx: i32, gy: i32) -> Option<usize> {
        let lx = gx.checked_sub(self.band_rect.x)?;
        let ly = gy.checked_sub(self.band_rect.y)?;
        if lx < 0 || ly < 0 {
            return None;
        }
        let ux = lx as u32;
        let uy = ly as u32;
        if ux >= self.band_rect.w || uy >= self.band_rect.h {
            return None;
        }
        Some((uy as usize) * (self.band_rect.w as usize) + (ux as usize))
    }

    #[inline]
    pub fn put_pixel(&mut self, gx: i32, gy: i32, color: P) {
        if let Some(idx) = self.local_index(gx, gy) {
            self.pixels[idx] = color;
        }
    }

    #[inline]
    pub fn get_pixel(&self, gx: i32, gy: i32) -> Option<P> {
        self.local_index(gx, gy).map(|idx| self.pixels[idx])
    }

    pub fn fill_rect(&mut self, rect: Rect, color: P) {
        let inter = rect.intersect(self.band_rect);
        if inter.is_empty() {
            return;
        }
        let stride = self.band_rect.w as usize;
        let x0 = (inter.x - self.band_rect.x) as usize;
        let span = inter.w as usize;
        let y0 = (inter.y - self.band_rect.y) as usize;
        for ry in 0..inter.h as usize {
            let start = (y0 + ry) * stride + x0;
            self.pixels[start..start + span].fill(color);
        }
    }

    #[inline]
    pub fn blend_pixel_a4(&mut self, gx: i32, gy: i32, fg: P, alpha4: u8) {
        if let Some(idx) = self.local_index(gx, gy) {
            self.pixels[idx] = self.pixels[idx].blend_a4(fg, alpha4);
        }
    }

    #[inline]
    pub fn blend_pixel_a8(&mut self, gx: i32, gy: i32, fg: P, alpha8: u8) {
        if let Some(idx) = self.local_index(gx, gy) {
            self.pixels[idx] = self.pixels[idx].blend_a8(fg, alpha8);
        }
    }

    /// Copies pixels from a tight `RGB565` buffer covering `src_rect` into this target clipped by `band_rect`.
    pub fn blit_from_rgb565(&mut self, src: &[u16], src_rect: Rect) {
        let Some(expected) = (src_rect.w as usize).checked_mul(src_rect.h as usize) else {
            return;
        };
        if src.len() < expected {
            return;
        }
        let inter = src_rect.intersect(self.band_rect);
        if inter.is_empty() {
            return;
        }
        let dst_stride = self.band_rect.w as usize;
        let src_stride = src_rect.w as usize;
        for ry in 0..inter.h as usize {
            let gy = inter.y + ry as i32;
            let src_y = (gy - src_rect.y) as usize;
            let dst_y = (gy - self.band_rect.y) as usize;
            let src_x0 = (inter.x - src_rect.x) as usize;
            let dst_x0 = (inter.x - self.band_rect.x) as usize;
            for rx in 0..inter.w as usize {
                self.pixels[dst_y * dst_stride + dst_x0 + rx] =
                    P::from_rgb565_u16(src[src_y * src_stride + src_x0 + rx]);
            }
        }
    }

    /// Porter-Duff Source-Over blends `src` placed at `src.band_rect` + `offset` onto `self`.
    pub fn blend_over(&mut self, src: &TypedBandTarget<'_, P>, offset: Point, global_alpha8: u8) {
        if global_alpha8 == 0 {
            return;
        }
        let src_dst_rect = src.band_rect.translate(offset.x, offset.y);
        let inter = src_dst_rect.intersect(self.band_rect);
        if inter.is_empty() {
            return;
        }
        let dst_stride = self.band_rect.w as usize;
        let src_stride = src.band_rect.w as usize;
        for ry in 0..inter.h as usize {
            let gy = inter.y + ry as i32;
            let sy = (gy - src_dst_rect.y) as usize;
            let dy = (gy - self.band_rect.y) as usize;
            let sx0 = (inter.x - src_dst_rect.x) as usize;
            let dx0 = (inter.x - self.band_rect.x) as usize;
            for rx in 0..inter.w as usize {
                let s_px = src.pixels[sy * src_stride + sx0 + rx];
                let d_idx = dy * dst_stride + dx0 + rx;
                self.pixels[d_idx] = self.pixels[d_idx].blend_a8(s_px, global_alpha8);
            }
        }
    }

    /// Serializes local scanline `local_y` into wire little-endian bytes using `P::FORMAT`.
    pub fn encode_row_bytes(&self, local_y: u32, out: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let needed = P::FORMAT.row_bytes(self.band_rect.w);
        if local_y >= self.band_rect.h || out.len() < needed {
            return Err(BufferTooSmall);
        }
        let w = self.band_rect.w as usize;
        let row = &self.pixels[(local_y as usize) * w..(local_y as usize) * w + w];
        for (i, &px) in row.iter().enumerate() {
            px.write_le(&mut out[i * P::BYTES_PER_PIXEL..]);
        }
        Ok(needed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_and_argb8888_surface_pixels_roundtrip_and_blend() {
        let p16 = PixelRgb565::from_rgb888(Rgb888::new(255, 128, 0));
        assert_eq!(p16.to_rgb565_u16(), rgb565_to_u16(Rgb565::new(31, 32, 0)));
        assert_eq!(
            p16.blend_a4(PixelRgb565::new(0xFFFF), 15),
            PixelRgb565::new(0xFFFF)
        );
        assert_eq!(p16.blend_a8(PixelRgb565::new(0x0000), 0), p16);

        let fg = PixelArgb8888::new(128, 200, 100, 50);
        let bg = PixelArgb8888::new(255, 0, 200, 100);
        let blended = bg.blend_a8(fg, 255);
        assert_eq!(blended.a(), 255);
        assert!((98..=102).contains(&blended.r()));
        assert!((148..=152).contains(&blended.g()));
        assert!((73..=77).contains(&blended.b()));

        // Drawing over transparent background preserves source color without dark fringe
        let over_clear =
            PixelArgb8888::TRANSPARENT.blend_a8(PixelArgb8888::from_rgb(200, 40, 80), 128);
        assert_eq!(over_clear, PixelArgb8888::new(128, 200, 40, 80));
    }

    #[test]
    fn typed_band_target_blit_blend_over_and_row_encode() {
        let mut dst_buf = [PixelArgb8888::from_rgb(10, 20, 30); 4 * 4];
        let mut dst = TypedBandTarget::new(&mut dst_buf, Rect::new(10, 20, 4, 4)).unwrap();
        dst.fill_rect(Rect::new(11, 21, 2, 2), PixelArgb8888::from_rgb(255, 0, 0));
        assert_eq!(
            dst.get_pixel(11, 21),
            Some(PixelArgb8888::from_rgb(255, 0, 0))
        );
        assert_eq!(dst.get_pixel(9, 20), None);

        let mut src_buf = [PixelArgb8888::new(128, 0, 0, 255); 2 * 2];
        let src = TypedBandTarget::new(&mut src_buf, Rect::new(0, 0, 2, 2)).unwrap();
        dst.blend_over(&src, Point::new(11, 21), 255);
        let corner = dst.get_pixel(11, 21).unwrap();
        assert_eq!(corner.a(), 255);
        assert!((126..=129).contains(&corner.r()));
        assert!((127..=130).contains(&corner.b()));

        let mut row_bytes = [0u8; 4 * 4];
        assert!(dst.encode_row_bytes(4, &mut row_bytes).is_err());
        assert!(dst.encode_row_bytes(0, &mut row_bytes[..15]).is_err());
        assert_eq!(dst.encode_row_bytes(1, &mut row_bytes).unwrap(), 16);
        assert_eq!(
            &row_bytes[4..8],
            &dst.get_pixel(11, 21).unwrap().0.to_le_bytes()
        );

        let mut rgb565_buf = [PixelRgb565::default(); 4];
        let mut t16 = TypedBandTarget::new(&mut rgb565_buf, Rect::new(0, 0, 2, 2)).unwrap();
        t16.blit_from_rgb565(&[0xF800, 0x07E0, 0x001F, 0xFFFF], Rect::new(0, 0, 2, 2));
        assert_eq!(t16.get_pixel(0, 0), Some(PixelRgb565::new(0xF800)));
        let mut r16 = [0u8; 4];
        assert_eq!(t16.encode_row_bytes(1, &mut r16).unwrap(), 4);
        assert_eq!(r16, [0x1F, 0x00, 0xFF, 0xFF]);
    }
}
