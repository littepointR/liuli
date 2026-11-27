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
    Path(&'static Path<'static>, FillRule),
}
impl Outline {
    pub fn coverage(self, rect: Rect, radius: u8, p: Point) -> u8 {
        if !rect.contains_point(p) {
            return 0;
        }
        if let Self::Path(path, rule) = self {
            return coverage(path.edges.iter(), rect, p, PathPaint::Fill(rule));
        }
        let r = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
        let x = (i64::from(p.x) - rect.left64()) as u32;
        let y = (i64::from(p.y) - rect.top64()) as u32;
        let x = x.min(rect.w - 1 - x);
        let y = y.min(rect.h - 1 - y);
        if x >= r || y >= r {
            return 15;
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
        let clip = if self.clip_content {
            self.outline.coverage(rect, radius, p)
        } else {
            15
        };
        mul_a4(clip, self.mask.coverage(rect, p))
    }
    pub const fn masks_content(self) -> bool {
        self.clip_content || !matches!(self.mask, Mask::Opaque)
    }
}
