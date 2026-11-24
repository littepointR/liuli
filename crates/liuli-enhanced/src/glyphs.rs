//! Coverage rasterization and LRU glyph caching for shaped outline glyph IDs.
use crate::ShapingFont;
use ab_glyph_rasterizer::{Point, Rasterizer, point};
use alloc::vec;
use alloc::vec::Vec;
use ttf_parser::{GlyphId, OutlineBuilder};
struct Outline {
    raster: Rasterizer,
    scale: f32,
    left: f32,
    top: f32,
    cursor: Point,
    start: Point,
}
impl Outline {
    fn map(&self, x: f32, y: f32) -> Point {
        point(x * self.scale - self.left, self.top - y * self.scale)
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cursor = self.map(x, y);
        self.start = self.cursor;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let end = self.map(x, y);
        self.raster.draw_line(self.cursor, end);
        self.cursor = end;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let end = self.map(x, y);
        self.raster.draw_quad(self.cursor, self.map(x1, y1), end);
        self.cursor = end;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let end = self.map(x, y);
        self.raster
            .draw_cubic(self.cursor, self.map(x1, y1), self.map(x2, y2), end);
        self.cursor = end;
    }
    fn close(&mut self) {
        self.raster.draw_line(self.cursor, self.start);
        self.cursor = self.start;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphError {
    InvalidSize,
    MissingGlyph,
    PixelBudget,
}

/// Draw the outline at an integer baseline. Coverage ranges from zero to fifteen.
/// The pixel budget bounds raster area per glyph, excluding the rasterizer's internal padding.
pub fn raster_glyph(
    font: &ShapingFont<'_>,
    id: u16,
    px: u16,
    baseline: (i32, i32),
    pixel_budget: usize,
    mut pixel: impl FnMut(i32, i32, u8),
) -> Result<(), GlyphError> {
    if px == 0 {
        return Err(GlyphError::InvalidSize);
    }
    let id = GlyphId(id);
    if id.0 >= font.cmap.number_of_glyphs() {
        return Err(GlyphError::MissingGlyph);
    }
    let Some(bounds) = font.cmap.glyph_bounding_box(id) else {
        return Ok(());
    };
    let scale = f32::from(px) / f32::from(font.cmap.units_per_em());
    let left = libm::floorf(f32::from(bounds.x_min) * scale);
    let top = libm::ceilf(f32::from(bounds.y_max) * scale);
    let w = (libm::ceilf(f32::from(bounds.x_max) * scale) - left) as usize;
    let h = (top - libm::floorf(f32::from(bounds.y_min) * scale)) as usize;
    if w == 0 || h == 0 {
        return Ok(());
    }
    w.checked_mul(h)
        .filter(|&area| area <= pixel_budget && area <= isize::MAX as usize / 8)
        .ok_or(GlyphError::PixelBudget)?;
    let mut outline = Outline {
        raster: Rasterizer::new(w, h),
        scale,
        left,
        top,
        cursor: point(0., 0.),
        start: point(0., 0.),
    };
    font.cmap
        .outline_glyph(id, &mut outline)
        .ok_or(GlyphError::MissingGlyph)?;
    let x0 = baseline.0.saturating_add(left as i32);
    let y0 = baseline.1.saturating_sub(top as i32);
    outline.raster.for_each_pixel_2d(|x, y, coverage| {
        let a = libm::roundf(15. * coverage.clamp(0., 1.)) as u8;
        if a > 0 {
            pixel(x0.saturating_add(x as i32), y0.saturating_add(y as i32), a);
        }
    });
    Ok(())
}

/// Cached 4-bit (MSB-first packed nibbles per row) glyph bitmap produced by [`GlyphCache`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedGlyph {
    pub left: i16,
    pub top: i16,
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

struct CacheEntry {
    key: (usize, u16, u16),
    last_used: u64,
    glyph: CachedGlyph,
}

/// Bounded-byte LRU cache for [`ShapingFont`] outline rasterization.
pub struct GlyphCache {
    max_bytes: usize,
    used_bytes: usize,
    tick: u64,
    hits: u64,
    misses: u64,
    entries: Vec<CacheEntry>,
}

impl GlyphCache {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            used_bytes: 0,
            tick: 0,
            hits: 0,
            misses: 0,
            entries: Vec::new(),
        }
    }

