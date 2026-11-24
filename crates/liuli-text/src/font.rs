use liuli_core::view::FontId;

use crate::cache::FontCacheStats;
use crate::coverage::validate_rle;

pub const LF4_MAGIC: &[u8; 4] = b"LLF4";
pub const LF4_VERSION: u16 = 1;
pub const LF4_VERSION_V2: u16 = 2;
pub const LF4_HEADER_SIZE: usize = 24;
pub const LF4_V2_HEADER_SIZE: usize = 32;
pub const LF4_ENTRY_SIZE: usize = 20;
pub const LF4_COMPACT_ENTRY_SIZE: usize = 12;
pub const LF4_KERN_ENTRY_SIZE: usize = 12;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum GlyphFormat {
    A1 = 1,
    A2 = 2,
    #[default]
    A4 = 4,
    A8 = 8,
    LcdRgb = 12,
}

impl GlyphFormat {
    #[inline]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::A1),
            2 => Some(Self::A2),
            4 => Some(Self::A4),
            8 => Some(Self::A8),
            12 => Some(Self::LcdRgb),
            _ => None,
        }
    }

    #[inline]
    pub const fn row_bytes(self, width: u16) -> usize {
        let w = width as usize;
        match self {
            Self::A1 => w.div_ceil(8),
            Self::A2 => w.div_ceil(4),
            Self::A4 => w.div_ceil(2),
            Self::A8 => w,
            Self::LcdRgb => (w * 3).div_ceil(2),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum GlyphEncoding {
    #[default]
    Raw = 0,
    Rle = 1,
}

impl GlyphEncoding {
    #[inline]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Raw),
            1 => Some(Self::Rle),
            _ => None,
        }
    }
}

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
    UnsupportedFormat,
    InvalidKerningTable,
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

