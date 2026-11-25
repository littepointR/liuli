pub use embedded_graphics_core::geometry::{Point, Size};
pub use embedded_graphics_core::pixelcolor::raw::{RawData, RawU16};
pub use embedded_graphics_core::pixelcolor::{IntoStorage, PixelColor, Rgb565};
pub use embedded_graphics_core::primitives::Rectangle;

/// Half-open axis-aligned rectangle `[x, x + w) x [y, y + h)`.
///
/// All boundary arithmetic uses `i64`/`u64` intermediates so extreme inputs never wrap.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };

    #[inline]
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    #[inline]
    pub const fn from_point_size(top_left: Point, size: Size) -> Self {
        Self {
            x: top_left.x,
            y: top_left.y,
            w: size.width,
            h: size.height,
        }
    }

    #[inline]
    pub fn from_eg(rect: Rectangle) -> Self {
        Self {
            x: rect.top_left.x,
            y: rect.top_left.y,
            w: rect.size.width,
            h: rect.size.height,
        }
    }

    #[inline]
    pub const fn to_eg(self) -> Rectangle {
        Rectangle::new(Point::new(self.x, self.y), Size::new(self.w, self.h))
    }

    #[inline]
    pub const fn top_left(self) -> Point {
        Point::new(self.x, self.y)
    }

    #[inline]
    pub const fn size(self) -> Size {
        Size::new(self.w, self.h)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }

    #[inline]
    pub const fn left64(self) -> i64 {
        self.x as i64
    }

    #[inline]
    pub const fn top64(self) -> i64 {
        self.y as i64
    }

    #[inline]
    pub const fn right64(self) -> i64 {
        self.x as i64 + self.w as i64
    }

    #[inline]
    pub const fn bottom64(self) -> i64 {
        self.y as i64 + self.h as i64
    }

    #[inline]
    pub const fn area64(self) -> u64 {
        (self.w as u64) * (self.h as u64)
    }

    #[inline]
    pub fn contains_point(self, p: Point) -> bool {
        if self.is_empty() {
            return false;
        }
        let px = p.x as i64;
        let py = p.y as i64;
        px >= self.left64() && px < self.right64() && py >= self.top64() && py < self.bottom64()
    }

    #[inline]
    pub fn intersects(self, other: Self) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.left64() < other.right64()
            && self.right64() > other.left64()
            && self.top64() < other.bottom64()
            && self.bottom64() > other.top64()
    }

    pub fn intersect(self, other: Self) -> Self {
        if self.is_empty() || other.is_empty() {
            return Self::ZERO;
        }
        let l = self.left64().max(other.left64());
        let t = self.top64().max(other.top64());
        let r = self.right64().min(other.right64());
        let b = self.bottom64().min(other.bottom64());
        if r <= l || b <= t {
            return Self::ZERO;
        }
        Self::from_bounds64(l, t, r, b)
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let l = self.left64().min(other.left64());
        let t = self.top64().min(other.top64());
        let r = self.right64().max(other.right64());
        let b = self.bottom64().max(other.bottom64());
        Self::from_bounds64(l, t, r, b)
    }

    pub fn shares_edge_with(self, other: Self) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        let x_overlap = self.left64() < other.right64() && self.right64() > other.left64();
        let y_overlap = self.top64() < other.bottom64() && self.bottom64() > other.top64();
        let x_touch = self.right64() == other.left64() || other.right64() == self.left64();
        let y_touch = self.bottom64() == other.top64() || other.bottom64() == self.top64();
        (x_touch && y_overlap) || (y_touch && x_overlap)
    }

    pub fn translate(self, dx: i32, dy: i32) -> Self {
        if self.is_empty() {
            return Self::ZERO;
        }
        let x = (self.x as i64 + dx as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let y = (self.y as i64 + dy as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        Self {
            x,
            y,
            w: self.w,
            h: self.h,
        }
    }

    pub fn expand(self, margin: u32) -> Self {
        if self.is_empty() {
            return Self::ZERO;
        }
        let m = margin as i64;
        Self::from_bounds64(
            self.left64() - m,
            self.top64() - m,
            self.right64() + m,
            self.bottom64() + m,
        )
    }

    pub fn inset(self, left: u32, top: u32, right: u32, bottom: u32) -> Self {
        if self.is_empty() {
            return Self::ZERO;
        }
        let l = self.left64() + left as i64;
        let t = self.top64() + top as i64;
        let r = self.right64() - right as i64;
        let b = self.bottom64() - bottom as i64;
        if r <= l || b <= t {
            return Self::ZERO;
        }
        Self::from_bounds64(l, t, r, b)
    }

    fn from_bounds64(l: i64, t: i64, r: i64, b: i64) -> Self {
        let l_clamped = l.clamp(i32::MIN as i64, i32::MAX as i64);
        let t_clamped = t.clamp(i32::MIN as i64, i32::MAX as i64);
        let w = (r - l_clamped).clamp(0, u32::MAX as i64) as u32;
        let h = (b - t_clamped).clamp(0, u32::MAX as i64) as u32;
        Self {
            x: l_clamped as i32,
            y: t_clamped as i32,
            w,
            h,
        }
    }
}

#[inline(always)]
pub fn rgb565_to_u16(color: Rgb565) -> u16 {
    color.into_storage()
}

#[inline(always)]
pub fn u16_to_rgb565(word: u16) -> Rgb565 {
    Rgb565::from(RawU16::new(word))
}

/// Logical clockwise screen rotation relative to the physical panel coordinates.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum Rotation {
    #[default]
    Deg0 = 0,
    Deg90 = 1,
    Deg180 = 2,
    Deg270 = 3,
}

