//! Seekable LF4 (v1 & v2) assets. Font construction validates the index without loading
//! bitmaps. The caller owns both the byte cache and glyph scratch, including memory placement.
use crate::font::{
    FontError, GlyphBitmap, GlyphEncoding, GlyphFormat, GlyphMetrics, LF4_COMPACT_ENTRY_SIZE,
    LF4_ENTRY_SIZE, LF4_HEADER_SIZE, LF4_KERN_ENTRY_SIZE, LF4_MAGIC, LF4_V2_HEADER_SIZE,
    LF4_VERSION, LF4_VERSION_V2, read_entry_mode,
};

pub type KernBounds = ((u32, u32), (u32, u32));

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
/// Configurable page-slot LRU cache (`PAGES` slots) over a backing [`ReadAt`] storage medium.
/// Set `PAGES = 0` or pass an empty buffer to bypass paging (e.g. byte-addressable SPI NOR Flash).
pub struct BlockCache<'a, R, const PAGES: usize = 8> {
    source: R,
    buf: &'a mut [u8],
    pages: [Page; PAGES],
    clock: u64,
    metrics: IoMetrics,
}

/// Default 8-page LRU block cache alias preserved for backward compatibility.
pub type Cached<'a, R> = BlockCache<'a, R, 8>;

impl<'a, R: ReadAt, const PAGES: usize> BlockCache<'a, R, PAGES> {
    pub fn new(source: R, buf: &'a mut [u8]) -> Self {
        Self {
            source,
            buf,
            pages: [Page::default(); PAGES],
            clock: 0,
            metrics: IoMetrics::default(),
        }
    }
    pub fn metrics(&self) -> IoMetrics {
        self.metrics
    }
    pub fn reset_metrics(&mut self) {
        self.metrics = IoMetrics::default();
    }
    pub fn clear(&mut self) {
        self.pages = [Page::default(); PAGES];
        self.clock = 0;
    }
    pub fn page_size(&self) -> usize {
        if PAGES == 0 || self.buf.is_empty() {
            0
        } else {
            let slots = self.buf.len().min(PAGES);
            self.buf.len() / slots
        }
    }
}
impl<R: ReadAt, const PAGES: usize> ReadAt for BlockCache<'_, R, PAGES> {
    type Error = R::Error;
    fn byte_len(&self) -> usize {
        self.source.byte_len()
    }
    fn read_at(&mut self, pos: usize, out: &mut [u8]) -> Result<usize, Self::Error> {
        if out.is_empty() || pos >= self.byte_len() {
            return Ok(0);
        }
        if PAGES == 0 || self.buf.is_empty() {
            self.metrics.reads += 1;
            let n = self.source.read_at(pos, out)?;
            self.metrics.bytes += n as u64;
            return Ok(n);
        }
        let slots = self.buf.len().min(PAGES);
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

/// L0 sparse codepoint directory sampled from an LF4 index table to collapse multi-step
/// storage binary search down to a narrow local bracket.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SparseIndex<const ANCHORS: usize> {
    anchors: [(u32, u16); ANCHORS],
    len: u16,
    stride: u16,
}