const FN_V2: u8 = 1 << 0;
const FN_COMPACT: u8 = 1 << 1;
const FN_Q2: u8 = 1 << 2;
const FN_ASCII_DENSE: u8 = 1 << 3;
const FN_RLE: u8 = 1 << 4;
const FN_HAS_KERN: u8 = 1 << 7;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Lf4Font<'a> {
    bytes: &'a [u8],
    table_offset: u32,
    bitmap_offset: u32,
    count: u16,
    ascii_lo: u16,
    ascii_hi: u16,
    ascii_first: u8,
    flags: u8,
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
        if version != LF4_VERSION && version != LF4_VERSION_V2 {
            return Err(FontError::UnsupportedVersion);
        }
        if version == LF4_VERSION_V2 && bytes.len() < LF4_V2_HEADER_SIZE {
            return Err(FontError::TooShort);
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

        let min_header = if version == LF4_VERSION_V2 {
            LF4_V2_HEADER_SIZE
        } else {
            LF4_HEADER_SIZE
        };
        let (format, encoding, compact_index, subpixel_phases, advance_q2, kern_offset) = if version
            == LF4_VERSION_V2
        {
            let format = GlyphFormat::from_raw(bytes[24]).ok_or(FontError::UnsupportedFormat)?;
            let encoding =
                GlyphEncoding::from_raw(bytes[25]).ok_or(FontError::UnsupportedFormat)?;
            let flags = bytes[26];
            let compact_index = (flags & 0x01) != 0;
            let phases: u8 = match (flags >> 1) & 0x03 {
                0 => 1,
                1 => 2,
                2 => 4,
                _ => return Err(FontError::UnsupportedFormat),
            };
            let advance_q2 = (flags & 0x08) != 0;
            let kern_offset = read_u32(bytes, 28) as usize;
            (
                format,
                encoding,
                compact_index,
                phases,
                advance_q2,
                kern_offset,
            )
        } else {
            (GlyphFormat::A4, GlyphEncoding::Raw, false, 1, false, 0)
        };

        let entry_size = if compact_index {
            LF4_COMPACT_ENTRY_SIZE
        } else {
            LF4_ENTRY_SIZE
        };

        if table_offset < min_header || bitmap_offset > bytes.len() {
            return Err(FontError::InvalidOffsets);
        }
        let table_len = (count as usize)
            .checked_mul(entry_size)
            .ok_or(FontError::InvalidOffsets)?;
        let table_end = table_offset
            .checked_add(table_len)
            .ok_or(FontError::InvalidOffsets)?;
        if table_end > bitmap_offset {
            return Err(FontError::InvalidOffsets);
        }

        let kern_count = if kern_offset != 0 {
            if kern_offset < min_header || kern_offset.saturating_add(4) > table_offset {
                return Err(FontError::InvalidKerningTable);
            }
            let kc = read_u32(bytes, kern_offset) as usize;
            let kend = kern_offset
                .checked_add(4)
                .and_then(|s| {
                    kc.checked_mul(LF4_KERN_ENTRY_SIZE)
                        .and_then(|l| s.checked_add(l))
                })
                .ok_or(FontError::InvalidKerningTable)?;
            if kend > table_offset {
                return Err(FontError::InvalidKerningTable);
            }
            let mut prev_pair: Option<(u32, u32)> = None;
            for i in 0..kc {
                let off = kern_offset + 4 + i * LF4_KERN_ENTRY_SIZE;
                let l = read_u32(bytes, off);
                let r = read_u32(bytes, off + 4);
                if char::from_u32(l).is_none() || char::from_u32(r).is_none() {
                    return Err(FontError::InvalidKerningTable);
                }
                if let Some(prev) = prev_pair
                    && (l, r) <= prev
                {
                    return Err(FontError::InvalidKerningTable);
                }
                prev_pair = Some((l, r));
            }
            kc
        } else {
            0
        };

        let bitmap_len = bytes.len() - bitmap_offset;
        let mut prev_unicode: Option<u32> = None;
        let mut ascii_lo = count;
        let mut ascii_hi = 0u16;
        let mut ascii_first = 0u32;
        let mut ascii_last = 0u32;

        for i in 0..count as usize {
            let entry = read_entry_mode(bytes, table_offset + i * entry_size, compact_index);
            if char::from_u32(entry.unicode).is_none() {
                return Err(FontError::InvalidScalar);
            }
            if let Some(prev) = prev_unicode
                && entry.unicode <= prev
            {
                return Err(FontError::UnsortedIndex);
            }
            prev_unicode = Some(entry.unicode);

            if (0x20..=0x7E).contains(&entry.unicode) {
                let idx = i as u16;
                if ascii_lo == count {
                    ascii_lo = idx;
                    ascii_first = entry.unicode;
                }
                ascii_hi = idx + 1;
                ascii_last = entry.unicode;
            }

            let row_bytes = format.row_bytes(entry.width);
            let raw_size = row_bytes
                .checked_mul(entry.height as usize)
                .and_then(|s| s.checked_mul(usize::from(subpixel_phases)))
                .ok_or(FontError::TruncatedBitmap)?;
            if encoding == GlyphEncoding::Raw {
                let bmp_end = (entry.data_offset as usize)
                    .checked_add(raw_size)
                    .ok_or(FontError::TruncatedBitmap)?;
                if bmp_end > bitmap_len {
                    return Err(FontError::TruncatedBitmap);
                }
            } else if raw_size > 0 {
                let off = entry.data_offset as usize;
                let hdr_end = off.checked_add(2).ok_or(FontError::TruncatedBitmap)?;
                if hdr_end > bitmap_len {
                    return Err(FontError::TruncatedBitmap);
                }
                let rle_len = usize::from(read_u16(bytes, bitmap_offset + off));
                let bmp_end = hdr_end
                    .checked_add(rle_len)
                    .ok_or(FontError::TruncatedBitmap)?;
                if bmp_end > bitmap_len {
                    return Err(FontError::TruncatedBitmap);
                }
                if !validate_rle(
                    &bytes[bitmap_offset + hdr_end..bitmap_offset + bmp_end],
                    raw_size,
                ) {
                    return Err(FontError::TruncatedBitmap);
                }
            }
        }

        let ascii_dense =
            ascii_hi > ascii_lo && (ascii_last - ascii_first + 1) == u32::from(ascii_hi - ascii_lo);
        let _ = (ascent, descent, line_gap);
        let phase_code = match subpixel_phases {
            2 => 1u8 << 5,
            4 => 2u8 << 5,
            _ => 0,
        };
        let fn_flags = (if version == LF4_VERSION_V2 { FN_V2 } else { 0 })
            | (if compact_index { FN_COMPACT } else { 0 })
            | (if advance_q2 { FN_Q2 } else { 0 })
            | (if ascii_dense { FN_ASCII_DENSE } else { 0 })
            | (if encoding == GlyphEncoding::Rle {
                FN_RLE
            } else {
                0
            })
            | phase_code
            | (if kern_count > 0 { FN_HAS_KERN } else { 0 });

        Ok(Self {
            bytes,
            table_offset: table_offset as u32,
            bitmap_offset: bitmap_offset as u32,
            count,
            ascii_lo,
            ascii_hi,
            ascii_first: ascii_first as u8,
            flags: fn_flags,
        })
    }

    #[inline]
    pub const fn version(&self) -> u16 {
        if (self.flags & FN_V2) != 0 {
            LF4_VERSION_V2
        } else {
            LF4_VERSION
        }
    }

    #[inline]
    pub const fn format(&self) -> GlyphFormat {
        if (self.flags & FN_V2) != 0 {
            match self.bytes[24] {
                1 => GlyphFormat::A1,
                2 => GlyphFormat::A2,
                8 => GlyphFormat::A8,
                12 => GlyphFormat::LcdRgb,
                _ => GlyphFormat::A4,
            }
        } else {
            GlyphFormat::A4
        }
    }

    #[inline]
    pub const fn encoding(&self) -> GlyphEncoding {
        if (self.flags & FN_RLE) != 0 {
            GlyphEncoding::Rle
        } else {
            GlyphEncoding::Raw
        }
    }

    #[inline]
    pub const fn subpixel_phases(&self) -> u8 {
        match (self.flags >> 5) & 0x03 {
            1 => 2,
            2 => 4,
            _ => 1,
        }
    }

    #[inline]
    pub const fn advance_is_q2(&self) -> bool {
        (self.flags & FN_Q2) != 0
    }

    #[inline]
    pub const fn compact_index(&self) -> bool {
        (self.flags & FN_COMPACT) != 0
    }

    #[inline]
    pub const fn has_kerning(&self) -> bool {
        (self.flags & FN_HAS_KERN) != 0
    }

    #[inline]
    pub const fn ascent(&self) -> i16 {
        read_i16(self.bytes, 8)
    }

    #[inline]
    pub const fn descent(&self) -> u16 {
        read_u16(self.bytes, 10)
    }

    #[inline]
    pub const fn line_gap(&self) -> u16 {
        read_u16(self.bytes, 12)
    }

    #[inline]
    pub const fn line_height(&self) -> u32 {
        let ascent = self.ascent();
        let asc = if ascent > 0 { ascent as u32 } else { 0 };
        asc + (self.descent() as u32) + (self.line_gap() as u32)
    }

    #[inline]
    pub const fn glyph_count(&self) -> u16 {
        self.count
    }

    #[inline]
    pub fn kerning(&self, left: char, right: char) -> i16 {
        if !self.has_kerning() {
            return 0;
        }
        let kern_offset = read_u32(self.bytes, 28) as usize;
        let kern_count = read_u32(self.bytes, kern_offset) as usize;
        let target = (left as u32, right as u32);
        let mut lo = 0usize;
        let mut hi = kern_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let off = kern_offset + 4 + mid * LF4_KERN_ENTRY_SIZE;
            let k = (read_u32(self.bytes, off), read_u32(self.bytes, off + 4));
            if k < target {
                lo = mid + 1;
            } else if k > target {
                hi = mid;
            } else {
                return read_i16(self.bytes, off + 8);
            }
        }
        0
    }

    pub fn lookup_exact(&self, ch: char) -> Option<GlyphBitmap<'a>> {
        let target = ch as u32;
        let compact = self.compact_index();
        let table_offset = self.table_offset as usize;
        let entry_size = if compact {
            LF4_COMPACT_ENTRY_SIZE
        } else {
            LF4_ENTRY_SIZE
        };
        if (0x20..=0x7E).contains(&target) {
            if (self.flags & FN_ASCII_DENSE) != 0 {
                let first = u32::from(self.ascii_first);
                let last = first + u32::from(self.ascii_hi - self.ascii_lo) - 1;
                if target >= first && target <= last {
                    let idx = self.ascii_lo as usize + (target - first) as usize;
                    let entry =
                        read_entry_mode(self.bytes, table_offset + idx * entry_size, compact);
                    return Some(self.glyph_at(entry));
                }
                return None;
            }
            if self.ascii_hi > self.ascii_lo {
                let mut lo = self.ascii_lo as usize;
                let mut hi = self.ascii_hi as usize;
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    let off = table_offset + mid * entry_size;
                    let code = peek_unicode(self.bytes, off);
                    if code < target {
                        lo = mid + 1;
                    } else if code > target {
                        hi = mid;
                    } else {
                        let entry = read_entry_mode(self.bytes, off, compact);
                        return Some(self.glyph_at(entry));
                    }
                }
                return None;
            }
        }
        let mut lo = 0usize;
        let mut hi = self.count as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let off = table_offset + mid * entry_size;
            let code = peek_unicode(self.bytes, off);
            if code < target {
                lo = mid + 1;
            } else if code > target {
                hi = mid;
            } else {
                let entry = read_entry_mode(self.bytes, off, compact);
                return Some(self.glyph_at(entry));
            }
        }
        None
    }

    pub(crate) fn fallback_glyph(&self) -> GlyphBitmap<'a> {
        let compact = self.compact_index();
        let entry_size = if compact {
            LF4_COMPACT_ENTRY_SIZE
        } else {
            LF4_ENTRY_SIZE
        };
        let fallback_index = match self.bytes.get(14..16) {
            Some(&[b0, b1]) => u16::from_le_bytes([b0, b1]) as usize,
            _ => 0,
        };
        let fb_entry = read_entry_mode(
            self.bytes,
            (self.table_offset as usize) + fallback_index * entry_size,
            compact,
        );
        self.glyph_at(fb_entry)
    }

    pub fn lookup_or_fallback(&self, ch: char) -> (GlyphBitmap<'a>, bool) {
        if let Some(g) = self.lookup_exact(ch) {
            return (g, false);
        }
        (self.fallback_glyph(), true)
    }

    fn glyph_at(&self, metrics: GlyphMetrics) -> GlyphBitmap<'a> {
        let row_bytes = self.format().row_bytes(metrics.width);
        let raw_len = row_bytes * (metrics.height as usize) * usize::from(self.subpixel_phases());
        if raw_len == 0 {
            return GlyphBitmap { metrics, data: &[] };
        }
        let start = (self.bitmap_offset as usize) + (metrics.data_offset as usize);
        match self.encoding() {
            GlyphEncoding::Raw => GlyphBitmap {
                metrics,
                data: &self.bytes[start..start + raw_len],
            },
            GlyphEncoding::Rle => {
                let rle_len = usize::from(read_u16(self.bytes, start));
                GlyphBitmap {
                    metrics,
                    data: &self.bytes[start + 2..start + 2 + rle_len],
                }
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FontSet<'a> {
    slots: [Lf4Font<'a>; 4],
    fallback: Option<Lf4Font<'a>>,
}

impl<'a> FontSet<'a> {
    #[inline]
    pub const fn new(body16: Lf4Font<'a>, small12: Lf4Font<'a>) -> Self {
        Self {
            slots: [body16, small12, body16, body16],
            fallback: None,
        }
    }

    #[inline]
    pub const fn single(font: Lf4Font<'a>) -> Self {
        Self {
            slots: [font; 4],
            fallback: None,
        }
    }

    #[inline]
    pub const fn with_slot(mut self, id: FontId, font: Lf4Font<'a>) -> Self {
        self.slots[id as usize] = font;
        self
    }

    #[inline]
    pub const fn with_title(self, font: Lf4Font<'a>) -> Self {
        self.with_slot(FontId::Title20, font)
    }

    #[inline]
    pub const fn with_display(self, font: Lf4Font<'a>) -> Self {
        self.with_slot(FontId::Display28, font)
    }

    #[inline]
    pub const fn with_fallback(mut self, font: Lf4Font<'a>) -> Self {
        self.fallback = Some(font);
        self
    }

    #[inline]
    pub const fn fallback(&self) -> Option<&Lf4Font<'a>> {
        self.fallback.as_ref()
    }

    #[inline]
    pub const fn get(&self, id: FontId) -> &Lf4Font<'a> {
        &self.slots[id as usize]
    }

    #[inline]
    pub const fn body16(&self) -> &Lf4Font<'a> {
        &self.slots[FontId::Body16 as usize]
    }

    #[inline]
    pub const fn small12(&self) -> &Lf4Font<'a> {
        &self.slots[FontId::Small12 as usize]
    }
}

