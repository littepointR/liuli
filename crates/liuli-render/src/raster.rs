use core::convert::Infallible;

use embedded_graphics::Drawable;
use embedded_graphics::Pixel;
use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::Size;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::{
    Circle, Line, Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
    StrokeAlignment, Triangle,
};
use liuli_core::geometry::{Point, Rect, rgb565_to_u16};
use liuli_core::paint::{CommandList, PaintCmd, SceneResources};
use liuli_core::view::IconKind;
use liuli_text::{FontSet, TextLayoutOptions, a4_pixel, visit_layout_glyphs};

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
    let offset = Point::new(-band_rect.x, -band_rect.y);
    let mut translated = target.translated(offset);
    let mut clipped = translated.clipped(&eff_clip.to_eg());
    drawable.draw(&mut clipped)
}

pub fn render_band<'f, R>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &FontSet<'f>,
) -> u32
where
    R: SceneResources + ?Sized,
{
    render_band_with_blend(target, cmds, resources, fonts, blend_solid)
}

/// Scalar RGB565 reference used by hosts and unaccelerated boards.
pub fn blend_solid(row: &mut [u16], color: Rgb565, alpha: u8) {
    let a = alpha.min(15);
    if a == 0 {
        return;
    }
    let fg = rgb565_to_u16(color);
    if a == 15 {
        row.fill(fg);
        return;
    }
    let a32 = u32::from(a);
    let inv = 15 - a32;
    let fg32 = u32::from(fg);
    let fg_r_a = (fg32 >> 11) * a32 + 7;
    let fg_g_a = ((fg32 >> 5) & 63) * a32 + 7;
    let fg_b_a = (fg32 & 31) * a32 + 7;
    for word in row {
        let bg = u32::from(*word);
        let r = ((fg_r_a + (bg >> 11) * inv) * 4370) >> 16;
        let g = ((fg_g_a + ((bg >> 5) & 63) * inv) * 4370) >> 16;
        let b = ((fg_b_a + (bg & 31) * inv) * 4370) >> 16;
        *word = ((r << 11) | (g << 5) | b) as u16;
    }
}

pub fn render_band_with_blend<'f, R>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &FontSet<'f>,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
) -> u32
where
    R: SceneResources + ?Sized,
{
    match render_band_inner(
        target,
        cmds,
        resources,
        fonts,
        blend,
        &mut liuli_text::storage::InlinePixels,
    ) {
        Ok(count) => count,
        Err(never) => match never {},
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GlyphReadError<E> {
    Source(E),
    Short { needed: usize, actual: usize },
}

struct CheckedGlyphs<'p, G>(&'p mut G);
impl<G: liuli_text::storage::GlyphPixels> liuli_text::storage::GlyphPixels
    for CheckedGlyphs<'_, G>
{
    type Error = GlyphReadError<G::Error>;
    fn begin_band(&mut self, band: Rect) {
        self.0.begin_band(band);
    }
    fn pixels<'a>(
        &'a mut self,
        font: liuli_core::view::FontId,
        glyph: liuli_text::GlyphBitmap<'a>,
    ) -> Result<&'a [u8], Self::Error> {
        let needed =
            usize::from(glyph.metrics.width).div_ceil(2) * usize::from(glyph.metrics.height);
        let bytes = self.0.pixels(font, glyph).map_err(GlyphReadError::Source)?;
        bytes.get(..needed).ok_or(GlyphReadError::Short {
            needed,
            actual: bytes.len(),
        })
    }
}
pub fn render_band_with_pixels<'f, R, G: liuli_text::storage::GlyphPixels>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &FontSet<'f>,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
    glyphs: &mut G,
) -> Result<u32, GlyphReadError<G::Error>>
where
    R: SceneResources + ?Sized,
{
    render_band_inner(
        target,
        cmds,
        resources,
        fonts,
        blend,
        &mut CheckedGlyphs(glyphs),
    )
}

