use liuli_core::view::FontId;

pub const LF4_MAGIC: &[u8; 4] = b"LLF4";
pub const LF4_VERSION: u16 = 1;
pub const LF4_HEADER_SIZE: usize = 24;
pub const LF4_ENTRY_SIZE: usize = 20;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FontError {
    TooShort,
    InvalidMagic,
    UnsupportedVersion,
    EmptyTable,
    InvalidOffsets,
    InvalidFallbackIndex,
    InvalidScalar,
    UnsortedIndex,
    TruncatedBitmap,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct GlyphMetrics {
    pub unicode: u32,
    pub advance: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub width: u16,
    pub height: u16,
    pub data_offset: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct GlyphBitmap<'a> {
    pub metrics: GlyphMetrics,
    pub data: &'a [u8],
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Lf4Font<'a> {
    bytes: &'a [u8],
    count: u16,
    ascent: i16,
    descent: u16,
    line_gap: u16,
    fallback_index: u16,
    table_offset: usize,
    bitmap_offset: usize,
}

impl<'a> Lf4Font<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, FontError> {
        if bytes.len() < LF4_HEADER_SIZE {
            return Err(FontError::TooShort);
        }
        if &bytes[0..4] != LF4_MAGIC {
            return Err(FontError::InvalidMagic);
        }
        let version = read_u16(bytes, 4);
        if version != LF4_VERSION {
            return Err(FontError::UnsupportedVersion);
        }
        let count = read_u16(bytes, 6);
        if count == 0 {
            return Err(FontError::EmptyTable);
        }
        let ascent = read_i16(bytes, 8);
        let descent = read_u16(bytes, 10);
        let line_gap = read_u16(bytes, 12);
        let fallback_index = read_u16(bytes, 14);
        if fallback_index >= count {
            return Err(FontError::InvalidFallbackIndex);
        }
        let table_offset = read_u32(bytes, 16) as usize;
        let bitmap_offset = read_u32(bytes, 20) as usize;

        if table_offset < LF4_HEADER_SIZE || bitmap_offset > bytes.len() {
            return Err(FontError::InvalidOffsets);
        }
        let table_len = (count as usize)
            .checked_mul(LF4_ENTRY_SIZE)
            .ok_or(FontError::InvalidOffsets)?;
        let table_end = table_offset
            .checked_add(table_len)
            .ok_or(FontError::InvalidOffsets)?;
        if table_end > bitmap_offset {
            return Err(FontError::InvalidOffsets);
        }

        let bitmap_len = bytes.len() - bitmap_offset;
        let mut prev_unicode: Option<u32> = None;

        for i in 0..count as usize {
            let entry = read_entry(bytes, table_offset + i * LF4_ENTRY_SIZE);
            if char::from_u32(entry.unicode).is_none() {
                return Err(FontError::InvalidScalar);
            }
            if let Some(prev) = prev_unicode
                && entry.unicode <= prev
            {
                return Err(FontError::UnsortedIndex);
            }
            prev_unicode = Some(entry.unicode);

            let row_bytes = (entry.width as usize).div_ceil(2);
            let bmp_size = row_bytes
                .checked_mul(entry.height as usize)
                .ok_or(FontError::TruncatedBitmap)?;
            let bmp_end = (entry.data_offset as usize)
                .checked_add(bmp_size)
                .ok_or(FontError::TruncatedBitmap)?;
            if bmp_end > bitmap_len {
                return Err(FontError::TruncatedBitmap);
            }
        }

        Ok(Self {
            bytes,
            count,
            ascent,
            descent,
            line_gap,
            fallback_index,
            table_offset,
            bitmap_offset,
        })
    }

    #[inline]
    pub const fn ascent(&self) -> i16 {
        self.ascent
    }

    #[inline]
    pub const fn descent(&self) -> u16 {
        self.descent
    }

    #[inline]
    pub const fn line_gap(&self) -> u16 {
        self.line_gap
    }

    #[inline]
    pub const fn line_height(&self) -> u32 {
        let asc = if self.ascent > 0 {
            self.ascent as u32
        } else {
            0
        };
        asc + (self.descent as u32) + (self.line_gap as u32)
    }

    #[inline]
    pub const fn glyph_count(&self) -> u16 {
        self.count
    }

    pub fn lookup_exact(&self, ch: char) -> Option<GlyphBitmap<'a>> {
        let target = ch as u32;
        let mut lo = 0usize;
        let mut hi = self.count as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let entry = read_entry(self.bytes, self.table_offset + mid * LF4_ENTRY_SIZE);
            if entry.unicode < target {
                lo = mid + 1;
            } else if entry.unicode > target {
                hi = mid;
            } else {
                return Some(self.glyph_at(entry));
            }
        }
        None
    }

    pub fn lookup_or_fallback(&self, ch: char) -> (GlyphBitmap<'a>, bool) {
        if let Some(g) = self.lookup_exact(ch) {
            return (g, false);
        }
        let fb_entry = read_entry(
            self.bytes,
            self.table_offset + (self.fallback_index as usize) * LF4_ENTRY_SIZE,
        );
        (self.glyph_at(fb_entry), true)
    }

    fn glyph_at(&self, metrics: GlyphMetrics) -> GlyphBitmap<'a> {
        let row_bytes = (metrics.width as usize).div_ceil(2);
        let len = row_bytes * (metrics.height as usize);
        let start = self.bitmap_offset + (metrics.data_offset as usize);
        GlyphBitmap {
            metrics,
            data: &self.bytes[start..start + len],
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FontSet<'a> {
    body16: Lf4Font<'a>,
    small12: Lf4Font<'a>,
}

impl<'a> FontSet<'a> {
    #[inline]
    pub const fn new(body16: Lf4Font<'a>, small12: Lf4Font<'a>) -> Self {
        Self { body16, small12 }
    }

    #[inline]
    pub const fn single(font: Lf4Font<'a>) -> Self {
        Self {
            body16: font,
            small12: font,
        }
    }

    #[inline]
    pub const fn get(&self, id: FontId) -> &Lf4Font<'a> {
        match id {
            FontId::Body16 => &self.body16,
            FontId::Small12 => &self.small12,
        }
    }

    #[inline]
    pub const fn body16(&self) -> &Lf4Font<'a> {
        &self.body16
    }

    #[inline]
    pub const fn small12(&self) -> &Lf4Font<'a> {
        &self.small12
    }
}

#[inline]
fn read_entry(bytes: &[u8], off: usize) -> GlyphMetrics {
    GlyphMetrics {
        unicode: read_u32(bytes, off),
        advance: read_u16(bytes, off + 4),
        bearing_x: read_i16(bytes, off + 6),
        bearing_y: read_i16(bytes, off + 8),
        width: read_u16(bytes, off + 10),
        height: read_u16(bytes, off + 12),
        data_offset: read_u32(bytes, off + 16),
    }
}

#[inline]
fn read_u16(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

#[inline]
fn read_i16(bytes: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([bytes[off], bytes[off + 1]])
}

#[inline]
fn read_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}