/// Static format, compression, phase, and advance-precision metadata for a resolved font face.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FaceInfo {
    pub format: GlyphFormat,
    pub encoding: GlyphEncoding,
    pub phases: u8,
    pub advance_q2: bool,
}

/// Glyph metrics paired with the resolving face's encoding/format descriptor.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ResolvedGlyph {
    pub metrics: GlyphMetrics,
    pub info: FaceInfo,
    pub from_fallback: bool,
}

/// Unified read/stream font face abstraction spanning XIP slice fonts and non-XIP storage fonts.
pub trait FontFace {
    type Error;

    fn ascent(&self) -> i16;
    fn descent(&self) -> u16;
    fn line_gap(&self) -> u16;
    fn line_height(&self) -> u32;
    fn face_info(&self) -> FaceInfo;
    fn has_kerning(&self) -> bool;
    fn kerning(&self, left: char, right: char) -> Result<i16, Self::Error>;
    fn lookup_exact_metrics(&self, ch: char) -> Result<Option<GlyphMetrics>, Self::Error>;
    fn fallback_metrics(&self) -> Result<GlyphMetrics, Self::Error>;
    fn with_glyph_bytes<R>(
        &self,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error>;

    fn cache_stats(&self) -> FontCacheStats {
        FontCacheStats::default()
    }

    fn clear_caches(&self) {}
}

impl FontFace for Lf4Font<'_> {
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn ascent(&self) -> i16 {
        Lf4Font::ascent(self)
    }