fn render_band_inner<'f, R, G: liuli_text::storage::GlyphPixels>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &FontSet<'f>,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
    glyphs: &mut G,
) -> Result<u32, G::Error>
where
    R: SceneResources + ?Sized,
{
    render_range(
        target,
        cmds,
        resources,
        fonts,
        blend,
        glyphs,
        0..cmds.count,
        cmds.root_bg,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_range<'f, R: SceneResources + ?Sized, G: liuli_text::storage::GlyphPixels>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &FontSet<'f>,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
    glyphs: &mut G,
    range: core::ops::Range<usize>,
    clear: Rgb565,
) -> Result<u32, G::Error> {
    let _ = target.clear(clear);
    let band_rect = target.band_rect();
    glyphs.begin_band(band_rect);
    let mut replayed = 0u32;

    let mut cursor = range.start;
    while cursor < range.end {
        let index = cursor;
        let cmd = &cmds.as_slice()[index];
        cursor += 1;
        #[cfg(feature = "layers")]
        if let PaintCmd::Group { end, .. } = *cmd {
            cursor = usize::from(end).clamp(cursor, range.end);
        }
        if !cmd.bounds().intersects(band_rect) {
            continue;
        }
        replayed += 1;

        match *cmd {
            PaintCmd::Empty => {}
            PaintCmd::Shadow {
                rect,
                clip,
                shadow,
                projection,
                radius,
            } => {
                crate::effects::draw_shadow(target, rect, clip, shadow, projection, radius);
            }
            PaintCmd::Group { .. } => {
                #[cfg(feature = "layers")]
                if let PaintCmd::Group {
                    rect,
                    index: layer_id,
                    ..
                } = *cmd
                {
                    let cache =
                        &cmds.layers.as_ref().expect("emitted layer")[usize::from(layer_id)];
                    if cache.style.opacity == 0 || matches!(cache.style.projection.0, 64 | 192) {
                        continue;
                    }
                    let mut group = cache.group.borrow_mut();
                    if !group.ready {
                        let local = index + 1..cursor;
                        let liuli_core::layers::GroupPixels { color, alpha, .. } = &mut *group;
                        let mut tile = BandTarget::new_with_alpha::<()>(color, alpha, cache.source)
                            .expect("reserved group");
                        replayed += render_range(
                            &mut tile,
                            cmds,
                            resources,
                            fonts,
                            blend,
                            glyphs,
                            local,
                            Rgb565::new(0, 0, 0),
                        )?;
                        group.ready = true;
                    }
                    let expected_tint = (cache.style.tint_alpha > 0)
                        .then_some((cache.style.tint_color, cache.style.tint_alpha));
                    let mut backdrop = cache.backdrop.borrow_mut();
                    if cache.style.backdrop_radius > 0
                        && (backdrop.clear != Some(clear) || backdrop.tint != expected_tint)
                    {
                        backdrop.clear = None;
                        backdrop.tint = None;
                        let mut tile = BandTarget::new::<()>(&mut backdrop.pixels, cache.capture)
                            .expect("reserved backdrop");
                        replayed += render_range(
                            &mut tile,
                            cmds,
                            resources,
                            fonts,
                            blend,
                            glyphs,
                            range.start..cache.backdrop_end.max(range.start),
                            clear,
                        )?;
                        crate::effects::blur(
                            &mut backdrop,
                            cache.capture,
                            cache.style.backdrop_radius,
                        );
                        if let Some((tint_color, tint_alpha)) = expected_tint {
                            blend(&mut backdrop.pixels, tint_color, tint_alpha);
                        }
                        backdrop.clear = Some(clear);
                        backdrop.tint = expected_tint;
                    }
                    glyphs.begin_band(band_rect);
                    crate::effects::composite(target, rect, cache, &group, &backdrop);
                }
            }
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
                let area = rect.intersect(clip).intersect(band_rect);
                let stride = target.width() as usize;
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
                            let index = (y - i64::from(band_rect.y)) as usize * stride
                                + (x - i64::from(band_rect.x)) as usize;
                            target.blend_pixel_a4(index, raw, alpha);
                        }
                    }
                }
            }

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
            PaintCmd::StrokeRoundedRect {
                rect,
                clip,
                color,
                width,
                radius,
            } => {
                if width > 0 && !rect.is_empty() {
                    let r = (radius as u32).min(rect.w / 2).min(rect.h / 2);
                    let style = PrimitiveStyleBuilder::new()
                        .stroke_color(color)
                        .stroke_width(width as u32)
                        .stroke_alignment(StrokeAlignment::Inside)
                        .build();
                    let shape = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r))
                        .into_styled(style);
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
                let x0 = (eff.x - band_rect.x) as usize;
                let x1 = x0 + eff.w as usize;
                let y0 = (eff.y - band_rect.y) as usize;
                let y1 = y0 + eff.h as usize;

                if alpha >= 15 {
                    let raw = rgb565_to_u16(color);
                    let has_alpha = target.has_alpha();
                    for ly in y0..y1 {
                        let row_start = ly * stride;
                        target.words_mut()[row_start + x0..row_start + x1].fill(raw);
                        if has_alpha {
                            target.set_opaque_span(row_start + x0, x1 - x0);
                        }
                    }
                } else if target.has_alpha() {
                    let raw = rgb565_to_u16(color);
                    for ly in y0..y1 {
                        let row_start = ly * stride;
                        for lx in x0..x1 {
                            target.blend_pixel_a4(row_start + lx, raw, alpha);
                        }
                    }
                } else {
                    let words = target.words_mut();
                    for ly in y0..y1 {
                        let row = &mut words[ly * stride + x0..ly * stride + x1];
                        blend(row, color, alpha);
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
                let eff_clip = clip.intersect(band_rect);
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
                let stride = target.width() as usize;
                let fg_raw = rgb565_to_u16(color);

                let mut resource_error = None;
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
                        if resource_error.is_some() {
                            return;
                        }
                        let gx = pos.x.saturating_add(pg.x);
                        let gy = pos.y.saturating_add(pg.y);
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
                        let data = match glyphs.pixels(font_id, pg.glyph) {
                            Ok(bytes) => bytes,
                            Err(e) => {
                                resource_error = Some(e);
                                return;
                            }
                        };
                        for py in vis.y..(vis.y + vis.h as i32) {
                            let src_y = (py - gy) as u16;
                            let ly = (py - band_rect.y) as usize;
                            let row_off = ly * stride;
                            for px in vis.x..(vis.x + vis.w as i32) {
                                let src_x = (px - gx) as u16;
                                let cov = a4_pixel(data, gw, src_x, src_y);
                                if cov == 0 {
                                    continue;
                                }
                                let lx = (px - band_rect.x) as usize;
                                target.blend_pixel_a4(row_off + lx, fg_raw, cov);
                            }
                        }
                    },
                );
                if let Some(e) = resource_error {
                    return Err(e);
                }
            }
            PaintCmd::Icon {
                rect,
                clip,
                kind,
                color,
            } => {
                draw_icon(target, rect, clip, kind, color);
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
                draw_sparkline(
                    target,
                    rect,
                    clip,
                    color,
                    resources.sparkline(series),
                    min,
                    max,
                    stroke_width,
                );
            }
        }
    }

    Ok(replayed)
}

