//! Coverage rasterization for shaped glyph IDs from outline fonts.
use crate::ShapingFont;
use ab_glyph_rasterizer::{Point, Rasterizer, point};
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
