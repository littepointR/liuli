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

        if (0x4E00..=0x9FFF).contains(&glyph.unicode) && has_intermediate {
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

#[inline]
fn read_u16_le(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

#[inline]
fn read_u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

pub fn verify_lf4_invariants(bytes: &[u8], atlas: &Atlas) -> Result<(), String> {
    if bytes.len() < HEADER_SIZE {
        return Err(format!(
            "LF4 shorter than 24-byte header ({} bytes)",
            bytes.len()
        ));
    }
    if &bytes[0..4] != LF4_MAGIC {
        return Err("LF4 magic mismatch".into());
    }
    let version = read_u16_le(bytes, 4);
    if version != LF4_VERSION {
        return Err(format!("unexpected LF4 version {version}"));
    }
    let count = usize::from(read_u16_le(bytes, 6));
    if count == 0 || count != atlas.glyphs.len() {
        return Err(format!(
            "LF4 glyph count mismatch: got {count}, expected {}",
            atlas.glyphs.len()
        ));
    }
    let fallback_index = usize::from(read_u16_le(bytes, 14));
    if fallback_index >= count {
        return Err(format!(
            "LF4 fallback_index {fallback_index} >= count {count}"
        ));
    }
    let table_offset = read_u32_le(bytes, 16) as usize;
    let bitmap_offset = read_u32_le(bytes, 20) as usize;
    let expected_bitmap_off = HEADER_SIZE + count * ENTRY_SIZE;
    if table_offset != HEADER_SIZE || bitmap_offset != expected_bitmap_off {
        return Err(format!(
            "LF4 offsets mismatch: table={table_offset} bitmap={bitmap_offset} expected_bitmap={expected_bitmap_off}"
        ));
    }
    if bitmap_offset > bytes.len() {
        return Err("LF4 bitmap_offset past EOF".into());
    }

    let mut prev_unicode: Option<u32> = None;
    let mut sum_bmp_bytes = 0usize;
    let mut cjk_intermediate = 0usize;

    for i in 0..count {
        let off = table_offset + i * ENTRY_SIZE;
        let unicode = read_u32_le(bytes, off);
        let w = usize::from(read_u16_le(bytes, off + 10));
        let h = usize::from(read_u16_le(bytes, off + 12));
        let data_offset = read_u32_le(bytes, off + 16) as usize;

        if char::from_u32(unicode).is_none() {
            return Err(format!("invalid scalar U+{unicode:04X} at entry {i}"));
        }
        if let Some(prev) = prev_unicode
            && unicode <= prev
        {
            return Err(format!("unsorted or duplicate scalar U+{unicode:04X}"));
        }
        prev_unicode = Some(unicode);
        if i == fallback_index && unicode != FALLBACK_CODEPOINT {
            return Err(format!(
                "fallback entry points to U+{unicode:04X} instead of U+{FALLBACK_CODEPOINT:04X}"
            ));
        }

        let row_bytes = w.div_ceil(2);
        let bmp_bytes = row_bytes
            .checked_mul(h)
            .ok_or_else(|| "LF4 bitmap entry overflow".to_string())?;
        let start = bitmap_offset + data_offset;
        let end = start
            .checked_add(bmp_bytes)
            .ok_or_else(|| "LF4 bitmap span overflow".to_string())?;
        if end > bytes.len() {
            return Err(format!("LF4 entry U+{unicode:04X} truncated past EOF"));
        }
        sum_bmp_bytes += bmp_bytes;

        let mut has_mid = false;
        let odd_w = w % 2 == 1;
        for row in 0..h {
            let row_slice = &bytes[start + row * row_bytes..start + (row + 1) * row_bytes];
            if odd_w && row_bytes > 0 && (row_slice[row_bytes - 1] & 0x0F) != 0 {
                return Err(format!(
                    "odd-width glyph U+{unicode:04X} has non-zero low trailing nibble at row {row}"
                ));
            }
            for &byte in row_slice {
                let hi = byte >> 4;
                let lo = byte & 0x0F;
                if (1..=14).contains(&hi) || (1..=14).contains(&lo) {
                    has_mid = true;
                }
            }
        }
        if (0x4E00..=0x9FFF).contains(&unicode) && has_mid {
            cjk_intermediate += 1;
        }
    }

    let expected_total = HEADER_SIZE + count * ENTRY_SIZE + sum_bmp_bytes;
    if bytes.len() != expected_total {
        return Err(format!(
            "LF4 length formula mismatch: actual={} expected={expected_total}",
            bytes.len()
        ));
    }
    if cjk_intermediate == 0 {
        return Err("LF4 has zero CJK glyphs (U+4E00..=U+9FFF) with intermediate coverage".into());
    }
    Ok(())
}
