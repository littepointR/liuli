use crate::band::BandTarget;
#[cfg(test)]
use liuli_core::geometry::u16_to_rgb565;
use liuli_core::{
    effects::{Projection, Shadow},
    geometry::{Rect, Rgb565, rgb565_to_u16},
};
use liuli_text::blend_rgb565_a4_u16;

const TILE_W: usize = 32;

/// Exact unsigned division by 15 (`n <= 4681`, covers RGB565 a4 blend up to 952).
#[inline(always)]
fn div15(n: u32) -> u32 {
    (n * 4370) >> 16
}

/// Exact fast RGB565 4-bit alpha blend on raw `u16` colors matching `liuli_text::blend_rgb565_a4`.
#[inline(always)]
fn blend_a4_u16(fg_raw: u16, bg: u16, alpha: u8) -> u16 {
    blend_rgb565_a4_u16(fg_raw, bg, alpha)
}

#[cfg(test)]
#[inline(always)]
fn blend_a4_fast(fg: Rgb565, bg: u16, alpha: u8) -> u16 {
    blend_a4_u16(rgb565_to_u16(fg), bg, alpha)
}

/// Precomputed 4-bit alpha blender for a loop-invariant foreground color and alpha.
#[derive(Clone, Copy)]
struct BlendA4Pre {
    alpha: u8,
    inv: u32,
    fg_raw: u16,
    fg_r_a: u32,
    fg_g_a: u32,
    fg_b_a: u32,
}

impl BlendA4Pre {
    #[inline(always)]
    fn new(fg: Rgb565, alpha: u8) -> Self {
        let fg_raw = rgb565_to_u16(fg);
        let a = u32::from(alpha);
        let raw = u32::from(fg_raw);
        Self {
            alpha,
            inv: 15 - a,
            fg_raw,
            fg_r_a: (raw >> 11) * a + 7,
            fg_g_a: ((raw >> 5) & 63) * a + 7,
            fg_b_a: (raw & 31) * a + 7,
        }
    }

    #[inline(always)]
    fn blend(self, bg: u16) -> u16 {
        if self.alpha == 0 {
            return bg;
        }
        if self.alpha >= 15 {
            return self.fg_raw;
        }
        let bg_raw = u32::from(bg);
        let r = div15(self.fg_r_a + (bg_raw >> 11) * self.inv);
        let g = div15(self.fg_g_a + ((bg_raw >> 5) & 63) * self.inv);
        let b = div15(self.fg_b_a + (bg_raw & 31) * self.inv);
        ((r << 11) | (g << 5) | b) as u16
    }
}

/// Exact 2x threshold matching `embedded_graphics::primitives::circle::diameter_to_threshold(2 * r)`.
#[inline(always)]
fn rounded_rect_threshold(r: u32) -> i32 {
    match r {
        0 => 1,
        1 => 3,
        2 => 14,
        _ => ((r * 2) as i32).pow(2),
    }
}

/// Exact rounded-box containment and 2x squared-distance evaluator matching
/// `embedded_graphics::primitives::RoundedRectangle`, using 32-bit math for display-range
/// coordinates and falling back to `i64` for extreme logical rects.
#[derive(Clone, Copy)]
struct RoundedBox {
    rect: Rect,
    r: u32,
    thresh: i32,
    fast: bool,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    core_l: i32,
    core_r: i32,
    core_t: i32,
    core_b: i32,
    il2: i32,
    ir2: i32,
    it2: i32,
    ib2: i32,
}

impl RoundedBox {
    #[inline(always)]
    fn new(rect: Rect, radius: u32) -> Self {
        let r_i32 = radius as i32;
        let fast = !rect.is_empty()
            && rect.x >= -500_000
            && rect.y >= -500_000
            && rect.right64() <= 500_000
            && rect.bottom64() <= 500_000;
        let (x0, x1, y0, y1) = if fast {
            (
                rect.x,
                rect.right64() as i32 - 1,
                rect.y,
                rect.bottom64() as i32 - 1,
            )
        } else {
            (0, -1, 0, -1)
        };
        let r2 = if radius == 0 { 0 } else { 2 * r_i32 - 1 };
        Self {
            rect,
            r: radius,
            thresh: rounded_rect_threshold(radius),
            fast,
            x0,
            x1,
            y0,
            y1,
            core_l: x0 + r_i32,
            core_r: x1 - r_i32,
            core_t: y0 + r_i32,
            core_b: y1 - r_i32,
            il2: x0 * 2 + r2,
            ir2: x1 * 2 - r2,
            it2: y0 * 2 + r2,
            ib2: y1 * 2 - r2,
        }
    }

    #[inline(always)]
    fn distance2_fast(self, x: i32, y: i32) -> i32 {
        if self.fast && (-500_000..=500_000).contains(&x) && (-500_000..=500_000).contains(&y) {
            let x2 = x * 2;
            let y2 = y * 2;
            let dx2 = (self.il2 - x2).max(x2 - self.ir2).clamp(0, 1023);
            let dy2 = (self.it2 - y2).max(y2 - self.ib2).clamp(0, 1023);
            dx2 * dx2 + dy2 * dy2
        } else {
            distance2_2x(self.rect, self.r, x, y)
        }
    }

