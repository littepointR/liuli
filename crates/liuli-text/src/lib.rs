#![no_std]
#![forbid(unsafe_code)]

pub mod coverage;
pub mod font;
pub mod layout;

pub use coverage::{a4_pixel, blend_rgb565_a4};
pub use font::{FontError, GlyphBitmap, GlyphMetrics, Lf4Font};
pub use layout::{PositionedGlyph, TextLayoutOptions, visit_layout_glyphs};