    pub fn bytes_used(&self) -> usize {
        self.used_bytes
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.used_bytes = 0;
    }

    /// Looks up `(font_idx, id, px)` in the LRU cache, rasterizing and inserting on miss.
    /// Returns `Ok(None)` when the glyph has no outline/area or exceeds `max_bytes`.
    pub fn raster_glyph_cached(
        &mut self,
        font_idx: usize,
        font: &ShapingFont<'_>,
        id: u16,
        px: u16,
        pixel_budget: usize,
    ) -> Result<Option<&CachedGlyph>, GlyphError> {
        if px == 0 {
            return Err(GlyphError::InvalidSize);
        }
        let key = (font_idx, id, px);
        if let Some(pos) = self.entries.iter().position(|e| e.key == key) {
            self.tick = self.tick.wrapping_add(1);
            self.hits = self.hits.wrapping_add(1);
            self.entries[pos].last_used = self.tick;
            return Ok(Some(&self.entries[pos].glyph));
        }

        let gid = GlyphId(id);
        if gid.0 >= font.cmap.number_of_glyphs() {
            return Err(GlyphError::MissingGlyph);
        }
        let Some(bounds) = font.cmap.glyph_bounding_box(gid) else {
            return Ok(None);
        };
        let scale = f32::from(px) / f32::from(font.cmap.units_per_em());
        let left_f = libm::floorf(f32::from(bounds.x_min) * scale);
        let top_f = libm::ceilf(f32::from(bounds.y_max) * scale);
        let w = (libm::ceilf(f32::from(bounds.x_max) * scale) - left_f) as usize;
        let h = (top_f - libm::floorf(f32::from(bounds.y_min) * scale)) as usize;
        if w == 0 || h == 0 {
            return Ok(None);
        }
        w.checked_mul(h)
            .filter(|&area| area <= pixel_budget && area <= isize::MAX as usize / 8)
            .ok_or(GlyphError::PixelBudget)?;

        let row_bytes = w.div_ceil(2);
        let data_len = row_bytes * h;
        if data_len > self.max_bytes {
            return Ok(None);
        }
        let Ok(left) = i16::try_from(left_f as i32) else {
            return Ok(None);
        };
        let Ok(top) = i16::try_from(top_f as i32) else {
            return Ok(None);
        };
        let (Ok(width), Ok(height)) = (u16::try_from(w), u16::try_from(h)) else {
            return Ok(None);
        };

        self.misses = self.misses.wrapping_add(1);
        let mut outline = Outline {
            raster: Rasterizer::new(w, h),
            scale,
            left: left_f,
            top: top_f,
            cursor: point(0., 0.),
            start: point(0., 0.),
        };
        font.cmap
            .outline_glyph(gid, &mut outline)
            .ok_or(GlyphError::MissingGlyph)?;
        let mut data = vec![0u8; data_len];
        outline.raster.for_each_pixel_2d(|x, y, coverage| {
            let a = libm::roundf(15. * coverage.clamp(0., 1.)) as u8;
            if a > 0 {
                let idx = (y as usize) * row_bytes + (x as usize >> 1);
                data[idx] |= if (x & 1) == 0 { a << 4 } else { a };
            }
        });

        while self.used_bytes + data_len > self.max_bytes && !self.entries.is_empty() {
            let victim = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(i, _)| i)
                .unwrap();
            let removed = self.entries.swap_remove(victim);
            self.used_bytes -= removed.glyph.data.len();
        }
        self.tick = self.tick.wrapping_add(1);
        self.used_bytes += data_len;
        self.entries.push(CacheEntry {
            key,
            last_used: self.tick,
            glyph: CachedGlyph {
                left,
                top,
                width,
                height,
                data,
            },
        });
        Ok(self.entries.last().map(|e| &e.glyph))
    }
}
