use crate::BandTarget;
use liuli_core::{
    composition::mul_a4,
    effects::Shadow,
    geometry::{Point, Rect, Rgb565, rgb565_to_u16},
    path::{self, PathEdge, PathPaint},
};

/// Generate horizontal coverage spans. Each edge contributes a winding change
/// at its intersection with a sub-scanline. No edge list is rebuilt per band.
pub(crate) fn draw_path<'a, I: Iterator<Item = &'a PathEdge> + Clone>(
    target: &mut BandTarget<'_>,
    rect: Rect,
    clip: Rect,
    edges: I,
    paint: PathPaint,
    color: Rgb565,
) {
    let band = target.band_rect();
    let area = band.intersect(clip).intersect(rect);
    let raw = rgb565_to_u16(color);
    for row in 0..area.h {
        let y = area.y.saturating_add_unsigned(row);
        for tile in (0..area.w).step_by(32) {
            let x = area.x.saturating_add_unsigned(tile);
            let n = (area.w - tile).min(32) as usize;
            let mut alpha = [0; 32];
            path::coverage_span(
                edges.clone(),
                rect,
                Point::new(x, y),
                paint,
                &mut alpha[..n],
            );
            let start = (y - band.y) as usize * band.w as usize + (x - band.x) as usize;
            let mut i = 0;
            while i < n {
                if alpha[i] == 15 {
                    let first = i;
                    while i < n && alpha[i] == 15 {
                        i += 1
                    }
                    target.words_mut()[start + first..start + i].fill(raw);
                    target.set_opaque_span(start + first, i - first);
                } else {
                    if alpha[i] != 0 {
                        target.blend_pixel_a4(start + i, raw, alpha[i]);
                    }
                    i += 1
                }
            }
        }
    }
}

/// Box-filter alpha, treating samples outside the caster as transparent.
/// A sliding sum keeps work per output pixel linear in the filter diameter.
pub(crate) fn coverage_shadow(
    target: &mut BandTarget<'_>,
    rect: Rect,
    clip: Rect,
    shadow: Shadow,
    alpha: impl Fn(Point) -> u8,
) {
    let band = target.band_rect();
    let area = shadow.bounds(rect).intersect(clip).intersect(band);
    if area.is_empty() || shadow.opacity == 0 {
        return;
    }
    let r = i32::from(shadow.blur);
    let denom = (2 * r + 1) * (2 * r + 1);
    let raw = rgb565_to_u16(shadow.color);
    for row in 0..area.h {
        let y = area.y.saturating_add_unsigned(row);
        let sy = y.saturating_sub(i32::from(shadow.y));
        let column = |sx: i32| -> i32 {
            if i64::from(sx) < rect.left64() || i64::from(sx) >= rect.right64() {
                return 0;
            }
            let top = i64::from(sy.saturating_sub(r)).max(rect.top64());
            let bottom = (i64::from(sy.saturating_add(r)) + 1).min(rect.bottom64());
            (top..bottom)
                .map(|v| i32::from(alpha(Point::new(sx, v as i32))))
                .sum()
        };
        let first = area.x.saturating_sub(i32::from(shadow.x));
        let mut sum: i32 = (-r..=r).map(|dx| column(first.saturating_add(dx))).sum();
        let offset = (y - band.y) as usize * band.w as usize + (area.x - band.x) as usize;
        for x in 0..area.w {
            let a = mul_a4(((sum + denom / 2) / denom) as u8, shadow.opacity);
            if a != 0 {
                target.blend_pixel_a4(offset + x as usize, raw, a)
            }
            if x + 1 < area.w {
                let sx = first.saturating_add_unsigned(x);
                sum += column(sx.saturating_add(r + 1)) - column(sx.saturating_sub(r));
            }
        }
    }
}
