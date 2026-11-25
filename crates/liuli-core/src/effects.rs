//! Effects are attached to stable keys and never change measurement or stacking order.
use crate::{
    capacity::BuildError,
    geometry::{Point, Rect, Rgb565},
    node::RetainedTree,
    time::Easing,
    view::Key,
};

/// Logical pixel offsets, falloff width and 4-bit opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shadow {
    pub color: Rgb565,
    pub x: i8,
    pub y: i8,
    pub blur: u8,
    pub opacity: u8,
}

impl Shadow {
    pub fn bounds(self, rect: Rect) -> Rect {
        rect.translate(self.x.into(), self.y.into())
            .expand(self.blur.into())
    }
    pub fn pressed(self, amount: u8) -> Self {
        self.pressed_scaled(amount, 15)
    }
    /// Compress shadow offsets and blur proportionally to `amount` (0..=15)
    /// up to `max_shrink` out of 30 (`15` = 50% compression, `30` = collapse to zero).
    pub fn pressed_scaled(self, amount: u8, max_shrink: u8) -> Self {
        let shrink = ((u16::from(amount.min(15)) * u16::from(max_shrink.min(30)) + 7) / 15) as i16;
        let scale = 30 - shrink;
        Self {
            x: (i16::from(self.x) * scale / 30) as i8,
            y: (i16::from(self.y) * scale / 30) as i8,
            blur: (i16::from(self.blur) * scale / 30) as u8,
            ..self
        }
    }
}

/// Shadow compression profile when a decorated node or descendant is pressed.
/// All-zero (`DEFAULT`) matches the standard 120 ms EaseOutCubic 50% compression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PressDepth {
    duration_ms: u16,
    scale: u8,
    mode: u8,
}

impl PressDepth {
    pub const DEFAULT: Self = Self {
        duration_ms: 0,
        scale: 0,
        mode: 0,
    };
    pub const OFF: Self = Self {
        duration_ms: 0,
        scale: 0,
        mode: 1,
    };

    /// Custom press compression: `scale` in `0..=30` (`15` = 50%, `0` disables).
    #[inline]
    pub const fn custom(duration_ms: u16, scale: u8, easing: Easing) -> Self {
        if scale == 0 {
            return Self::OFF;
        }
        let clamped = if scale > 30 { 30 } else { scale };
        let mode = match easing {
            Easing::Linear => 2,
            Easing::EaseOutCubic => 3,
        };
        Self {
            duration_ms,
            scale: clamped,
            mode,
        }
    }

    #[inline]
    pub const fn is_enabled(self) -> bool {
        self.mode != 1 && (self.mode == 0 || self.scale > 0)
    }

    #[inline]
    pub const fn duration_ms(self) -> u16 {
        match self.mode {
            0 => 120,
            1 => 0,
            _ => self.duration_ms,
        }
    }

    #[inline]
    pub const fn scale(self) -> u8 {
        match self.mode {
            0 => 15,
            1 => 0,
            _ => self.scale,
        }
    }

    #[inline]
    pub const fn easing(self) -> Easing {
        match self.mode {
            2 => Easing::Linear,
            _ => Easing::EaseOutCubic,
        }
    }
}

impl Default for PressDepth {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A plane rotating about its vertical centre. One complete turn is 256 units.
/// Positive angles bring the left edge nearer. The projected plane fits its layout box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Projection(pub u8);

/// The inverse mapping for a vertical screen column. Rendering reuses the
/// horizontal perspective solve for every pixel in that column.
#[derive(Clone, Copy)]
pub struct ProjectedColumn {
    x: i32,
    top: i32,
    height: u32,
    gain: i32,
    fit: i32,
}

impl ProjectedColumn {
    #[inline(always)]
    pub const fn x(self) -> i32 {
        self.x
    }

    #[inline(always)]
    pub fn contains_y_norm(self, norm: i32) -> bool {
        if self.fit == 0 {
            return true;
        }
        if norm >= 0 {
            norm * self.gain < 1024 * self.fit
        } else {
            (-norm) * self.gain < 1025 * self.fit
        }
    }

