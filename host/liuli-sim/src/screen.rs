use core::convert::Infallible;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::{OriginDimensions, Size};
use embedded_graphics::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics::primitives::{
    Circle, Line, Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
    StrokeAlignment, Triangle,
};
use embedded_graphics::{Drawable, Pixel};
use liuli::core::node::RetainedTree;
use liuli::core::paint::{CommandList, PaintCmd, TextLookup};
use liuli::prelude::{IconKind, Point, Rect};
use liuli::text::{Lf4Font, TextLayoutOptions, a4_pixel, blend_rgb565_a4, visit_layout_glyphs};

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
    tree: &RetainedTree,
    font: &Lf4Font<'_>,
) -> MemoryScreen {
    let mut screen = MemoryScreen::new(cmds.screen.w, cmds.screen.h);
    let _ = screen.clear(cmds.root_bg);
    let full = cmds.screen;
    let stride = screen.w as usize;

    for cmd in cmds.as_slice() {
        match *cmd {
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
                ink: _,
                text,
                color,
                max_width,
                max_lines,
                ellipsis,
            } => {
                let eff_clip = clip.intersect(full);
                if eff_clip.is_empty() || max_width == 0 {
                    continue;
                }
                let width_limit_clip = eff_clip.intersect(Rect::new(
                    eff_clip.x,
                    eff_clip.y,
                    ((pos.x as i64 + max_width as i64) - eff_clip.x as i64)
                        .clamp(0, u32::MAX as i64) as u32,
                    eff_clip.h,
                ));
                if width_limit_clip.is_empty() {
                    continue;
                }
                let s = tree.lookup(text);
                visit_layout_glyphs(
                    font,
                    s,
                    TextLayoutOptions {
                        max_width,
                        max_lines,
                        ellipsis,
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
                let eff_clip = clip.intersect(full);
                if eff_clip.is_empty() {
                    continue;
                }
                let mut clipped = screen.clipped(&eff_clip.to_eg());
                let x = rect.x;
                let y = rect.y;
                let w = rect.w as i32;
                let h = rect.h as i32;
                let stroke = PrimitiveStyle::with_stroke(color, 2);
                let thin = PrimitiveStyle::with_stroke(color, 1);
                match kind {
                    IconKind::None => {}
                    IconKind::ChevronRight => {
                        let mid_y = y + h / 2;
                        let _ = Line::new(Point::new(x + 4, y + 3), Point::new(x + w - 4, mid_y))
                            .into_styled(stroke)
                            .draw(&mut clipped);
                        let _ =
                            Line::new(Point::new(x + w - 4, mid_y), Point::new(x + 4, y + h - 3))
                                .into_styled(stroke)
                                .draw(&mut clipped);
                    }
                    IconKind::Check => {
                        let _ = Line::new(
                            Point::new(x + 2, y + h / 2),
                            Point::new(x + w / 2 - 1, y + h - 3),
                        )
                        .into_styled(stroke)
                        .draw(&mut clipped);
                        let _ = Line::new(
                            Point::new(x + w / 2 - 1, y + h - 3),
                            Point::new(x + w - 2, y + 3),
                        )
                        .into_styled(stroke)
                        .draw(&mut clipped);
                    }
                    IconKind::Warning => {
                        let _ = Triangle::new(
                            Point::new(x + w / 2, y + 1),
                            Point::new(x + 1, y + h - 2),
                            Point::new(x + w - 2, y + h - 2),
                        )
                        .into_styled(thin)
                        .draw(&mut clipped);
                    }
                    IconKind::Sliders => {
                        let _ = Line::new(Point::new(x + 1, y + 4), Point::new(x + w - 2, y + 4))
                            .into_styled(thin)
                            .draw(&mut clipped);
                        let _ = Line::new(
                            Point::new(x + 1, y + h - 5),
                            Point::new(x + w - 2, y + h - 5),
                        )
                        .into_styled(thin)
                        .draw(&mut clipped);
                        let _ = Circle::new(Point::new(x + 4, y + 2), 5)
                            .into_styled(PrimitiveStyle::with_fill(color))
                            .draw(&mut clipped);
                        let _ = Circle::new(Point::new(x + w - 9, y + h - 7), 5)
                            .into_styled(PrimitiveStyle::with_fill(color))
                            .draw(&mut clipped);
                    }
                    IconKind::Bell | IconKind::Moon | IconKind::Thermometer | IconKind::Reset => {
                        let _ = Circle::new(Point::new(x + 2, y + 2), (w.min(h) - 4).max(4) as u32)
                            .into_styled(thin)
                            .draw(&mut clipped);
                    }
                }
            }
        }
    }

    screen
}