    #[inline(always)]
    fn contains(self, x: i32, y: i32) -> bool {
        if self.fast {
            if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
                return false;
            }
            if (x >= self.core_l && x <= self.core_r) || (y >= self.core_t && y <= self.core_b) {
                return true;
            }
            let x2 = x * 2;
            let y2 = y * 2;
            let dx2 = (self.il2 - x2).max(x2 - self.ir2).clamp(0, 1023);
            let dy2 = (self.it2 - y2).max(y2 - self.ib2).clamp(0, 1023);
            dx2 * dx2 + dy2 * dy2 < self.thresh
        } else {
            distance2_2x(self.rect, self.r, x, y) < self.thresh
        }
    }
}

/// 2x squared distance to the rounded-corner core of `rect`, aligned with `embedded_graphics`.
#[inline(always)]
fn distance2_2x(rect: Rect, r: u32, x: i32, y: i32) -> i32 {
    if rect.is_empty() {
        return 1023 * 1023 * 2;
    }
    let r2 = if r == 0 { 0 } else { 2 * i64::from(r) - 1 };
    let x2 = 2 * i64::from(x);
    let y2 = 2 * i64::from(y);
    let dx2 = (2 * rect.left64() + r2 - x2)
        .max(x2 - (2 * (rect.right64() - 1) - r2))
        .clamp(0, 1023) as i32;
    let dy2 = (2 * rect.top64() + r2 - y2)
        .max(y2 - (2 * (rect.bottom64() - 1) - r2))
        .clamp(0, 1023) as i32;
    // Radius and falloff are bytes. Beyond 510 pixels all shadow coverage is zero.
    dx2 * dx2 + dy2 * dy2
}

/// Fixed-point reciprocal for shadow radial falloff `((outer - d) * opacity) / (outer - inner)`.
#[derive(Clone, Copy)]
enum FalloffRecip {
    None,
    /// Q32 reciprocal exact for `denom in 2..=16_384` (widening 32×32->64 multiply).
    Q32(u32),
    /// Q46 reciprocal exact for `denom in 16_385..=1_040_400`.
    Q46(u64),
}

impl FalloffRecip {
    #[inline(always)]
    fn new(denom: i32) -> Self {
        if denom <= 1 {
            Self::None
        } else if denom <= 16_384 {
            let d = denom as u64;
            Self::Q32((1u64 << 32).div_ceil(d) as u32)
        } else {
            let d = denom as u64;
            Self::Q46((1u64 << 46).div_ceil(d))
        }
    }