    #[inline]
    pub fn at(self, y: i32) -> Option<Point> {
        let norm = Projection::row_norm(self.top, self.height, y)?;
        if self.fit == 0 {
            return Some(Point::new(self.x, y));
        }
        let v = norm * self.gain / self.fit;
        if !(-1024..1024).contains(&v) {
            return None;
        }
        let row = (i64::from(v + 1024) * i64::from(self.height)) >> 11;
        Some(Point::new(self.x, (i64::from(self.top) + row) as i32))
    }

    /// Evaluate the inverse perspective mapping using a precomputed row normal and fit reciprocal.
    /// Produces the exact same result as [`Self::at`] without a per-pixel division.
    #[inline]
    pub fn at_norm(self, y: i32, norm: i32, fit_recip: u32) -> Option<Point> {
        if self.fit == 0 {
            return Some(Point::new(self.x, y));
        }
        let prod = norm * self.gain;
        let q = ((u64::from(prod.unsigned_abs()) * u64::from(fit_recip)) >> 34) as i32;
        let v = if prod < 0 { -q } else { q };
        if !(-1024..1024).contains(&v) {
            return None;
        }
        let row = if self.height <= (u32::MAX >> 11) {
            i64::from(((v + 1024) as u32 * self.height) >> 11)
        } else {
            (i64::from(v + 1024) * i64::from(self.height)) >> 11
        };
        Some(Point::new(self.x, (i64::from(self.top) + row) as i32))
    }
}

impl Projection {
    /// Precompute the normalized vertical offset for scanline `y` inside `[rect_y, rect_y + rect_h)`.
    #[inline]
    pub fn row_norm(rect_y: i32, rect_h: u32, y: i32) -> Option<i32> {
        let dy = i64::from(y) - i64::from(rect_y);
        if dy < 0 || dy >= i64::from(rect_h) {
            return None;
        }
        // A normal display fits the fixed-point numerator in a native register.
        // Retain the exact same rounding for arbitrarily tall logical rectangles.
        let normalized = if rect_h <= (i32::MAX / 1024) as u32 {
            ((2 * dy as i32 + 1 - rect_h as i32) * 1024).div_euclid(rect_h as i32)
        } else {
            ((2 * dy + 1 - i64::from(rect_h)) * 1024).div_euclid(i64::from(rect_h)) as i32
        };
        Some(normalized)
    }

    /// Precompute the Q34 ceiling reciprocal of `fit` for [`ProjectedColumn::at_norm`].
    #[inline]
    pub fn fit_recip(self) -> u32 {
        if self.0 == 0 {
            return 0;
        }
        let sin = i64::from(crate::bitmap::sine(self.0)) / 16;
        let fit = (3 * 1024 - sin.abs()) as u64;
        (1u64 << 34).div_ceil(fit) as u32
    }

    /// Map a screen pixel centre back to the plane. Input and rendering share the rounding.
    pub fn unproject(self, rect: Rect, p: Point) -> Option<Point> {
        self.column(rect, p.x)?.at(p.y)
    }

