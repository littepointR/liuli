//! Seekable LF4 assets. Font construction validates the index without loading bitmaps.
//! The caller owns both the byte cache and glyph scratch, including their memory placement.
use crate::font::{
    FontError, GlyphBitmap, GlyphMetrics, LF4_ENTRY_SIZE, LF4_HEADER_SIZE, LF4_MAGIC, LF4_VERSION,
    read_entry,
};

pub trait ReadAt {
    type Error;
    fn byte_len(&self) -> usize;
    /// Return bytes actually read. Zero before the declared end is a short read.
    fn read_at(&mut self, offset: usize, out: &mut [u8]) -> Result<usize, Self::Error>;
}
impl ReadAt for &[u8] {
    type Error = core::convert::Infallible;
    fn byte_len(&self) -> usize {
        self.len()
    }
    fn read_at(&mut self, offset: usize, out: &mut [u8]) -> Result<usize, Self::Error> {
        let bytes = self.get(offset..).unwrap_or(&[]);
        let n = out.len().min(bytes.len());
        out[..n].copy_from_slice(&bytes[..n]);
        Ok(n)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError<E> {
    Io(E),
    ShortRead,
    Format(FontError),
    Scratch { needed: usize },
}
fn exact<R: ReadAt>(
    r: &mut R,
    mut pos: usize,
    mut dest: &mut [u8],
) -> Result<(), ReadError<R::Error>> {
    while !dest.is_empty() {
        let n = r.read_at(pos, dest).map_err(ReadError::Io)?;
        if n == 0 || n > dest.len() {
            return Err(ReadError::ShortRead);
        }
        pos = pos.checked_add(n).ok_or(ReadError::ShortRead)?;
        dest = &mut dest[n..];
    }
    Ok(())
}

#[derive(Default, Copy, Clone, Debug)]
pub struct IoMetrics {
    pub reads: u64,
    pub bytes: u64,
    pub cache_hits: u64,
}
#[derive(Clone, Copy, Default)]
struct Page {
    start: usize,
    valid: usize,
    used: u64,
}
/// Eight reusable pages share the caller's byte budget. Backing reads and hits are counted separately.
pub struct Cached<'a, R> {
    source: R,
    buf: &'a mut [u8],
    pages: [Page; 8],
    clock: u64,
    metrics: IoMetrics,
}
impl<'a, R: ReadAt> Cached<'a, R> {
    pub fn new(source: R, buf: &'a mut [u8]) -> Self {
        Self {
            source,
            buf,
            pages: [Page::default(); 8],
            clock: 0,
            metrics: IoMetrics::default(),
        }
    }
    pub fn metrics(&self) -> IoMetrics {
        self.metrics
    }
}
impl<R: ReadAt> ReadAt for Cached<'_, R> {
    type Error = R::Error;
    fn byte_len(&self) -> usize {
        self.source.byte_len()
    }
    fn read_at(&mut self, pos: usize, out: &mut [u8]) -> Result<usize, Self::Error> {
        if out.is_empty() || pos >= self.byte_len() {
            return Ok(0);
        }
        if self.buf.is_empty() {
            self.metrics.reads += 1;
            let n = self.source.read_at(pos, out)?;
            self.metrics.bytes += n as u64;
            return Ok(n);
        }
        let slots = self.buf.len().min(self.pages.len());
        let page_size = self.buf.len() / slots;
        let start = pos / page_size * page_size;
        let offset = pos - start;
        self.clock = self.clock.saturating_add(1);
        let slot = if let Some(slot) = self.pages[..slots]
            .iter()
            .position(|p| p.start == start && offset < p.valid)
        {
            self.metrics.cache_hits += 1;
            slot
        } else {
            let slot = self.pages[..slots]
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.used)
                .unwrap()
                .0;
            self.pages[slot] = Page::default();
            let len = page_size.min(self.byte_len() - start);
            let dest = &mut self.buf[slot * page_size..slot * page_size + len];
            let mut filled = 0;
            while filled < len {
                self.metrics.reads += 1;
                let count = self.source.read_at(start + filled, &mut dest[filled..])?;
                if count == 0 || count > len - filled {
                    break;
                }
                self.metrics.bytes += count as u64;
                filled += count;
            }
            if filled <= offset {
                return Ok(0);
            }
            self.pages[slot] = Page {
                start,
                valid: filled,
                used: self.clock,
            };
            slot
        };
        self.pages[slot].used = self.clock;
        let base = slot * page_size;
        let bytes = &self.buf[base + offset..base + self.pages[slot].valid];
        let n = out.len().min(bytes.len());
        out[..n].copy_from_slice(&bytes[..n]);
        Ok(n)
    }
}

