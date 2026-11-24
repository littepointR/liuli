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

/// Exact rounded-box containment and squared-distance evaluator using 32-bit math
/// for display-range coordinates and falling back to `i64` for extreme logical rects.
#[derive(Clone, Copy)]
struct RoundedBox {
    rect: Rect,
    r: u32,
    r_sq: i32,
    fast: bool,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    il: i32,
    ir: i32,
    it: i32,
    ib: i32,
}

impl RoundedBox {
    #[inline(always)]
    fn new(rect: Rect, radius: u32) -> Self {
        let r_i32 = radius as i32;
        let fast = !rect.is_empty()
            && rect.x >= -1_000_000
            && rect.y >= -1_000_000
            && rect.right64() <= 1_000_000
            && rect.bottom64() <= 1_000_000;
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
        Self {
            rect,
            r: radius,
            r_sq: r_i32 * r_i32,
            fast,
            x0,
            x1,
            y0,
            y1,
            il: x0 + r_i32,
            ir: x1 - r_i32,
            it: y0 + r_i32,
            ib: y1 - r_i32,
        }
    }

    #[inline(always)]
    fn distance2_fast(self, x: i32, y: i32) -> i32 {
        if self.fast
            && (-1_000_000..=1_000_000).contains(&x)
            && (-1_000_000..=1_000_000).contains(&y)
        {
            let dx = (self.il - x).max(x - self.ir).clamp(0, 511);
            let dy = (self.it - y).max(y - self.ib).clamp(0, 511);
            dx * dx + dy * dy
        } else {
            distance2(self.rect, self.r, x, y)
        }
    }

    #[inline(always)]
    fn contains(self, x: i32, y: i32) -> bool {
        if self.fast {
            if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
                return false;
            }
            if (x >= self.il && x <= self.ir) || (y >= self.it && y <= self.ib) {
                return true;
            }
            let dx = if x < self.il {
                self.il - x
            } else {
                x - self.ir
            }
            .min(511);
            let dy = if y < self.it {
                self.it - y
            } else {
                y - self.ib
            }
            .min(511);
            dx * dx + dy * dy <= self.r_sq
        } else {
            distance2(self.rect, self.r, x, y) <= self.r_sq
        }
    }
}

/// Distance to the central rectangle of a rounded box. Used by both the horizontal
/// edges and the four corners, independently of the current band or damage window.
#[inline(always)]
fn distance2(rect: Rect, r: u32, x: i32, y: i32) -> i32 {
    let r = i64::from(r);
    let dx = (rect.left64() + r - i64::from(x))
        .max(i64::from(x) - (rect.right64() - 1 - r))
        .clamp(0, 511) as i32;
    let dy = (rect.top64() + r - i64::from(y))
        .max(i64::from(y) - (rect.bottom64() - 1 - r))
        .clamp(0, 511) as i32;
    // Radius and falloff are bytes. Beyond 510 pixels all shadow coverage is zero.
    dx * dx + dy * dy
}

/// Fixed-point reciprocal for shadow radial falloff `((outer - d) * opacity) / (outer - inner)`.
#[derive(Clone, Copy)]
enum FalloffRecip {
    None,
    /// Q32 reciprocal exact for `denom in 2..=16_384` (widening 32×32->64 multiply).
    Q32(u32),
    /// Q42 reciprocal exact for `denom > 16_384`.
    Q42(u64),
}

impl FalloffRecip {
    #[inline(always)]
    fn new(denom: i32) -> Self {
        if denom <= 1 {
            Self::None
        } else if denom <= 16_384 {
            let d = denom as u64;
            Self::Q32((((1u64 << 32) + d - 1) / d) as u32)
        } else {
            let d = denom as u64;
            Self::Q42(((1u64 << 42) + d - 1) / d)
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
            Self::Q42(recip) => {
                let num = u64::from(diff) * u64::from(opacity);
                ((num * recip) >> 42) as u8
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
    inner: i32,
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
    if d <= inner {
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
    let radius = u32::from(radius)
        .min(rect.w.saturating_sub(1) / 2)
        .min(rect.h.saturating_sub(1) / 2);
    let caster_box = RoundedBox::new(rect, radius);
    let shifted_box = RoundedBox::new(shifted, radius);
    let blur_i32 = i32::from(s.blur);
    let outer_x0 = shifted_box.x0 - blur_i32;
    let outer_x1 = shifted_box.x1 + blur_i32;
    let outer_y0 = shifted_box.y0 - blur_i32;
    let outer_y1 = shifted_box.y1 + blur_i32;
    let inner = caster_box.r_sq;
    let outer = ((radius + u32::from(s.blur)) as i32).pow(2);
    let recip = FalloffRecip::new(outer - inner);
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
                        inner,
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
                && cast_col.is_some_and(|c| c.x() >= caster_box.il && c.x() <= caster_box.ir);
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
                    inner,
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
        ((1u32 << 20) + 2) / 3,
        ((1u32 << 20) + 4) / 5,
        ((1u32 << 20) + 6) / 7,
        ((1u32 << 20) + 8) / 9,
        ((1u32 << 20) + 10) / 11,
        ((1u32 << 20) + 12) / 13,
        ((1u32 << 20) + 14) / 15,
        ((1u32 << 20) + 16) / 17,
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

    /// Two separable running sums executed 100% in-place on `backdrop.pixels`
    /// (zero heap scratch buffer) using small stack ring buffers.
    pub fn blur(backdrop: &mut Backdrop, rect: Rect, radius: u8) {
        let w = rect.w as usize;
        let h = rect.h as usize;
        if w == 0 || h == 0 || radius == 0 {
            return;
        }
        let r = usize::from(radius);
        let recip = BLUR_RECIP[r];
        let half = r as i32; // `(2 * r + 1) / 2 == r`
        let pixels = &mut backdrop.pixels;

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
        let radius = u32::from(cache.radius)
            .min(rect.w.saturating_sub(1) / 2)
            .min(rect.h.saturating_sub(1) / 2);
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
                    for lx in 0..len {
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
                    for lx in 0..len {
                        let px = area.x + lx as i32;
                        composite_pixel(
                            &group.color,
                            &group.alpha,
                            src_off + lx,
                            &mut out_row[lx],
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
                    col_in_core[lx] = card_box.fast && c.x() >= card_box.il && c.x() <= card_box.ir;
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
}
#[cfg(feature = "layers")]
pub use layers::{blur, composite};

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
        for denom in [1i32, 2, 15, 64, 128, 1024, 16_384, 260_100] {
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
                    blur(&mut backdrop, Rect::new(0, 0, w as u32, h as u32), r);
                    assert_eq!(backdrop.pixels, expected, "w={w} h={h} r={r}");
                }
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

        for r in [0u32, 1, 8, 16, 30] {
            let rect = Rect::new(20, 40, 200, 140);
            let r_clamped = r
                .min(rect.w.saturating_sub(1) / 2)
                .min(rect.h.saturating_sub(1) / 2);
            let box_fast = RoundedBox::new(rect, r_clamped);
            let r_sq = (r_clamped as i32).pow(2);
            for y in (rect.y - 12)..=(rect.bottom64() as i32 + 12) {
                for x in (rect.x - 12)..=(rect.right64() as i32 + 12) {
                    let expected_d2 = distance2(rect, r_clamped, x, y);
                    assert_eq!(
                        box_fast.distance2_fast(x, y),
                        expected_d2,
                        "r={r} x={x} y={y}"
                    );
                    assert_eq!(
                        box_fast.contains(x, y),
                        expected_d2 <= r_sq,
                        "contains r={r} x={x} y={y}"
                    );
                }
            }
        }
    }
}