#[inline]
pub fn map_sparkline_point(
    rect: Rect,
    idx: usize,
    total: usize,
    sample: i16,
    min: i16,
    max: i16,
) -> Point {
    let w_span = (rect.w as i64).saturating_sub(1);
    let h_span = (rect.h as i64).saturating_sub(1);
    let x = if total <= 1 || w_span <= 0 {
        rect.x + (w_span.max(0) / 2) as i32
    } else {
        rect.x + ((idx as i64 * w_span) / ((total - 1) as i64)) as i32
    };
    let y = if h_span <= 0 {
        rect.y
    } else if min >= max {
        rect.y + (h_span / 2) as i32
    } else {
        let span = max as i64 - min as i64;
        let clamped = (sample as i64).clamp(min as i64, max as i64);
        let num = (max as i64 - clamped) * h_span + span / 2;
        rect.y + (num / span) as i32
    };
    Point::new(x, y)
}

#[allow(clippy::too_many_arguments)]
fn draw_sparkline(
    target: &mut BandTarget<'_>,
    rect: Rect,
    clip: Rect,
    color: Rgb565,
    samples: &[i16],
    min: i16,
    max: i16,
    stroke_width: u8,
) {
    let eff_clip = clip.intersect(rect);
    if eff_clip.is_empty() || rect.is_empty() || samples.is_empty() {
        return;
    }
    let band_clip = eff_clip.intersect(target.band_rect());
    if band_clip.is_empty() {
        return;
    }
    let n = samples.len();
    if n == 1 {
        let p = map_sparkline_point(rect, 0, 1, samples[0], min, max);
        let _ = draw_eg(target, eff_clip, &Pixel(p, color));
        return;
    }
    let style = PrimitiveStyle::with_stroke(color, stroke_width.max(1) as u32);
    let mut prev = map_sparkline_point(rect, 0, n, samples[0], min, max);
    for (i, &val) in samples.iter().enumerate().skip(1) {
        let next = map_sparkline_point(rect, i, n, val, min, max);
        let seg = Line::new(prev, next).into_styled(style);
        let _ = draw_eg(target, eff_clip, &seg);
        prev = next;
    }
}

