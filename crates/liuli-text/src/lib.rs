#![no_std]
#![forbid(unsafe_code)]

pub mod cache;
pub mod coverage;
pub mod font;
pub mod layout;
pub mod storage;

pub use cache::{FontCacheStats, GlyphPoolCache, MetricsCache, StreamingFont};
pub use coverage::{
    a1_pixel, a2_pixel, a4_pixel, a8_pixel, blend_over_rgb565_a4, blend_rgb565_a4,
    blend_rgb565_a4_u16, blend_rgb565_a8_u16, blend_rgb565_lcd_a4_u16, decode_rle_into,
    decode_rle_rows, gamma_remap_a4, gamma_remap_a8, lcd_rgb_pixel, validate_rle,
};
pub use font::{
    FaceInfo, FontError, FontFace, FontProvider, FontSet, FontSlot, GlyphBitmap, GlyphEncoding,
    GlyphFormat, GlyphMetrics, HybridFontSet, Lf4Font, ResolvedGlyph,
};
pub use layout::{
    LineDecor, PositionedGlyph, PositionedGlyphEx, PositionedGlyphMetrics, ProviderLayoutStyle,
    TextLayoutOptions, TextLayoutStyle, visit_layout_glyphs, visit_layout_glyphs_provider,
    visit_layout_glyphs_styled,
};
pub use storage::{BlockCache, Cached, ReadAt, ReadError, SparseIndex, StoredFont};
