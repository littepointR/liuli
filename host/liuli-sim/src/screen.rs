use core::convert::Infallible;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::{OriginDimensions, Size};
use embedded_graphics::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics::primitives::{
    Circle, ContainsPoint, Line, Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle,
    RoundedRectangle, StrokeAlignment, Triangle,
};
use embedded_graphics::{Drawable, Pixel};
use liuli::core::paint::{CommandList, PaintCmd, SceneResources};
use liuli::prelude::{
    FontId, IconKind, Point, Rect, SubpixelOrder, TextAlign, TextGamma, TextStyleFlags,
};
use liuli::text::{
    FontProvider, FontSet, GlyphEncoding, GlyphFormat, LineDecor, ProviderLayoutStyle,
    TextLayoutOptions, TextLayoutStyle, a2_pixel, blend_over_rgb565_a4, blend_rgb565_a4,
    blend_rgb565_a4_u16, blend_rgb565_a8_u16, blend_rgb565_lcd_a4_u16, decode_rle_rows,
    gamma_remap_a4, gamma_remap_a8, lcd_rgb_pixel, visit_layout_glyphs_provider,
    visit_layout_glyphs_styled,
};

use crate::image::{encode_png_rgb8, encode_ppm, rgb565_to_rgb8};

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct MemoryScreen {
    w: u32,
    h: u32,
    pixels: Vec<u16>,
    alpha: Option<Vec<u8>>,
}

impl MemoryScreen {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            pixels: vec![0; (w as usize) * (h as usize)],
            alpha: None,
        }
    }

    pub fn new_with_alpha(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            pixels: vec![0; (w as usize) * (h as usize)],
            alpha: Some(vec![0; (w as usize) * (h as usize)]),
        }
    }

    #[inline]
    fn put_opaque(&mut self, idx: usize, raw: u16) {
        self.pixels[idx] = raw;
        if let Some(a) = &mut self.alpha {
            a[idx] = 15;
        }
    }

    #[inline]
    fn blend_pixel_a4(&mut self, idx: usize, fg_raw: u16, cov: u8) {
        if let Some(a) = &mut self.alpha {
            let (rgb, out_a) = blend_over_rgb565_a4(self.pixels[idx], a[idx], fg_raw, cov);
            self.pixels[idx] = rgb;
            a[idx] = out_a;
        } else {
            self.pixels[idx] = blend_rgb565_a4_u16(fg_raw, self.pixels[idx], cov);
        }
    }
}

impl liuli::render::vector::PixelSink for MemoryScreen {
    #[inline(always)]
    fn put_opaque(&mut self, idx: usize, fg_raw: u16) {
        MemoryScreen::put_opaque(self, idx, fg_raw);
    }

    #[inline(always)]
    fn blend_a4(&mut self, idx: usize, fg_raw: u16, cov: u8) {
        self.blend_pixel_a4(idx, fg_raw, cov);
    }
}

impl MemoryScreen {
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

impl liuli::render::DisplaySink for MemoryScreen {
    type Error = Infallible;

    fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
        let inter = rect.intersect(Rect::new(0, 0, self.w, self.h));
        if inter.is_empty() {
            return Ok(());
        }
        let skip_x = (inter.x - rect.x) as usize;
        let skip_y = (inter.y - rect.y) as usize;
        let src_stride = rect.w as usize;
        let dst_stride = self.w as usize;
        let vis_w = inter.w as usize;
        let vis_h = inter.h as usize;
        let dst_x0 = inter.x as usize;
        let dst_y0 = inter.y as usize;

