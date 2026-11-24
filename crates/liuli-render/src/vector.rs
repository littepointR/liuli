use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::geometry::{Point, Rect, rgb565_to_u16};
use liuli_text::blend_rgb565_a4_u16;

/// Target pixel + optional alpha buffer sink for band and full-screen rasterizers.
pub trait PixelSink {
    fn put_opaque(&mut self, idx: usize, fg_raw: u16);
    fn blend_a4(&mut self, idx: usize, fg_raw: u16, cov: u8);
}

impl PixelSink for &mut [u16] {
    #[inline(always)]
    fn put_opaque(&mut self, idx: usize, fg_raw: u16) {
        self[idx] = fg_raw;
    }

    #[inline(always)]
    fn blend_a4(&mut self, idx: usize, fg_raw: u16, cov: u8) {
        self[idx] = blend_rgb565_a4_u16(fg_raw, self[idx], cov);
    }
}

const SIN_Q12: [i16; 91] = [
    0, 71, 143, 214, 286, 357, 428, 499, 570, 641, 711, 782, 852, 921, 991, 1060, 1129, 1198, 1266,
    1334, 1401, 1468, 1534, 1600, 1666, 1731, 1796, 1860, 1923, 1986, 2048, 2110, 2171, 2231, 2290,
    2349, 2408, 2465, 2522, 2578, 2633, 2687, 2741, 2793, 2845, 2896, 2946, 2996, 3044, 3091, 3138,
    3183, 3228, 3271, 3314, 3355, 3396, 3435, 3474, 3511, 3547, 3582, 3617, 3650, 3681, 3712, 3742,
    3770, 3798, 3824, 3849, 3873, 3896, 3917, 3937, 3956, 3974, 3991, 4006, 4021, 4034, 4046, 4056,
    4065, 4074, 4080, 4086, 4090, 4094, 4095, 4096,
];

const TAN_Q12: [u16; 46] = [
    0, 71, 143, 215, 286, 358, 431, 503, 576, 649, 722, 796, 871, 946, 1021, 1098, 1175, 1252,
    1331, 1410, 1491, 1572, 1655, 1739, 1824, 1910, 1998, 2087, 2178, 2270, 2365, 2461, 2559, 2660,
    2763, 2868, 2976, 3087, 3200, 3317, 3437, 3561, 3688, 3820, 3955, 4096,
];

