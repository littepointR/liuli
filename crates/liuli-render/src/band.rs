use core::convert::Infallible;

use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Size};
use embedded_graphics::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics::primitives::Rectangle;
use liuli_core::capacity::PresentError;
use liuli_core::geometry::Rect;

pub struct BandTarget<'a> {
    words: &'a mut [u16],

    alpha: Option<&'a mut [u8]>,
    width: u32,
    height: u32,
    band_rect: Rect,
}

impl<'a> BandTarget<'a> {
    pub fn new<E>(words: &'a mut [u16], band_rect: Rect) -> Result<Self, PresentError<E>> {
        let len = (band_rect.w as usize)
            .checked_mul(band_rect.h as usize)
            .ok_or(PresentError::BufferTooSmall)?;
        if len > words.len() {
            return Err(PresentError::BufferTooSmall);
        }
        Ok(Self {
            words: &mut words[..len],

            alpha: None,
            width: band_rect.w,
            height: band_rect.h,
            band_rect,
        })
    }

    pub fn new_with_alpha<E>(
        words: &'a mut [u16],
        alpha: &'a mut [u8],
        band_rect: Rect,
    ) -> Result<Self, PresentError<E>> {
        let len = (band_rect.w as usize)
            .checked_mul(band_rect.h as usize)
            .ok_or(PresentError::BufferTooSmall)?;
        let alpha_len = len.div_ceil(2);
        if len > words.len() || alpha_len > alpha.len() {
            return Err(PresentError::BufferTooSmall);
        }
        Ok(Self {
            words: &mut words[..len],
            alpha: Some(&mut alpha[..alpha_len]),
            width: band_rect.w,
            height: band_rect.h,
            band_rect,
        })
    }

    #[inline(always)]
    pub const fn has_alpha(&self) -> bool {
        self.alpha.is_some()
    }

    #[inline]
    pub const fn band_rect(&self) -> Rect {
        self.band_rect
    }

    #[inline]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn words(&self) -> &[u16] {
        self.words
    }

    #[inline]
    pub fn words_mut(&mut self) -> &mut [u16] {
        self.words
    }

    #[inline(always)]
    pub fn put_opaque(&mut self, idx: usize, raw: u16) {
        self.words[idx] = raw;

        if let Some(a) = &mut self.alpha {
            liuli_core::composition::set_a4(a, idx, 15);
        }
    }

    #[inline(always)]
    pub fn set_opaque_span(&mut self, start: usize, len: usize) {
        if let Some(a) = &mut self.alpha {
            let mut i = start;
            let end = start + len;
            if (i & 1) != 0 && i < end {
                liuli_core::composition::set_a4(a, i, 15);
                i += 1;
            }
            let pairs = (end - i) / 2;
            if pairs > 0 {
                a[i >> 1..(i >> 1) + pairs].fill(0xff);
                i += pairs * 2;
            }
            if i < end {
                liuli_core::composition::set_a4(a, i, 15);
            }
        }
    }

    #[inline(always)]
    pub fn blend_pixel_a4(&mut self, idx: usize, fg_raw: u16, cov: u8) {
        if let Some(a) = &mut self.alpha {
            let dst_a = liuli_core::composition::get_a4(a, idx);
            let (rgb, out_a) =
                liuli_text::blend_over_rgb565_a4(self.words[idx], dst_a, fg_raw, cov);
            self.words[idx] = rgb;
            liuli_core::composition::set_a4(a, idx, out_a);
            return;
        }
        self.words[idx] = liuli_text::blend_rgb565_a4_u16(fg_raw, self.words[idx], cov);
    }
}

impl crate::vector::PixelSink for BandTarget<'_> {
    #[inline(always)]
    fn put_opaque(&mut self, idx: usize, fg_raw: u16) {
        self.put_opaque(idx, fg_raw);
    }

    #[inline(always)]
    fn blend_a4(&mut self, idx: usize, fg_raw: u16, cov: u8) {
        self.blend_pixel_a4(idx, fg_raw, cov);
    }
}