        for ry in 0..vis_h {
            let src_off = (skip_y + ry) * src_stride + skip_x;
            if src_off >= pixels.len() {
                break;
            }
            let avail = (pixels.len() - src_off).min(vis_w);
            let dst_off = (dst_y0 + ry) * dst_stride + dst_x0;
            self.pixels[dst_off..dst_off + avail]
                .copy_from_slice(&pixels[src_off..src_off + avail]);
            if let Some(a) = &mut self.alpha {
                a[dst_off..dst_off + avail].fill(15);
            }
            if avail < vis_w {
                break;
            }
        }
        Ok(())
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
                self.put_opaque((p.y as usize) * stride + (p.x as usize), c.into_storage());
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
            if let Some(a) = &mut self.alpha {
                a[y * stride + x0..y * stride + x1].fill(15);
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
                        self.put_opaque((gy as usize) * stride + (gx as usize), c.into_storage());
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
    match reference_range_with_source(
        cmds,
        resources,
        &SimLegacySource(fonts),
        0..cmds.count,
        cmds.root_bg,
        false,
        cmds.screen,
    ) {
        Ok(screen) => screen,
        Err(never) => match never {},
    }
}

pub fn render_reference_fullscreen_with_provider<F: FontProvider>(
    cmds: &CommandList,
    resources: &impl SceneResources,
    fonts: &F,
) -> Result<MemoryScreen, F::Error> {
    fonts.begin_frame();
    reference_range_with_source(
        cmds,
        resources,
        &SimProviderSource(fonts),
        0..cmds.count,
        cmds.root_bg,
        false,
        cmds.screen,
    )
}

trait SimTextSource {
    type Error;
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &self,
        screen: &mut MemoryScreen,
        stride: usize,
        width_limit_clip: Rect,
        pos: Point,
        s: &str,
        text_raw: u16,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font_id: liuli::prelude::FontId,
        align: liuli::prelude::TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
        vis_y: Option<(i32, i32)>,
        row_buf: &mut [u8],
    ) -> Result<(), Self::Error>;
}

struct SimLegacySource<'a, 'f>(&'a FontSet<'f>);
impl SimTextSource for SimLegacySource<'_, '_> {
    type Error = core::convert::Infallible;

    fn draw_text(
        &self,
        screen: &mut MemoryScreen,
        stride: usize,
        width_limit_clip: Rect,
        pos: Point,
        s: &str,
        text_raw: u16,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font_id: liuli::prelude::FontId,
        align: liuli::prelude::TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
        vis_y: Option<(i32, i32)>,
        row_buf: &mut [u8],
    ) -> Result<(), Self::Error> {
        let font = self.0.get(font_id);
        let perceptual = style.gamma() == TextGamma::Perceptual;
        let subpixel = style.subpixel();
        let mut decors = Vec::new();
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
                fallback: self.0.fallback(),
                vis_y,
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
                blit_sim_decoded_glyph(
                    screen,
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
                    text_raw,
                    perceptual,
                    row_buf,
                    pg.glyph.data,
                );
            },
            |dec| decors.push(dec),
        );
        for dec in decors {
            draw_sim_line_decor(screen, pos, width_limit_clip, stride, text_raw, style, dec);
        }
        Ok(())
    }
}

struct SimProviderSource<'a, F>(&'a F);
impl<F: FontProvider> SimTextSource for SimProviderSource<'_, F> {
    type Error = F::Error;

    fn draw_text(
        &self,
        screen: &mut MemoryScreen,
        stride: usize,
        width_limit_clip: Rect,
        pos: Point,
        s: &str,
        text_raw: u16,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font_id: liuli::prelude::FontId,
        align: liuli::prelude::TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
        vis_y: Option<(i32, i32)>,
        row_buf: &mut [u8],
    ) -> Result<(), Self::Error> {
        let perceptual = style.gamma() == TextGamma::Perceptual;
        let subpixel = style.subpixel();
        let mut decors = Vec::new();
        let mut fetch_err = None;
        visit_layout_glyphs_provider(
            self.0,
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
                let gx = pos.x + pg.x;
                let gy = pos.y + pg.y;
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
                if let Err(e) =
                    self.0
                        .with_glyph_bytes(font_id, pg.from_fallback, pg.metrics, |data, enc| {
                            blit_sim_decoded_glyph(
                                screen, stride, vis, gx, gy, gw, gh, pg.phase, pg.format, enc,
                                subpixel, text_raw, perceptual, row_buf, data,
                            );
                        })
                {
                    fetch_err = Some(e);
                }
            },
            |dec| decors.push(dec),
        )?;
        if let Some(e) = fetch_err {
            return Err(e);
        }
        for dec in decors {
            draw_sim_line_decor(screen, pos, width_limit_clip, stride, text_raw, style, dec);
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_sim_decoded_glyph(
    screen: &mut MemoryScreen,
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
    text_raw: u16,
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
                let sy = (py - gy) as usize;
                let rstart = phase_off + sy * row_bytes;
                let Some(row) = data.get(rstart..rstart + row_bytes) else {
                    break;
                };
                blit_sim_glyph_row(
                    screen,
                    (py as usize) * stride,
                    gx,
                    gw,
                    vis_x0,
                    vis_x1,
                    row,
                    format,
                    subpixel,
                    text_raw,
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
                    blit_sim_glyph_row(
                        screen,
                        (py as usize) * stride,
                        gx,
                        gw,
                        vis_x0,
                        vis_x1,
                        row,
                        format,
                        subpixel,
                        text_raw,
                        perceptual,
                    );
                },
            );
        }
    }
}