/// Exact integer floor(sqrt(n)) without floating point.
#[inline]
pub fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut bit = 1u64 << 62;
    while bit > x {
        bit >>= 2;
    }
    let mut res = 0u64;
    while bit != 0 {
        if x >= res + bit {
            x -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    res
}

/// Returns `(sin_q12, cos_q12)` in `[-4096..=4096]` for `deg` (`0` = +X right, `90` = +Y down).
#[inline]
pub fn sin_cos_q12(deg: i32) -> (i32, i32) {
    let a = deg.rem_euclid(360) as usize;
    if a <= 90 {
        (i32::from(SIN_Q12[a]), i32::from(SIN_Q12[90 - a]))
    } else if a <= 180 {
        let k = 180 - a;
        (i32::from(SIN_Q12[k]), -i32::from(SIN_Q12[90 - k]))
    } else if a <= 270 {
        let k = a - 180;
        (-i32::from(SIN_Q12[k]), -i32::from(SIN_Q12[90 - k]))
    } else {
        let k = 360 - a;
        (-i32::from(SIN_Q12[k]), i32::from(SIN_Q12[90 - k]))
    }
}

/// Returns angle in Q4 degrees `[0..5760)` for screen vector `(dx, dy)` (`0` = +X, `1440` = +Y).
#[inline]
pub fn atan2_deg_q4(dy: i32, dx: i32) -> i32 {
    let ax = i64::from(dx).abs();
    let ay = i64::from(dy).abs();
    if ax == 0 && ay == 0 {
        return 0;
    }
    let (maj, min_v, swap) = if ax >= ay {
        (ax, ay, false)
    } else {
        (ay, ax, true)
    };
    let ratio = ((min_v << 12) / maj).min(4096) as u16;
    let mut lo = 0usize;
    let mut hi = 45usize;
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if TAN_Q12[mid] <= ratio {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let frac_q4 = if lo >= 45 {
        0
    } else {
        let a0 = i32::from(TAN_Q12[lo]);
        let a1 = i32::from(TAN_Q12[lo + 1]);
        let span = (a1 - a0).max(1);
        ((i32::from(ratio) - a0) * 16 + span / 2) / span
    };
    let oct_q4 = (lo as i32) * 16 + frac_q4;
    let q0_q4 = if swap { 90 * 16 - oct_q4 } else { oct_q4 };
    match (dx >= 0, dy >= 0) {
        (true, true) => q0_q4,
        (false, true) => 180 * 16 - q0_q4,
        (false, false) => 180 * 16 + q0_q4,
        (true, false) => (360 * 16 - q0_q4).rem_euclid(360 * 16),
    }
}

#[inline(always)]
fn emit_px<S: PixelSink>(sink: &mut S, idx: usize, fg: u16, cov: u8) {
    if cov == 0 {
        return;
    }
    if cov >= 15 {
        sink.put_opaque(idx, fg);
    } else {
        sink.blend_a4(idx, fg, cov);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_line<S: PixelSink>(
    sink: &mut S,
    buf_rect: Rect,
    p0: Point,
    p1: Point,
    clip: Rect,
    color: Rgb565,
    width: u8,
    aa: bool,
) {
    if width == 0 {
        return;
    }
    let pad = i32::from(width) / 2 + 2;
    let x0 = p0.x.min(p1.x).saturating_sub(pad);
    let y0 = p0.y.min(p1.y).saturating_sub(pad);
    let x1 = p0.x.max(p1.x).saturating_add(pad + 1);
    let y1 = p0.y.max(p1.y).saturating_add(pad + 1);
    let bounds = Rect::new(
        x0,
        y0,
        x1.saturating_sub(x0).max(0) as u32,
        y1.saturating_sub(y0).max(0) as u32,
    );
    let eff = bounds.intersect(clip).intersect(buf_rect);
    if eff.is_empty() {
        return;
    }
    let fg = rgb565_to_u16(color);
    let stride = buf_rect.w as usize;
    let vx = i64::from(p1.x) - i64::from(p0.x);
    let vy = i64::from(p1.y) - i64::from(p0.y);
    let len_sq = vx * vx + vy * vy;
    let len_q4 = isqrt_u64((len_sq as u64).saturating_mul(256)).max(1) as i64;
    let half_w_q4 = i64::from(width) * 8;

    let gx0 = eff.x;
    let gx1 = eff.x + eff.w as i32;
    let gy0 = eff.y;
    let gy1 = eff.y + eff.h as i32;

    for gy in gy0..gy1 {
        let row_off = ((gy - buf_rect.y) as usize) * stride;
        let wy = i64::from(gy) - i64::from(p0.y);
        let wy1 = i64::from(gy) - i64::from(p1.y);
        for gx in gx0..gx1 {
            let wx = i64::from(gx) - i64::from(p0.x);
            let dist_q4 = if len_sq == 0 {
                let d2 = (wx * 16) * (wx * 16) + (wy * 16) * (wy * 16);
                isqrt_u64(d2 as u64) as i64
            } else {
                let dot = wx * vx + wy * vy;
                if dot <= 0 {
                    let d2 = (wx * 16) * (wx * 16) + (wy * 16) * (wy * 16);
                    isqrt_u64(d2 as u64) as i64
                } else if dot >= len_sq {
                    let wx1 = i64::from(gx) - i64::from(p1.x);
                    let d2 = (wx1 * 16) * (wx1 * 16) + (wy1 * 16) * (wy1 * 16);
                    isqrt_u64(d2 as u64) as i64
                } else {
                    let cross = (wx * vy - wy * vx).abs();
                    (cross * 256) / len_q4
                }
            };
            let cov = if aa {
                (half_w_q4 + 8 - dist_q4).clamp(0, 15) as u8
            } else if dist_q4 <= half_w_q4 {
                15
            } else {
                0
            };
            if cov > 0 {
                let idx = row_off + (gx - buf_rect.x) as usize;
                emit_px(sink, idx, fg, cov);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_circle<S: PixelSink>(
    sink: &mut S,
    buf_rect: Rect,
    center: Point,
    radius: u16,
    clip: Rect,
    fill: Option<Rgb565>,
    border_color: Option<Rgb565>,
    border_width: u8,
) {
    let bw = if border_color.is_some() && border_width > 0 {
        i32::from(border_width).min(i32::from(radius) + 1)
    } else {
        0
    };
    if fill.is_none() && bw == 0 {
        return;
    }
    let bound_r = i32::from(radius) + 2;
    let bounds = Rect::new(
        center.x - bound_r,
        center.y - bound_r,
        (bound_r * 2 + 1) as u32,
        (bound_r * 2 + 1) as u32,
    );
    let eff = bounds.intersect(clip).intersect(buf_rect);
    if eff.is_empty() {
        return;
    }
    let stride = buf_rect.w as usize;
    let r_outer_q4 = i64::from(radius) * 16;
    let r_inner_q4 = (i64::from(radius) - i64::from(bw)) * 16;
    let fill_u16 = fill.map(rgb565_to_u16);
    let border_u16 = border_color.filter(|_| bw > 0).map(rgb565_to_u16);

    let gx0 = eff.x;
    let gx1 = eff.x + eff.w as i32;
    let gy0 = eff.y;
    let gy1 = eff.y + eff.h as i32;

    for gy in gy0..gy1 {
        let row_off = ((gy - buf_rect.y) as usize) * stride;
        let dy_q4 = i64::from(gy - center.y) * 16;
        let dy2 = dy_q4 * dy_q4;
        for gx in gx0..gx1 {
            let dx_q4 = i64::from(gx - center.x) * 16;
            let dist_q4 = isqrt_u64((dx_q4 * dx_q4 + dy2) as u64) as i64;
            let cov_outer = (r_outer_q4 + 8 - dist_q4).clamp(0, 15) as u8;
            if cov_outer == 0 {
                continue;
            }
            let idx = row_off + (gx - buf_rect.x) as usize;
            match (fill_u16, border_u16) {
                (Some(fcol), Some(bcol)) => {
                    let cov_inner = (r_inner_q4 + 8 - dist_q4).clamp(0, 15) as u8;
                    if cov_inner >= 15 {
                        sink.put_opaque(idx, fcol);
                    } else if cov_inner == 0 {
                        emit_px(sink, idx, bcol, cov_outer);
                    } else {
                        let b_frac = ((cov_outer.saturating_sub(cov_inner) as u16) * 15
                            / u16::from(cov_outer)) as u8;
                        let mixed = blend_rgb565_a4_u16(bcol, fcol, b_frac);
                        emit_px(sink, idx, mixed, cov_outer);
                    }
                }
                (Some(fcol), None) => emit_px(sink, idx, fcol, cov_outer),
                (None, Some(bcol)) => {
                    let cov_inner = (r_inner_q4 + 8 - dist_q4).clamp(0, 15) as u8;
                    let bcov = cov_outer.saturating_sub(cov_inner);
                    emit_px(sink, idx, bcol, bcov);
                }
                (None, None) => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_arc<S: PixelSink>(
    sink: &mut S,
    buf_rect: Rect,
    center: Point,
    radius: u16,
    start_deg: i16,
    sweep_deg: i16,
    width: u8,
    cap_round: bool,
    clip: Rect,
    color: Rgb565,
) {
    if width == 0 || sweep_deg == 0 {
        return;
    }
    let bound_r = i32::from(radius) + i32::from(width) + 2;
    let bounds = Rect::new(
        center.x - bound_r,
        center.y - bound_r,
        (bound_r * 2 + 1) as u32,
        (bound_r * 2 + 1) as u32,
    );
    let eff = bounds.intersect(clip).intersect(buf_rect);
    if eff.is_empty() {
        return;
    }
    let fg = rgb565_to_u16(color);
    let stride = buf_rect.w as usize;
    let r_q4 = i64::from(radius) * 16;
    let half_w_q4 = i64::from(width) * 8;
    let full_ring = i32::from(sweep_deg).abs() >= 360;

    let (norm_start_deg, norm_sweep_deg) = if sweep_deg < 0 {
        (
            (i32::from(start_deg) + i32::from(sweep_deg)).rem_euclid(360),
            (-i32::from(sweep_deg)).min(360),
        )
    } else {
        (
            i32::from(start_deg).rem_euclid(360),
            i32::from(sweep_deg).min(360),
        )
    };
    let end_deg = (norm_start_deg + norm_sweep_deg).rem_euclid(360);
    let start_q4 = norm_start_deg * 16;
    let sweep_q4 = norm_sweep_deg * 16;

    let (ss, sc) = sin_cos_q12(norm_start_deg);
    let (es, ec) = sin_cos_q12(end_deg);
    let p_start_x_q4 = i64::from(center.x) * 16 + ((i64::from(radius) * i64::from(sc)) >> 8);
    let p_start_y_q4 = i64::from(center.y) * 16 + ((i64::from(radius) * i64::from(ss)) >> 8);
    let p_end_x_q4 = i64::from(center.x) * 16 + ((i64::from(radius) * i64::from(ec)) >> 8);
    let p_end_y_q4 = i64::from(center.y) * 16 + ((i64::from(radius) * i64::from(es)) >> 8);

    let gx0 = eff.x;
    let gx1 = eff.x + eff.w as i32;
    let gy0 = eff.y;
    let gy1 = eff.y + eff.h as i32;

    for gy in gy0..gy1 {
        let row_off = ((gy - buf_rect.y) as usize) * stride;
        let dy = gy - center.y;
        let dy_q4 = i64::from(dy) * 16;
        let dy2 = dy_q4 * dy_q4;
        let gy_q4 = i64::from(gy) * 16;
        for gx in gx0..gx1 {
            let dx = gx - center.x;
            let dx_q4 = i64::from(dx) * 16;
            let dist_q4 = isqrt_u64((dx_q4 * dx_q4 + dy2) as u64) as i64;
            let radial_diff_q4 = (dist_q4 - r_q4).abs();
            let radial_cov = (half_w_q4 + 8 - radial_diff_q4).clamp(0, 15) as u8;
            if radial_cov == 0 && !cap_round {
                continue;
            }
            let in_sector = if full_ring {
                true
            } else {
                let th_q4 = atan2_deg_q4(dy, dx);
                let rel_q4 = (th_q4 - start_q4).rem_euclid(360 * 16);
                rel_q4 <= sweep_q4
            };
            let cov = if in_sector {
                radial_cov
            } else if cap_round {
                let gx_q4 = i64::from(gx) * 16;
                let dsx = gx_q4 - p_start_x_q4;
                let dsy = gy_q4 - p_start_y_q4;
                let dex = gx_q4 - p_end_x_q4;
                let dey = gy_q4 - p_end_y_q4;
                let d_cap_sq = (dsx * dsx + dsy * dsy).min(dex * dex + dey * dey);
                let d_cap_q4 = isqrt_u64(d_cap_sq as u64) as i64;
                (half_w_q4 + 8 - d_cap_q4).clamp(0, 15) as u8
            } else {
                0
            };
            if cov > 0 {
                let idx = row_off + (gx - buf_rect.x) as usize;
                emit_px(sink, idx, fg, cov);
            }
        }
    }
}
