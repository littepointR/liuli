use core::convert::Infallible;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::{OriginDimensions, Size};
use embedded_graphics::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics::primitives::{
    Circle, Line, Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
    StrokeAlignment, Triangle,
};
use embedded_graphics::{Drawable, Pixel};
use liuli::core::paint::{CommandList, PaintCmd, SceneResources};
use liuli::prelude::{IconKind, Point, Rect};
use liuli::text::{FontSet, TextLayoutOptions, a4_pixel, blend_rgb565_a4, visit_layout_glyphs};

use crate::image::{encode_png_rgb8, encode_ppm, rgb565_to_rgb8};

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct MemoryScreen {
    w: u32,
    h: u32,
    pixels: Vec<u16>,
}

impl MemoryScreen {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            pixels: vec![0; (w as usize) * (h as usize)],
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.w
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.h
    }

    #[inline]
    pub fn words(&self) -> &[u16] {
        &self.pixels
    }

    #[inline]
    pub fn words_mut(&mut self) -> &mut [u16] {
        &mut self.pixels
    }

    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> u16 {
        self.pixels[(y as usize) * (self.w as usize) + (x as usize)]
    }

    pub fn to_rgb8(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.pixels.len() * 3);
        for &w in &self.pixels {
            rgb.extend_from_slice(&rgb565_to_rgb8(w));
        }
        rgb
    }

    pub fn to_ppm(&self) -> Vec<u8> {
        encode_ppm(self.w, self.h, &self.to_rgb8())
    }

    pub fn to_png(&self) -> Vec<u8> {
        encode_png_rgb8(self.w, self.h, &self.to_rgb8())
    }

    pub fn diff_first_mismatch(&self, expected: &Self) -> Option<(u32, u32, u16, u16)> {
        if self.w != expected.w || self.h != expected.h {
            return Some((0, 0, 0, 0));
        }
        for y in 0..self.h {
            for x in 0..self.w {
                let a = self.pixel(x, y);
                let b = expected.pixel(x, y);
                if a != b {
                    return Some((x, y, a, b));
                }
            }
        }
        None
    }
}