    #[inline(always)]
    fn descent(&self) -> u16 {
        Lf4Font::descent(self)
    }

    #[inline(always)]
    fn line_gap(&self) -> u16 {
        Lf4Font::line_gap(self)
    }

    #[inline(always)]
    fn line_height(&self) -> u32 {
        Lf4Font::line_height(self)
    }

    #[inline(always)]
    fn face_info(&self) -> FaceInfo {
        FaceInfo {
            format: self.format(),
            encoding: self.encoding(),
            phases: self.subpixel_phases(),
            advance_q2: self.advance_is_q2(),
        }
    }

    #[inline(always)]
    fn has_kerning(&self) -> bool {
        Lf4Font::has_kerning(self)
    }

    #[inline(always)]
    fn kerning(&self, left: char, right: char) -> Result<i16, Self::Error> {
        Ok(Lf4Font::kerning(self, left, right))
    }

    #[inline(always)]
    fn lookup_exact_metrics(&self, ch: char) -> Result<Option<GlyphMetrics>, Self::Error> {
        Ok(self.lookup_exact(ch).map(|g| g.metrics))
    }

    #[inline(always)]
    fn fallback_metrics(&self) -> Result<GlyphMetrics, Self::Error> {
        let fb = read_u16(self.bytes, 14) as usize;
        let entry_size = if self.compact_index() {
            LF4_COMPACT_ENTRY_SIZE
        } else {
            LF4_ENTRY_SIZE
        };
        Ok(read_entry_mode(
            self.bytes,
            (self.table_offset as usize) + fb * entry_size,
            self.compact_index(),
        ))
    }