pub struct StoredFont<R> {
    source: R,
    count: u16,
    fallback: u16,
    table: usize,
    bitmap: usize,
    pub ascent: i16,
    pub descent: u16,
    pub gap: u16,
}
impl<R: ReadAt> StoredFont<R> {
    pub fn open(mut source: R) -> Result<Self, ReadError<R::Error>> {
        use ReadError::Format;
        let mut h = [0; LF4_HEADER_SIZE];
        exact(&mut source, 0, &mut h)?;
        let word = |i| u16::from_le_bytes([h[i], h[i + 1]]);
        if &h[..4] != LF4_MAGIC {
            return Err(Format(FontError::InvalidMagic));
        }
        if word(4) != LF4_VERSION {
            return Err(Format(FontError::UnsupportedVersion));
        }
        let count = word(6);
        let fallback = word(14);
        if count == 0 {
            return Err(Format(FontError::EmptyTable));
        }
        if fallback >= count {
            return Err(Format(FontError::InvalidFallbackIndex));
        }
        let table = u32::from_le_bytes(h[16..20].try_into().unwrap()) as usize;
        let bitmap = u32::from_le_bytes(h[20..24].try_into().unwrap()) as usize;
        if table < LF4_HEADER_SIZE
            || table
                .checked_add(usize::from(count) * LF4_ENTRY_SIZE)
                .is_none_or(|end| end > bitmap)
            || bitmap > source.byte_len()
        {
            return Err(Format(FontError::InvalidOffsets));
        }
        let mut font = Self {
            source,
            count,
            fallback,
            table,
            bitmap,
            ascent: word(8) as i16,
            descent: word(10),
            gap: word(12),
        };
        let mut previous = None;
        for i in 0..count {
            let g = font.entry(i)?;
            if char::from_u32(g.unicode).is_none() {
                return Err(Format(FontError::InvalidScalar));
            }
            if previous.is_some_and(|u| u >= g.unicode) {
                return Err(Format(FontError::UnsortedIndex));
            }
            font.bitmap_range(g)?;
            previous = Some(g.unicode);
        }
        Ok(font)
    }
    pub fn glyph_count(&self) -> u16 {
        self.count
    }
    pub fn source(&self) -> &R {
        &self.source
    }
    fn entry(&mut self, i: u16) -> Result<GlyphMetrics, ReadError<R::Error>> {
        let mut b = [0; LF4_ENTRY_SIZE];
        exact(
            &mut self.source,
            self.table + usize::from(i) * LF4_ENTRY_SIZE,
            &mut b,
        )?;
        Ok(read_entry(&b, 0))
    }
    fn bitmap_range(
        &self,
        g: GlyphMetrics,
    ) -> Result<core::ops::Range<usize>, ReadError<R::Error>> {
        let error = || ReadError::Format(FontError::TruncatedBitmap);
        let start = self
            .bitmap
            .checked_add(g.data_offset as usize)
            .ok_or_else(error)?;
        let len = usize::from(g.width).div_ceil(2) * usize::from(g.height);
        let end = start
            .checked_add(len)
            .filter(|&end| end <= self.source.byte_len())
            .ok_or_else(error)?;
        Ok(start..end)
    }
    pub fn lookup(&mut self, ch: char) -> Result<Option<GlyphMetrics>, ReadError<R::Error>> {
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let g = self.entry(mid)?;
            match g.unicode.cmp(&(ch as u32)) {
                core::cmp::Ordering::Equal => return Ok(Some(g)),
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
            }
        }
        Ok(None)
    }
    pub fn fallback(&mut self) -> Result<GlyphMetrics, ReadError<R::Error>> {
        self.entry(self.fallback)
    }
    pub fn read_bitmap<'b>(
        &mut self,
        g: GlyphMetrics,
        out: &'b mut [u8],
    ) -> Result<GlyphBitmap<'b>, ReadError<R::Error>> {
        let range = self.bitmap_range(g)?;
        let len = range.len();
        let data = out
            .get_mut(..len)
            .ok_or(ReadError::Scratch { needed: len })?;
        exact(&mut self.source, range.start, data)?;
        Ok(GlyphBitmap { metrics: g, data })
    }
}

/// Bitmap source called only for glyphs intersecting a raster band. Each successful read
/// must supply the LF4 row stride times the height for the supplied metrics.
pub trait GlyphPixels {
    type Error;
    fn begin_band(&mut self, _band: liuli_core::geometry::Rect) {}
    fn pixels<'a>(
        &'a mut self,
        slot: liuli_core::view::FontId,
        glyph: GlyphBitmap<'a>,
    ) -> Result<&'a [u8], Self::Error>;
}
pub struct InlinePixels;
impl GlyphPixels for InlinePixels {
    type Error = core::convert::Infallible;
    fn pixels<'a>(
        &'a mut self,
        _: liuli_core::view::FontId,
        glyph: GlyphBitmap<'a>,
    ) -> Result<&'a [u8], Self::Error> {
        Ok(glyph.data)
    }
}
/// Use when both slots share the same LF4 asset and index.
pub struct StoredPixels<'a, R> {
    pub font: StoredFont<R>,
    pub scratch: &'a mut [u8],
}
impl<R: ReadAt> GlyphPixels for StoredPixels<'_, R> {
    type Error = ReadError<R::Error>;
    fn pixels<'a>(
        &'a mut self,
        _: liuli_core::view::FontId,
        glyph: GlyphBitmap<'a>,
    ) -> Result<&'a [u8], Self::Error> {
        Ok(self.font.read_bitmap(glyph.metrics, self.scratch)?.data)
    }
}
