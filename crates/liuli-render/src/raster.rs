use core::convert::Infallible;

use embedded_graphics::Drawable;
use embedded_graphics::Pixel;
use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::Size;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::{
    Circle, ContainsPoint, Line, Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle,
    RoundedRectangle, StrokeAlignment, Triangle,
};
use liuli_core::geometry::{Point, Rect, rgb565_to_u16};
use liuli_core::paint::{CommandList, PaintCmd, SceneResources};
use liuli_core::view::{FontId, IconKind, SubpixelOrder, TextAlign, TextGamma, TextStyleFlags};
use liuli_text::{
    FontProvider, FontSet, GlyphEncoding, GlyphFormat, LineDecor, ProviderLayoutStyle,
    TextLayoutOptions, TextLayoutStyle, a2_pixel, blend_rgb565_a4_u16, blend_rgb565_a8_u16,
    blend_rgb565_lcd_a4_u16, decode_rle_rows, gamma_remap_a4, gamma_remap_a8, lcd_rgb_pixel,
    visit_layout_glyphs_provider, visit_layout_glyphs_styled,
};

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
    let mut accel = crate::accel::SoftAccel;
    match render_range(
        target,
        cmds,
        resources,
        blend,
        &mut accel,
        &mut LegacyTextSource {
            fonts,
            glyphs: &mut liuli_text::storage::InlinePixels,
        },
        0..cmds.count,
        cmds.root_bg,
    ) {
        Ok(count) => count,
        Err(err) => match err {
            GlyphReadError::Source(never) => match never {},
            GlyphReadError::Short { .. } => 0,
        },
    }
}

/// Renders one band delegating solid fills and row operations to hardware [`crate::accel::DrawAccel`].
pub fn render_band_with_accel<'f, R, A>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &FontSet<'f>,
    accel: &mut A,
) -> u32
where
    R: SceneResources + ?Sized,
    A: crate::accel::DrawAccel,
{
    match render_range(
        target,
        cmds,
        resources,
        blend_solid,
        accel,
        &mut LegacyTextSource {
            fonts,
            glyphs: &mut liuli_text::storage::InlinePixels,
        },
        0..cmds.count,
        cmds.root_bg,
    ) {
        Ok(count) => count,
        Err(err) => match err {
            GlyphReadError::Source(never) => match never {},
            GlyphReadError::Short { .. } => 0,
        },
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GlyphReadError<E> {
    Source(E),
    Short { needed: usize, actual: usize },
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
    let mut accel = crate::accel::SoftAccel;
    render_range(
        target,
        cmds,
        resources,
        blend,
        &mut accel,
        &mut LegacyTextSource { fonts, glyphs },
        0..cmds.count,
        cmds.root_bg,
    )
}

pub fn render_band_with_provider<R, F: FontProvider>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    fonts: &F,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
) -> Result<u32, F::Error>
where
    R: SceneResources + ?Sized,
{
    let mut accel = crate::accel::SoftAccel;
    render_range(
        target,
        cmds,
        resources,
        blend,
        &mut accel,
        &mut ProviderTextSource { fonts },
        0..cmds.count,
        cmds.root_bg,
    )
}

trait BandTextSource {
    type Error;
    fn begin_band(&mut self, band: Rect);
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        target: &mut BandTarget<'_>,
        band_rect: Rect,
        stride: usize,
        width_limit_clip: Rect,
        pos: Point,
        s: &str,
        fg_raw: u16,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font_id: FontId,
        align: TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
        vis_y: Option<(i32, i32)>,
        row_buf: &mut [u8],
    ) -> Result<(), Self::Error>;
}

struct LegacyTextSource<'a, 'f, G> {
    fonts: &'a FontSet<'f>,
    glyphs: &'a mut G,
}

impl<G: liuli_text::storage::GlyphPixels> BandTextSource for LegacyTextSource<'_, '_, G> {
    type Error = GlyphReadError<G::Error>;

    fn begin_band(&mut self, band: Rect) {
        self.glyphs.begin_band(band);
    }