    #[inline]
    pub fn column(self, rect: Rect, screen_x: i32) -> Option<ProjectedColumn> {
        let dx = i64::from(screen_x) - i64::from(rect.x);
        let w = i64::from(rect.w);
        if dx < 0 || dx >= w || rect.h == 0 {
            return None;
        }
        let mut column = ProjectedColumn {
            x: screen_x,
            top: rect.y,
            height: rect.h,
            gain: 0,
            fit: 0,
        };
        if self.0 == 0 {
            return Some(column);
        }
        let sin = i32::from(crate::bitmap::sine(self.0)) / 16;
        let cos = i32::from(crate::bitmap::sine(self.0.wrapping_add(64))) / 16;
        if cos == 0 {
            return None;
        }
        const Q: i32 = 1024;
        let x = if rect.w <= (i32::MAX / Q) as u32 {
            ((2 * dx as i32 + 1 - rect.w as i32) * Q).div_euclid(rect.w as i32)
        } else {
            ((2 * dx + 1 - w) * i64::from(Q)).div_euclid(w) as i32
        };
        let fit = 3 * Q - sin.abs();
        let denominator = cos * fit - x * sin;
        if denominator == 0 {
            return None;
        }
        let num = (3 * (Q as u32) * (Q as u32)) * x.unsigned_abs();
        let mag = num / denominator.unsigned_abs();
        let neg = (x < 0) ^ (denominator < 0);
        if mag > Q as u32 || (!neg && mag == Q as u32) {
            return None;
        }
        let u = if neg { -(mag as i32) } else { mag as i32 };
        let col_dx = if rect.w <= (u32::MAX >> 11) {
            i64::from(((u + Q) as u32 * rect.w) >> 11)
        } else {
            (i64::from(u + Q) * w) >> 11
        };
        column.x = (i64::from(rect.x) + col_dx) as i32;
        column.gain = 3 * Q + (u * sin) / Q;
        column.fit = fit;
        Some(column)
    }
}

/// Isolate the decorated node and all its descendants as one transparent surface.
/// Backdrop radius is in logical pixels (0..=8). Its input includes preceding siblings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layer {
    pub opacity: u8,
    pub projection: Projection,
    pub backdrop_radius: u8,
    pub tint_color: Rgb565,
    pub tint_alpha: u8,
}
impl Layer {
    pub const OPAQUE: Self = Self {
        opacity: 15,
        projection: Projection(0),
        backdrop_radius: 0,
        tint_color: Rgb565::new(0, 0, 0),
        tint_alpha: 0,
    };

    #[inline]
    pub const fn with_opacity(mut self, opacity: u8) -> Self {
        self.opacity = opacity;
        self
    }

    #[inline]
    pub const fn with_projection(mut self, projection: Projection) -> Self {
        self.projection = projection;
        self
    }

    #[inline]
    pub const fn with_backdrop_radius(mut self, backdrop_radius: u8) -> Self {
        self.backdrop_radius = backdrop_radius;
        self
    }

