//! Prepared outlines in a 1024 × 1024 view box. Caller-owned storage works in flash or RAM.
use crate::{
    capacity::BuildError,
    geometry::{Point, Rect},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathVerb {
    Move(u16, u16),
    Line(u16, u16),
    Quad((u16, u16), (u16, u16)),
    Cubic((u16, u16), (u16, u16), (u16, u16)),
    Close,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathPaint {
    Fill(FillRule),
    /// Width in view-box units, with round caps and joins.
    Stroke(u16),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PathEdge {
    from: (u16, u16),
    to: (u16, u16),
    fill_only: bool,
}
impl PathEdge {
    pub const EMPTY: Self = Self {
        from: (0, 0),
        to: (0, 0),
        fill_only: false,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path<'a> {
    pub edges: &'a [PathEdge],
}
pub struct PreparedPath<const N: usize> {
    edges: [PathEdge; N],
    len: usize,
}

impl<const N: usize> PreparedPath<N> {
    /// Flatten curves once. Each curve is sampled at `steps` equal parameter intervals.
    /// All coordinates must lie within 0..=1024. Open contours are closed for fills only.
    pub const fn prepare(verbs: &[PathVerb], steps: u8) -> Result<Self, BuildError> {
        let mut path = Self {
            edges: [PathEdge::EMPTY; N],
            len: 0,
        };
        if steps == 0 {
            return Err(BuildError::InvalidRange);
        }
        let mut first = None;
        let mut last = (0, 0);
        let mut i = 0;
        while i <= verbs.len() {
            let end = i == verbs.len();
            let v = if end { PathVerb::Close } else { verbs[i] };
            if end || matches!(v, PathVerb::Move(..) | PathVerb::Close) {
                if let Some(p) = first {
                    if !path.add(last, p, end || !matches!(v, PathVerb::Close)) {
                        return Err(BuildError::PathEdges);
                    }
                    last = p;
                }
                first = None;
            }
            if end {
                break;
            }
            let (a, b, c, parts) = match v {
                PathVerb::Move(x, y) => {
                    if x > 1024 || y > 1024 {
                        return Err(BuildError::InvalidRange);
                    }
                    first = Some((x, y));
                    last = (x, y);
                    i += 1;
                    continue;
                }
                PathVerb::Close => {
                    i += 1;
                    continue;
                }
                PathVerb::Line(x, y) => ((x, y), (x, y), (x, y), 1u32),
                PathVerb::Quad(a, b) => (a, b, b, steps as u32),
                PathVerb::Cubic(a, b, c) => (a, b, c, steps as u32),
            };
            if first.is_none()
                || a.0 > 1024
                || a.1 > 1024
                || b.0 > 1024
                || b.1 > 1024
                || c.0 > 1024
                || c.1 > 1024
            {
                return Err(BuildError::InvalidRange);
            }
            let start = last;
            let mut t = 1;
            while t <= parts {
                let p = match v {
                    PathVerb::Quad(..) => {
                        lerp(lerp(start, a, t, parts), lerp(a, b, t, parts), t, parts)
                    }
                    PathVerb::Cubic(..) => lerp(
                        lerp(lerp(start, a, t, parts), lerp(a, b, t, parts), t, parts),
                        lerp(lerp(a, b, t, parts), lerp(b, c, t, parts), t, parts),
                        t,
                        parts,
                    ),
                    _ => a,
                };
                if !path.add(last, p, false) {
                    return Err(BuildError::PathEdges);
                }
                last = p;
                t += 1;
            }
            i += 1;
        }
        Ok(path)
    }
    const fn add(&mut self, from: (u16, u16), to: (u16, u16), fill_only: bool) -> bool {
        if from.0 == to.0 && from.1 == to.1 {
            return true;
        }
        if self.len == N {
            return false;
        }
        self.edges[self.len] = PathEdge {
            from,
            to,
            fill_only,
        };
        self.len += 1;
        true
    }
    pub const fn path(&self) -> Path<'_> {
        Path {
            edges: self.edges.split_at(self.len).0,
        }
    }
}
const fn lerp(a: (u16, u16), b: (u16, u16), t: u32, n: u32) -> (u16, u16) {
    (
        ((a.0 as u32 * (n - t) + b.0 as u32 * t + n / 2) / n) as u16,
        ((a.1 as u32 * (n - t) + b.1 as u32 * t + n / 2) / n) as u16,
    )
}

impl Path<'_> {
    pub fn contains(self, rect: Rect, p: Point, rule: FillRule) -> bool {
        rect.contains_point(p)
            && contains(
                self.edges.iter(),
                sample(p.x, rect.x, rect.w, 2),
                sample(p.y, rect.y, rect.h, 2),
                PathPaint::Fill(rule),
            )
    }
}
// Coordinates in quarter view-box units. Pixel samples are independent of band boundaries.
pub fn sample(pixel: i32, origin: i32, extent: u32, quarter: u8) -> i32 {
    (((i64::from(pixel) - i64::from(origin)) * 4 + i64::from(quarter)) * 1024
        / i64::from(extent.max(1)))
    .clamp(-65536, 65536) as i32
}
pub fn contains<'a>(
    edges: impl Iterator<Item = &'a PathEdge>,
    x: i32,
    y: i32,
    paint: PathPaint,
) -> bool {
    let mut winding = 0i32;
    for edge in edges {
        let (ax, ay) = (i32::from(edge.from.0) * 4, i32::from(edge.from.1) * 4);
        let (bx, by) = (i32::from(edge.to.0) * 4, i32::from(edge.to.1) * 4);
        let (dx, dy) = (bx - ax, by - ay);
        match paint {
            PathPaint::Fill(_) => {
                if (ay > y) == (by > y) {
                    continue;
                }
                let cross = dx * (y - ay) - dy * (x - ax);
                if dy > 0 && cross > 0 {
                    winding += 1
                } else if dy < 0 && cross < 0 {
                    winding -= 1
                }
            }
            PathPaint::Stroke(width) => {
                if edge.fill_only || width == 0 {
                    continue;
                }
                let r = i32::from(width) * 2;
                if x < ax.min(bx) - r
                    || x > ax.max(bx) + r
                    || y < ay.min(by) - r
                    || y > ay.max(by) + r
                {
                    continue;
                }
                let (px, py) = (i64::from(x - ax), i64::from(y - ay));
                let (dx, dy) = (i64::from(dx), i64::from(dy));
                let dot = px * dx + py * dy;
                let len = dx * dx + dy * dy;
                let r2 = i64::from(r) * i64::from(r);
                let inside = if dot <= 0 {
                    px * px + py * py <= r2
                } else if dot >= len {
                    (px - dx) * (px - dx) + (py - dy) * (py - dy) <= r2
                } else {
                    let cross = px * dy - py * dx;
                    cross * cross <= r2 * len
                };
                if inside {
                    return true;
                }
            }
        }
    }
    match paint {
        PathPaint::Fill(FillRule::EvenOdd) => winding % 2 != 0,
        PathPaint::Fill(FillRule::NonZero) => winding != 0,
        PathPaint::Stroke(_) => false,
    }
}

/// Coverage for consecutive pixels already clipped to `rect`. Uses 2x2 subpixel samples.
pub fn coverage_span<'a, I: Iterator<Item = &'a PathEdge> + Clone>(
    edges: I,
    rect: Rect,
    origin: Point,
    paint: PathPaint,
    out: &mut [u8],
) {
    let PathPaint::Fill(rule) = paint else {
        for (i, a) in out.iter_mut().enumerate() {
            *a = coverage(
                edges.clone(),
                rect,
                Point::new(origin.x.saturating_add_unsigned(i as u32), origin.y),
                paint,
            )
        }
        return;
    };
    for (tile, bytes) in out.chunks_mut(32).enumerate() {
        let x0 = origin.x.saturating_add_unsigned((tile * 32) as u32);
        let len = bytes.len() * 2;
        let mut samples = [0; 64];
        for (i, x) in samples[..len].iter_mut().enumerate() {
            *x = sample(
                x0.saturating_add_unsigned((i / 2) as u32),
                rect.x,
                rect.w,
                1 + 2 * (i % 2) as u8,
            )
        }
        bytes.fill(0);
        for sub_y in [1, 3] {
            let y = sample(origin.y, rect.y, rect.h, sub_y);
            let mut change = [0i32; 65];
            for edge in edges.clone() {
                let (mut ax, mut ay) = (4 * i32::from(edge.from.0), 4 * i32::from(edge.from.1));
                let (mut bx, mut by) = (4 * i32::from(edge.to.0), 4 * i32::from(edge.to.1));
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
                let cross = ax * (by - ay) + (bx - ax) * (y - ay);
                let end = samples[..len].partition_point(|x| x * (by - ay) < cross);
                change[0] += sign;
                change[end] -= sign;
            }
            let mut winding = 0;
            for i in 0..len {
                winding += change[i];
                if match rule {
                    FillRule::EvenOdd => winding % 2 != 0,
                    FillRule::NonZero => winding != 0,
                } {
                    bytes[i / 2] += 1
                }
            }
        }
        for a in bytes {
            *a = (*a * 15 + 2) / 4
        }
    }
}

pub fn coverage<'a, I: Iterator<Item = &'a PathEdge> + Clone>(
    edges: I,
    rect: Rect,
    p: Point,
    paint: PathPaint,
) -> u8 {
    if !rect.contains_point(p) {
        return 0;
    }
    let mut count = 0;
    for sy in [1, 3] {
        let y = sample(p.y, rect.y, rect.h, sy);
        for sx in [1, 3] {
            if contains(edges.clone(), sample(p.x, rect.x, rect.w, sx), y, paint) {
                count += 1
            }
        }
    }
    (count * 15 + 2) / 4
}