fn reference_path<'a>(
    screen: &mut MemoryScreen,
    rect: Rect,
    clip: Rect,
    paint: liuli::prelude::PathPaint,
    color: Rgb565,
    edges: impl Iterator<Item = &'a liuli::core::path::PathEdge> + Clone,
) {
    let area = rect
        .intersect(clip)
        .intersect(Rect::new(0, 0, screen.w, screen.h));
    for y in area.y..area.bottom64() as i32 {
        for x in area.x..area.right64() as i32 {
            let a = liuli::core::path::coverage(edges.clone(), rect, Point::new(x, y), paint);
            if a != 0 {
                screen.blend_pixel_a4(
                    y as usize * screen.w as usize + x as usize,
                    color.into_storage(),
                    a,
                )
            }
        }
    }
}
fn reference_shadow(
    screen: &mut MemoryScreen,
    rect: Rect,
    clip: Rect,
    s: liuli::prelude::Shadow,
    alpha: impl Fn(Point) -> u8,
) {
    let area = s
        .bounds(rect)
        .intersect(clip)
        .intersect(Rect::new(0, 0, screen.w, screen.h));
    let r = i32::from(s.blur);
    let size = ((2 * r + 1) * (2 * r + 1)) as u32;
    for y in area.y..area.bottom64() as i32 {
        for x in area.x..area.right64() as i32 {
            let mut sum = 0u32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let p = Point::new(
                        x.saturating_sub(i32::from(s.x)).saturating_add(dx),
                        y.saturating_sub(i32::from(s.y)).saturating_add(dy),
                    );
                    if rect.contains_point(p) {
                        sum += u32::from(alpha(p))
                    }
                }
            }
            let a = liuli::core::composition::mul_a4(((sum + size / 2) / size) as u8, s.opacity);
            if a != 0 {
                screen.blend_pixel_a4(
                    y as usize * screen.w as usize + x as usize,
                    s.color.into_storage(),
                    a,
                )
            }
        }
    }
}
// Shift global commands into the reference image. A content-shadow caster can
// extend past the screen or outside a parent's scroll viewport.
fn reference_command(mut cmd: PaintCmd, origin: Rect) -> PaintCmd {
    if origin.x == 0 && origin.y == 0 {
        return cmd;
    }
    let dx = origin.x.saturating_neg();
    let dy = origin.y.saturating_neg();
    let point = |p: &mut Point| {
        p.x = p.x.saturating_add(dx);
        p.y = p.y.saturating_add(dy);
    };
    let clip = match &mut cmd {
        PaintCmd::Empty | PaintCmd::PathEdges { .. } => return cmd,
        PaintCmd::Text { pos, clip, .. } | PaintCmd::InlineText { pos, clip, .. } => {
            point(pos);
            clip
        }
        PaintCmd::Circle { center, clip, .. } | PaintCmd::Arc { center, clip, .. } => {
            point(center);
            clip
        }
        PaintCmd::Line { p0, p1, clip, .. } => {
            point(p0);
            point(p1);
            clip
        }
        PaintCmd::Bitmap { rect, clip, .. }
        | PaintCmd::Surface { rect, clip, .. }
        | PaintCmd::Group { rect, clip, .. }
        | PaintCmd::Shadow { rect, clip, .. }
        | PaintCmd::FillRect { rect, clip, .. }
        | PaintCmd::StrokeRect { rect, clip, .. }
        | PaintCmd::FillRoundedRect { rect, clip, .. }
        | PaintCmd::StrokeRoundedRect { rect, clip, .. }
        | PaintCmd::RoundedOverlay { rect, clip, .. }
        | PaintCmd::Overlay { rect, clip, .. }
        | PaintCmd::Icon { rect, clip, .. }
        | PaintCmd::Sparkline { rect, clip, .. }
        | PaintCmd::GradientRect { rect, clip, .. }
        | PaintCmd::AlphaMask { rect, clip, .. }
        | PaintCmd::MaskGroup { rect, clip, .. }
        | PaintCmd::OutlineShadow { rect, clip, .. }
        | PaintCmd::StaticPath { rect, clip, .. }
        | PaintCmd::PathRun { rect, clip, .. } => {
            *rect = rect.translate(dx, dy);
            clip
        }
    };
    *clip = clip.translate(dx, dy);
    cmd
}
fn reference_range_with_source<S: SimTextSource>(
    cmds: &CommandList,
    resources: &impl SceneResources,
    text_source: &S,
    range: std::ops::Range<usize>,
    clear: Rgb565,
    with_alpha: bool,
    viewport: Rect,
) -> Result<MemoryScreen, S::Error> {
    let mut screen = if with_alpha {
        MemoryScreen::new_with_alpha(viewport.w, viewport.h)
    } else {
        MemoryScreen::new(viewport.w, viewport.h)
    };
    let _ = screen.clear(clear);
    let full = Rect::new(0, 0, viewport.w, viewport.h);
    let stride = screen.w as usize;

    let mut cursor = range.start;
    while cursor < range.end {
        let group_cmd_idx = cursor;
        let cmd = &reference_command(cmds.as_slice()[cursor], viewport);
        cursor += 1;
        match *cmd {
            PaintCmd::Empty | PaintCmd::PathEdges { .. } => {}
            PaintCmd::StaticPath {
                rect,
                clip,
                path,
                paint,
                color,
            } => reference_path(&mut screen, rect, clip, paint, color, path.edges.iter()),
            PaintCmd::PathRun {
                rect,
                clip,
                paint,
                color,
                chunks,
            } => {
                let end = (cursor + usize::from(chunks)).min(range.end);
                let edges = cmds.as_slice()[cursor..end].iter().flat_map(|c| match c {
                    PaintCmd::PathEdges { len, edges } => {
                        &edges[..usize::from(*len).min(edges.len())]
                    }
                    _ => &[][..],
                });
                reference_path(&mut screen, rect, clip, paint, color, edges);
                cursor = end;
            }
            PaintCmd::OutlineShadow {
                rule,
                path,
                rect,
                clip,
                shadow,
            } => reference_shadow(&mut screen, rect, clip, shadow, |p| {
                liuli::core::path::coverage(
                    path.edges.iter(),
                    rect,
                    p,
                    liuli::prelude::PathPaint::Fill(rule),
                )
            }),
            PaintCmd::MaskGroup {
                radius,
                rect,
                clip,
                end,
                composition,
            } => {
                let end = usize::from(end).clamp(cursor, range.end);
                let group = reference_range_with_source(
                    cmds,
                    resources,
                    text_source,
                    cursor..end,
                    Rgb565::new(0, 0, 0),
                    true,
                    viewport,
                )?;
                cursor = end;
                let area = rect.intersect(clip).intersect(full);
                for y in area.y..area.bottom64() as i32 {
                    for x in area.x..area.right64() as i32 {
                        let i = y as usize * stride + x as usize;
                        let a = liuli::core::composition::mul_a4(
                            group.alpha.as_ref().unwrap()[i],
                            composition.coverage(rect, radius, Point::new(x, y)),
                        );
                        if a != 0 {
                            screen.blend_pixel_a4(i, group.pixels[i], a)
                        }
                    }
                }
            }
            PaintCmd::Shadow {
                rect,
                clip,
                shadow,
                projection,
                radius,
            } => {
                let shape = rect.translate(shadow.x.into(), shadow.y.into());
                let bounds = shadow.bounds(rect);
                let area = bounds.intersect(clip).intersect(full);
                let r = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
                let inner_thresh = i64::from(rounded_rect_threshold(r));
                let inner_falloff = if r == 0 { 0 } else { inner_thresh };
                let outer = if shadow.blur == 0 {
                    inner_thresh
                } else {
                    i64::from(2 * (r + u32::from(shadow.blur))).pow(2)
                };
                let shadow_raw = shadow.color.into_storage();
                for y in area.y..area.bottom64() as i32 {
                    for x in area.x..area.right64() as i32 {
                        let pos = Point::new(x, y);
                        if projection
                            .unproject(rect, pos)
                            .is_some_and(|p| corner_distance_2x(rect, r, p) < inner_thresh)
                        {
                            continue;
                        }
                        if let Some(p) = projection.unproject(bounds, pos) {
                            let d = corner_distance_2x(shape, r, p);
                            let a = if d < inner_thresh {
                                shadow.opacity
                            } else if d >= outer {
                                0
                            } else {
                                ((outer - d) * i64::from(shadow.opacity) / (outer - inner_falloff))
                                    as u8
                            };
                            let i = y as usize * stride + x as usize;
                            screen.blend_pixel_a4(i, shadow_raw, a);
                        }
                    }
                }
            }
            PaintCmd::Group {
                rect,
                clip,
                index,
                end,
            } => {
                let content = cursor..usize::from(end);
                cursor = content.end;
                let (style, radius) = cmds.group_style(index).expect("emitted group");
                if style.opacity == 0 || matches!(style.projection.0, 64 | 192) {
                    continue;
                }
                let source = cmds.group_source(index).expect("group source");
                let group_viewport = viewport.union(source);
                let source =
                    source.translate(viewport.x.saturating_neg(), viewport.y.saturating_neg());
                let group_index = |p: Point| {
                    ((i64::from(p.y) + viewport.top64() - group_viewport.top64()) as usize)
                        * group_viewport.w as usize
                        + (i64::from(p.x) + viewport.left64() - group_viewport.left64()) as usize
                };
                let group = reference_range_with_source(
                    cmds,
                    resources,
                    text_source,
                    content,
                    Rgb565::new(0, 0, 0),
                    true,
                    group_viewport,
                )?;
                let backdrop_end = cmds.group_backdrop_end(index).unwrap_or(group_cmd_idx);
                let mut blurred = (style.backdrop_radius > 0)
                    .then(|| -> Result<MemoryScreen, S::Error> {
                        let bg_raw = reference_range_with_source(
                            cmds,
                            resources,
                            text_source,
                            range.start..backdrop_end.max(range.start),
                            clear,
                            false,
                            viewport,
                        )?;
                        Ok(reference_blur(&bg_raw, style.backdrop_radius))
                    })
                    .transpose()?;
                let rgb = liuli::core::geometry::u16_to_rgb565;
                if let Some(ref mut b) = blurred
                    && style.tint_alpha > 0
                {
                    for word in &mut b.pixels {
                        *word = blend_rgb565_a4(style.tint_color, rgb(*word), style.tint_alpha)
                            .into_storage();
                    }
                }
                let group_alpha = group.alpha.as_ref().expect("group alpha");
                let (rect, composition, shadow) = cmds
                    .group_composition(index)
                    .map(|(layout, c, s)| {
                        (
                            layout.translate(
                                viewport.x.saturating_neg(),
                                viewport.y.saturating_neg(),
                            ),
                            c,
                            s,
                        )
                    })
                    .unwrap_or((rect, None, None));
                if let (Some(c), Some(s)) = (composition, shadow) {
                    reference_shadow(&mut screen, rect, clip, s, |q| {
                        let Some(p) = style.projection.unproject(rect, q) else {
                            return 0;
                        };
                        if c.shadow_source == liuli::prelude::ShadowSource::Outline {
                            return c.outline.coverage(rect, radius, p);
                        }
                        if !source.contains_point(p) {
                            return 0;
                        }
                        liuli::core::composition::mul_a4(
                            group_alpha[group_index(p)],
                            c.coverage(rect, radius, p),
                        )
                    })
                }
                let area = rect.intersect(clip).intersect(full);
                let radius = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
                let radius_thresh = i64::from(rounded_rect_threshold(radius));
                for y in area.y..area.bottom64() as i32 {
                    for x in area.x..area.right64() as i32 {
                        let Some(p) = style.projection.unproject(rect, Point::new(x, y)) else {
                            continue;
                        };
                        if !source.contains_point(p) {
                            continue;
                        }
                        let i = y as usize * stride + x as usize;
                        let g_idx = group_index(p);
                        let original = screen.pixels[i];
                        let a = group_alpha[g_idx];
                        if composition.is_some() || with_alpha {
                            let inside = corner_distance_2x(rect, radius, p) < radius_thresh;
                            let (rgb, alpha) = if inside && blurred.is_some() {
                                (
                                    blend_rgb565_a4_u16(
                                        group.pixels[g_idx],
                                        blurred.as_ref().unwrap().pixels[i],
                                        a,
                                    ),
                                    15,
                                )
                            } else if inside && style.tint_alpha > 0 {
                                blend_over_rgb565_a4(
                                    style.tint_color.into_storage(),
                                    style.tint_alpha,
                                    group.pixels[g_idx],
                                    a,
                                )
                            } else {
                                (group.pixels[g_idx], a)
                            };
                            let amount = liuli::core::composition::mul_a4(
                                style.opacity,
                                composition.map_or(15, |c| c.coverage(rect, radius as u8, p)),
                            );
                            screen.blend_pixel_a4(
                                i,
                                rgb,
                                liuli::core::composition::mul_a4(alpha, amount),
                            );
                            continue;
                        }
                        let color = if a == 15 {
                            group.pixels[g_idx]
                        } else {
                            let inside = corner_distance_2x(rect, radius, p) < radius_thresh;
                            let bg = if inside {
                                if let Some(ref b) = blurred {
                                    b.pixels[i]
                                } else if style.tint_alpha > 0 {
                                    blend_rgb565_a4(
                                        style.tint_color,
                                        rgb(original),
                                        style.tint_alpha,
                                    )
                                    .into_storage()
                                } else {
                                    original
                                }
                            } else {
                                original
                            };
                            blend_rgb565_a4_u16(group.pixels[g_idx], bg, a)
                        };
                        screen.pixels[i] =
                            blend_rgb565_a4(rgb(color), rgb(original), style.opacity)
                                .into_storage();
                    }
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
                let area = rect.intersect(clip).intersect(full);
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
                                let index = (y - i64::from(full.y)) as usize * stride
                                    + (x - i64::from(full.x)) as usize;
                                screen.blend_pixel_a4(index, raw, eff_alpha);
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
                let area = rect.intersect(clip).intersect(full);
                let raw_color = color.into_storage();
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
                                let index = (y - i64::from(full.y)) as usize * stride
                                    + (x - i64::from(full.x)) as usize;
                                screen.blend_pixel_a4(index, raw_color, eff_alpha);
                            }
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
            PaintCmd::StrokeRoundedRect {
                rect,
                clip,
                color,
                width,
                radius,
            } => {
                let eff_clip = clip.intersect(full);
                if width > 0 && !rect.is_empty() && !eff_clip.is_empty() {
                    let r = (radius as u32).min(rect.w / 2).min(rect.h / 2);
                    let style = PrimitiveStyleBuilder::new()
                        .stroke_color(color)
                        .stroke_width(width as u32)
                        .stroke_alignment(StrokeAlignment::Inside)
                        .build();
                    let mut clipped = screen.clipped(&eff_clip.to_eg());
                    let _ = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r))
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
            PaintCmd::RoundedOverlay {
                rect,
                clip,
                color,
                alpha,
                radius,
            } => {
                let area = rect.intersect(clip).intersect(full);
                let r = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
                let shape = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r));
                for y in area.y..area.bottom64() as i32 {
                    for x in area.x..area.right64() as i32 {
                        if shape.contains(Point::new(x, y)) {
                            screen.blend_pixel_a4(
                                y as usize * stride + x as usize,
                                color.into_storage(),
                                alpha,
                            );
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
                let eff = rect.intersect(clip).intersect(full);
                if eff.is_empty() || alpha == 0 {
                    continue;
                }
                let overlay_raw = color.into_storage();
                for y in eff.y..(eff.y + eff.h as i32) {
                    let row = (y as usize) * stride;
                    for x in eff.x..(eff.x + eff.w as i32) {
                        screen.blend_pixel_a4(row + x as usize, overlay_raw, alpha);
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
                draw_sim_text_cmd(
                    &mut screen,
                    full,
                    stride,
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
                draw_sim_text_cmd(
                    &mut screen,
                    full,
                    stride,
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
            PaintCmd::Line {
                p0,
                p1,
                clip,
                color,
                width,
                aa,
            } => {
                liuli::render::vector::draw_line(&mut screen, full, p0, p1, clip, color, width, aa);
            }
            PaintCmd::Circle {
                center,
                clip,
                radius,
                fill,
                border_color,
                border_width,
            } => {
                liuli::render::vector::draw_circle(
                    &mut screen,
                    full,
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
                liuli::render::vector::draw_arc(
                    &mut screen,
                    full,
                    center,
                    radius,
                    start_deg,
                    sweep_deg,
                    width,
                    cap_round,
                    clip,
                    color,
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
                liuli::render::vector::draw_gradient_rect(
                    &mut screen,
                    full,
                    rect,
                    clip,
                    start,
                    end,
                    direction,
                    radius,
                );
            }
        }
    }

    Ok(screen)
}

#[allow(clippy::too_many_arguments)]
fn draw_sim_text_cmd<S: SimTextSource>(
    screen: &mut MemoryScreen,
    full: Rect,
    stride: usize,
    text_source: &S,
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
) -> Result<(), S::Error> {
    let eff_clip = clip.intersect(full);
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
    let text_raw = color.into_storage();
    let decor_pad = if style.underline() || style.strikethrough() {
        4
    } else {
        0
    };
    let vis_y = Some((
        width_limit_clip.y - pos.y - decor_pad,
        (width_limit_clip.y + width_limit_clip.h as i32) - pos.y + decor_pad,
    ));
    let mut row_buf = [0u8; 256];
    text_source.draw_text(
        screen,
        stride,
        width_limit_clip,
        pos,
        s,
        text_raw,
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

fn rounded_rect_threshold(r: u32) -> i32 {
    match r {
        0 => 1,
        1 => 3,
        2 => 14,
        _ => ((r * 2) as i32).pow(2),
    }
}

fn corner_distance_2x(rect: Rect, r: u32, p: Point) -> i64 {
    if rect.is_empty() {
        return i64::from(1023 * 1023 * 2);
    }
    let r2 = if r == 0 { 0 } else { 2 * i64::from(r) - 1 };
    let x2 = 2 * i64::from(p.x);
    let y2 = 2 * i64::from(p.y);
    let dx2 = (2 * rect.left64() + r2 - x2)
        .max(x2 - (2 * (rect.right64() - 1) - r2))
        .clamp(0, 1023);
    let dy2 = (2 * rect.top64() + r2 - y2)
        .max(y2 - (2 * (rect.bottom64() - 1) - r2))
        .clamp(0, 1023);
    dx2 * dx2 + dy2 * dy2
}

// Independent direct convolution over the full framebuffer (no sampling halo or running sums).
fn reference_blur(screen: &MemoryScreen, r: u8) -> MemoryScreen {
    let mut image = screen.clone();
    let radius = i32::from(r);
    let count = (2 * radius + 1) as u32;
    for vertical in [false, true] {
        let src = image.clone();
        for y in 0..screen.h as i32 {
            for x in 0..screen.w as i32 {
                let mut red = 0;
                let mut green = 0;
                let mut blue = 0;
                for t in -radius..=radius {
                    let sx = (if vertical { x } else { x + t }).clamp(0, screen.w as i32 - 1);
                    let sy = (if vertical { y + t } else { y }).clamp(0, screen.h as i32 - 1);
                    let p = u32::from(src.pixel(sx as u32, sy as u32));
                    red += p >> 11;
                    green += (p >> 5) & 63;
                    blue += p & 31;
                }
                let avg = |s: u32| (s + count / 2) / count;
                image.pixels[y as usize * screen.w as usize + x as usize] =
                    ((avg(red) << 11) | (avg(green) << 5) | avg(blue)) as u16;
            }
        }
    }
    image
}

#[inline]
fn blit_sim_a4_cov(screen: &mut MemoryScreen, idx: usize, fg_raw: u16, cov: u8, perceptual: bool) {
    if cov == 0 {
        return;
    }
    if screen.alpha.is_some() {
        screen.blend_pixel_a4(idx, fg_raw, cov);
        return;
    }
    if cov == 15 {
        screen.pixels[idx] = fg_raw;
    } else {
        let bg = screen.pixels[idx];
        let c = if perceptual {
            gamma_remap_a4(cov, fg_raw, bg)
        } else {
            cov
        };
        if c != 0 {
            screen.pixels[idx] = blend_rgb565_a4_u16(fg_raw, bg, c);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_sim_glyph_row(
    screen: &mut MemoryScreen,
    row_off: usize,
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
    let has_alpha = screen.alpha.is_some();
    match format {
        GlyphFormat::A4 => {
            for px in vis_x0..vis_x1 {
                let sx = (px - gx) as usize;
                let b = row[sx >> 1];
                let cov = if (sx & 1) == 0 { b >> 4 } else { b & 0x0f };
                blit_sim_a4_cov(screen, row_off + px as usize, fg_raw, cov, perceptual);
            }
        }
        GlyphFormat::A1 => {
            for px in vis_x0..vis_x1 {
                let sx = (px - gx) as usize;
                if ((row[sx >> 3] >> (7 - (sx & 7))) & 1) != 0 {
                    blit_sim_a4_cov(screen, row_off + px as usize, fg_raw, 15, false);
                }
            }
        }
        GlyphFormat::A2 => {
            for px in vis_x0..vis_x1 {
                let sx = (px - gx) as u16;
                let cov = a2_pixel(row, gw, sx, 0);
                if cov != 0 {
                    blit_sim_a4_cov(screen, row_off + px as usize, fg_raw, cov, perceptual);
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
                let idx = row_off + px as usize;
                if has_alpha {
                    let a4 = ((u16::from(a8) * 15 + 127) / 255) as u8;
                    screen.blend_pixel_a4(idx, fg_raw, a4);
                } else if a8 == 255 {
                    screen.pixels[idx] = fg_raw;
                } else {
                    let bg = screen.pixels[idx];
                    let a = if perceptual {
                        gamma_remap_a8(a8, fg_raw, bg)
                    } else {
                        a8
                    };
                    screen.pixels[idx] = blend_rgb565_a8_u16(fg_raw, bg, a);
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
                let idx = row_off + px as usize;
                if has_alpha || subpixel == SubpixelOrder::None {
                    let cov = ((u16::from(r) + u16::from(g) * 2 + u16::from(b) + 2) >> 2) as u8;
                    blit_sim_a4_cov(screen, idx, fg_raw, cov, perceptual);
                } else {
                    let bg = screen.pixels[idx];
                    let (cr, cg, cb) = if perceptual {
                        (
                            gamma_remap_a4(r, fg_raw, bg),
                            gamma_remap_a4(g, fg_raw, bg),
                            gamma_remap_a4(b, fg_raw, bg),
                        )
                    } else {
                        (r, g, b)
                    };
                    screen.pixels[idx] = blend_rgb565_lcd_a4_u16(
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

fn draw_sim_h_span(
    screen: &mut MemoryScreen,
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
    let row_off = (y as usize) * stride;
    for x in sx0..sx1 {
        screen.put_opaque(row_off + x as usize, fg_raw);
    }
}

fn draw_sim_line_decor(
    screen: &mut MemoryScreen,
    pos: Point,
    clip: Rect,
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
        draw_sim_h_span(screen, stride, clip, x0, x1, uy, fg_raw);
    }
    if style.strikethrough() {
        let sy = pos
            .y
            .saturating_add(dec.baseline_y)
            .saturating_sub((i32::from(dec.ascent) * 3) / 8);
        draw_sim_h_span(screen, stride, clip, x0, x1, sy, fg_raw);
    }
}