    #[inline]
    pub const fn with_tint(mut self, tint_color: Rgb565, tint_alpha: u8) -> Self {
        self.tint_color = tint_color;
        self.tint_alpha = tint_alpha;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SurfaceStyle {
    pub shadow: Option<Shadow>,
    pub press_depth: PressDepth,
    #[cfg(feature = "layers")]
    pub layer: Option<Layer>,
}
impl SurfaceStyle {
    pub const FLAT: Self = Self {
        shadow: None,
        press_depth: PressDepth::DEFAULT,
        #[cfg(feature = "layers")]
        layer: None,
    };
    pub const fn has_layers_support() -> bool {
        cfg!(feature = "layers")
    }
    #[inline]
    pub const fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
    #[inline]
    pub const fn with_press_depth(mut self, press_depth: PressDepth) -> Self {
        self.press_depth = press_depth;
        self
    }
    #[inline]
    #[allow(unused_mut)]
    pub const fn with_layer(mut self, _layer: Layer) -> Self {
        #[cfg(feature = "layers")]
        {
            self.layer = Some(_layer);
        }
        self
    }
    pub fn layer(self) -> Option<Layer> {
        #[cfg(feature = "layers")]
        {
            self.layer
        }
        #[cfg(not(feature = "layers"))]
        {
            None
        }
    }
}

pub const MAX_EFFECTS: usize = 8;
#[derive(Clone, Copy)]
pub struct Effects {
    items: [(Key, SurfaceStyle); MAX_EFFECTS],
    len: u8,
}
impl Effects {
    pub const EMPTY: Self = Self {
        items: [(Key(0), SurfaceStyle::FLAT); MAX_EFFECTS],
        len: 0,
    };
    pub fn iter(&self) -> &[(Key, SurfaceStyle)] {
        &self.items[..usize::from(self.len)]
    }
    pub fn get(&self, key: Key) -> SurfaceStyle {
        self.iter()
            .iter()
            .find(|item| item.0 == key)
            .map_or(SurfaceStyle::FLAT, |item| item.1)
    }
    pub(crate) fn add(&mut self, key: Key, style: SurfaceStyle) -> Result<(), BuildError> {
        if style.shadow.is_some_and(|s| s.opacity > 15)
            || style.press_depth.scale() > 30
            || style
                .layer()
                .is_some_and(|s| s.opacity > 15 || s.backdrop_radius > 8 || s.tint_alpha > 15)
        {
            return Err(BuildError::InvalidRange);
        }
        if self.iter().iter().any(|i| i.0 == key) {
            return Err(BuildError::DuplicateKey);
        }
        if style.shadow.is_none() && style.layer().is_none() {
            return Ok(());
        }
        let entry = self
            .items
            .get_mut(usize::from(self.len))
            .ok_or(BuildError::Effects)?;
        *entry = (key, style);
        self.len += 1;
        Ok(())
    }
}
impl RetainedTree {
    /// Propagate local invalidation through effects, including a filter's sampling halo.
    pub fn expand_effect_damage(&self, damage: &mut crate::damage::DamageSet, screen: Rect) {
        if damage.is_empty() || damage.is_full_screen() {
            return;
        }
        let mut expanded = 0u16;
        for _ in 0..self.effects.iter().len() {
            let previous = expanded;
            for (i, &(key, style)) in self.effects.iter().iter().enumerate() {
                if expanded & (1 << i) != 0 {
                    continue;
                }
                let Some(node) = self.node_by_key(key) else {
                    continue;
                };
                if !node.props.visible() {
                    continue;
                }
                let rect = node.layout_rect;
                let mut output = rect;
                if let Some(shadow) = style.shadow {
                    output = output.union(shadow.bounds(rect));
                }
                let output = output.intersect(node.clip_rect);
                let input =
                    output.expand(style.layer().map_or(0, |s| u32::from(s.backdrop_radius)));
                if damage.as_slice().iter().any(|r| r.intersects(input)) {
                    expanded |= 1 << i;
                    if !damage
                        .as_slice()
                        .iter()
                        .any(|r| r.intersect(output) == output)
                    {
                        damage.add(output, screen);
                    }
                }
            }
            if previous == expanded {
                break;
            }
        }
    }
    pub fn sync_press_depth(&self, anims: &mut crate::time::AnimPool, now: crate::time::InstantMs) {
        use crate::time::AnimProp::PressDepth;
        if self.effects.iter().is_empty() {
            return;
        }
        let mut mask = crate::capacity::NodeMask::ZERO;
        for (i, n) in self.nodes.iter().enumerate() {
            if n.occupied && n.pressed {
                mask.set(i);
            }
        }
        for &(key, style) in self.effects.iter() {
            if style.shadow.is_none()
                || !style.press_depth.is_enabled()
                || style.press_depth.duration_ms() == 0
            {
                continue;
            }
            let Some(slot) = self.find_slot_by_key(key) else {
                continue;
            };
            let target = if self.subtree_pressed(slot, mask) {
                15
            } else {
                0
            };
            let rect = self.nodes[usize::from(slot)].visible_rect;
            if target != anims.target_value(key, PressDepth).unwrap_or(0) {
                anims.start_with_easing(
                    key,
                    PressDepth,
                    0,
                    target,
                    now,
                    u32::from(style.press_depth.duration_ms()),
                    rect,
                    style.press_depth.easing(),
                );
            } else {
                anims.update_rect(key, PressDepth, rect);
            }
        }
    }
    fn subtree_pressed(&self, ancestor: u16, mut mask: crate::capacity::NodeMask) -> bool {
        while let Some(idx) = mask.pop_first() {
            let mut cur = Some(idx as u16);
            while let Some(s) = cur {
                if s == ancestor {
                    return true;
                }
                cur = self.nodes[usize::from(s)].parent.get_u16();
            }
        }
        false
    }
    /// Pixel in the layout coordinates of a node, including its ancestor projections.
    pub fn local_point(&self, slot: u16, point: Point) -> Option<Point> {
        #[cfg(feature = "layers")]
        if !self.effects.iter().is_empty() {
            let mut chain = [0u16; crate::capacity::MAX_DEPTH + 1];
            let mut count = 0;
            let mut current = Some(slot);
            while let Some(index) = current {
                *chain.get_mut(count)? = index;
                count += 1;
                current = self.nodes[usize::from(index)].parent.get_u16();
            }
            let mut p = point;
            for &index in chain[..count].iter().rev() {
                let n = &self.nodes[usize::from(index)];
                if let Some(layer) = self.effects.get(n.key).layer() {
                    if layer.opacity == 0 || !n.clip_rect.contains_point(p) {
                        return None;
                    }
                    p = layer.projection.unproject(n.layout_rect, p)?;
                }
            }
            return Some(p);
        }
        let _ = slot;
        Some(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_column_32bit_matches_i64_reference() {
        fn ref_column(proj: Projection, rect: Rect, screen_x: i32) -> Option<(i32, i32, i32)> {
            let dx = i64::from(screen_x) - i64::from(rect.x);
            let w = i64::from(rect.w);
            if dx < 0 || dx >= w || rect.h == 0 {
                return None;
            }
            if proj.0 == 0 {
                return Some((screen_x, 0, 0));
            }
            let sin = i64::from(crate::bitmap::sine(proj.0)) / 16;
            let cos = i64::from(crate::bitmap::sine(proj.0.wrapping_add(64))) / 16;
            if cos == 0 {
                return None;
            }
            const Q: i64 = 1024;
            let x = ((2 * dx + 1 - w) * Q).div_euclid(w);
            let fit = 3 * Q - sin.abs();
            let denominator = cos * fit - x * sin;
            if denominator == 0 {
                return None;
            }
            let u = (3 * Q * Q * x) / denominator;
            if !(-Q..Q).contains(&u) {
                return None;
            }
            Some((
                (i64::from(rect.x) + (((u + Q) * w) >> 11)) as i32,
                (3 * Q + u * sin / Q) as i32,
                fit as i32,
            ))
        }

        for w in [1u32, 9, 173, 240, 800] {
            let rect = Rect::new(15, 25, w, 120);
            for angle in 0u8..=255 {
                let proj = Projection(angle);
                for dx in 0..w {
                    let sx = rect.x + dx as i32;
                    let actual = proj.column(rect, sx).map(|c| (c.x, c.gain, c.fit));
                    let expected = ref_column(proj, rect, sx);
                    assert_eq!(actual, expected, "w={w} angle={angle} sx={sx}");
                }
            }
        }
    }

    #[test]
    fn projected_column_at_norm_matches_at_bit_for_bit() {
        let rect = Rect::new(40, 30, 173, 97);
        for angle in 0u8..=255 {
            let proj = Projection(angle);
            let fit_recip = proj.fit_recip();
            for x in [rect.x, rect.x + 1, rect.x + 86, rect.x + 172] {
                let col = proj.column(rect, x);
                for y in rect.y - 1..=rect.bottom64() as i32 {
                    let expected = col.and_then(|c| c.at(y));
                    let actual = col.and_then(|c| {
                        let norm = Projection::row_norm(rect.y, rect.h, y)?;
                        c.at_norm(y, norm, fit_recip)
                    });
                    assert_eq!(actual, expected, "angle={angle} x={x} y={y}");
                    if y >= rect.y && y < rect.bottom64() as i32 {
                        let norm = Projection::row_norm(rect.y, rect.h, y).unwrap();
                        if let Some(c) = col {
                            assert_eq!(
                                c.contains_y_norm(norm),
                                c.at(y).is_some(),
                                "contains_y_norm angle={angle} x={x} y={y}"
                            );
                        }
                    }
                }
            }
        }
    }
}