    #[inline(always)]
    fn with_glyph_bytes<R>(
        &self,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error> {
        let bmp = self.glyph_at(metrics);
        Ok(f(bmp.data, self.encoding()))
    }
}

/// Unified 4-slot font provider (+ optional fallback face) consumed by layout, rasterizer, and `Engine`.
pub trait FontProvider {
    type Error;

    fn line_height(&self, slot: FontId) -> u32;
    fn ascent(&self, slot: FontId) -> i16;
    fn descent(&self, slot: FontId) -> u16;
    fn line_gap(&self, slot: FontId) -> u16;
    fn has_kerning(&self, slot: FontId) -> bool;
    fn kerning(&self, slot: FontId, left: char, right: char) -> Result<i16, Self::Error>;
    fn resolve_char(&self, slot: FontId, ch: char) -> Result<ResolvedGlyph, Self::Error>;
    fn with_glyph_bytes<R>(
        &self,
        slot: FontId,
        from_fallback: bool,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error>;

    fn begin_frame(&self) {}
    fn begin_band(&self, _band: liuli_core::geometry::Rect) {}
    fn cache_stats(&self) -> FontCacheStats {
        FontCacheStats::default()
    }
    fn clear_caches(&self) {}
}

#[inline]
fn resolve_with_faces<Primary: FontFace, Fallback: FontFace<Error = Primary::Error>>(
    primary: &Primary,
    fallback: Option<&Fallback>,
    ch: char,
) -> Result<ResolvedGlyph, Primary::Error> {
    if let Some(metrics) = primary.lookup_exact_metrics(ch)? {
        return Ok(ResolvedGlyph {
            metrics,
            info: primary.face_info(),
            from_fallback: false,
        });
    }
    if let Some(fb) = fallback
        && let Some(metrics) = fb.lookup_exact_metrics(ch)?
    {
        return Ok(ResolvedGlyph {
            metrics,
            info: fb.face_info(),
            from_fallback: true,
        });
    }
    Ok(ResolvedGlyph {
        metrics: primary.fallback_metrics()?,
        info: primary.face_info(),
        from_fallback: false,
    })
}

impl FontProvider for Lf4Font<'_> {
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn line_height(&self, _: FontId) -> u32 {
        Lf4Font::line_height(self)
    }

    #[inline(always)]
    fn ascent(&self, _: FontId) -> i16 {
        Lf4Font::ascent(self)
    }

    #[inline(always)]
    fn descent(&self, _: FontId) -> u16 {
        Lf4Font::descent(self)
    }

    #[inline(always)]
    fn line_gap(&self, _: FontId) -> u16 {
        Lf4Font::line_gap(self)
    }

    #[inline(always)]
    fn has_kerning(&self, _: FontId) -> bool {
        Lf4Font::has_kerning(self)
    }

    #[inline(always)]
    fn kerning(&self, _: FontId, left: char, right: char) -> Result<i16, Self::Error> {
        Ok(Lf4Font::kerning(self, left, right))
    }

    #[inline(always)]
    fn resolve_char(&self, _: FontId, ch: char) -> Result<ResolvedGlyph, Self::Error> {
        resolve_with_faces::<Lf4Font<'_>, Lf4Font<'_>>(self, None, ch)
    }

    #[inline(always)]
    fn with_glyph_bytes<R>(
        &self,
        _: FontId,
        _: bool,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error> {
        <Self as FontFace>::with_glyph_bytes(self, metrics, f)
    }
}

impl FontProvider for FontSet<'_> {
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn line_height(&self, slot: FontId) -> u32 {
        self.get(slot).line_height()
    }

