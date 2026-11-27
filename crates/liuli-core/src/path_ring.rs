//! Disk dilation/erosion of the filled silhouette on the path rasterizer's 2x2 sample grid.
//! Distances are in device pixels after layout, independent of the path's aspect ratio.
use crate::{
    geometry::{Point, Rect},
    path::{FillRule, Path},
};

fn sample(half: i64, origin: i32, extent: u32) -> i32 {
    ((2 * (half - 2 * i64::from(origin)) + 1) * 1024)
        .div_euclid(i64::from(extent.max(1)))
        .clamp(-65536, 65536) as i32
}

fn scan(path: Path<'_>, rule: FillRule, xs: &[i32], y: i32) -> u64 {
    if !(0..4096).contains(&y) {
        return 0;
    }
    let mut changes = [0i32; 65];
    for e in path.edges {
        let (mut ax, mut ay) = (4 * i32::from(e.from.0), 4 * i32::from(e.from.1));
        let (mut bx, mut by) = (4 * i32::from(e.to.0), 4 * i32::from(e.to.1));
        let sign = if ay > by {
            core::mem::swap(&mut ax, &mut bx);
            core::mem::swap(&mut ay, &mut by);
            -1
        } else {
            1
        };
        if y < ay || y >= by {
            continue;
        }
        let intercept = ax * (by - ay) + (bx - ax) * (y - ay);
        let end = xs.partition_point(|v| *v * (by - ay) < intercept);
        changes[0] += sign;
        changes[end] -= sign;
    }
    let (mut bits, mut winding) = (0, 0);
    for (i, &delta) in changes[..xs.len()].iter().enumerate() {
        winding += delta;
        if match rule {
            FillRule::EvenOdd => winding % 2 != 0,
            FillRule::NonZero => winding != 0,
        } {
            bits |= 1u64 << i;
        }
    }
    bits
}
fn combine(acc: &mut u64, row: u64, dy: i32, offset: i8, halo: u32) {
    let r = 2 * i32::from(offset).abs();
    if dy.abs() > r {
        return;
    }
    let span = (r * r - dy * dy).isqrt();
    for dx in -span..=span {
        let aligned = row >> (i64::from(halo) + i64::from(dx)) as u32;
        if offset < 0 {
            *acc &= aligned
        } else {
            *acc |= aligned
        }
    }
}
/// Render into arbitrary horizontal spans with bounded row scratch. The public ring
/// style permits a seven-pixel outset and a three-pixel stroke.
pub fn coverage_span(
    path: Path<'_>,
    rule: FillRule,
    rect: Rect,
    origin: Point,
    outer: i8,
    inner: i8,
    out: &mut [u8],
) {
    if rect.is_empty() {
        out.fill(0);
        return;
    }
    let outer = outer.clamp(-7, 7);
    let inner = inner.clamp(-7, outer);
    let halo = 2 * u32::from(outer.unsigned_abs().max(inner.unsigned_abs()));
    for (tile, chunk) in out.chunks_mut(16).enumerate() {
        let x0 = i64::from(origin.x) + 16 * tile as i64;
        let mut xs = [0; 64];
        let count = 2 * chunk.len() + 2 * halo as usize;
        for (i, x) in xs[..count].iter_mut().enumerate() {
            *x = sample(2 * x0 + i as i64 - i64::from(halo), rect.x, rect.w);
        }
        chunk.fill(0);
        for sy in 0..2 {
            let mut outside = if outer < 0 { u64::MAX } else { 0 };
            let mut inside = if inner < 0 { u64::MAX } else { 0 };
            for dy in -(halo as i32)..=halo as i32 {
                let y = sample(i64::from(origin.y) * 2 + sy + i64::from(dy), rect.y, rect.h);
                let row = scan(path, rule, &xs[..count], y);
                combine(&mut outside, row, dy, outer, halo);
                combine(&mut inside, row, dy, inner, halo);
            }
            let mut bits = outside & !inside;
            for a in chunk.iter_mut() {
                *a += (bits & 3).count_ones() as u8;
                bits >>= 2;
            }
        }
        for a in chunk {
            *a = (*a * 15 + 2) / 4
        }
    }
}