impl<const ANCHORS: usize> SparseIndex<ANCHORS> {
    pub const fn empty() -> Self {
        Self {
            anchors: [(0, 0); ANCHORS],
            len: 0,
            stride: 1,
        }
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn stride(&self) -> u16 {
        self.stride
    }

    /// Narrows the binary search range `[0, total_count)` around `target`.
    pub fn narrow(&self, target: u32, total_count: u16) -> (u16, u16) {
        let n = usize::from(self.len);
        if n == 0 || total_count == 0 {
            return (0, total_count);
        }
        if target < self.anchors[0].0 {
            return (0, self.anchors[0].1);
        }
        let pos = self.anchors[..n].partition_point(|&(u, _)| u <= target);
        if pos == 0 {
            return (0, self.anchors[0].1);
        }
        let (prev_u, prev_idx) = self.anchors[pos - 1];
        if prev_u == target {
            return (prev_idx, prev_idx.saturating_add(1).min(total_count));
        }
        let hi = if pos < n {
            self.anchors[pos].1
        } else {
            total_count
        };
        (prev_idx.saturating_add(1).min(hi), hi)
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
    format: GlyphFormat,
    encoding: GlyphEncoding,
    compact_index: bool,
    subpixel_phases: u8,
    advance_q2: bool,
    kern_offset: usize,
    kern_count: usize,
}
impl<R: ReadAt> StoredFont<R> {
    pub fn open(mut source: R) -> Result<Self, ReadError<R::Error>> {
        use ReadError::Format;
        let mut h = [0; LF4_V2_HEADER_SIZE];
        exact(&mut source, 0, &mut h[..LF4_HEADER_SIZE])?;
        let word = |buf: &[u8], i| u16::from_le_bytes([buf[i], buf[i + 1]]);
        if &h[..4] != LF4_MAGIC {
            return Err(Format(FontError::InvalidMagic));
        }
        let version = word(&h, 4);
        if version != LF4_VERSION && version != LF4_VERSION_V2 {
            return Err(Format(FontError::UnsupportedVersion));
        }
        if version == LF4_VERSION_V2 {
            exact(&mut source, LF4_HEADER_SIZE, &mut h[LF4_HEADER_SIZE..])?;
        }
        let count = word(&h, 6);
        let fallback = word(&h, 14);
        if count == 0 {
            return Err(Format(FontError::EmptyTable));
        }
        if fallback >= count {
            return Err(Format(FontError::InvalidFallbackIndex));
        }
        let table = u32::from_le_bytes(h[16..20].try_into().unwrap()) as usize;
        let bitmap = u32::from_le_bytes(h[20..24].try_into().unwrap()) as usize;
        let min_header = if version == LF4_VERSION_V2 {
            LF4_V2_HEADER_SIZE
        } else {
            LF4_HEADER_SIZE
        };
        let (format, encoding, compact_index, subpixel_phases, advance_q2, kern_offset) =
            if version == LF4_VERSION_V2 {
                let format =
                    GlyphFormat::from_raw(h[24]).ok_or(Format(FontError::UnsupportedFormat))?;
                let encoding =
                    GlyphEncoding::from_raw(h[25]).ok_or(Format(FontError::UnsupportedFormat))?;
                let flags = h[26];
                let compact = (flags & 0x01) != 0;
                let phases = match (flags >> 1) & 0x03 {
                    0 => 1,
                    1 => 2,
                    2 => 4,
                    _ => return Err(Format(FontError::UnsupportedFormat)),
                };
                let adv_q2 = (flags & 0x08) != 0;
                let ko = u32::from_le_bytes(h[28..32].try_into().unwrap()) as usize;
                (format, encoding, compact, phases, adv_q2, ko)
            } else {
                (GlyphFormat::A4, GlyphEncoding::Raw, false, 1, false, 0)
            };
        let entry_size = if compact_index {
            LF4_COMPACT_ENTRY_SIZE
        } else {
            LF4_ENTRY_SIZE
        };
        if table < min_header
            || table
                .checked_add(usize::from(count) * entry_size)
                .is_none_or(|end| end > bitmap)
            || bitmap > source.byte_len()
        {
            return Err(Format(FontError::InvalidOffsets));
        }
        let kern_count = if kern_offset != 0 {
            if kern_offset < min_header || kern_offset.saturating_add(4) > table {
                return Err(Format(FontError::InvalidKerningTable));
            }
            let mut kc_buf = [0u8; 4];
            exact(&mut source, kern_offset, &mut kc_buf)?;
            let kc = u32::from_le_bytes(kc_buf) as usize;
            let kend = kern_offset
                .checked_add(4)
                .and_then(|s| {
                    kc.checked_mul(LF4_KERN_ENTRY_SIZE)
                        .and_then(|l| s.checked_add(l))
                })
                .ok_or(Format(FontError::InvalidKerningTable))?;
            if kend > table {
                return Err(Format(FontError::InvalidKerningTable));
            }
            kc
        } else {
            0
        };
        let mut font = Self {
            source,
            count,
            fallback,
            table,
            bitmap,
            ascent: word(&h, 8) as i16,
            descent: word(&h, 10),
            gap: word(&h, 12),
            format,
            encoding,
            compact_index,
            subpixel_phases,
            advance_q2,
            kern_offset,
            kern_count,
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
    pub fn source_mut(&mut self) -> &mut R {
        &mut self.source
    }
    pub fn format(&self) -> GlyphFormat {
        self.format
    }
    pub fn encoding(&self) -> GlyphEncoding {
        self.encoding
    }
    pub fn subpixel_phases(&self) -> u8 {
        self.subpixel_phases
    }
    pub fn advance_is_q2(&self) -> bool {
        self.advance_q2
    }
    pub fn compact_index(&self) -> bool {
        self.compact_index
    }
    pub fn has_kerning(&self) -> bool {
        self.kern_count > 0
    }
    pub fn kern_bounds(&mut self) -> Result<Option<KernBounds>, ReadError<R::Error>> {
        if self.kern_count == 0 {
            return Ok(None);
        }
        let mut buf = [0u8; LF4_KERN_ENTRY_SIZE];
        exact(&mut self.source, self.kern_offset + 4, &mut buf)?;
        let min_pair = (
            u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            u32::from_le_bytes(buf[4..8].try_into().unwrap()),
        );
        let max_pair = if self.kern_count == 1 {
            min_pair
        } else {
            exact(
                &mut self.source,
                self.kern_offset + 4 + (self.kern_count - 1) * LF4_KERN_ENTRY_SIZE,
                &mut buf,
            )?;
            (
                u32::from_le_bytes(buf[0..4].try_into().unwrap()),
                u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            )
        };
        Ok(Some((min_pair, max_pair)))
    }
    pub fn line_height(&self) -> u32 {
        let asc = if self.ascent > 0 {
            self.ascent as u32
        } else {
            0
        };
        asc + u32::from(self.descent) + u32::from(self.gap)
    }
    pub fn raw_bitmap_len(&self, g: GlyphMetrics) -> usize {
        self.format.row_bytes(g.width) * usize::from(g.height) * usize::from(self.subpixel_phases)
    }
    pub fn build_sparse_index<const ANCHORS: usize>(
        &mut self,
    ) -> Result<SparseIndex<ANCHORS>, ReadError<R::Error>> {
        if ANCHORS == 0 || self.count == 0 {
            return Ok(SparseIndex::empty());
        }
        let stride = self.count.div_ceil(ANCHORS as u16).max(1);
        let mut anchors = [(0u32, 0u16); ANCHORS];
        let mut len = 0u16;
        let mut idx = 0u16;
        while idx < self.count && usize::from(len) < ANCHORS {
            let g = self.entry(idx)?;
            anchors[usize::from(len)] = (g.unicode, idx);
            len += 1;
            let Some(next) = idx.checked_add(stride) else {
                break;
            };
            idx = next;
        }
        Ok(SparseIndex {
            anchors,
            len,
            stride,
        })
    }
    pub fn kerning(&mut self, left: char, right: char) -> Result<i16, ReadError<R::Error>> {
        if self.kern_count == 0 {
            return Ok(0);
        }
        let target = (left as u32, right as u32);
        let mut lo = 0usize;
        let mut hi = self.kern_count;
        let mut buf = [0u8; LF4_KERN_ENTRY_SIZE];
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            exact(
                &mut self.source,
                self.kern_offset + 4 + mid * LF4_KERN_ENTRY_SIZE,
                &mut buf,
            )?;
            let k = (
                u32::from_le_bytes(buf[0..4].try_into().unwrap()),
                u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            );
            if k < target {
                lo = mid + 1;
            } else if k > target {
                hi = mid;
            } else {
                return Ok(i16::from_le_bytes([buf[8], buf[9]]));
            }
        }
        Ok(0)
    }
    pub fn lookup_in_range(
        &mut self,
        ch: char,
        mut lo: u16,
        mut hi: u16,
    ) -> Result<Option<GlyphMetrics>, ReadError<R::Error>> {
        let hi_clamped = hi.min(self.count);
        hi = hi_clamped;
        let target = ch as u32;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let g = self.entry(mid)?;
            match g.unicode.cmp(&target) {
                core::cmp::Ordering::Equal => return Ok(Some(g)),
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
            }
        }
        Ok(None)
    }
    fn entry(&mut self, i: u16) -> Result<GlyphMetrics, ReadError<R::Error>> {
        let entry_size = if self.compact_index {
            LF4_COMPACT_ENTRY_SIZE
        } else {
            LF4_ENTRY_SIZE
        };
        let mut b = [0; LF4_ENTRY_SIZE];
        exact(
            &mut self.source,
            self.table + usize::from(i) * entry_size,
            &mut b[..entry_size],
        )?;
        Ok(read_entry_mode(&b[..entry_size], 0, self.compact_index))
    }
    fn bitmap_range(
        &mut self,
        g: GlyphMetrics,
    ) -> Result<core::ops::Range<usize>, ReadError<R::Error>> {
        let error = || ReadError::Format(FontError::TruncatedBitmap);
        let start = self
            .bitmap
            .checked_add(g.data_offset as usize)
            .ok_or_else(error)?;
        let raw_len = self.format.row_bytes(g.width)
            * usize::from(g.height)
            * usize::from(self.subpixel_phases);
        if raw_len == 0 {
            return Ok(start..start);
        }
        let (payload_start, len) = match self.encoding {
            GlyphEncoding::Raw => (start, raw_len),
            GlyphEncoding::Rle => {
                if start
                    .checked_add(2)
                    .is_none_or(|e| e > self.source.byte_len())
                {
                    return Err(error());
                }
                let mut len_bytes = [0u8; 2];
                exact(&mut self.source, start, &mut len_bytes)?;
                (start + 2, usize::from(u16::from_le_bytes(len_bytes)))
            }
        };
        let end = payload_start
            .checked_add(len)
            .filter(|&end| end <= self.source.byte_len())
            .ok_or_else(error)?;
        Ok(payload_start..end)
    }
    pub fn lookup(&mut self, ch: char) -> Result<Option<GlyphMetrics>, ReadError<R::Error>> {
        self.lookup_in_range(ch, 0, self.count)
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
/// must supply the encoded bitmap slice matching the glyph's format/encoding.
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