    fn draw_text(
        &mut self,
        target: &mut BandTarget<'_>,
        band_rect: Rect,
        stride: usize,
        width_limit_clip: Rect,
        pos: Point,
        s: &str,
        fg_raw: u16,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font_id: FontId,
        align: TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
        vis_y: Option<(i32, i32)>,
        row_buf: &mut [u8],
    ) -> Result<(), Self::Error> {
        let font = self.fonts.get(font_id);
        let fallback_face = self.fonts.fallback();
        let draw_decor = style.underline() || style.strikethrough();
        let perceptual = style.gamma() == TextGamma::Perceptual;
        let subpixel = style.subpixel();
        let mut decors = [LineDecor {
            line: 0,
            x_start: 0,
            x_end: 0,
            baseline_y: 0,
            ascent: 0,
        }; 32];
        let mut decor_count = 0usize;
        let mut resource_error: Option<Self::Error> = None;

        visit_layout_glyphs_styled(
            font,
            s,
            TextLayoutOptions {
                max_width: wrap_width,
                max_lines,
                ellipsis,
                align,
            },
            TextLayoutStyle {
                wrap: style.wrap(),
                letter_spacing,
                line_spacing,
                fallback: fallback_face,
                vis_y,
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
                let data = if pg.from_fallback {
                    pg.glyph.data
                } else {
                    match self.glyphs.pixels(font_id, pg.glyph) {
                        Ok(bytes) => {
                            let needed = if pg.encoding == GlyphEncoding::Rle {
                                1
                            } else {
                                pg.format.row_bytes(gw)
                                    * usize::from(gh)
                                    * usize::from(font.subpixel_phases())
                            };
                            if bytes.len() < needed {
                                resource_error = Some(GlyphReadError::Short {
                                    needed,
                                    actual: bytes.len(),
                                });
                                return;
                            }
                            bytes
                        }
                        Err(e) => {
                            resource_error = Some(GlyphReadError::Source(e));
                            return;
                        }
                    }
                };
                blit_decoded_glyph(
                    target,
                    band_rect,
                    stride,
                    vis,
                    gx,
                    gy,
                    gw,
                    gh,
                    pg.phase,
                    pg.format,
                    pg.encoding,
                    subpixel,
                    fg_raw,
                    perceptual,
                    row_buf,
                    data,
                );
            },
            |dec| {
                if draw_decor && decor_count < decors.len() {
                    decors[decor_count] = dec;
                    decor_count += 1;
                }
            },
        );
        if let Some(e) = resource_error {
            return Err(e);
        }
        for &dec in &decors[..decor_count] {
            draw_line_decor(
                target,
                pos,
                width_limit_clip,
                band_rect,
                stride,
                fg_raw,
                style,
                dec,
            );
        }
        Ok(())
    }
}

struct ProviderTextSource<'a, F> {
    fonts: &'a F,
}