    #[inline(always)]
    fn eval(self, diff: u32, opacity: u8) -> u8 {
        match self {
            Self::None => 0,
            Self::Q32(recip) => {
                let num = diff * u32::from(opacity);
                ((u64::from(num) * u64::from(recip)) >> 32) as u8
            }
            Self::Q46(recip) => {
                let num = u64::from(diff) * u64::from(opacity);
                ((num * recip) >> 46) as u8
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn eval_shadow_alpha(
    shifted_box: RoundedBox,
    outer_x0: i32,
    outer_x1: i32,
    outer_y0: i32,
    outer_y1: i32,
    inner_thresh: i32,
    outer: i32,
    opacity: u8,
    recip: FalloffRecip,
    x: i32,
    y: i32,
) -> u8 {
    if shifted_box.fast && (x < outer_x0 || x > outer_x1 || y < outer_y0 || y > outer_y1) {
        return 0;
    }
    let d = shifted_box.distance2_fast(x, y);
    if d < inner_thresh {
        opacity
    } else if d >= outer {
        0
    } else {
        recip.eval((outer - d) as u32, opacity)
    }
}

pub fn draw_shadow(
    target: &mut BandTarget<'_>,
    rect: Rect,
    clip: Rect,
    s: Shadow,
    projection: Projection,
    radius: u8,
) {
    if s.opacity == 0 || matches!(projection.0, 64 | 192) {
        return;
    }
    let band = target.band_rect();
    let shifted = rect.translate(s.x.into(), s.y.into());
    let bounds = s.bounds(rect);
    let area = bounds.intersect(clip).intersect(band);
    if area.is_empty() {
        return;
    }
    let radius = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
    let caster_box = RoundedBox::new(rect, radius);
    let shifted_box = RoundedBox::new(shifted, radius);
    let blur_i32 = i32::from(s.blur);
    let outer_x0 = shifted_box.x0 - blur_i32;
    let outer_x1 = shifted_box.x1 + blur_i32;
    let outer_y0 = shifted_box.y0 - blur_i32;
    let outer_y1 = shifted_box.y1 + blur_i32;
    let inner_thresh = caster_box.thresh;
    let inner_falloff = if radius == 0 { 0 } else { inner_thresh };
    let outer = if s.blur == 0 {
        inner_thresh
    } else {
        ((2 * (radius + u32::from(s.blur))) as i32).pow(2)
    };
    let recip = FalloffRecip::new(outer - inner_falloff);
    let shadow_raw = rgb565_to_u16(s.color);
    let full_blend = BlendA4Pre::new(s.color, s.opacity);
    let band_w = band.w as usize;
    let has_alpha = target.has_alpha();

    if projection.0 == 0 {
        let r_i32 = radius as i32;
        let rect_x = rect.x;
        let rect_right = rect.right64() as i32;
        let rect_y = rect.y;
        let rect_bottom = rect.bottom64() as i32;
        let area_left = area.x;
        let area_right = area.right64() as i32;

        for y in area.y..area.bottom64() as i32 {
            let row_base = (y - band.y) as usize * band_w;
            // Inside `[rect_y, rect_bottom)`, exclude the opaque interior span of the caster in O(1).
            let (skip_l, skip_r) = if y >= rect_y && y < rect_bottom {
                if y >= rect_y + r_i32 && y < rect_bottom - r_i32 {
                    (
                        rect_x.clamp(area_left, area_right),
                        rect_right.clamp(area_left, area_right),
                    )
                } else {
                    (
                        (rect_x + r_i32).clamp(area_left, area_right),
                        (rect_right - r_i32).clamp(area_left, area_right),
                    )
                }
            } else {
                (area_right, area_right)
            };

            for (x0, x1) in [(area_left, skip_l), (skip_r.max(skip_l), area_right)] {
                for x in x0..x1 {
                    if caster_box.contains(x, y) {
                        continue;
                    }
                    let alpha = eval_shadow_alpha(
                        shifted_box,
                        outer_x0,
                        outer_x1,
                        outer_y0,
                        outer_y1,
                        inner_thresh,
                        outer,
                        s.opacity,
                        recip,
                        x,
                        y,
                    );
                    if alpha != 0 {
                        let idx = row_base + (x - band.x) as usize;
                        if has_alpha {
                            target.blend_pixel_a4(idx, shadow_raw, alpha);
                        } else {
                            let words = target.words_mut();
                            words[idx] = if alpha == s.opacity {
                                full_blend.blend(words[idx])
                            } else {
                                blend_a4_u16(shadow_raw, words[idx], alpha)
                            };
                        }
                    }
                }
            }
        }
        return;
    }

    let fit_recip = projection.fit_recip();
    let area_right = area.right64() as i32;
    let area_bottom = area.bottom64() as i32;
    for tile_x in (area.x..area_right).step_by(TILE_W) {
        let tile_end = (tile_x + TILE_W as i32).min(area_right);
        let len = (tile_end - tile_x) as usize;
        let mut foot_cols = [None; TILE_W];
        let mut cast_cols = [None; TILE_W];
        let mut cast_in_core = [false; TILE_W];
        let mut any_foot = false;
        for lx in 0..len {
            let x = tile_x + lx as i32;
            foot_cols[lx] = projection.column(bounds, x);
            let cast_col = projection.column(rect, x);
            cast_cols[lx] = cast_col;
            cast_in_core[lx] = caster_box.fast
                && cast_col
                    .is_some_and(|c| c.x() >= caster_box.core_l && c.x() <= caster_box.core_r);
            any_foot |= foot_cols[lx].is_some();
        }
        if !any_foot {
            continue;
        }
        let tile_x_off = (tile_x - band.x) as usize;
        for y in area.y..area_bottom {
            let Some(foot_norm) = Projection::row_norm(bounds.y, bounds.h, y) else {
                continue;
            };
            let cast_norm = Projection::row_norm(rect.y, rect.h, y);
            let row_off = (y - band.y) as usize * band_w + tile_x_off;
            for lx in 0..len {
                if let (Some(caster), Some(cn)) = (cast_cols[lx], cast_norm) {
                    if cast_in_core[lx] {
                        if caster.contains_y_norm(cn) {
                            continue;
                        }
                    } else if let Some(cp) = caster.at_norm(y, cn, fit_recip)
                        && caster_box.contains(cp.x, cp.y)
                    {
                        continue;
                    }
                }
                let Some(foot) = foot_cols[lx] else {
                    continue;
                };
                let Some(p) = foot.at_norm(y, foot_norm, fit_recip) else {
                    continue;
                };
                let alpha = eval_shadow_alpha(
                    shifted_box,
                    outer_x0,
                    outer_x1,
                    outer_y0,
                    outer_y1,
                    inner_thresh,
                    outer,
                    s.opacity,
                    recip,
                    p.x,
                    p.y,
                );
                if alpha != 0 {
                    let idx = row_off + lx;
                    if has_alpha {
                        target.blend_pixel_a4(idx, shadow_raw, alpha);
                    } else {
                        let words = target.words_mut();
                        words[idx] = if alpha == s.opacity {
                            full_blend.blend(words[idx])
                        } else {
                            blend_a4_u16(shadow_raw, words[idx], alpha)
                        };
                    }
                }
            }
        }
    }
}

#[cfg(feature = "layers")]
mod layers {
    use super::*;
    use liuli_core::layers::{Backdrop, GroupPixels, LayerCache, get_a4};

    const BLUR_TILE_W: usize = 16;

    /// Q20 ceiling reciprocals for `divisor = 2 * r + 1`, `r in 0..=8`.
    const BLUR_RECIP: [u32; 9] = [
        1 << 20,
        (1u32 << 20).div_ceil(3),
        (1u32 << 20).div_ceil(5),
        (1u32 << 20).div_ceil(7),
        (1u32 << 20).div_ceil(9),
        (1u32 << 20).div_ceil(11),
        (1u32 << 20).div_ceil(13),
        (1u32 << 20).div_ceil(15),
        (1u32 << 20).div_ceil(17),
    ];

    #[inline(always)]
    fn blur_pack(sr: i32, sg: i32, sb: i32, half: i32, recip: u32) -> u16 {
        let r = ((u32::from((sr + half) as u16) * recip) >> 20) as u16;
        let g = ((u32::from((sg + half) as u16) * recip) >> 20) as u16;
        let b = ((u32::from((sb + half) as u16) * recip) >> 20) as u16;
        (r << 11) | (g << 5) | b
    }

    #[inline(always)]
    fn unpack(v: u16) -> (i32, i32, i32) {
        (
            i32::from(v >> 11),
            i32::from((v >> 5) & 63),
            i32::from(v & 31),
        )
    }

    /// A run of identical scanlines can omit input rows whose complete blur
    /// windows are identical. Keep both transitions and one interior row.
    pub fn redundant_rows(
        rect: Rect,
        radius: u8,
        commands: &[liuli_core::paint::PaintCmd],
    ) -> core::ops::Range<usize> {
        use liuli_core::paint::PaintCmd;
        let height = rect.h as usize;
        let mid = height / 2;
        let mut uniform = 0..height;
        for cmd in commands {
            let overlap = cmd.bounds().intersect(rect);
            if overlap.is_empty() {
                continue;
            }
            let start = (overlap.y - rect.y) as usize;
            let end = start + overlap.h as usize;
            if end <= mid {
                uniform.start = uniform.start.max(end);
            } else if start > mid {
                uniform.end = uniform.end.min(start);
            } else if matches!(cmd, PaintCmd::FillRect { .. } | PaintCmd::Overlay { .. }) {
                uniform.start = uniform.start.max(start);
                uniform.end = uniform.end.min(end);
            } else {
                return height..height;
            }
        }
        let r = usize::from(radius);
        let first = uniform.start + usize::from(uniform.start != 0) * r + 1;
        let end = uniform
            .end
            .saturating_sub(usize::from(uniform.end != height) * r);
        if first < end {
            first..end
        } else {
            height..height
        }
    }

    /// Input contains the retained scanlines packed at the front, in logical order.
    pub fn filter_backdrop(
        backdrop: &mut Backdrop,
        rect: Rect,
        radius: u8,
        omitted: core::ops::Range<usize>,
        tint: impl FnOnce(&mut [u16]),
    ) {
        let w = rect.w as usize;
        let h = rect.h as usize;
        let pixels = &mut backdrop.pixels[..w * h];
        let r = usize::from(radius);
        if w == 0 || h == 0 {
            tint(pixels);
            return;
        }
        let skip = omitted.len();
        let kept = h - skip;
        if r != 0 {
            blur_rows(&mut pixels[..kept * w], w, kept, r);
        }
        tint(&mut pixels[..kept * w]);
        if skip > 0 {
            pixels.copy_within(omitted.start * w..kept * w, omitted.end * w);
            let (head, tail) = pixels.split_at_mut(omitted.start * w);
            let row = &head[head.len() - w..];
            for dst in tail[..skip * w].chunks_exact_mut(w) {
                dst.copy_from_slice(row);
            }
        }
    }

    // Two separable in-place running sums using stack ring buffers.
    fn blur_rows(pixels: &mut [u16], w: usize, h: usize, r: usize) {
        let recip = BLUR_RECIP[r];
        let half = r as i32; // `(2 * r + 1) / 2 == r`

        // Horizontal in-place pass with a 16-entry ring saving the trailing `r <= 8` inputs.
        let mut h_ring = [0u16; 16];
        for row_idx in 0..h {
            let base = row_idx * w;
            let row = &mut pixels[base..base + w];
            let (mut sr, mut sg, mut sb) = (0i32, 0i32, 0i32);
            for j in 0..=2 * r {
                let (cr, cg, cb) = unpack(row[j.saturating_sub(r).min(w - 1)]);
                sr += cr;
                sg += cg;
                sb += cb;
            }
            for p in 0..w {
                h_ring[p & 15] = row[p];
                row[p] = blur_pack(sr, sg, sb, half, recip);
                if p + 1 < w {
                    let a_raw = if p < r {
                        h_ring[0]
                    } else {
                        h_ring[(p - r) & 15]
                    };
                    let b_raw = row[(p + 1 + r).min(w - 1)];
                    let (ar, ag, ab) = unpack(a_raw);
                    let (br, bg, bb) = unpack(b_raw);
                    sr += br - ar;
                    sg += bg - ag;
                    sb += bb - ab;
                }
            }
        }

        if h == 1 {
            return;
        }
        // Vertical in-place pass: 16-column tiles swept row-major with a 16-row stack ring.
        let mut v_ring = [[0u16; BLUR_TILE_W]; 16];
        for col_start in (0..w).step_by(BLUR_TILE_W) {
            let len = (w - col_start).min(BLUR_TILE_W);
            let mut sr = [0i32; BLUR_TILE_W];
            let mut sg = [0i32; BLUR_TILE_W];
            let mut sb = [0i32; BLUR_TILE_W];
            for j in 0..=2 * r {
                let row = j.saturating_sub(r).min(h - 1);
                let src_row = &pixels[row * w + col_start..row * w + col_start + len];
                for lx in 0..len {
                    let (cr, cg, cb) = unpack(src_row[lx]);
                    sr[lx] += cr;
                    sg[lx] += cg;
                    sb[lx] += cb;
                }
            }
            for y in 0..h {
                let off = y * w + col_start;
                v_ring[y & 15][..len].copy_from_slice(&pixels[off..off + len]);
                let dst_row = &mut pixels[off..off + len];
                for lx in 0..len {
                    dst_row[lx] = blur_pack(sr[lx], sg[lx], sb[lx], half, recip);
                }
                if y + 1 < h {
                    let a_row = if y < r {
                        &v_ring[0][..len]
                    } else {
                        &v_ring[(y - r) & 15][..len]
                    };
                    let by = (y + 1 + r).min(h - 1) * w + col_start;
                    let b_row = &pixels[by..by + len];
                    for lx in 0..len {
                        let (ar, ag, ab) = unpack(a_row[lx]);
                        let (br, bg, bb) = unpack(b_row[lx]);
                        sr[lx] += br - ar;
                        sg[lx] += bg - ag;
                        sb[lx] += bb - ab;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn composite_pixel(
        color: &[u16],
        alpha_buf: &[u8],
        idx: usize,
        out_word: &mut u16,
        bd_word: u16,
        card_box: RoundedBox,
        px: i32,
        py: i32,
        inside_known: bool,
        has_backdrop: bool,
        tint_blend: BlendA4Pre,
        opacity: u8,
    ) {
        let a = get_a4(alpha_buf, idx);
        if a == 15 {
            let fg = color[idx];
            *out_word = if opacity == 15 {
                fg
            } else {
                blend_a4_u16(fg, *out_word, opacity)
            };
            return;
        }
        let dst_word = *out_word;
        if a == 0 {
            if !has_backdrop && tint_blend.alpha == 0 {
                return;
            }
            if !(inside_known || card_box.contains(px, py)) {
                return;
            }
            let bg = if has_backdrop {
                bd_word
            } else {
                tint_blend.blend(dst_word)
            };
            *out_word = if opacity == 15 {
                bg
            } else {
                blend_a4_u16(bg, dst_word, opacity)
            };
            return;
        }
        let bg = if (has_backdrop || tint_blend.alpha > 0)
            && (inside_known || card_box.contains(px, py))
        {
            if has_backdrop {
                bd_word
            } else {
                tint_blend.blend(dst_word)
            }
        } else {
            dst_word
        };
        let mixed = blend_rgb565_a4_u16(color[idx], bg, a);
        *out_word = if opacity == 15 {
            mixed
        } else {
            blend_a4_u16(mixed, dst_word, opacity)
        };
    }

    pub fn composite(
        target: &mut BandTarget<'_>,
        rect: Rect,
        cache: &LayerCache,
        group: &GroupPixels,
        backdrop: &Backdrop,
    ) {
        let opacity = cache.style.opacity;
        if opacity == 0 || matches!(cache.style.projection.0, 64 | 192) {
            return;
        }
        let band = target.band_rect();
        let area = cache.source.intersect(band);
        if area.is_empty() {
            return;
        }
        let radius = u32::from(cache.radius).min(rect.w / 2).min(rect.h / 2);
        let card_box = RoundedBox::new(rect, radius);
        let has_backdrop = cache.style.backdrop_radius > 0;
        let tint_blend = BlendA4Pre::new(cache.style.tint_color, cache.style.tint_alpha);
        let band_w = band.w as usize;
        let src_w = cache.source.w as usize;
        let cap_w = cache.capture.w as usize;
        let words = target.words_mut();

        if cache.style.projection.0 == 0 {
            let len = area.w as usize;
            let out_x = (area.x - band.x) as usize;
            let src_x = (area.x - cache.source.x) as usize;
            let bd_x = if has_backdrop {
                (area.x - cache.capture.x) as usize
            } else {
                0
            };
            for y in area.y..area.bottom64() as i32 {
                let out_off = (y - band.y) as usize * band_w + out_x;
                let src_off = (y - cache.source.y) as usize * src_w + src_x;
                let out_row = &mut words[out_off..out_off + len];
                if has_backdrop {
                    let bd_off = (y - cache.capture.y) as usize * cap_w + bd_x;
                    let bd_row = &backdrop.pixels[bd_off..bd_off + len];
                    // Inside the rounded body, fully transparent group pixels show
                    // only the filtered backdrop. Long opaque runs show only the
                    // group's color. Both are contiguous RGB565 copies at opacity 15.
                    let middle = if opacity == 15 && card_box.fast {
                        if (card_box.core_t..=card_box.core_b).contains(&y) {
                            0..len
                        } else {
                            let left = (card_box.core_l - area.x).clamp(0, len as i32) as usize;
                            let right =
                                (card_box.core_r + 1 - area.x).clamp(0, len as i32) as usize;
                            left..right.max(left)
                        }
                    } else {
                        0..0
                    };
                    composite_glass_span(
                        group,
                        src_off + middle.start,
                        &bd_row[middle.clone()],
                        &mut out_row[middle.clone()],
                    );
                    for lx in (0..middle.start).chain(middle.end..len) {
                        let px = area.x + lx as i32;
                        composite_pixel(
                            &group.color,
                            &group.alpha,
                            src_off + lx,
                            &mut out_row[lx],
                            bd_row[lx],
                            card_box,
                            px,
                            y,
                            false,
                            true,
                            tint_blend,
                            opacity,
                        );
                    }
                } else {
                    for (lx, dst_px) in out_row.iter_mut().enumerate().take(len) {
                        let px = area.x + lx as i32;
                        composite_pixel(
                            &group.color,
                            &group.alpha,
                            src_off + lx,
                            dst_px,
                            0,
                            card_box,
                            px,
                            y,
                            false,
                            false,
                            tint_blend,
                            opacity,
                        );
                    }
                }
            }
            return;
        }

        let fit_recip = cache.style.projection.fit_recip();
        let src_x0 = cache.source.x;
        let src_x1 = cache.source.right64() as i32;
        let src_y0 = cache.source.y;
        let src_y1 = cache.source.bottom64() as i32;
        let area_right = area.right64() as i32;
        let area_bottom = area.bottom64() as i32;
        for tile_x in (area.x..area_right).step_by(TILE_W) {
            let tile_end = (tile_x + TILE_W as i32).min(area_right);
            let len = (tile_end - tile_x) as usize;
            let mut cols = [None; TILE_W];
            let mut col_in_core = [false; TILE_W];
            let mut col_x_off = [0usize; TILE_W];
            let mut any_col = false;
            for lx in 0..len {
                let col = cache
                    .style
                    .projection
                    .column(rect, tile_x + lx as i32)
                    .filter(|c| c.x() >= src_x0 && c.x() < src_x1);
                cols[lx] = col;
                if let Some(c) = col {
                    col_in_core[lx] =
                        card_box.fast && c.x() >= card_box.core_l && c.x() <= card_box.core_r;
                    col_x_off[lx] = (c.x() - src_x0) as usize;
                    any_col = true;
                }
            }
            if !any_col {
                continue;
            }
            let out_x = (tile_x - band.x) as usize;
            let bd_x = if has_backdrop {
                (tile_x - cache.capture.x) as usize
            } else {
                0
            };
            for y in area.y..area_bottom {
                let Some(norm) = Projection::row_norm(rect.y, rect.h, y) else {
                    continue;
                };
                let out_off = (y - band.y) as usize * band_w + out_x;
                let out_row = &mut words[out_off..out_off + len];
                let bd_off = if has_backdrop {
                    (y - cache.capture.y) as usize * cap_w + bd_x
                } else {
                    0
                };
                for lx in 0..len {
                    let Some(col) = cols[lx] else {
                        continue;
                    };
                    let Some(p) = col.at_norm(y, norm, fit_recip) else {
                        continue;
                    };
                    if p.y < src_y0 || p.y >= src_y1 {
                        continue;
                    }
                    let i = (p.y - src_y0) as usize * src_w + col_x_off[lx];
                    let bd_word = if has_backdrop {
                        backdrop.pixels[bd_off + lx]
                    } else {
                        0
                    };
                    composite_pixel(
                        &group.color,
                        &group.alpha,
                        i,
                        &mut out_row[lx],
                        bd_word,
                        card_box,
                        p.x,
                        p.y,
                        col_in_core[lx],
                        has_backdrop,
                        tint_blend,
                        opacity,
                    );
                }
            }
        }
    }

    fn composite_glass_span(group: &GroupPixels, start: usize, bg: &[u16], dst: &mut [u16]) {
        let mut x = 0;
        while x < dst.len() {
            let i = start + x;
            if i.is_multiple_of(2) && x + 8 <= dst.len() {
                let mask = &group.alpha[i / 2..i / 2 + 4];
                if mask == [0; 4] || mask == [0xff; 4] {
                    let opaque = mask[0] != 0;
                    let mut end = x + 8;
                    while end + 8 <= dst.len()
                        && group.alpha[(start + end) / 2..(start + end) / 2 + 4] == *mask
                    {
                        end += 8;
                    }
                    let pixels = if opaque {
                        &group.color[i..start + end]
                    } else {
                        &bg[x..end]
                    };
                    dst[x..end].copy_from_slice(pixels);
                    x = end;
                    continue;
                }
            }
            dst[x] = blend_rgb565_a4_u16(group.color[i], bg[x], get_a4(&group.alpha, i));
            x += 1;
        }
    }

    #[cfg(test)]
    #[test]
    fn glass_span_matches_scalar_for_clipped_odd_and_even_alpha_runs() {
        let mut group = GroupPixels {
            color: (0..99).map(|i| (i * 613) as u16).collect(),
            alpha: alloc::vec![0; 50],
            ..Default::default()
        };
        for i in 0..99 {
            let alpha = match i {
                17..=40 => 15,
                47..=69 => (i % 16) as u8,
                _ => 0,
            };
            liuli_core::layers::set_a4(&mut group.alpha, i, alpha);
        }
        let bg: alloc::vec::Vec<_> = (0..99).map(|i| (0xf800 ^ (i * 39)) as u16).collect();
        for start in 0..20 {
            for end in [start, start + 1, 62, 98, 99] {
                let mut dst = alloc::vec![0x1234; end - start];
                composite_glass_span(&group, start, &bg[start..end], &mut dst);
                for (i, actual) in (start..end).zip(dst) {
                    assert_eq!(
                        actual,
                        blend_rgb565_a4_u16(group.color[i], bg[i], get_a4(&group.alpha, i))
                    );
                }
            }
        }
    }
}
#[cfg(feature = "layers")]
pub use layers::{composite, filter_backdrop, redundant_rows};

#[cfg(test)]
mod tests {
    use super::*;
    use liuli_text::blend_rgb565_a4;

    #[test]
    fn fast_dividers_match_exact_integer_division() {
        for n in 0..=1024u32 {
            assert_eq!(div15(n), n / 15, "div15({n})");
        }
    }

    #[test]
    fn fast_blend_a4_matches_reference() {
        let samples = [0u16, 0x001f, 0x07e0, 0xf800, 0x52aa, 0xa514, 0xffff];
        for &fg_u in &samples {
            let fg = u16_to_rgb565(fg_u);
            for &bg in &samples {
                for alpha in 0..=15 {
                    assert_eq!(
                        blend_a4_fast(fg, bg, alpha),
                        rgb565_to_u16(blend_rgb565_a4(fg, u16_to_rgb565(bg), alpha)),
                        "fg={fg_u:#06x} bg={bg:#06x} alpha={alpha}"
                    );
                }
            }
        }
    }

    #[test]
    fn fast_falloff_matches_reference_division() {
        for denom in [1i32, 2, 15, 64, 128, 1024, 16_384, 260_100, 1_040_400] {
            let recip = FalloffRecip::new(denom);
            for opacity in [1u8, 7, 15] {
                for diff in [0, 1, denom / 2, denom - 1, denom] {
                    if denom <= 1 && diff > 0 {
                        continue;
                    }
                    let expected = (diff * i32::from(opacity) / denom) as u8;
                    let actual = recip.eval(diff as u32, opacity);
                    assert_eq!(
                        actual, expected,
                        "denom={denom} diff={diff} opacity={opacity}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "layers")]
    #[test]
    fn backdrop_rows_respect_clips_and_nonuniform_paint() {
        use liuli_core::paint::PaintCmd;
        let capture = Rect::new(20, 30, 40, 40);
        let color = Rgb565::new(1, 2, 3);
        let stripe = PaintCmd::FillRect {
            rect: capture,
            clip: Rect::new(25, 33, 10, 32),
            color,
        };
        let shadowed = |rect| PaintCmd::StrokeRect {
            rect,
            clip: capture,
            color,
            width: 1,
        };
        assert_eq!(redundant_rows(capture, 4, &[stripe]), 8..31);
        assert_eq!(
            redundant_rows(capture, 4, &[stripe, shadowed(Rect::new(22, 36, 3, 4))]),
            15..31
        );
        assert!(redundant_rows(capture, 4, &[stripe, shadowed(capture)]).is_empty());
    }

    #[cfg(feature = "layers")]
    #[test]
    fn blur_recip_and_inplace_blur_match_reference() {
        use alloc::vec::Vec;
        use liuli_core::layers::Backdrop;

        fn ref_blur(src: &[u16], w: usize, h: usize, radius: u8) -> Vec<u16> {
            let mut scratch = src.to_vec();
            let mut out = src.to_vec();
            if w == 0 || h == 0 || radius == 0 {
                return out;
            }
            let r = usize::from(radius);
            let div = (2 * r + 1) as i32;
            let unpack = |v: u16| -> [i32; 3] {
                [
                    i32::from(v >> 11),
                    i32::from((v >> 5) & 63),
                    i32::from(v & 31),
                ]
            };
            let pack =
                |c: [i32; 3]| -> u16 { ((c[0] as u16) << 11) | ((c[1] as u16) << 5) | c[2] as u16 };
            for y in 0..h {
                for x in 0..w {
                    let mut sum = [0; 3];
                    for t in -(r as isize)..=(r as isize) {
                        let sx = (x as isize + t).clamp(0, w as isize - 1) as usize;
                        let c = unpack(src[y * w + sx]);
                        for k in 0..3 {
                            sum[k] += c[k];
                        }
                    }
                    scratch[y * w + x] = pack(sum.map(|n| (n + div / 2) / div));
                }
            }
            for y in 0..h {
                for x in 0..w {
                    let mut sum = [0; 3];
                    for t in -(r as isize)..=(r as isize) {
                        let sy = (y as isize + t).clamp(0, h as isize - 1) as usize;
                        let c = unpack(scratch[sy * w + x]);
                        for k in 0..3 {
                            sum[k] += c[k];
                        }
                    }
                    out[y * w + x] = pack(sum.map(|n| (n + div / 2) / div));
                }
            }
            out
        }

        for r in 0..=8u8 {
            for w in [1usize, 2, 7, 16, 17, 65, 200] {
                for h in [1usize, 5, 17, 64] {
                    let mut init = Vec::with_capacity(w * h);
                    for y in 0..h {
                        for x in 0..w {
                            let c = (((x * 37 + y * 17) & 31) << 11)
                                | (((x * 13 + y * 29) & 63) << 5)
                                | ((x * 7 + y * 53) & 31);
                            init.push(c as u16);
                        }
                    }
                    let expected = ref_blur(&init, w, h, r);
                    let mut backdrop = Backdrop {
                        pixels: init,
                        clear: None,
                        tint: None,
                    };
                    filter_backdrop(
                        &mut backdrop,
                        Rect::new(0, 0, w as u32, h as u32),
                        r,
                        h..h,
                        |_| {},
                    );
                    assert_eq!(backdrop.pixels, expected, "w={w} h={h} r={r}");
                }
            }
        }

        // Scrolling a clipped pane puts runs of identical scanlines against
        // different content at either edge of the capture. Check both seams
        // and tint against the independent, uncompressed two-pass reference.
        let w = 67;
        let h = 47;
        for r in 1..=8 {
            for run in [0..h, 0..26, 9..37, 21..h, 11..14] {
                let input: Vec<u16> = (0..h)
                    .flat_map(|y| {
                        let y = if run.contains(&y) { run.start } else { y };
                        (0..w).map(move |x| ((x * 637 + y * 919) & 0xffff) as u16)
                    })
                    .collect();
                let mut expected = ref_blur(&input, w, h, r);
                let tint =
                    |row: &mut [u16]| crate::raster::blend_solid(row, Rgb565::new(4, 33, 12), 7);
                tint(&mut expected);
                let mut backdrop = Backdrop {
                    pixels: input,
                    clear: None,
                    tint: None,
                };
                let rect = Rect::new(7, 30, w as u32, h as u32);
                let cmd = liuli_core::paint::PaintCmd::FillRect {
                    rect: Rect::new(7, 30 + run.start as i32, w as u32, run.len() as u32),
                    clip: rect,
                    color: Rgb565::new(0, 0, 0),
                };
                // Rows outside the synthetic run vary; mark each as a separate draw.
                let mut commands = alloc::vec![cmd];
                for y in (0..h).filter(|y| !run.contains(y)) {
                    commands.push(liuli_core::paint::PaintCmd::FillRect {
                        rect: Rect::new(7, 30 + y as i32, w as u32, 1),
                        clip: rect,
                        color: Rgb565::new(0, 0, 0),
                    });
                }
                let omitted = redundant_rows(rect, r, &commands);
                backdrop
                    .pixels
                    .copy_within(omitted.end * w.., omitted.start * w);
                filter_backdrop(&mut backdrop, rect, r, omitted, tint);
                assert_eq!(backdrop.pixels, expected, "run={run:?} r={r}");
            }
        }
    }

    #[test]
    fn rounded_box_and_precomputed_blend_match_reference() {
        let samples = [0u16, 0x001f, 0x07e0, 0xf800, 0x52aa, 0xa514, 0xffff];
        for &fg_u in &samples {
            let fg = u16_to_rgb565(fg_u);
            for alpha in 0..=15 {
                let pre = BlendA4Pre::new(fg, alpha);
                for &bg in &samples {
                    let expected = rgb565_to_u16(blend_rgb565_a4(fg, u16_to_rgb565(bg), alpha));
                    assert_eq!(
                        pre.blend(bg),
                        expected,
                        "fg={fg_u:#06x} bg={bg:#06x} alpha={alpha}"
                    );
                    assert_eq!(
                        blend_a4_u16(fg_u, bg, alpha),
                        expected,
                        "u16 fg={fg_u:#06x} bg={bg:#06x} alpha={alpha}"
                    );
                }
            }
        }

        use embedded_graphics::{
            geometry::Size,
            primitives::{ContainsPoint, RoundedRectangle},
        };
        for (w, h) in [
            (200u32, 140u32),
            (24, 24),
            (25, 25),
            (20, 10),
            (1, 1),
            (2, 2),
            (5, 4),
        ] {
            let rect = Rect::new(20, 40, w, h);
            for r in 0..=16u32 {
                let r_clamped = r.min(rect.w / 2).min(rect.h / 2);
                let box_fast = RoundedBox::new(rect, r_clamped);
                let eg_rr = RoundedRectangle::with_equal_corners(rect.to_eg(), Size::new(r, r));
                let thresh = rounded_rect_threshold(r_clamped);
                for y in (rect.y - 4)..=(rect.bottom64() as i32 + 3) {
                    for x in (rect.x - 4)..=(rect.right64() as i32 + 3) {
                        let expected_d2 = distance2_2x(rect, r_clamped, x, y);
                        assert_eq!(
                            box_fast.distance2_fast(x, y),
                            expected_d2,
                            "w={w} h={h} r={r} x={x} y={y}"
                        );
                        let contains = box_fast.contains(x, y);
                        assert_eq!(
                            contains,
                            expected_d2 < thresh,
                            "thresh w={w} h={h} r={r} x={x} y={y}"
                        );
                        assert_eq!(
                            contains,
                            eg_rr.contains(embedded_graphics::geometry::Point::new(x, y)),
                            "eg_rr parity w={w} h={h} r={r} x={x} y={y}"
                        );
                    }
                }
            }
        }
    }
}