impl OriginDimensions for MemoryScreen {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for MemoryScreen {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let w = self.w as i32;
        let h = self.h as i32;
        let stride = self.w as usize;
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.x < w && p.y >= 0 && p.y < h {
                self.pixels[(p.y as usize) * stride + (p.x as usize)] = c.into_storage();
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let req = Rect::from_eg(*area);
        let inter = req.intersect(Rect::new(0, 0, self.w, self.h));
        if inter.is_empty() {
            return Ok(());
        }
        let raw = color.into_storage();
        let stride = self.w as usize;
        let x0 = inter.x as usize;
        let x1 = x0 + inter.w as usize;
        for y in inter.y as usize..(inter.y as usize + inter.h as usize) {
            self.pixels[y * stride + x0..y * stride + x1].fill(raw);
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
        let screen = Rect::new(0, 0, self.w, self.h);
        if req.intersect(screen).is_empty() {
            return Ok(());
        }
        let mut iter = colors.into_iter();
        let stride = self.w as usize;
        for ry in 0..req.h as i64 {
            let gy = req.y as i64 + ry;
            if gy >= self.h as i64 {
                break;
            }
            let y_in = gy >= 0;
            for rx in 0..req.w as i64 {
                let Some(c) = iter.next() else {
                    return Ok(());
                };
                if y_in {
                    let gx = req.x as i64 + rx;
                    if gx >= 0 && gx < self.w as i64 {
                        self.pixels[(gy as usize) * stride + (gx as usize)] = c.into_storage();
                    }
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.pixels.fill(color.into_storage());
        Ok(())
    }
}

/// Independent full-screen reference renderer.
///
/// Bypasses `DamageSet`, `BandIter`, `BandTarget`, local translation, and `present_sync`,
/// rendering the full scene directly into a full-screen `MemoryScreen` in global coordinates.
pub fn render_reference_fullscreen(
    cmds: &CommandList,
    resources: &impl SceneResources,
    fonts: &FontSet<'_>,
) -> MemoryScreen {
    let mut screen = MemoryScreen::new(cmds.screen.w, cmds.screen.h);
    let _ = screen.clear(cmds.root_bg);
    let full = cmds.screen;
    let stride = screen.w as usize;

    for cmd in cmds.as_slice() {
        match *cmd {
            PaintCmd::Empty => {}
            PaintCmd::Bitmap {
                rect,
                clip,
                angle,
                alpha,
                ..
            }
            | PaintCmd::Surface {
                rect,
                clip,
                angle,
                alpha,
                ..
            } => {
                let Some(source) = cmds.image(*cmd) else {
                    continue;
                };
                let area = rect.intersect(clip).intersect(full);
                let pixels = screen.words_mut();
                for py in 0..area.h {
                    for px in 0..area.w {
                        let x = area.x as i64 + i64::from(px);
                        let y = area.y as i64 + i64::from(py);
                        if let Some(raw) = source.sample(
                            (x - i64::from(rect.x)) as u32,
                            (y - i64::from(rect.y)) as u32,
                            rect.w,
                            rect.h,
                            angle,
                        ) {
                            let index = (y - i64::from(full.y)) as usize * stride
                                + (x - i64::from(full.x)) as usize;
                            let bg = liuli::core::geometry::u16_to_rgb565(pixels[index]);
                            let fg = liuli::core::geometry::u16_to_rgb565(raw);
                            pixels[index] = liuli::core::geometry::rgb565_to_u16(blend_rgb565_a4(
                                fg, bg, alpha,
                            ));
                        }
                    }
                }
            }

            PaintCmd::FillRect { rect, clip, color } => {
                let eff = rect.intersect(clip).intersect(full);
                if !eff.is_empty() {
                    let _ = screen.fill_solid(&eff.to_eg(), color);
                }
            }
            PaintCmd::StrokeRect {
                rect,
                clip,
                color,
                width,
            } => {
                let eff_clip = clip.intersect(full);
                if width > 0 && !eff_clip.is_empty() {
                    let style = PrimitiveStyleBuilder::new()
                        .stroke_color(color)
                        .stroke_width(width as u32)
                        .stroke_alignment(StrokeAlignment::Inside)
                        .build();
                    let mut clipped = screen.clipped(&eff_clip.to_eg());
                    let _ = Rectangle::new(rect.top_left(), rect.size())
                        .into_styled(style)
                        .draw(&mut clipped);
                }
            }
            PaintCmd::FillRoundedRect {
                rect,
                clip,
                color,
                radius,
                border_color,
                border_width,
            } => {
                let eff_clip = clip.intersect(full);
                if !eff_clip.is_empty() {
                    let r = (radius as u32).min(rect.w / 2).min(rect.h / 2);
                    let mut builder = PrimitiveStyleBuilder::new().fill_color(color);
                    if let Some(bc) = border_color
                        && border_width > 0
                    {
                        builder = builder
                            .stroke_color(bc)
                            .stroke_width(border_width as u32)
                            .stroke_alignment(StrokeAlignment::Inside);
                    }
                    let mut clipped = screen.clipped(&eff_clip.to_eg());
                    let _ = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r))
                        .into_styled(builder.build())
                        .draw(&mut clipped);
                }
            }
            PaintCmd::Overlay {
                rect,
                clip,
                color,
                alpha,
            } => {
                let eff = rect.intersect(clip).intersect(full);
                if eff.is_empty() || alpha == 0 {
                    continue;
                }
                for y in eff.y..(eff.y + eff.h as i32) {
                    let row = (y as usize) * stride;
                    for x in eff.x..(eff.x + eff.w as i32) {
                        let bg = Rgb565::from(liuli::core::geometry::RawU16::new(
                            screen.pixels[row + x as usize],
                        ));
                        screen.pixels[row + x as usize] =
                            blend_rgb565_a4(color, bg, alpha).into_storage();
                    }
                }
            }
            PaintCmd::Text {
                pos,
                clip,
                wrap_width,
                text,
                color,
                font: font_id,
                align,
                max_lines,
                ellipsis,
            } => {
                let eff_clip = clip.intersect(full);
                if eff_clip.is_empty() || wrap_width == 0 {
                    continue;
                }
                let width_limit_clip = eff_clip.intersect(Rect::new(
                    eff_clip.x,
                    eff_clip.y,
                    ((pos.x as i64 + wrap_width as i64) - eff_clip.x as i64)
                        .clamp(0, u32::MAX as i64) as u32,
                    eff_clip.h,
                ));
                if width_limit_clip.is_empty() {
                    continue;
                }
                let s = resources.text(text);
                let font = fonts.get(font_id);
                visit_layout_glyphs(
                    font,
                    s,
                    TextLayoutOptions {
                        max_width: wrap_width,
                        max_lines,
                        ellipsis,
                        align,
                    },
                    |pg| {
                        let gx = pos.x + pg.x;
                        let gy = pos.y + pg.y;
                        let gw = pg.glyph.metrics.width;
                        let gh = pg.glyph.metrics.height;
                        if gw == 0 || gh == 0 {
                            return;
                        }
                        let g_rect = Rect::new(gx, gy, gw as u32, gh as u32);
                        let vis = g_rect.intersect(width_limit_clip);
                        for py in vis.y..(vis.y + vis.h as i32) {
                            let sy = (py - gy) as u16;
                            let row = (py as usize) * stride;
                            for px in vis.x..(vis.x + vis.w as i32) {
                                let sx = (px - gx) as u16;
                                let cov = a4_pixel(pg.glyph.data, gw, sx, sy);
                                if cov == 0 {
                                    continue;
                                }
                                let bg = Rgb565::from(liuli::core::geometry::RawU16::new(
                                    screen.pixels[row + px as usize],
                                ));
                                screen.pixels[row + px as usize] =
                                    blend_rgb565_a4(color, bg, cov).into_storage();
                            }
                        }
                    },
                );
            }
            PaintCmd::Icon {
                rect,
                clip,
                kind,
                color,
            } => {
                if rect.w < 3 || rect.h < 3 {
                    continue;
                }
                let eff_clip = clip.intersect(rect).intersect(full);
                if eff_clip.is_empty() {
                    continue;
                }
                let mut clipped = screen.clipped(&eff_clip.to_eg());
                let sx = |u: i32| rect.x + (u * (rect.w as i32)) / 14;
                let sy = |v: i32| rect.y + (v * (rect.h as i32)) / 14;
                let sw = |u: i32| ((u * (rect.w as i32)) / 14).max(1) as u32;
                let stroke = PrimitiveStyle::with_stroke(
                    color,
                    if rect.w >= 10 && rect.h >= 10 { 2 } else { 1 },
                );
                let thin = PrimitiveStyle::with_stroke(color, 1);
                let fill = PrimitiveStyle::with_fill(color);
                let draw_line =
                    |clipped: &mut embedded_graphics::draw_target::Clipped<'_, MemoryScreen>,
                     x0,
                     y0,
                     x1,
                     y1,
                     st| {
                        let _ = Line::new(Point::new(sx(x0), sy(y0)), Point::new(sx(x1), sy(y1)))
                            .into_styled(st)
                            .draw(clipped);
                    };

                match kind {
                    IconKind::None => {}
                    IconKind::ChevronRight => {
                        draw_line(&mut clipped, 4, 3, 10, 7, stroke);
                        draw_line(&mut clipped, 10, 7, 4, 11, stroke);
                    }
                    IconKind::Check => {
                        draw_line(&mut clipped, 2, 7, 6, 11, stroke);
                        draw_line(&mut clipped, 6, 11, 12, 3, stroke);
                    }
                    IconKind::Warning => {
                        let _ = Triangle::new(
                            Point::new(sx(7), sy(1)),
                            Point::new(sx(1), sy(12)),
                            Point::new(sx(12), sy(12)),
                        )
                        .into_styled(thin)
                        .draw(&mut clipped);
                        draw_line(&mut clipped, 7, 5, 7, 8, thin);
                        let _ = Pixel(Point::new(sx(7), sy(10)), color).draw(&mut clipped);
                    }
                    IconKind::Sliders => {
                        draw_line(&mut clipped, 1, 4, 12, 4, thin);
                        draw_line(&mut clipped, 1, 9, 12, 9, thin);
                        let d = sw(4).max(2);
                        let _ = Circle::new(Point::new(sx(3), sy(2)), d)
                            .into_styled(fill)
                            .draw(&mut clipped);
                        let _ = Circle::new(Point::new(sx(7), sy(7)), d)
                            .into_styled(fill)
                            .draw(&mut clipped);
                    }
                    IconKind::Bell => {
                        draw_line(&mut clipped, 5, 2, 9, 2, thin);
                        draw_line(&mut clipped, 4, 3, 4, 8, thin);
                        draw_line(&mut clipped, 10, 3, 10, 8, thin);
                        draw_line(&mut clipped, 4, 3, 5, 2, thin);
                        draw_line(&mut clipped, 9, 2, 10, 3, thin);
                        draw_line(&mut clipped, 4, 8, 2, 10, thin);
                        draw_line(&mut clipped, 10, 8, 12, 10, thin);
                        draw_line(&mut clipped, 2, 10, 12, 10, thin);
                        draw_line(&mut clipped, 6, 12, 8, 12, thin);
                    }
                    IconKind::Moon => {
                        draw_line(&mut clipped, 9, 2, 5, 3, thin);
                        draw_line(&mut clipped, 5, 3, 3, 6, thin);
                        draw_line(&mut clipped, 3, 6, 3, 8, thin);
                        draw_line(&mut clipped, 3, 8, 5, 11, thin);
                        draw_line(&mut clipped, 5, 11, 9, 12, thin);
                        draw_line(&mut clipped, 9, 12, 7, 9, thin);
                        draw_line(&mut clipped, 7, 9, 7, 5, thin);
                        draw_line(&mut clipped, 7, 5, 9, 2, thin);
                    }
                    IconKind::Thermometer => {
                        draw_line(&mut clipped, 7, 2, 7, 9, stroke);
                        let d = sw(5).max(3);
                        let _ = Circle::new(Point::new(sx(5), sy(8)), d)
                            .into_styled(fill)
                            .draw(&mut clipped);
                        draw_line(&mut clipped, 10, 3, 12, 3, thin);
                        draw_line(&mut clipped, 10, 6, 12, 6, thin);
                    }
                    IconKind::Reset => {
                        draw_line(&mut clipped, 7, 2, 11, 4, thin);
                        draw_line(&mut clipped, 11, 4, 11, 9, thin);
                        draw_line(&mut clipped, 11, 9, 7, 11, thin);
                        draw_line(&mut clipped, 7, 11, 3, 9, thin);
                        draw_line(&mut clipped, 3, 9, 3, 5, thin);
                        draw_line(&mut clipped, 3, 5, 6, 2, thin);
                        draw_line(&mut clipped, 3, 2, 3, 5, thin);
                        draw_line(&mut clipped, 3, 5, 6, 5, thin);
                    }
                    IconKind::Gauge => {
                        draw_line(&mut clipped, 2, 10, 3, 6, thin);
                        draw_line(&mut clipped, 3, 6, 7, 3, thin);
                        draw_line(&mut clipped, 7, 3, 11, 6, thin);
                        draw_line(&mut clipped, 11, 6, 12, 10, thin);
                        draw_line(&mut clipped, 7, 10, 11, 5, thin);
                        let d = sw(3).max(2);
                        let _ = Circle::new(Point::new(sx(6), sy(9)), d)
                            .into_styled(fill)
                            .draw(&mut clipped);
                    }
                    IconKind::Waveform => {
                        draw_line(&mut clipped, 1, 7, 4, 7, thin);
                        draw_line(&mut clipped, 4, 7, 5, 3, thin);
                        draw_line(&mut clipped, 5, 3, 7, 11, thin);
                        draw_line(&mut clipped, 7, 11, 9, 4, thin);
                        draw_line(&mut clipped, 9, 4, 10, 7, thin);
                        draw_line(&mut clipped, 10, 7, 13, 7, thin);
                    }
                    IconKind::Diagnostics => {
                        let _ = Rectangle::new(
                            Point::new(sx(1), sy(2)),
                            Size::new(
                                (sx(13) - sx(1)).max(1) as u32,
                                (sy(12) - sy(2)).max(1) as u32,
                            ),
                        )
                        .into_styled(thin)
                        .draw(&mut clipped);
                        draw_line(&mut clipped, 4, 10, 4, 7, stroke);
                        draw_line(&mut clipped, 7, 10, 7, 4, stroke);
                        draw_line(&mut clipped, 10, 10, 10, 6, stroke);
                    }
                }
            }
            PaintCmd::Sparkline {
                rect,
                clip,
                color,
                series,
                min,
                max,
                stroke_width,
            } => {
                let samples = resources.sparkline(series);
                let eff_clip = clip.intersect(rect).intersect(full);
                if eff_clip.is_empty() || rect.is_empty() || samples.is_empty() {
                    continue;
                }
                let map_pt = |idx: usize, total: usize, sample: i16| -> Point {
                    let w_span = (rect.w as i64).saturating_sub(1);
                    let h_span = (rect.h as i64).saturating_sub(1);
                    let px = if total <= 1 || w_span <= 0 {
                        rect.x + (w_span.max(0) / 2) as i32
                    } else {
                        rect.x + ((idx as i64 * w_span) / ((total - 1) as i64)) as i32
                    };
                    let py = if h_span <= 0 {
                        rect.y
                    } else if min >= max {
                        rect.y + (h_span / 2) as i32
                    } else {
                        let span = max as i64 - min as i64;
                        let clamped = (sample as i64).clamp(min as i64, max as i64);
                        let num = (max as i64 - clamped) * h_span + span / 2;
                        rect.y + (num / span) as i32
                    };
                    Point::new(px, py)
                };
                let mut clipped = screen.clipped(&eff_clip.to_eg());
                let n = samples.len();
                if n == 1 {
                    let _ = Pixel(map_pt(0, 1, samples[0]), color).draw(&mut clipped);
                } else {
                    let style = PrimitiveStyle::with_stroke(color, stroke_width.max(1) as u32);
                    let mut prev = map_pt(0, n, samples[0]);
                    for (i, &val) in samples.iter().enumerate().skip(1) {
                        let next = map_pt(i, n, val);
                        let _ = Line::new(prev, next).into_styled(style).draw(&mut clipped);
                        prev = next;
                    }
                }
            }
        }
    }

    screen
}