impl<F: FontProvider> BandTextSource for ProviderTextSource<'_, F> {
    type Error = F::Error;

    fn begin_band(&mut self, band: Rect) {
        self.fonts.begin_band(band);
    }

    fn draw_text(
        &mut self,
        target: &mut BandTarget<'_>,
        band_rect: Rect,
        stride: usize,
        width_limit_clip: Rect,
        pos: Point,
        s: &str,
        fg_raw: u16,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font_id: FontId,
        align: TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
        vis_y: Option<(i32, i32)>,
        row_buf: &mut [u8],
    ) -> Result<(), Self::Error> {
        let draw_decor = style.underline() || style.strikethrough();
        let perceptual = style.gamma() == TextGamma::Perceptual;
        let subpixel = style.subpixel();
        let mut decors = [LineDecor {
            line: 0,
            x_start: 0,
            x_end: 0,
            baseline_y: 0,
            ascent: 0,
        }; 32];
        let mut decor_count = 0usize;
        let mut fetch_err: Option<F::Error> = None;

        visit_layout_glyphs_provider(
            self.fonts,
            font_id,
            s,
            TextLayoutOptions {
                max_width: wrap_width,
                max_lines,
                ellipsis,
                align,
            },
            ProviderLayoutStyle {
                wrap: style.wrap(),
                letter_spacing,
                line_spacing,
                vis_y,
            },
            |pg| {
                if fetch_err.is_some() {
                    return;
                }
                let gx = pos.x.saturating_add(pg.x);
                let gy = pos.y.saturating_add(pg.y);
                let gw = pg.metrics.width;
                let gh = pg.metrics.height;
                if gw == 0 || gh == 0 {
                    return;
                }
                let g_rect = Rect::new(gx, gy, gw as u32, gh as u32);
                let vis = g_rect.intersect(width_limit_clip);
                if vis.is_empty() {
                    return;
                }
                if let Err(e) = self.fonts.with_glyph_bytes(
                    font_id,
                    pg.from_fallback,
                    pg.metrics,
                    |data, enc| {
                        blit_decoded_glyph(
                            target, band_rect, stride, vis, gx, gy, gw, gh, pg.phase, pg.format,
                            enc, subpixel, fg_raw, perceptual, row_buf, data,
                        );
                    },
                ) {
                    fetch_err = Some(e);
                }
            },
            |dec| {
                if draw_decor && decor_count < decors.len() {
                    decors[decor_count] = dec;
                    decor_count += 1;
                }
            },
        )?;
        if let Some(e) = fetch_err {
            return Err(e);
        }
        for &dec in &decors[..decor_count] {
            draw_line_decor(
                target,
                pos,
                width_limit_clip,
                band_rect,
                stride,
                fg_raw,
                style,
                dec,
            );
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_decoded_glyph(
    target: &mut BandTarget<'_>,
    band_rect: Rect,
    stride: usize,
    vis: Rect,
    gx: i32,
    gy: i32,
    gw: u16,
    gh: u16,
    phase: u8,
    format: GlyphFormat,
    encoding: GlyphEncoding,
    subpixel: SubpixelOrder,
    fg_raw: u16,
    perceptual: bool,
    row_buf: &mut [u8],
    data: &[u8],
) {
    let row_bytes = format.row_bytes(gw);
    let vis_x0 = vis.x;
    let vis_x1 = vis.x + vis.w as i32;
    match encoding {
        GlyphEncoding::Raw => {
            let phase_off = usize::from(phase) * usize::from(gh) * row_bytes;
            for py in vis.y..(vis.y + vis.h as i32) {
                let src_y = (py - gy) as usize;
                let rstart = phase_off + src_y * row_bytes;
                let Some(row) = data.get(rstart..rstart + row_bytes) else {
                    break;
                };
                let ly = (py - band_rect.y) as usize;
                blit_glyph_row(
                    target,
                    ly * stride,
                    band_rect.x,
                    gx,
                    gw,
                    vis_x0,
                    vis_x1,
                    row,
                    format,
                    subpixel,
                    fg_raw,
                    perceptual,
                );
            }
        }
        GlyphEncoding::Rle => {
            if row_bytes > row_buf.len() {
                return;
            }
            let phase_start_row = usize::from(phase) * usize::from(gh);
            let total_rows = phase_start_row + usize::from(gh);
            let skip_rows = phase_start_row + (vis.y - gy) as usize;
            let _ = decode_rle_rows(
                data,
                row_bytes,
                total_rows,
                skip_rows,
                vis.h as usize,
                &mut row_buf[..row_bytes],
                |row_idx, row| {
                    let py = gy + (row_idx - phase_start_row) as i32;
                    let ly = (py - band_rect.y) as usize;
                    blit_glyph_row(
                        target,
                        ly * stride,
                        band_rect.x,
                        gx,
                        gw,
                        vis_x0,
                        vis_x1,
                        row,
                        format,
                        subpixel,
                        fg_raw,
                        perceptual,
                    );
                },
            );
        }
    }
}

// A run of vertical fills (stripes, charts) writes the same opaque span on
// every scanline. Paint it once, then copy contiguous rows. Column-at-a-time
// writes thrash the MCU data cache, especially on an SDRAM backdrop.
fn fill_adjacent_rects(
    target: &mut BandTarget<'_>,
    first: Rect,
    color: Rgb565,
    following: &[PaintCmd],
) -> usize {
    let band = target.band_rect();
    let stride = band.w as usize;
    let start = (first.y - band.y) as usize * stride + (first.x - band.x) as usize;
    let mut width = first.w as usize;
    let mut used = 0;
    for cmd in following {
        let PaintCmd::FillRect {
            rect,
            clip,
            color: next,
        } = *cmd
        else {
            break;
        };
        let r = rect.intersect(clip).intersect(band);
        if r.y != first.y || r.h != first.h || i64::from(r.x) != first.left64() + width as i64 {
            break;
        }
        let words = target.words_mut();
        if used == 0 {
            words[start..start + width].fill(rgb565_to_u16(color));
        }
        let end = width + r.w as usize;
        words[start + width..start + end].fill(rgb565_to_u16(next));
        width = end;
        used += 1;
    }
    if used > 0 {
        target.set_opaque_span(start, width);
        for dy in 1..first.h as usize {
            let dst = start + dy * stride;
            target.words_mut().copy_within(start..start + width, dst);
            target.set_opaque_span(dst, width);
        }
    }
    used
}

#[allow(clippy::too_many_arguments)]
fn render_range<R: SceneResources + ?Sized, T: BandTextSource, A: crate::accel::DrawAccel>(
    target: &mut BandTarget<'_>,
    cmds: &CommandList,
    resources: &R,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
    accel: &mut A,
    text_source: &mut T,
    range: core::ops::Range<usize>,
    clear: Rgb565,
) -> Result<u32, T::Error> {
    let _ = target.clear(clear);
    let band_rect = target.band_rect();
    text_source.begin_band(band_rect);
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
                            blend,
                            accel,
                            text_source,
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
                        let underlay = range.start..cache.backdrop_end.max(range.start);
                        let capture = cache.capture;
                        let omitted = crate::effects::redundant_rows(
                            capture,
                            cache.style.backdrop_radius,
                            &cmds.as_slice()[underlay.clone()],
                        );
                        let mut offset = 0;
                        for rows in [0..omitted.start, omitted.end..capture.h as usize] {
                            if rows.is_empty() {
                                continue;
                            }
                            let rect = Rect::new(
                                capture.x,
                                capture.y + rows.start as i32,
                                capture.w,
                                rows.len() as u32,
                            );
                            let mut tile =
                                BandTarget::new::<()>(&mut backdrop.pixels[offset..], rect)
                                    .expect("reserved backdrop");
                            replayed += render_range(
                                &mut tile,
                                cmds,
                                resources,
                                blend,
                                accel,
                                text_source,
                                underlay.clone(),
                                clear,
                            )?;
                            offset += rows.len() * capture.w as usize;
                        }
                        crate::effects::filter_backdrop(
                            &mut backdrop,
                            cache.capture,
                            cache.style.backdrop_radius,
                            omitted,
                            |pixels| {
                                if let Some((color, alpha)) = expected_tint {
                                    blend(pixels, color, alpha);
                                }
                            },
                        );
                        backdrop.clear = Some(clear);
                        backdrop.tint = expected_tint;
                    }
                    text_source.begin_band(band_rect);
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
                        if let Some((raw, px_alpha)) = source.sample_rgba(
                            (x - i64::from(rect.x)) as u32,
                            (y - i64::from(rect.y)) as u32,
                            rect.w,
                            rect.h,
                            angle,
                        ) {
                            let eff_alpha =
                                ((u16::from(px_alpha) * u16::from(alpha) + 7) / 15) as u8;
                            if eff_alpha > 0 {
                                let index = (y - i64::from(band_rect.y)) as usize * stride
                                    + (x - i64::from(band_rect.x)) as usize;
                                target.blend_pixel_a4(index, raw, eff_alpha);
                            }
                        }
                    }
                }
            }
            PaintCmd::AlphaMask {
                rect,
                clip,
                source,
                color,
                alpha,
            } => {
                if alpha == 0 || rect.w == 0 || rect.h == 0 {
                    continue;
                }
                let area = rect.intersect(clip).intersect(band_rect);
                if area.is_empty() {
                    continue;
                }
                let stride = target.width() as usize;
                if !target.has_alpha()
                    && alpha == 15
                    && rect.w == u32::from(source.width())
                    && rect.h == u32::from(source.height())
                    && area.x >= rect.x
                    && area.y >= rect.y
                    && let Some(mask_bytes) = source.mask_a4_bytes()
                {
                    let src_w = usize::from(source.width());
                    let sx0 = (area.x - rect.x) as usize;
                    let width = area.w as usize;
                    let dst_x0 = (area.x - band_rect.x) as usize;
                    for py in 0..area.h {
                        let y = area.y as i64 + i64::from(py);
                        let sy = (y - i64::from(rect.y)) as usize;
                        let row_start = (y - i64::from(band_rect.y)) as usize * stride + dst_x0;
                        let mask_offset_px = sy * src_w + sx0;
                        accel.blend_a4_mask_row(
                            &mut target.words_mut()[row_start..row_start + width],
                            mask_bytes,
                            mask_offset_px,
                            color,
                        );
                    }
                    continue;
                }
                let raw_color = rgb565_to_u16(color);
                for py in 0..area.h {
                    for px in 0..area.w {
                        let x = area.x as i64 + i64::from(px);
                        let y = area.y as i64 + i64::from(py);
                        if let Some((_, px_alpha)) = source.sample_rgba(
                            (x - i64::from(rect.x)) as u32,
                            (y - i64::from(rect.y)) as u32,
                            rect.w,
                            rect.h,
                            0,
                        ) {
                            let eff_alpha =
                                ((u16::from(px_alpha) * u16::from(alpha) + 7) / 15) as u8;
                            if eff_alpha > 0 {
                                let index = (y - i64::from(band_rect.y)) as usize * stride
                                    + (x - i64::from(band_rect.x)) as usize;
                                target.blend_pixel_a4(index, raw_color, eff_alpha);
                            }
                        }
                    }
                }
            }

            PaintCmd::FillRect { rect, clip, color } => {
                let eff = rect.intersect(clip).intersect(band_rect);
                if eff.h > 1 {
                    let used = fill_adjacent_rects(
                        target,
                        eff,
                        color,
                        &cmds.as_slice()[cursor..range.end],
                    );
                    if used > 0 {
                        cursor += used;
                        replayed += used as u32;
                        continue;
                    }
                }
                accel.fill_rect(target, eff, color);
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
            PaintCmd::RoundedOverlay {
                rect,
                clip,
                color,
                alpha,
                radius,
            } => {
                let area = rect.intersect(clip).intersect(band_rect);
                let r = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
                let shape = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r));
                let raw = rgb565_to_u16(color);
                for dy in 0..area.h {
                    let y = area.y + dy as i32;
                    for dx in 0..area.w {
                        let x = area.x + dx as i32;
                        if shape.contains(Point::new(x, y)) {
                            let i = (y - band_rect.y) as usize * target.width() as usize
                                + (x - band_rect.x) as usize;
                            target.blend_pixel_a4(i, raw, alpha);
                        }
                    }
                }
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
                style,
                letter_spacing,
                line_spacing,
            } => {
                draw_text_cmd(
                    target,
                    band_rect,
                    text_source,
                    pos,
                    clip,
                    wrap_width,
                    resources.text(text),
                    color,
                    font_id,
                    align,
                    max_lines,
                    ellipsis,
                    style,
                    letter_spacing,
                    line_spacing,
                )?;
            }
            PaintCmd::InlineText {
                pos,
                clip,
                color,
                wrap_width,
                font: font_id,
                align,
                len,
                bytes,
            } => {
                let s = core::str::from_utf8(&bytes[..usize::from(len).min(12)]).unwrap_or("");
                draw_text_cmd(
                    target,
                    band_rect,
                    text_source,
                    pos,
                    clip,
                    u32::from(wrap_width),
                    s,
                    color,
                    font_id,
                    align,
                    1,
                    false,
                    TextStyleFlags::DEFAULT,
                    0,
                    0,
                )?;
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
            PaintCmd::Line {
                p0,
                p1,
                clip,
                color,
                width,
                aa,
            } => {
                crate::vector::draw_line(target, band_rect, p0, p1, clip, color, width, aa);
            }
            PaintCmd::Circle {
                center,
                clip,
                radius,
                fill,
                border_color,
                border_width,
            } => {
                crate::vector::draw_circle(
                    target,
                    band_rect,
                    center,
                    radius,
                    clip,
                    fill,
                    border_color,
                    border_width,
                );
            }
            PaintCmd::Arc {
                center,
                clip,
                radius,
                start_deg,
                sweep_deg,
                color,
                width,
                cap_round,
            } => {
                crate::vector::draw_arc(
                    target, band_rect, center, radius, start_deg, sweep_deg, width, cap_round,
                    clip, color,
                );
            }
            PaintCmd::GradientRect {
                rect,
                clip,
                start,
                end,
                direction,
                radius,
            } => {
                crate::vector::draw_gradient_rect(
                    target, band_rect, rect, clip, start, end, direction, radius,
                );
            }
        }
    }

    Ok(replayed)
}