    #[inline(always)]
    fn ascent(&self, slot: FontId) -> i16 {
        self.get(slot).ascent()
    }

    #[inline(always)]
    fn descent(&self, slot: FontId) -> u16 {
        self.get(slot).descent()
    }

    #[inline(always)]
    fn line_gap(&self, slot: FontId) -> u16 {
        self.get(slot).line_gap()
    }

    #[inline(always)]
    fn has_kerning(&self, slot: FontId) -> bool {
        self.get(slot).has_kerning()
    }

    #[inline(always)]
    fn kerning(&self, slot: FontId, left: char, right: char) -> Result<i16, Self::Error> {
        Ok(self.get(slot).kerning(left, right))
    }

    #[inline(always)]
    fn resolve_char(&self, slot: FontId, ch: char) -> Result<ResolvedGlyph, Self::Error> {
        resolve_with_faces(self.get(slot), self.fallback(), ch)
    }

    #[inline(always)]
    fn with_glyph_bytes<R>(
        &self,
        slot: FontId,
        from_fallback: bool,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error> {
        let face = if from_fallback {
            self.fallback().unwrap_or_else(|| self.get(slot))
        } else {
            self.get(slot)
        };
        FontFace::with_glyph_bytes(face, metrics, f)
    }
}

impl<
    R: crate::storage::ReadAt,
    const ANCHORS: usize,
    const METRICS: usize,
    const GLYPHS: usize,
    const PAGES: usize,
> FontProvider for crate::cache::StreamingFont<'_, R, ANCHORS, METRICS, GLYPHS, PAGES>
{
    type Error = crate::storage::ReadError<R::Error>;

    fn line_height(&self, _: FontId) -> u32 {
        FontFace::line_height(self)
    }

    fn ascent(&self, _: FontId) -> i16 {
        FontFace::ascent(self)
    }

    fn descent(&self, _: FontId) -> u16 {
        FontFace::descent(self)
    }

    fn line_gap(&self, _: FontId) -> u16 {
        FontFace::line_gap(self)
    }

    fn has_kerning(&self, _: FontId) -> bool {
        FontFace::has_kerning(self)
    }

    fn kerning(&self, _: FontId, left: char, right: char) -> Result<i16, Self::Error> {
        FontFace::kerning(self, left, right)
    }

    fn resolve_char(&self, _: FontId, ch: char) -> Result<ResolvedGlyph, Self::Error> {
        resolve_with_faces::<Self, Self>(self, None, ch)
    }

    fn with_glyph_bytes<Res>(
        &self,
        _: FontId,
        _: bool,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> Res,
    ) -> Result<Res, Self::Error> {
        FontFace::with_glyph_bytes(self, metrics, f)
    }

    fn cache_stats(&self) -> FontCacheStats {
        self.stats()
    }

    fn clear_caches(&self) {
        FontFace::clear_caches(self);
    }
}

/// Per-slot backing source in [`HybridFontSet`]: either zero-copy internal XIP or external streaming storage.
pub enum FontSlot<'a, S> {
    Xip(Lf4Font<'a>),
    Stream(&'a S),
}

impl<'a, S> Copy for FontSlot<'a, S> {}
impl<'a, S> Clone for FontSlot<'a, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, S: FontFace> FontFace for FontSlot<'a, S> {
    type Error = S::Error;

    fn ascent(&self) -> i16 {
        match self {
            Self::Xip(f) => f.ascent(),
            Self::Stream(s) => s.ascent(),
        }
    }

    fn descent(&self) -> u16 {
        match self {
            Self::Xip(f) => f.descent(),
            Self::Stream(s) => s.descent(),
        }
    }

    fn line_gap(&self) -> u16 {
        match self {
            Self::Xip(f) => f.line_gap(),
            Self::Stream(s) => s.line_gap(),
        }
    }

    fn line_height(&self) -> u32 {
        match self {
            Self::Xip(f) => f.line_height(),
            Self::Stream(s) => s.line_height(),
        }
    }

    fn face_info(&self) -> FaceInfo {
        match self {
            Self::Xip(f) => f.face_info(),
            Self::Stream(s) => s.face_info(),
        }
    }

    fn has_kerning(&self) -> bool {
        match self {
            Self::Xip(f) => f.has_kerning(),
            Self::Stream(s) => s.has_kerning(),
        }
    }

    fn kerning(&self, left: char, right: char) -> Result<i16, Self::Error> {
        match self {
            Self::Xip(f) => Ok(f.kerning(left, right)),
            Self::Stream(s) => s.kerning(left, right),
        }
    }

    fn lookup_exact_metrics(&self, ch: char) -> Result<Option<GlyphMetrics>, Self::Error> {
        match self {
            Self::Xip(f) => Ok(f.lookup_exact(ch).map(|g| g.metrics)),
            Self::Stream(s) => s.lookup_exact_metrics(ch),
        }
    }

    fn fallback_metrics(&self) -> Result<GlyphMetrics, Self::Error> {
        match self {
            Self::Xip(f) => {
                let Ok(m) = <Lf4Font<'_> as FontFace>::fallback_metrics(f);
                Ok(m)
            }
            Self::Stream(s) => s.fallback_metrics(),
        }
    }

    fn with_glyph_bytes<R>(
        &self,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error> {
        match self {
            Self::Xip(lf4) => {
                let bmp = lf4.glyph_at(metrics);
                Ok(f(bmp.data, lf4.encoding()))
            }
            Self::Stream(s) => s.with_glyph_bytes(metrics, f),
        }
    }

    fn cache_stats(&self) -> FontCacheStats {
        match self {
            Self::Xip(_) => FontCacheStats::default(),
            Self::Stream(s) => s.cache_stats(),
        }
    }

    fn clear_caches(&self) {
        if let Self::Stream(s) = self {
            s.clear_caches();
        }
    }
}

/// Hot/cold 4-slot font provider mixing fast XIP slice fonts with streaming non-XIP faces.
pub struct HybridFontSet<'a, S> {
    slots: [FontSlot<'a, S>; 4],
    fallback: Option<FontSlot<'a, S>>,
}

impl<'a, S> Copy for HybridFontSet<'a, S> {}
impl<'a, S> Clone for HybridFontSet<'a, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, S> HybridFontSet<'a, S> {
    pub const fn from_xip(set: FontSet<'a>) -> Self {
        Self {
            slots: [
                FontSlot::Xip(set.slots[0]),
                FontSlot::Xip(set.slots[1]),
                FontSlot::Xip(set.slots[2]),
                FontSlot::Xip(set.slots[3]),
            ],
            fallback: match set.fallback {
                Some(f) => Some(FontSlot::Xip(f)),
                None => None,
            },
        }
    }

    pub const fn streaming(stream: &'a S) -> Self {
        Self {
            slots: [FontSlot::Stream(stream); 4],
            fallback: None,
        }
    }

    pub const fn with_slot(mut self, id: FontId, slot: FontSlot<'a, S>) -> Self {
        self.slots[id as usize] = slot;
        self
    }

    pub const fn with_xip_slot(self, id: FontId, font: Lf4Font<'a>) -> Self {
        self.with_slot(id, FontSlot::Xip(font))
    }

    pub const fn with_stream_slot(self, id: FontId, stream: &'a S) -> Self {
        self.with_slot(id, FontSlot::Stream(stream))
    }

    pub const fn with_stream_fallback(mut self, stream: &'a S) -> Self {
        self.fallback = Some(FontSlot::Stream(stream));
        self
    }

    pub const fn with_xip_fallback(mut self, font: Lf4Font<'a>) -> Self {
        self.fallback = Some(FontSlot::Xip(font));
        self
    }

    pub const fn get(&self, id: FontId) -> FontSlot<'a, S> {
        self.slots[id as usize]
    }
}

impl<S: FontFace> FontProvider for HybridFontSet<'_, S> {
    type Error = S::Error;

