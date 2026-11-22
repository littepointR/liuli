use crate::atlas::Atlas;

pub const LF4_MAGIC: &[u8; 4] = b"LLF4";
pub const LF4_VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 24;
pub const ENTRY_SIZE: usize = 20;
pub const FALLBACK_CODEPOINT: u32 = 0x25A1; // '□'

#[derive(Debug, Clone)]
pub struct PackStats {
    pub glyph_count: usize,
    pub total_bytes: usize,
    pub cjk_with_intermediate_cov: usize,
}

pub fn pack_lf4(atlas: &Atlas) -> Result<(Vec<u8>, PackStats), String> {
    let count_u16 = u16::try_from(atlas.glyphs.len())
        .map_err(|_| "glyph count exceeds u16::MAX".to_string())?;
    let fallback_index = atlas
        .glyphs
        .iter()
        .position(|g| g.unicode == FALLBACK_CODEPOINT)
        .ok_or_else(|| "missing fallback glyph U+25A1 ('□')".to_string())?;
    let fallback_index_u16 = u16::try_from(fallback_index).unwrap();

    let table_offset = HEADER_SIZE as u32;
    let bitmap_offset_usize = HEADER_SIZE + atlas.glyphs.len() * ENTRY_SIZE;
    let bitmap_offset =
        u32::try_from(bitmap_offset_usize).map_err(|_| "table size exceeds u32".to_string())?;

    let mut table_bytes = Vec::with_capacity(atlas.glyphs.len() * ENTRY_SIZE);
    let mut bitmap_bytes = Vec::new();
    let mut cjk_with_intermediate_cov = 0usize;

    for glyph in &atlas.glyphs {
        let data_offset = u32::try_from(bitmap_bytes.len())
            .map_err(|_| "bitmap section exceeds u32".to_string())?;

        let w = usize::from(glyph.width_px);
        let h = usize::from(glyph.height_px);
        let row_bytes = w.div_ceil(2);
        let mut has_intermediate = false;

        for ty in 0..h {
            let mut byte = 0u8;
            for tx in 0..w {
                let mut n = 0u16;
                let base_x = glyph.cell_x + tx * 4;
                let base_y = glyph.cell_y + ty * 4;
                for sy in 0..4 {
                    for sx in 0..4 {
                        if atlas.pixel(base_x + sx, base_y + sy) {
                            n += 1;
                        }
                    }
                }
                let cov = ((15 * n + 8) / 16) as u8;
                if (1..=14).contains(&cov) {
                    has_intermediate = true;
                }
                if tx % 2 == 0 {
                    byte = cov << 4;
                } else {
                    byte |= cov;
                    bitmap_bytes.push(byte);
                    byte = 0;
                }
            }
            if w % 2 == 1 {
                bitmap_bytes.push(byte);
            }
        }
        debug_assert_eq!(bitmap_bytes.len() - (data_offset as usize), row_bytes * h);

        if glyph.unicode >= 0x4E00 && has_intermediate {
            cjk_with_intermediate_cov += 1;
        }

        table_bytes.extend_from_slice(&glyph.unicode.to_le_bytes());
        table_bytes.extend_from_slice(&glyph.advance.to_le_bytes());
        table_bytes.extend_from_slice(&glyph.bearing_x.to_le_bytes());
        table_bytes.extend_from_slice(&glyph.bearing_y.to_le_bytes());
        table_bytes.extend_from_slice(&glyph.width_px.to_le_bytes());
        table_bytes.extend_from_slice(&glyph.height_px.to_le_bytes());
        table_bytes.extend_from_slice(&0u16.to_le_bytes());
        table_bytes.extend_from_slice(&data_offset.to_le_bytes());
    }

    let mut out = Vec::with_capacity(bitmap_offset_usize + bitmap_bytes.len());
    out.extend_from_slice(LF4_MAGIC);
    out.extend_from_slice(&LF4_VERSION.to_le_bytes());
    out.extend_from_slice(&count_u16.to_le_bytes());
    out.extend_from_slice(&atlas.ascent.to_le_bytes());
    out.extend_from_slice(&atlas.descent.to_le_bytes());
    out.extend_from_slice(&atlas.line_gap.to_le_bytes());
    out.extend_from_slice(&fallback_index_u16.to_le_bytes());
    out.extend_from_slice(&table_offset.to_le_bytes());
    out.extend_from_slice(&bitmap_offset.to_le_bytes());
    debug_assert_eq!(out.len(), HEADER_SIZE);

    out.extend_from_slice(&table_bytes);
    out.extend_from_slice(&bitmap_bytes);

    let stats = PackStats {
        glyph_count: atlas.glyphs.len(),
        total_bytes: out.len(),
        cjk_with_intermediate_cov,
    };
    Ok((out, stats))
}