#[allow(clippy::too_many_arguments)]
fn draw_text_cmd<T: BandTextSource>(
    target: &mut BandTarget<'_>,
    band_rect: Rect,
    text_source: &mut T,
    pos: Point,
    clip: Rect,
    wrap_width: u32,
    s: &str,
    color: Rgb565,
    font_id: FontId,
    align: TextAlign,
    max_lines: u8,
    ellipsis: bool,
    style: TextStyleFlags,
    letter_spacing: i8,
    line_spacing: i8,
) -> Result<(), T::Error> {
    let eff_clip = clip.intersect(band_rect);
    if eff_clip.is_empty() || wrap_width == 0 || s.is_empty() {
        return Ok(());
    }
    let width_limit_clip = eff_clip.intersect(Rect::new(
        eff_clip.x,
        eff_clip.y,
        ((pos.x as i64 + wrap_width as i64) - eff_clip.x as i64).clamp(0, u32::MAX as i64) as u32,
        eff_clip.h,
    ));
    if width_limit_clip.is_empty() {
        return Ok(());
    }
    let stride = target.width() as usize;
    let fg_raw = rgb565_to_u16(color);
    let decor_pad = if style.underline() || style.strikethrough() {
        4
    } else {
        0
    };
    let vis_y = Some((
        width_limit_clip.y - pos.y - decor_pad.max(8),
        (width_limit_clip.y + width_limit_clip.h as i32) - pos.y + decor_pad.max(8),
    ));
    let mut row_buf = [0u8; 256];
    text_source.draw_text(
        target,
        band_rect,
        stride,
        width_limit_clip,
        pos,
        s,
        fg_raw,
        wrap_width,
        max_lines,
        ellipsis,
        font_id,
        align,
        style,
        letter_spacing,
        line_spacing,
        vis_y,
        &mut row_buf,
    )
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

#[inline]
fn blit_a4_cov(
    target: &mut BandTarget<'_>,
    has_alpha: bool,
    idx: usize,
    fg_raw: u16,
    cov: u8,
    perceptual: bool,
) {
    if cov == 0 {
        return;
    }
    if has_alpha {
        target.blend_pixel_a4(idx, fg_raw, cov);
        return;
    }
    let words = target.words_mut();
    if cov == 15 {
        words[idx] = fg_raw;
    } else {
        let bg = words[idx];
        let c = if perceptual {
            gamma_remap_a4(cov, fg_raw, bg)
        } else {
            cov
        };
        if c != 0 {
            words[idx] = blend_rgb565_a4_u16(fg_raw, bg, c);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_glyph_row(
    target: &mut BandTarget<'_>,
    row_off: usize,
    band_x: i32,
    gx: i32,
    gw: u16,
    vis_x0: i32,
    vis_x1: i32,
    row: &[u8],
    format: GlyphFormat,
    subpixel: SubpixelOrder,
    fg_raw: u16,
    perceptual: bool,
) {
    let has_alpha = target.has_alpha();
    match format {
        GlyphFormat::A4 => {
            let mut px = vis_x0;
            while px < vis_x1 {
                let sx = (px - gx) as usize;
                let lx = (px - band_x) as usize;
                if (sx & 1) == 0 && px + 1 < vis_x1 {
                    let b = row[sx >> 1];
                    if b == 0 {
                        px += 2;
                        continue;
                    }
                    if b == 0xff && !has_alpha {
                        let words = target.words_mut();
                        words[row_off + lx] = fg_raw;
                        words[row_off + lx + 1] = fg_raw;
                        px += 2;
                        continue;
                    }
                    blit_a4_cov(target, has_alpha, row_off + lx, fg_raw, b >> 4, perceptual);
                    blit_a4_cov(
                        target,
                        has_alpha,
                        row_off + lx + 1,
                        fg_raw,
                        b & 0x0f,
                        perceptual,
                    );
                    px += 2;
                } else {
                    let b = row[sx >> 1];
                    let cov = if (sx & 1) == 0 { b >> 4 } else { b & 0x0f };
                    blit_a4_cov(target, has_alpha, row_off + lx, fg_raw, cov, perceptual);
                    px += 1;
                }
            }
        }
        GlyphFormat::A1 => {
            let mut px = vis_x0;
            while px < vis_x1 {
                let sx = (px - gx) as usize;
                let lx = (px - band_x) as usize;
                if (sx & 7) == 0 && px + 8 <= vis_x1 {
                    let b = row[sx >> 3];
                    if b == 0 {
                        px += 8;
                        continue;
                    }
                    if b == 0xff && !has_alpha {
                        target.words_mut()[row_off + lx..row_off + lx + 8].fill(fg_raw);
                        px += 8;
                        continue;
                    }
                }
                if ((row[sx >> 3] >> (7 - (sx & 7))) & 1) != 0 {
                    blit_a4_cov(target, has_alpha, row_off + lx, fg_raw, 15, false);
                }
                px += 1;
            }
        }
        GlyphFormat::A2 => {
            for px in vis_x0..vis_x1 {
                let sx = (px - gx) as u16;
                let cov = a2_pixel(row, gw, sx, 0);
                if cov != 0 {
                    let lx = (px - band_x) as usize;
                    blit_a4_cov(target, has_alpha, row_off + lx, fg_raw, cov, perceptual);
                }
            }
        }
        GlyphFormat::A8 => {
            for px in vis_x0..vis_x1 {
                let sx = (px - gx) as usize;
                let a8 = row[sx];
                if a8 == 0 {
                    continue;
                }
                let lx = (px - band_x) as usize;
                let idx = row_off + lx;
                if has_alpha {
                    let a4 = ((u16::from(a8) * 15 + 127) / 255) as u8;
                    target.blend_pixel_a4(idx, fg_raw, a4);
                } else {
                    let words = target.words_mut();
                    if a8 == 255 {
                        words[idx] = fg_raw;
                    } else {
                        let bg = words[idx];
                        let a = if perceptual {
                            gamma_remap_a8(a8, fg_raw, bg)
                        } else {
                            a8
                        };
                        words[idx] = blend_rgb565_a8_u16(fg_raw, bg, a);
                    }
                }
            }
        }
        GlyphFormat::LcdRgb => {
            for px in vis_x0..vis_x1 {
                let sx = (px - gx) as u16;
                let (r, g, b) = lcd_rgb_pixel(row, gw, sx, 0);
                if (r | g | b) == 0 {
                    continue;
                }
                let lx = (px - band_x) as usize;
                let idx = row_off + lx;
                if has_alpha || subpixel == SubpixelOrder::None {
                    let cov = ((u16::from(r) + u16::from(g) * 2 + u16::from(b) + 2) >> 2) as u8;
                    blit_a4_cov(target, has_alpha, idx, fg_raw, cov, perceptual);
                } else {
                    let words = target.words_mut();
                    let bg = words[idx];
                    let (cr, cg, cb) = if perceptual {
                        (
                            gamma_remap_a4(r, fg_raw, bg),
                            gamma_remap_a4(g, fg_raw, bg),
                            gamma_remap_a4(b, fg_raw, bg),
                        )
                    } else {
                        (r, g, b)
                    };
                    words[idx] = blend_rgb565_lcd_a4_u16(
                        fg_raw,
                        bg,
                        cr,
                        cg,
                        cb,
                        subpixel == SubpixelOrder::Bgr,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_h_span(
    target: &mut BandTarget<'_>,
    band_rect: Rect,
    stride: usize,
    clip: Rect,
    x0: i32,
    x1: i32,
    y: i32,
    fg_raw: u16,
) {
    if y < clip.y || y >= clip.y + clip.h as i32 {
        return;
    }
    let sx0 = x0.max(clip.x);
    let sx1 = x1.min(clip.x + clip.w as i32);
    if sx0 >= sx1 {
        return;
    }
    let row_off = ((y - band_rect.y) as usize) * stride;
    let start = row_off + (sx0 - band_rect.x) as usize;
    let len = (sx1 - sx0) as usize;
    target.words_mut()[start..start + len].fill(fg_raw);
    target.set_opaque_span(start, len);
}

#[allow(clippy::too_many_arguments)]
fn draw_line_decor(
    target: &mut BandTarget<'_>,
    pos: Point,
    clip: Rect,
    band_rect: Rect,
    stride: usize,
    fg_raw: u16,
    style: TextStyleFlags,
    dec: LineDecor,
) {
    if !(style.underline() || style.strikethrough()) || dec.x_end <= dec.x_start {
        return;
    }
    let x0 = pos.x.saturating_add(dec.x_start);
    let x1 = pos.x.saturating_add(dec.x_end);
    if style.underline() {
        let uy = pos.y.saturating_add(dec.baseline_y).saturating_add(2);
        draw_h_span(target, band_rect, stride, clip, x0, x1, uy, fg_raw);
    }
    if style.strikethrough() {
        let sy = pos
            .y
            .saturating_add(dec.baseline_y)
            .saturating_sub((i32::from(dec.ascent) * 3) / 8);
        draw_h_span(target, band_rect, stride, clip, x0, x1, sy, fg_raw);
    }
}