impl Rotation {
    /// Returns the logical UI screen dimensions given the `physical` panel dimensions.
    #[inline]
    pub const fn logical_size(self, physical: Size) -> Size {
        match self {
            Self::Deg0 | Self::Deg180 => physical,
            Self::Deg90 | Self::Deg270 => Size::new(physical.height, physical.width),
        }
    }

    /// Transforms a physical panel touch coordinate into logical UI coordinates.
    #[inline]
    pub const fn map_point_to_logical(self, pt: Point, physical: Size) -> Point {
        let max_x = physical.width.saturating_sub(1) as i32;
        let max_y = physical.height.saturating_sub(1) as i32;
        match self {
            Self::Deg0 => pt,
            Self::Deg90 => Point::new(pt.y, max_x.saturating_sub(pt.x)),
            Self::Deg180 => Point::new(max_x.saturating_sub(pt.x), max_y.saturating_sub(pt.y)),
            Self::Deg270 => Point::new(max_y.saturating_sub(pt.y), pt.x),
        }
    }

    /// Transforms a logical `(x, y)` coordinate into physical framebuffer `(px, py)` coordinates.
    #[inline]
    pub const fn map_point_to_physical(self, pt: Point, physical: Size) -> Point {
        let max_x = physical.width.saturating_sub(1) as i32;
        let max_y = physical.height.saturating_sub(1) as i32;
        match self {
            Self::Deg0 => pt,
            Self::Deg90 => Point::new(max_x.saturating_sub(pt.y), pt.x),
            Self::Deg180 => Point::new(max_x.saturating_sub(pt.x), max_y.saturating_sub(pt.y)),
            Self::Deg270 => Point::new(pt.y, max_y.saturating_sub(pt.x)),
        }
    }

    /// Maps a logical rectangle into physical panel coordinates.
    pub fn map_rect_to_physical(self, rect: Rect, physical: Size) -> Rect {
        let logical = self.logical_size(physical);
        let clipped = rect.intersect(Rect::new(0, 0, logical.width, logical.height));
        if clipped.is_empty() {
            return Rect::ZERO;
        }
        let x2 = clipped.x + clipped.w as i32 - 1;
        let y2 = clipped.y + clipped.h as i32 - 1;
        let p0 = self.map_point_to_physical(Point::new(clipped.x, clipped.y), physical);
        let p1 = self.map_point_to_physical(Point::new(x2, y2), physical);
        let min_x = p0.x.min(p1.x);
        let min_y = p0.y.min(p1.y);
        let max_x = p0.x.max(p1.x);
        let max_y = p0.y.max(p1.y);
        Rect::new(
            min_x,
            min_y,
            (max_x - min_x + 1).max(0) as u32,
            (max_y - min_y + 1).max(0) as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_roundtrips_points_and_rects() {
        let phys = Size::new(240, 320);
        for rot in [
            Rotation::Deg0,
            Rotation::Deg90,
            Rotation::Deg180,
            Rotation::Deg270,
        ] {
            let lsize = rot.logical_size(phys);
            for pt in [
                Point::new(0, 0),
                Point::new(15, 25),
                Point::new(lsize.width as i32 - 1, lsize.height as i32 - 1),
            ] {
                let phys_pt = rot.map_point_to_physical(pt, phys);
                assert_eq!(rot.map_point_to_logical(phys_pt, phys), pt);
            }
            let r = Rect::new(10, 20, 40, 30);
            let pr = rot.map_rect_to_physical(r, phys);
            assert_eq!(pr.w * pr.h, r.w * r.h);
        }
    }
}