    fn line_height(&self, slot: FontId) -> u32 {
        self.slots[slot as usize].line_height()
    }

    fn ascent(&self, slot: FontId) -> i16 {
        self.slots[slot as usize].ascent()
    }

    fn descent(&self, slot: FontId) -> u16 {
        self.slots[slot as usize].descent()
    }

    fn line_gap(&self, slot: FontId) -> u16 {
        self.slots[slot as usize].line_gap()
    }

    fn has_kerning(&self, slot: FontId) -> bool {
        self.slots[slot as usize].has_kerning()
    }

    fn kerning(&self, slot: FontId, left: char, right: char) -> Result<i16, Self::Error> {
        self.slots[slot as usize].kerning(left, right)
    }

    fn resolve_char(&self, slot: FontId, ch: char) -> Result<ResolvedGlyph, Self::Error> {
        resolve_with_faces(&self.slots[slot as usize], self.fallback.as_ref(), ch)
    }

    fn with_glyph_bytes<R>(
        &self,
        slot: FontId,
        from_fallback: bool,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> R,
    ) -> Result<R, Self::Error> {
        let face = if from_fallback {
            self.fallback.as_ref().unwrap_or(&self.slots[slot as usize])
        } else {
            &self.slots[slot as usize]
        };
        FontFace::with_glyph_bytes(face, metrics, f)
    }

