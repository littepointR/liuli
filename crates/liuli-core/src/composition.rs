//! A surface's geometry is shared by painting, content clipping, shadows and optional hit testing.
use crate::{
    bitmap::Bitmap,
    geometry::{Point, Rect},
    path::{FillRule, Path, PathPaint, coverage},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outline {
    /// Use the node's resolved corner radius, including local theme overrides.
    Surface,
    Rectangle,
    Rounded(u8),
    Circle,
    Capsule,
    Path(&'static Path<'static>, FillRule),
}
impl Outline {
    /// Resolve logical dimensions after layout, in the same coordinates as the input rectangle.
    pub fn bounds_and_radius(self, rect: Rect, radius: u8) -> (Rect, u32) {
        let side = rect.w.min(rect.h);
        match self {
            Self::Circle => (
                Rect::new(
                    rect.x.saturating_add_unsigned((rect.w - side) / 2),
                    rect.y.saturating_add_unsigned((rect.h - side) / 2),
                    side,
                    side,
                ),
                side / 2,
            ),
            Self::Capsule => (rect, side / 2),
            Self::Rounded(r) => (rect, u32::from(r).min(side / 2)),
            Self::Surface => (rect, u32::from(radius).min(side / 2)),
            _ => (rect, 0),
        }
    }
    pub fn coverage(self, rect: Rect, radius: u8, p: Point) -> u8 {
        let (rect, r) = self.bounds_and_radius(rect, radius);
        if !rect.contains_point(p) {
            return 0;
        }
        if let Self::Path(path, rule) = self {
            return coverage(path.edges.iter(), rect, p, PathPaint::Fill(rule));
        }
        rounded_coverage(rect, r, p)
    }
}

pub fn rounded_coverage(rect: Rect, r: u32, p: Point) -> u8 {
    if !rect.contains_point(p) {
        return 0;
    }
    let x = (i64::from(p.x) - rect.left64()) as u32;
    let y = (i64::from(p.y) - rect.top64()) as u32;
    let x = x.min(rect.w - 1 - x);
    let y = y.min(rect.h - 1 - y);
    if x >= r || y >= r {
        return 15;
    }
    // Keep the MCU's common case in 32-bit arithmetic. Large logical canvases
    // can extend far beyond the physical viewport.
    if r > 8191 {
        let dx = 4 * i128::from(r - x);
        let dy = 4 * i128::from(r - y);
        let limit = (4 * i128::from(r)).pow(2);
        let mut covered = 0;
        for a in [1, 3] {
            for b in [1, 3] {
                covered += u8::from((dx - a).pow(2) + (dy - b).pow(2) <= limit);
            }
        }
        return (covered * 15 + 2) / 4;
    }
    let dx = 4 * (r - x) as i32;
    let dy = 4 * (r - y) as i32;
    let limit = (4 * r) as i32;
    let mut n = 0;
    for a in [1, 3] {
        for b in [1, 3] {
            if (dx - a) * (dx - a) + (dy - b) * (dy - b) <= limit * limit {
                n += 1
            }
        }
    }
    (n * 15 + 2) / 4
}
pub fn round_band(rect: Rect, r: u32, width: u8, p: Point) -> u8 {
    let outer = rounded_coverage(rect, r, p);
    let w = u32::from(width);
    if w == 0 || outer == 0 {
        return outer;
    }
    outer.saturating_sub(rounded_coverage(
        rect.inset(w, w, w, w),
        r.saturating_sub(w),
        p,
    ))
}
/// Where a compound control places the part that receives its focus decoration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartBounds {
    Inset(crate::view::EdgeInsets),
    /// Edges in 0..=1024 along each axis, relative to the control's layout.
    Relative {
        left: u16,
        top: u16,
        right: u16,
        bottom: u16,
    },
    /// Full region covered by the slider thumb and track at any value.
    SliderTravel,
}
impl PartBounds {
    pub fn resolve(self, rect: Rect, parts: &crate::part_style::WidgetPartStyle) -> Rect {
        match self {
            Self::Inset(i) => {
                rect.inset(i.left.into(), i.top.into(), i.right.into(), i.bottom.into())
            }
            Self::Relative {
                left,
                top,
                right,
                bottom,
            } => {
                let scaled = |edge: u16, size: u32| {
                    ((u64::from(edge.min(1024)) * u64::from(size) + 512) / 1024) as u32
                };
                let (x, y) = (scaled(left, rect.w), scaled(top, rect.h));
                Rect::new(
                    rect.x.saturating_add_unsigned(x),
                    rect.y.saturating_add_unsigned(y),
                    scaled(right, rect.w).saturating_sub(x),
                    scaled(bottom, rect.h).saturating_sub(y),
                )
            }
            Self::SliderTravel => parts
                .slider_track_bounds(rect.size())
                .1
                .translate(rect.x, rect.y),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusPart {
    pub part: crate::part_style::WidgetPart,
    pub bounds: PartBounds,
    pub outline: Outline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusTarget {
    Main,
    Part(&'static FocusPart),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitArea {
    /// Use the composition's hit policy when present, otherwise the layout bounds.
    Inherit,
    Bounds,
    Outline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetGeometry {
    pub outline: Outline,
    pub focus: FocusTarget,
    pub hit: HitArea,
}
impl WidgetGeometry {
    pub const fn new(outline: Outline) -> Self {
        Self {
            outline,
            focus: FocusTarget::Main,
            hit: HitArea::Inherit,
        }
    }
    pub const SURFACE: Self = Self::new(Outline::Surface);
    pub const DIAL: Self = Self {
        hit: HitArea::Outline,
        ..Self::new(Outline::Circle)
    };
}
pub fn body_outline(
    geometry: Option<&WidgetGeometry>,
    composition: Option<&SurfaceComposition>,
) -> Outline {
    match geometry.map(|g| g.outline) {
        None | Some(Outline::Surface) => composition.map_or(Outline::Surface, |c| c.outline),
        Some(outline) => outline,
    }
}

/// Bounds, as well as the pixel-space offsets used to rasterize a focus stroke.
#[derive(Clone, Copy)]
pub struct FocusOutline {
    pub shape: Outline,
    pub body: Rect,
    pub bounds: Rect,
    pub radius: u32,
    pub outer: u8,
}
impl FocusOutline {
    pub fn resolve(
        shape: Outline,
        rect: Rect,
        radius: u8,
        ring: crate::view::FocusRingStyle,
        clip: Rect,
    ) -> Self {
        let (body, mut r) = shape.bounds_and_radius(rect, radius);
        if !ring.rounded && matches!(shape, Outline::Surface | Outline::Rounded(_)) {
            r = 0;
        }
        let clearance = (body.left64() - clip.left64())
            .min(body.top64() - clip.top64())
            .min(clip.right64() - body.right64())
            .min(clip.bottom64() - body.bottom64());
        let outer = clearance.clamp(0, i64::from(ring.offset.min(7))) as u8;
        let bounds = if body.is_empty() {
            body
        } else {
            body.expand(u32::from(outer))
        };
        Self {
            shape,
            body,
            bounds,
            radius: if r > 0 { r + u32::from(outer) } else { 0 },
            outer,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mask {
    Opaque,
    FadeBottom(u16),
    Alpha(&'static Bitmap),
}
impl Mask {
    pub fn coverage(self, rect: Rect, p: Point) -> u8 {
        if !rect.contains_point(p) {
            return 0;
        }
        match self {
            Self::Opaque | Self::FadeBottom(0) => 15,
            Self::FadeBottom(h) => {
                let span = i64::from(h).min(i64::from(rect.h));
                let remaining = rect.bottom64() - 1 - i64::from(p.y);
                ((remaining * 15 + span.saturating_sub(1) / 2) / (span - 1).max(1)).min(15) as u8
            }
            Self::Alpha(b) => b
                .sample_rgba(
                    (i64::from(p.x) - rect.left64()) as u32,
                    (i64::from(p.y) - rect.top64()) as u32,
                    rect.w,
                    rect.h,
                    0,
                )
                .map_or(0, |(_, a)| a),
        }
    }
}
#[inline(always)]
pub fn get_a4(alpha: &[u8], i: usize) -> u8 {
    let byte = alpha[i >> 1];
    if (i & 1) == 0 { byte & 0x0F } else { byte >> 4 }
}

#[inline(always)]
pub fn set_a4(alpha: &mut [u8], i: usize, a: u8) {
    let slot = &mut alpha[i >> 1];
    if (i & 1) == 0 {
        *slot = (*slot & 0xF0) | (a & 0x0F);
    } else {
        *slot = (*slot & 0x0F) | (a << 4);
    }
}

pub const fn mul_a4(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16 + 7) / 15) as u8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowSource {
    Outline,
    Content,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceComposition {
    pub outline: Outline,
    pub clip_content: bool,
    pub mask: Mask,
    pub hit_outline: bool,
    /// Content-alpha shadows require the enhanced profile.
    pub shadow_source: ShadowSource,
}
impl SurfaceComposition {
    pub const DEFAULT: Self = Self {
        outline: Outline::Surface,
        clip_content: false,
        mask: Mask::Opaque,
        hit_outline: false,
        shadow_source: ShadowSource::Outline,
    };
    pub fn coverage(self, rect: Rect, radius: u8, p: Point) -> u8 {
        self.coverage_with(self.outline, rect, radius, p)
    }
    pub fn coverage_with(self, outline: Outline, rect: Rect, radius: u8, p: Point) -> u8 {
        let clip = if self.clip_content {
            outline.coverage(rect, radius, p)
        } else {
            15
        };
        mul_a4(clip, self.mask.coverage(rect, p))
    }
    pub const fn masks_content(self) -> bool {
        self.clip_content || !matches!(self.mask, Mask::Opaque)
    }
}
