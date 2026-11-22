use core::convert::Infallible;

use embedded_graphics::Drawable;
use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::Size;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::{
    Circle, Line, Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
    StrokeAlignment, Triangle,
};
use liuli_core::geometry::{Point, Rect, rgb565_to_u16, u16_to_rgb565};
use liuli_core::paint::{CommandList, PaintCmd, TextLookup};
use liuli_core::view::IconKind;
use liuli_text::{Lf4Font, TextLayoutOptions, a4_pixel, blend_rgb565_a4, visit_layout_glyphs};

use crate::band::BandTarget;

/// Draws any `embedded-graphics` `Drawable<Color = Rgb565>` into the band using
/// global coordinates clipped against `global_clip` and the band's own rectangle.
pub fn draw_eg<D>(
    target: &mut BandTarget<'_>,
    global_clip: Rect,
    drawable: &D,
) -> Result<D::Output, Infallible>
where
    D: Drawable<Color = Rgb565>,
{
    let band_rect = target.band_rect();
    let eff_clip = global_clip.intersect(band_rect);
    if eff_clip.is_empty() {
        // Note: EG Drawable has arbitrary Output; caller skips empty eff_clip or uses Output = ()
    }
    let offset = Point::new(-band_rect.x, -band_rect.y);
    let mut translated = target.translated(offset);
    let mut clipped = translated.clipped(&eff_clip.to_eg());
    drawable.draw(&mut clipped)
}

pub fn render_band<'f, L>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    text_lookup: &L,
    font: &Lf4Font<'f>,
) -> u32
where
    L: TextLookup + ?Sized,
{
    let _ = target.clear(cmds.root_bg);
    let band_rect = target.band_rect();
    let mut replayed = 0u32;

    for cmd in cmds.as_slice() {
        if !cmd.bounds().intersects(band_rect) {
            continue;
        }
        replayed += 1;

        match *cmd {
            PaintCmd::FillRect { rect, clip, color } => {
                let eff = rect.intersect(clip).intersect(band_rect);
                if !eff.is_empty() {
                    let local = eff.translate(-band_rect.x, -band_rect.y);
                    let _ = target.fill_solid(&local.to_eg(), color);
                }
            }
            PaintCmd::StrokeRect {
                rect,
                clip,
                color,
                width,
            } => {
                if width > 0 {
                    let style = PrimitiveStyleBuilder::new()
                        .stroke_color(color)
                        .stroke_width(width as u32)
                        .stroke_alignment(StrokeAlignment::Inside)
                        .build();
                    let shape = Rectangle::new(rect.top_left(), rect.size()).into_styled(style);
                    let _ = draw_eg(target, clip, &shape);
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
                let style = builder.build();
                let shape = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r))
                    .into_styled(style);
                let _ = draw_eg(target, clip, &shape);
            }
            PaintCmd::Overlay {
                rect,
                clip,
                color,
                alpha,
            } => {
                let eff = rect.intersect(clip).intersect(band_rect);
                if eff.is_empty() || alpha == 0 {
                    continue;
                }
                let stride = target.width() as usize;
                let words = target.words_mut();
                let x0 = (eff.x - band_rect.x) as usize;
                let x1 = x0 + eff.w as usize;
                let y0 = (eff.y - band_rect.y) as usize;
                let y1 = y0 + eff.h as usize;

                if alpha >= 15 {
                    let raw = rgb565_to_u16(color);
                    for ly in y0..y1 {
                        words[ly * stride + x0..ly * stride + x1].fill(raw);
                    }
                } else {
                    for ly in y0..y1 {
                        let row = &mut words[ly * stride + x0..ly * stride + x1];
                        for w in row {
                            let bg = u16_to_rgb565(*w);
                            *w = rgb565_to_u16(blend_rgb565_a4(color, bg, alpha));
                        }
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
                let eff_clip = clip.intersect(band_rect);
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
                let s = text_lookup.lookup(text);
                let stride = target.width() as usize;
                let words = target.words_mut();

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
                        if vis.is_empty() {
                            return;
                        }
                        for py in vis.y..(vis.y + vis.h as i32) {
                            let src_y = (py - gy) as u16;
                            let ly = (py - band_rect.y) as usize;
                            let row_off = ly * stride;
                            for px in vis.x..(vis.x + vis.w as i32) {
                                let src_x = (px - gx) as u16;
                                let cov = a4_pixel(pg.glyph.data, gw, src_x, src_y);
                                if cov == 0 {
                                    continue;
                                }
                                let lx = (px - band_rect.x) as usize;
                                let bg = u16_to_rgb565(words[row_off + lx]);
                                words[row_off + lx] =
                                    rgb565_to_u16(blend_rgb565_a4(color, bg, cov));
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
                draw_icon(target, rect, clip, kind, color);
            }
        }
    }

    replayed
}

fn draw_icon(target: &mut BandTarget<'_>, rect: Rect, clip: Rect, kind: IconKind, color: Rgb565) {
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
            let l1 = Line::new(Point::new(x + 4, y + 3), Point::new(x + w - 4, mid_y))
                .into_styled(stroke);
            let l2 = Line::new(Point::new(x + w - 4, mid_y), Point::new(x + 4, y + h - 3))
                .into_styled(stroke);
            let _ = draw_eg(target, clip, &l1);
            let _ = draw_eg(target, clip, &l2);
        }
        IconKind::Check => {
            let l1 = Line::new(
                Point::new(x + 2, y + h / 2),
                Point::new(x + w / 2 - 1, y + h - 3),
            )
            .into_styled(stroke);
            let l2 = Line::new(
                Point::new(x + w / 2 - 1, y + h - 3),
                Point::new(x + w - 2, y + 3),
            )
            .into_styled(stroke);
            let _ = draw_eg(target, clip, &l1);
            let _ = draw_eg(target, clip, &l2);
        }
        IconKind::Warning => {
            let tri = Triangle::new(
                Point::new(x + w / 2, y + 1),
                Point::new(x + 1, y + h - 2),
                Point::new(x + w - 2, y + h - 2),
            )
            .into_styled(thin);
            let _ = draw_eg(target, clip, &tri);
        }
        IconKind::Sliders => {
            let l1 =
                Line::new(Point::new(x + 1, y + 4), Point::new(x + w - 2, y + 4)).into_styled(thin);
            let l2 = Line::new(
                Point::new(x + 1, y + h - 5),
                Point::new(x + w - 2, y + h - 5),
            )
            .into_styled(thin);
            let c1 = Circle::new(Point::new(x + 4, y + 2), 5)
                .into_styled(PrimitiveStyle::with_fill(color));
            let c2 = Circle::new(Point::new(x + w - 9, y + h - 7), 5)
                .into_styled(PrimitiveStyle::with_fill(color));
            let _ = draw_eg(target, clip, &l1);
            let _ = draw_eg(target, clip, &l2);
            let _ = draw_eg(target, clip, &c1);
            let _ = draw_eg(target, clip, &c2);
        }
        IconKind::Bell | IconKind::Moon | IconKind::Thermometer | IconKind::Reset => {
            let c = Circle::new(Point::new(x + 2, y + 2), (w.min(h) - 4).max(4) as u32)
                .into_styled(thin);
            let _ = draw_eg(target, clip, &c);
        }
    }
}