impl OriginDimensions for BandTarget<'_> {
    #[inline]
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for BandTarget<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let w = self.width as i32;
        let h = self.height as i32;
        let stride = self.width as usize;
        for Pixel(pt, color) in pixels {
            if pt.x >= 0 && pt.x < w && pt.y >= 0 && pt.y < h {
                self.put_opaque(
                    (pt.y as usize) * stride + (pt.x as usize),
                    color.into_storage(),
                );
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let req = Rect::from_eg(*area);
        let local = Rect::new(0, 0, self.width, self.height);
        let inter = req.intersect(local);
        if inter.is_empty() {
            return Ok(());
        }
        let raw = color.into_storage();
        let stride = self.width as usize;
        let x0 = inter.x as usize;
        let span_w = inter.w as usize;
        let x1 = x0 + span_w;
        let has_alpha = self.has_alpha();
        for y in inter.y as usize..(inter.y as usize + inter.h as usize) {
            let row_start = y * stride;
            self.words[row_start + x0..row_start + x1].fill(raw);
            if has_alpha {
                self.set_opaque_span(row_start + x0, span_w);
            }
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let req = Rect::from_eg(*area);
        if req.is_empty() {
            return Ok(());
        }
        let local = Rect::new(0, 0, self.width, self.height);
        let inter = req.intersect(local);
        if inter.is_empty() {
            return Ok(());
        }

        let mut iter = colors.into_iter();
        let w_usize = self.width as usize;

        if inter == req {
            let x0 = req.x as usize;
            let y0 = req.y as usize;
            for ry in 0..req.h as usize {
                let row_start = (y0 + ry) * w_usize + x0;
                for rx in 0..req.w as usize {
                    let Some(c) = iter.next() else {
                        return Ok(());
                    };
                    self.put_opaque(row_start + rx, c.into_storage());
                }
            }
            return Ok(());
        }

        for ry in 0..req.h as i64 {
            let gy = req.y as i64 + ry;
            if gy >= self.height as i64 {
                break;
            }
            let y_in = gy >= 0;
            for rx in 0..req.w as i64 {
                let Some(c) = iter.next() else {
                    return Ok(());
                };
                if y_in {
                    let gx = req.x as i64 + rx;
                    if gx >= 0 && gx < self.width as i64 {
                        self.put_opaque((gy as usize) * w_usize + (gx as usize), c.into_storage());
                    }
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        if let Some(a) = &mut self.alpha {
            a.fill(0);
            return Ok(());
        }
        self.words.fill(color.into_storage());
        Ok(())
    }
}

pub struct BandIter<'a> {
    rects: &'a [Rect],
    idx: usize,
    cur_y: i32,
    configured_band_h: u32,
    buf_capacity: usize,
}

impl<'a> BandIter<'a> {
    pub fn new<E>(
        rects: &'a [Rect],
        screen: Rect,
        configured_band_h: u32,
        buf_capacity: usize,
    ) -> Result<Self, PresentError<E>> {
        if buf_capacity < screen.w as usize {
            return Err(PresentError::BufferTooSmall);
        }
        let first_y = rects.first().map(|r| r.y).unwrap_or(0);
        Ok(Self {
            rects,
            idx: 0,
            cur_y: first_y,
            configured_band_h: configured_band_h.max(1),
            buf_capacity,
        })
    }
}

impl Iterator for BandIter<'_> {
    type Item = Rect;

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.rects.len() {
            let r = self.rects[self.idx];
            if r.is_empty() {
                self.idx += 1;
                if self.idx < self.rects.len() {
                    self.cur_y = self.rects[self.idx].y;
                }
                continue;
            }
            let bottom = r.bottom64();
            if (self.cur_y as i64) >= bottom {
                self.idx += 1;
                if self.idx < self.rects.len() {
                    self.cur_y = self.rects[self.idx].y;
                }
                continue;
            }
            let remaining_h = (bottom - self.cur_y as i64) as u32;
            let max_rows_for_buf = (self.buf_capacity / (r.w as usize)).max(1) as u32;
            let bh = self
                .configured_band_h
                .min(remaining_h)
                .min(max_rows_for_buf);
            let band = Rect::new(r.x, self.cur_y, r.w, bh);
            self.cur_y = (self.cur_y as i64 + bh as i64) as i32;
            return Some(band);
        }
        None
    }
}