#[inline]
fn icon_sx(rect: Rect, u: i32) -> i32 {
    rect.x + (u * (rect.w as i32)) / 14
}

#[inline]
fn icon_sy(rect: Rect, v: i32) -> i32 {
    rect.y + (v * (rect.h as i32)) / 14
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn draw_icon_line(
    target: &mut BandTarget<'_>,
    rect: Rect,
    eff_clip: Rect,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    style: PrimitiveStyle<Rgb565>,
) {
    let l = Line::new(
        Point::new(icon_sx(rect, x0), icon_sy(rect, y0)),
        Point::new(icon_sx(rect, x1), icon_sy(rect, y1)),
    )
    .into_styled(style);
    let _ = draw_eg(target, eff_clip, &l);
}

fn draw_icon(target: &mut BandTarget<'_>, rect: Rect, clip: Rect, kind: IconKind, color: Rgb565) {
    if rect.w < 3 || rect.h < 3 {
        return;
    }
    let eff_clip = clip.intersect(rect);
    if eff_clip.is_empty() {
        return;
    }
    let sw = |u: i32| ((u * (rect.w as i32)) / 14).max(1) as u32;
    let stroke =
        PrimitiveStyle::with_stroke(color, if rect.w >= 10 && rect.h >= 10 { 2 } else { 1 });
    let thin = PrimitiveStyle::with_stroke(color, 1);
    let fill = PrimitiveStyle::with_fill(color);

    match kind {
        IconKind::None => {}
        IconKind::ChevronRight => {
            draw_icon_line(target, rect, eff_clip, 4, 3, 10, 7, stroke);
            draw_icon_line(target, rect, eff_clip, 10, 7, 4, 11, stroke);
        }
        IconKind::Check => {
            draw_icon_line(target, rect, eff_clip, 2, 7, 6, 11, stroke);
            draw_icon_line(target, rect, eff_clip, 6, 11, 12, 3, stroke);
        }
        IconKind::Warning => {
            let tri = Triangle::new(
                Point::new(icon_sx(rect, 7), icon_sy(rect, 1)),
                Point::new(icon_sx(rect, 1), icon_sy(rect, 12)),
                Point::new(icon_sx(rect, 12), icon_sy(rect, 12)),
            )
            .into_styled(thin);
            let _ = draw_eg(target, eff_clip, &tri);
            draw_icon_line(target, rect, eff_clip, 7, 5, 7, 8, thin);
            let _ = draw_eg(
                target,
                eff_clip,
                &Pixel(Point::new(icon_sx(rect, 7), icon_sy(rect, 10)), color),
            );
        }
        IconKind::Sliders => {
            draw_icon_line(target, rect, eff_clip, 1, 4, 12, 4, thin);
            draw_icon_line(target, rect, eff_clip, 1, 9, 12, 9, thin);
            let d = sw(4).max(2);
            let c1 =
                Circle::new(Point::new(icon_sx(rect, 3), icon_sy(rect, 2)), d).into_styled(fill);
            let c2 =
                Circle::new(Point::new(icon_sx(rect, 7), icon_sy(rect, 7)), d).into_styled(fill);
            let _ = draw_eg(target, eff_clip, &c1);
            let _ = draw_eg(target, eff_clip, &c2);
        }
        IconKind::Bell => {
            draw_icon_line(target, rect, eff_clip, 5, 2, 9, 2, thin);
            draw_icon_line(target, rect, eff_clip, 4, 3, 4, 8, thin);
            draw_icon_line(target, rect, eff_clip, 10, 3, 10, 8, thin);
            draw_icon_line(target, rect, eff_clip, 4, 3, 5, 2, thin);
            draw_icon_line(target, rect, eff_clip, 9, 2, 10, 3, thin);
            draw_icon_line(target, rect, eff_clip, 4, 8, 2, 10, thin);
            draw_icon_line(target, rect, eff_clip, 10, 8, 12, 10, thin);
            draw_icon_line(target, rect, eff_clip, 2, 10, 12, 10, thin);
            draw_icon_line(target, rect, eff_clip, 6, 12, 8, 12, thin);
        }
        IconKind::Moon => {
            draw_icon_line(target, rect, eff_clip, 9, 2, 5, 3, thin);
            draw_icon_line(target, rect, eff_clip, 5, 3, 3, 6, thin);
            draw_icon_line(target, rect, eff_clip, 3, 6, 3, 8, thin);
            draw_icon_line(target, rect, eff_clip, 3, 8, 5, 11, thin);
            draw_icon_line(target, rect, eff_clip, 5, 11, 9, 12, thin);
            draw_icon_line(target, rect, eff_clip, 9, 12, 7, 9, thin);
            draw_icon_line(target, rect, eff_clip, 7, 9, 7, 5, thin);
            draw_icon_line(target, rect, eff_clip, 7, 5, 9, 2, thin);
        }
        IconKind::Thermometer => {
            draw_icon_line(target, rect, eff_clip, 7, 2, 7, 9, stroke);
            let d = sw(5).max(3);
            let bulb =
                Circle::new(Point::new(icon_sx(rect, 5), icon_sy(rect, 8)), d).into_styled(fill);
            let _ = draw_eg(target, eff_clip, &bulb);
            draw_icon_line(target, rect, eff_clip, 10, 3, 12, 3, thin);
            draw_icon_line(target, rect, eff_clip, 10, 6, 12, 6, thin);
        }
        IconKind::Reset => {
            draw_icon_line(target, rect, eff_clip, 7, 2, 11, 4, thin);
            draw_icon_line(target, rect, eff_clip, 11, 4, 11, 9, thin);
            draw_icon_line(target, rect, eff_clip, 11, 9, 7, 11, thin);
            draw_icon_line(target, rect, eff_clip, 7, 11, 3, 9, thin);
            draw_icon_line(target, rect, eff_clip, 3, 9, 3, 5, thin);
            draw_icon_line(target, rect, eff_clip, 3, 5, 6, 2, thin);
            draw_icon_line(target, rect, eff_clip, 3, 2, 3, 5, thin);
            draw_icon_line(target, rect, eff_clip, 3, 5, 6, 5, thin);
        }
        IconKind::Gauge => {
            draw_icon_line(target, rect, eff_clip, 2, 10, 3, 6, thin);
            draw_icon_line(target, rect, eff_clip, 3, 6, 7, 3, thin);
            draw_icon_line(target, rect, eff_clip, 7, 3, 11, 6, thin);
            draw_icon_line(target, rect, eff_clip, 11, 6, 12, 10, thin);
            draw_icon_line(target, rect, eff_clip, 7, 10, 11, 5, thin);
            let d = sw(3).max(2);
            let hub =
                Circle::new(Point::new(icon_sx(rect, 6), icon_sy(rect, 9)), d).into_styled(fill);
            let _ = draw_eg(target, eff_clip, &hub);
        }
        IconKind::Waveform => {
            draw_icon_line(target, rect, eff_clip, 1, 7, 4, 7, thin);
            draw_icon_line(target, rect, eff_clip, 4, 7, 5, 3, thin);
            draw_icon_line(target, rect, eff_clip, 5, 3, 7, 11, thin);
            draw_icon_line(target, rect, eff_clip, 7, 11, 9, 4, thin);
            draw_icon_line(target, rect, eff_clip, 9, 4, 10, 7, thin);
            draw_icon_line(target, rect, eff_clip, 10, 7, 13, 7, thin);
        }
        IconKind::Diagnostics => {
            let box_rect = Rectangle::new(
                Point::new(icon_sx(rect, 1), icon_sy(rect, 2)),
                Size::new(
                    (icon_sx(rect, 13) - icon_sx(rect, 1)).max(1) as u32,
                    (icon_sy(rect, 12) - icon_sy(rect, 2)).max(1) as u32,
                ),
            )
            .into_styled(thin);
            let _ = draw_eg(target, eff_clip, &box_rect);
            draw_icon_line(target, rect, eff_clip, 4, 10, 4, 7, stroke);
            draw_icon_line(target, rect, eff_clip, 7, 10, 7, 4, stroke);
            draw_icon_line(target, rect, eff_clip, 10, 10, 10, 6, stroke);
        }
    }
}
