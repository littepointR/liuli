#![no_std]
#![forbid(unsafe_code)]

pub mod coverage;
pub mod font;
pub mod layout;
pub mod storage;

pub use coverage::{a4_pixel, blend_over_rgb565_a4, blend_rgb565_a4, blend_rgb565_a4_u16};
pub use font::{FontError, FontSet, GlyphBitmap, GlyphMetrics, Lf4Font};
pub use layout::{PositionedGlyph, TextLayoutOptions, visit_layout_glyphs};