    fn cache_stats(&self) -> FontCacheStats {
        let mut seen: [*const S; 5] = [core::ptr::null(); 5];
        let mut count = 0usize;
        let mut total = FontCacheStats::default();
        for slot in self.slots.iter().chain(self.fallback.iter()) {
            if let FontSlot::Stream(s) = *slot {
                let ptr = core::ptr::from_ref(s);
                if !seen[..count].contains(&ptr) {
                    seen[count] = ptr;
                    count += 1;
                    total = total.saturating_add(s.cache_stats());
                }
            }
        }
        total
    }

    fn clear_caches(&self) {
        for slot in &self.slots {
            slot.clear_caches();
        }
        if let Some(fb) = &self.fallback {
            fb.clear_caches();
        }
    }
}

impl<S: FontFace> liuli_core::layout::TextMeasurer for HybridFontSet<'_, S> {
    fn line_height(&self, font: FontId) -> u32 {
        FontProvider::line_height(self, font)
    }

    fn measure_text(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: liuli_core::view::TextAlign,
    ) -> liuli_core::layout::TextMeasureOut {
        self.measure_text_styled(
            font,
            text,
            max_width,
            max_lines,
            ellipsis,
            align,
            liuli_core::view::WrapMode::Char,
            0,
            0,
        )
    }

    fn measure_text_styled(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: liuli_core::view::TextAlign,
        wrap: liuli_core::view::WrapMode,
        letter_spacing: i8,
        line_spacing: i8,
    ) -> liuli_core::layout::TextMeasureOut {
        match crate::layout::visit_layout_glyphs_provider(
            self,
            font,
            text,
            crate::layout::TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
                align,
            },
            crate::layout::ProviderLayoutStyle {
                wrap,
                letter_spacing,
                line_spacing,
                vis_y: None,
            },
            |_| {},
            |_| {},
        ) {
            Ok(out) => out,
            Err(_) => liuli_core::layout::TextMeasureOut {
                size: liuli_core::geometry::Size::new(0, FontProvider::line_height(self, font)),
                ink: liuli_core::geometry::Rect::ZERO,
                lines: 1,
            },
        }
    }
}

const ZERO_METRICS: GlyphMetrics = GlyphMetrics {
    unicode: 0,
    advance: 0,
    bearing_x: 0,
    bearing_y: 0,
    width: 0,
    height: 0,
    data_offset: 0,
};

#[inline]
fn peek_unicode(bytes: &[u8], off: usize) -> u32 {
    let Some(&[u0, u1, u2, u3]) = bytes.get(off..off + 4) else {
        return 0;
    };
    u32::from_le_bytes([u0, u1, u2, u3])
}

#[inline]
pub(crate) fn read_entry(bytes: &[u8], off: usize) -> GlyphMetrics {
    let Some(
        &[
            u0,
            u1,
            u2,
            u3,
            a0,
            a1,
            bx0,
            bx1,
            by0,
            by1,
            w0,
            w1,
            h0,
            h1,
            _,
            _,
            d0,
            d1,
            d2,
            d3,
        ],
    ) = bytes.get(off..off + LF4_ENTRY_SIZE)
    else {
        return ZERO_METRICS;
    };
    GlyphMetrics {
        unicode: u32::from_le_bytes([u0, u1, u2, u3]),
        advance: u16::from_le_bytes([a0, a1]),
        bearing_x: i16::from_le_bytes([bx0, bx1]),
        bearing_y: i16::from_le_bytes([by0, by1]),
        width: u16::from_le_bytes([w0, w1]),
        height: u16::from_le_bytes([h0, h1]),
        data_offset: u32::from_le_bytes([d0, d1, d2, d3]),
    }
}

#[inline]
pub(crate) fn read_compact_entry(bytes: &[u8], off: usize) -> GlyphMetrics {
    let Some(&[u0, u1, u2, u3, a0, bx0, by0, w0, h0, d0, d1, d2]) =
        bytes.get(off..off + LF4_COMPACT_ENTRY_SIZE)
    else {
        return ZERO_METRICS;
    };
    let data_offset = u32::from(d0) | (u32::from(d1) << 8) | (u32::from(d2) << 16);
    GlyphMetrics {
        unicode: u32::from_le_bytes([u0, u1, u2, u3]),
        advance: u16::from(a0),
        bearing_x: i16::from(bx0 as i8),
        bearing_y: i16::from(by0 as i8),
        width: u16::from(w0),
        height: u16::from(h0),
        data_offset,
    }
}

#[inline]
pub(crate) fn read_entry_mode(bytes: &[u8], off: usize, compact: bool) -> GlyphMetrics {
    if compact {
        read_compact_entry(bytes, off)
    } else {
        read_entry(bytes, off)
    }
}

#[inline]
const fn read_u16(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

#[inline]
const fn read_i16(bytes: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([bytes[off], bytes[off + 1]])
}

#[inline]
const fn read_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}
