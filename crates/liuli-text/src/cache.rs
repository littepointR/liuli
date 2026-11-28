//! Zero-allocation configurable multi-tier caches (`MetricsCache`, `GlyphPoolCache`) and
//! `StreamingFont` for non-XIP external storage (SPI NOR Flash, SD card, filesystem).

use core::cell::RefCell;

use crate::coverage::decode_rle_into;
use crate::font::{FaceInfo, FontFace, GlyphEncoding, GlyphFormat, GlyphMetrics};
use crate::layout::{ProviderLayoutStyle, TextLayoutOptions, visit_layout_glyphs_provider};
use crate::storage::{BlockCache, ReadAt, ReadError, SparseIndex, StoredFont};
use liuli_core::layout::{TextMeasureOut, TextMeasurer};
use liuli_core::view::{FontId, TextAlign, WrapMode};

/// Unified cache observability counters across metrics lookup, glyph bitmap pool, and backing I/O.
#[derive(Copy, Clone, Default, Eq, PartialEq, Debug)]
pub struct FontCacheStats {
    pub metrics_hits: u64,
    pub metrics_misses: u64,
    pub bitmap_hits: u64,
    pub bitmap_misses: u64,
    pub bitmap_evictions: u64,
    pub rle_decodes: u64,
    pub io_reads: u64,
    pub io_bytes: u64,
    pub pool_used_bytes: usize,
    pub pool_capacity_bytes: usize,
}

impl FontCacheStats {
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            metrics_hits: self.metrics_hits.saturating_add(other.metrics_hits),
            metrics_misses: self.metrics_misses.saturating_add(other.metrics_misses),
            bitmap_hits: self.bitmap_hits.saturating_add(other.bitmap_hits),
            bitmap_misses: self.bitmap_misses.saturating_add(other.bitmap_misses),
            bitmap_evictions: self.bitmap_evictions.saturating_add(other.bitmap_evictions),
            rle_decodes: self.rle_decodes.saturating_add(other.rle_decodes),
            io_reads: self.io_reads.saturating_add(other.io_reads),
            io_bytes: self.io_bytes.saturating_add(other.io_bytes),
            pool_used_bytes: self.pool_used_bytes.saturating_add(other.pool_used_bytes),
            pool_capacity_bytes: self
                .pool_capacity_bytes
                .saturating_add(other.pool_capacity_bytes),
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

#[derive(Copy, Clone)]
struct MetricsEntry {
    valid: bool,
    found: bool,
    key: u64,
    used: u64,
    metrics: GlyphMetrics,
}

impl MetricsEntry {
    const EMPTY: Self = Self {
        valid: false,
        found: false,
        key: 0,
        used: 0,
        metrics: ZERO_METRICS,
    };
}

// Capability selection also removes each counter field from cache storage.
#[derive(Clone, Copy)]
struct Counter([u64; liuli_core::capacity::METRICS as usize]);
impl Counter {
    const ZERO: Self = Self([0; liuli_core::capacity::METRICS as usize]);
    const fn get(self) -> u64 {
        match self.0.first() {
            Some(n) => *n,
            None => 0,
        }
    }
    fn increment(&mut self) {
        if let Some(n) = self.0.first_mut() {
            *n = n.saturating_add(1);
        }
    }
}

/// L1 fixed-slot LRU cache mapping `(font_slot, unicode)` -> `Option<GlyphMetrics>`.
/// Caches both present and missing glyphs so repeated fallback scans avoid backing storage I/O.
pub struct MetricsCache<const SLOTS: usize> {
    entries: [MetricsEntry; SLOTS],
    clock: u64,
    hits: Counter,
    misses: Counter,
}

impl<const SLOTS: usize> Default for MetricsCache<SLOTS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SLOTS: usize> MetricsCache<SLOTS> {
    pub const fn new() -> Self {
        Self {
            entries: [MetricsEntry::EMPTY; SLOTS],
            clock: 0,
            hits: Counter::ZERO,
            misses: Counter::ZERO,
        }
    }

    #[inline]
    fn key(slot: u8, ch: char) -> u64 {
        (u64::from(slot) << 32) | u64::from(ch as u32)
    }

    pub fn clear(&mut self) {
        self.entries = [MetricsEntry::EMPTY; SLOTS];
        self.clock = 0;
    }

    pub const fn hits(&self) -> u64 {
        self.hits.get()
    }

    pub const fn misses(&self) -> u64 {
        self.misses.get()
    }

    pub fn get(&mut self, slot: u8, ch: char) -> Option<Option<GlyphMetrics>> {
        if SLOTS == 0 {
            self.misses.increment();
            return None;
        }
        let k = Self::key(slot, ch);
        if let Some(entry) = self.entries.iter_mut().find(|e| e.valid && e.key == k) {
            self.clock = self.clock.saturating_add(1);
            entry.used = self.clock;
            self.hits.increment();
            return Some(entry.found.then_some(entry.metrics));
        }
        self.misses.increment();
        None
    }

    pub fn insert(&mut self, slot: u8, ch: char, metrics: Option<GlyphMetrics>) {
        if SLOTS == 0 {
            return;
        }
        let k = Self::key(slot, ch);
        self.clock = self.clock.saturating_add(1);
        if let Some(existing) = self.entries.iter_mut().find(|e| e.valid && e.key == k) {
            existing.found = metrics.is_some();
            existing.metrics = metrics.unwrap_or(ZERO_METRICS);
            existing.used = self.clock;
            return;
        }
        let target = if let Some(pos) = self.entries.iter().position(|e| !e.valid) {
            pos
        } else {
            self.entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.used)
                .map_or(0, |(i, _)| i)
        };
        self.entries[target] = MetricsEntry {
            valid: true,
            found: metrics.is_some(),
            key: k,
            used: self.clock,
            metrics: metrics.unwrap_or(ZERO_METRICS),
        };
    }
}

#[derive(Copy, Clone)]
struct PoolEntry {
    valid: bool,
    decoded: bool,
    key: u64,
    offset: u32,
    len: u32,
    used: u64,
}

impl PoolEntry {
    const EMPTY: Self = Self {
        valid: false,
        decoded: false,
        key: 0,
        offset: 0,
        len: 0,
        used: 0,
    };
}

/// L2 compacting LRU glyph bitmap cache backed by a caller-supplied `&mut [u8]` pool.
/// Works identically on internal SRAM, DTCM, or external PSRAM without heap allocation.
pub struct GlyphPoolCache<'a, const SLOTS: usize> {
    pool: &'a mut [u8],
    entries: [PoolEntry; SLOTS],
    bump: usize,
    clock: u64,
    hits: Counter,
    misses: Counter,
    evictions: Counter,
    rle_decodes: Counter,
}

impl<'a, const SLOTS: usize> GlyphPoolCache<'a, SLOTS> {
    pub fn new(pool: &'a mut [u8]) -> Self {
        Self {
            pool,
            entries: [PoolEntry::EMPTY; SLOTS],
            bump: 0,
            clock: 0,
            hits: Counter::ZERO,
            misses: Counter::ZERO,
            evictions: Counter::ZERO,
            rle_decodes: Counter::ZERO,
        }
    }

    #[inline]
    fn key(slot: u8, ch_u32: u32) -> u64 {
        (u64::from(slot) << 32) | u64::from(ch_u32)
    }

    pub fn clear(&mut self) {
        self.entries = [PoolEntry::EMPTY; SLOTS];
        self.bump = 0;
        self.clock = 0;
    }

    pub fn used_bytes(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.valid)
            .map(|e| e.len as usize)
            .sum()
    }

    pub const fn capacity_bytes(&self) -> usize {
        self.pool.len()
    }

    pub const fn hits(&self) -> u64 {
        self.hits.get()
    }

    pub const fn misses(&self) -> u64 {
        self.misses.get()
    }

    pub const fn evictions(&self) -> u64 {
        self.evictions.get()
    }

    pub const fn rle_decodes(&self) -> u64 {
        self.rle_decodes.get()
    }

    pub fn record_rle_decode(&mut self) {
        self.rle_decodes.increment();
    }

    pub fn get(&mut self, slot: u8, ch_u32: u32) -> Option<(&[u8], bool)> {
        if SLOTS == 0 {
            self.misses.increment();
            return None;
        }
        let k = Self::key(slot, ch_u32);
        let pos = self.entries.iter().position(|e| e.valid && e.key == k);
        if let Some(i) = pos {
            self.clock = self.clock.saturating_add(1);
            self.entries[i].used = self.clock;
            self.hits.increment();
            let off = self.entries[i].offset as usize;
            let len = self.entries[i].len as usize;
            return Some((&self.pool[off..off + len], self.entries[i].decoded));
        }
        self.misses.increment();
        None
    }

    fn evict_lru(&mut self) -> bool {
        let Some((victim, _)) = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.valid)
            .min_by_key(|(_, e)| e.used)
        else {
            return false;
        };
        self.entries[victim] = PoolEntry::EMPTY;
        self.evictions.increment();
        true
    }

    fn compact(&mut self) {
        let mut cursor = 0usize;
        let mut done = [false; SLOTS];
        loop {
            let mut next: Option<(usize, usize)> = None;
            for (i, e) in self.entries.iter().enumerate() {
                if e.valid && !done[i] {
                    let off = e.offset as usize;
                    if next.is_none_or(|(_, best)| off < best) {
                        next = Some((i, off));
                    }
                }
            }
            let Some((i, off)) = next else {
                break;
            };
            done[i] = true;
            let len = self.entries[i].len as usize;
            if off != cursor && len > 0 {
                self.pool.copy_within(off..off + len, cursor);
            }
            self.entries[i].offset = cursor as u32;
            cursor += len;
        }
        self.bump = cursor;
    }

    fn reserve(&mut self, needed: usize) -> Option<(usize, usize)> {
        if SLOTS == 0 || needed > self.pool.len() {
            return None;
        }
        if !self.entries.iter().any(|e| !e.valid) && !self.evict_lru() {
            return None;
        }
        while self.used_bytes().saturating_add(needed) > self.pool.len() {
            if !self.evict_lru() {
                return None;
            }
        }
        if self.bump.saturating_add(needed) > self.pool.len() {
            self.compact();
        }
        if self.bump.saturating_add(needed) > self.pool.len() {
            return None;
        }
        let slot_idx = self.entries.iter().position(|e| !e.valid)?;
        let off = self.bump;
        Some((slot_idx, off))
    }

    pub fn insert(&mut self, slot: u8, ch_u32: u32, data: &[u8], decoded: bool) -> Option<&[u8]> {
        let k = Self::key(slot, ch_u32);
        if let Some(i) = self.entries.iter().position(|e| e.valid && e.key == k) {
            self.entries[i] = PoolEntry::EMPTY;
        }
        let (slot_idx, off) = self.reserve(data.len())?;
        self.pool[off..off + data.len()].copy_from_slice(data);
        self.bump = off + data.len();
        self.clock = self.clock.saturating_add(1);
        self.entries[slot_idx] = PoolEntry {
            valid: true,
            decoded,
            key: k,
            offset: off as u32,
            len: data.len() as u32,
            used: self.clock,
        };
        Some(&self.pool[off..off + data.len()])
    }

    pub fn insert_in_place<E>(
        &mut self,
        slot: u8,
        ch_u32: u32,
        len: usize,
        decoded: bool,
        fill: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<Option<&[u8]>, E> {
        let k = Self::key(slot, ch_u32);
        if let Some(i) = self.entries.iter().position(|e| e.valid && e.key == k) {
            self.entries[i] = PoolEntry::EMPTY;
        }
        let Some((slot_idx, off)) = self.reserve(len) else {
            return Ok(None);
        };
        fill(&mut self.pool[off..off + len])?;
        if decoded {
            self.rle_decodes.increment();
        }
        self.bump = off + len;
        self.clock = self.clock.saturating_add(1);
        self.entries[slot_idx] = PoolEntry {
            valid: true,
            decoded,
            key: k,
            offset: off as u32,
            len: len as u32,
            used: self.clock,
        };
        Ok(Some(&self.pool[off..off + len]))
    }
}

use liuli_core::capacity::KERN_CACHE_SLOTS;

#[derive(Copy, Clone)]
struct KernCacheEntry {
    left: u32,
    right: u32,
    delta: i16,
    occupied: bool,
}

impl KernCacheEntry {
    const EMPTY: Self = Self {
        left: 0,
        right: 0,
        delta: 0,
        occupied: false,
    };
}

struct KernCache {
    slots: [KernCacheEntry; KERN_CACHE_SLOTS],
}

impl KernCache {
    const fn new() -> Self {
        Self {
            slots: [KernCacheEntry::EMPTY; KERN_CACHE_SLOTS],
        }
    }

    #[inline]
    fn index(left: u32, right: u32) -> usize {
        ((left.wrapping_mul(31)) ^ right) as usize % KERN_CACHE_SLOTS.max(1)
    }

    #[inline]
    fn get(&self, left: u32, right: u32) -> Option<i16> {
        let e = *self.slots.get(Self::index(left, right))?;
        if e.occupied && e.left == left && e.right == right {
            Some(e.delta)
        } else {
            None
        }
    }

    #[inline]
    fn insert(&mut self, left: u32, right: u32, delta: i16) {
        let Some(slot) = self.slots.get_mut(Self::index(left, right)) else {
            return;
        };
        *slot = KernCacheEntry {
            left,
            right,
            delta,
            occupied: true,
        };
    }

    #[inline]
    fn clear(&mut self) {
        self.slots = [KernCacheEntry::EMPTY; KERN_CACHE_SLOTS];
    }
}

struct StreamState<'a, R: ReadAt, const METRICS: usize, const GLYPHS: usize, const PAGES: usize> {
    font: StoredFont<BlockCache<'a, R, PAGES>>,
    metrics_cache: MetricsCache<METRICS>,
    bitmap_cache: GlyphPoolCache<'a, GLYPHS>,
    kern_cache: KernCache,
    scratch: &'a mut [u8],
    decode_rle_in_cache: bool,
    last_error: Option<ReadError<R::Error>>,
}

/// Complete non-XIP streaming font backed by L0 `SparseIndex`, L1 `MetricsCache`,
/// L2 `GlyphPoolCache` (with decode-once RLE support), and L3 `BlockCache`.
pub struct StreamingFont<
    'a,
    R: ReadAt,
    const ANCHORS: usize = 64,
    const METRICS: usize = 128,
    const GLYPHS: usize = 64,
    const PAGES: usize = 4,
> {
    ascent: i16,
    descent: u16,
    line_gap: u16,
    format: GlyphFormat,
    encoding: GlyphEncoding,
    subpixel_phases: u8,
    advance_q2: bool,
    has_kerning: bool,
    kern_bounds: Option<((u32, u32), (u32, u32))>,
    glyph_count: u16,
    sparse: SparseIndex<ANCHORS>,
    fallback_metrics: GlyphMetrics,
    state: RefCell<StreamState<'a, R, METRICS, GLYPHS, PAGES>>,
}

impl<
    'a,
    R: ReadAt,
    const ANCHORS: usize,
    const METRICS: usize,
    const GLYPHS: usize,
    const PAGES: usize,
> StreamingFont<'a, R, ANCHORS, METRICS, GLYPHS, PAGES>
{
    pub fn open(
        source: R,
        block_buf: &'a mut [u8],
        glyph_pool: &'a mut [u8],
        scratch: &'a mut [u8],
    ) -> Result<Self, ReadError<R::Error>> {
        let mut font = StoredFont::open(BlockCache::new(source, block_buf))?;
        let sparse = font.build_sparse_index::<ANCHORS>()?;
        let fallback_metrics = font.fallback()?;
        let kern_bounds = font.kern_bounds()?;
        let ascent = font.ascent;
        let descent = font.descent;
        let line_gap = font.gap;
        let format = font.format();
        let encoding = font.encoding();
        let subpixel_phases = font.subpixel_phases();
        let advance_q2 = font.advance_is_q2();
        let has_kerning = font.has_kerning();
        let glyph_count = font.glyph_count();
        Ok(Self {
            ascent,
            descent,
            line_gap,
            format,
            encoding,
            subpixel_phases,
            advance_q2,
            has_kerning,
            kern_bounds,
            glyph_count,
            sparse,
            fallback_metrics,
            state: RefCell::new(StreamState {
                font,
                metrics_cache: MetricsCache::new(),
                bitmap_cache: GlyphPoolCache::new(glyph_pool),
                kern_cache: KernCache::new(),
                scratch,
                decode_rle_in_cache: true,
                last_error: None,
            }),
        })
    }

    pub fn with_rle_decode_cache(self, enabled: bool) -> Self {
        self.state.borrow_mut().decode_rle_in_cache = enabled;
        self
    }

    pub const fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    pub const fn sparse_index(&self) -> &SparseIndex<ANCHORS> {
        &self.sparse
    }

    pub fn stats(&self) -> FontCacheStats {
        let s = self.state.borrow();
        let io = s.font.source().metrics();
        FontCacheStats {
            metrics_hits: s.metrics_cache.hits(),
            metrics_misses: s.metrics_cache.misses(),
            bitmap_hits: s.bitmap_cache.hits(),
            bitmap_misses: s.bitmap_cache.misses(),
            bitmap_evictions: s.bitmap_cache.evictions(),
            rle_decodes: s.bitmap_cache.rle_decodes(),
            io_reads: io.reads,
            io_bytes: io.bytes,
            pool_used_bytes: s.bitmap_cache.used_bytes(),
            pool_capacity_bytes: s.bitmap_cache.capacity_bytes(),
        }
    }

    pub fn reset_io_metrics(&self) {
        self.state.borrow_mut().font.source_mut().reset_metrics();
    }

    pub fn take_last_error(&self) -> Option<ReadError<R::Error>> {
        self.state.borrow_mut().last_error.take()
    }
}

impl<R: ReadAt, const ANCHORS: usize, const METRICS: usize, const GLYPHS: usize, const PAGES: usize>
    FontFace for StreamingFont<'_, R, ANCHORS, METRICS, GLYPHS, PAGES>
{
    type Error = ReadError<R::Error>;

    fn ascent(&self) -> i16 {
        self.ascent
    }

    fn descent(&self) -> u16 {
        self.descent
    }

    fn line_gap(&self) -> u16 {
        self.line_gap
    }

    fn line_height(&self) -> u32 {
        let asc = if self.ascent > 0 {
            self.ascent as u32
        } else {
            0
        };
        asc + u32::from(self.descent) + u32::from(self.line_gap)
    }

    fn face_info(&self) -> FaceInfo {
        FaceInfo {
            format: self.format,
            encoding: self.encoding,
            phases: self.subpixel_phases,
            advance_q2: self.advance_q2,
        }
    }

    fn has_kerning(&self) -> bool {
        self.has_kerning
    }

    fn kerning(&self, left: char, right: char) -> Result<i16, Self::Error> {
        let Some((min_pair, max_pair)) = self.kern_bounds else {
            return Ok(0);
        };
        let target = (left as u32, right as u32);
        if target < min_pair || target > max_pair {
            return Ok(0);
        }
        let mut s = self.state.borrow_mut();
        if let Some(delta) = s.kern_cache.get(target.0, target.1) {
            return Ok(delta);
        }
        let delta = s.font.kerning(left, right)?;
        s.kern_cache.insert(target.0, target.1, delta);
        Ok(delta)
    }

    fn lookup_exact_metrics(&self, ch: char) -> Result<Option<GlyphMetrics>, Self::Error> {
        let mut s = self.state.borrow_mut();
        if let Some(hit) = s.metrics_cache.get(0, ch) {
            return Ok(hit);
        }
        let (lo, hi) = self.sparse.narrow(ch as u32, self.glyph_count);
        let found = s.font.lookup_in_range(ch, lo, hi)?;
        s.metrics_cache.insert(0, ch, found);
        Ok(found)
    }

    fn fallback_metrics(&self) -> Result<GlyphMetrics, Self::Error> {
        Ok(self.fallback_metrics)
    }

    fn with_glyph_bytes<Res>(
        &self,
        metrics: GlyphMetrics,
        f: impl FnOnce(&[u8], GlyphEncoding) -> Res,
    ) -> Result<Res, Self::Error> {
        let mut s = self.state.borrow_mut();
        let StreamState {
            font,
            bitmap_cache,
            scratch,
            decode_rle_in_cache,
            ..
        } = &mut *s;
        if let Some((cached, decoded)) = bitmap_cache.get(0, metrics.unicode) {
            let eff_enc = if decoded {
                GlyphEncoding::Raw
            } else {
                self.encoding
            };
            return Ok(f(cached, eff_enc));
        }
        let raw_len = font.raw_bitmap_len(metrics);
        if raw_len == 0 {
            return Ok(f(&[], GlyphEncoding::Raw));
        }
        let enc_len = font.read_bitmap(metrics, scratch)?.data.len();
        if self.encoding == GlyphEncoding::Rle
            && *decode_rle_in_cache
            && raw_len <= bitmap_cache.capacity_bytes()
        {
            let rle_slice = &scratch[..enc_len];
            let res = bitmap_cache.insert_in_place(0, metrics.unicode, raw_len, true, |dst| {
                decode_rle_into(rle_slice, dst)
                    .map_err(|()| ReadError::Format(crate::font::FontError::TruncatedBitmap))
            })?;
            if let Some(decoded_slice) = res {
                return Ok(f(decoded_slice, GlyphEncoding::Raw));
            }
        }
        let payload = &scratch[..enc_len];
        if let Some(cached) = bitmap_cache.insert(0, metrics.unicode, payload, false) {
            return Ok(f(cached, self.encoding));
        }
        Ok(f(payload, self.encoding))
    }

    fn cache_stats(&self) -> FontCacheStats {
        self.stats()
    }

    fn clear_caches(&self) {
        let mut s = self.state.borrow_mut();
        s.metrics_cache.clear();
        s.bitmap_cache.clear();
        s.kern_cache.clear();
        s.font.source_mut().clear();
    }
}

impl<R: ReadAt, const ANCHORS: usize, const METRICS: usize, const GLYPHS: usize, const PAGES: usize>
    TextMeasurer for StreamingFont<'_, R, ANCHORS, METRICS, GLYPHS, PAGES>
{
    fn line_height(&self, _font: FontId) -> u32 {
        FontFace::line_height(self)
    }

    fn measure_text(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
    ) -> TextMeasureOut {
        self.measure_text_styled(
            font,
            text,
            max_width,
            max_lines,
            ellipsis,
            align,
            WrapMode::Char,
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
        align: TextAlign,
        wrap: WrapMode,
        letter_spacing: i8,
        line_spacing: i8,
    ) -> TextMeasureOut {
        match visit_layout_glyphs_provider(
            self,
            font,
            text,
            TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
                align,
            },
            ProviderLayoutStyle {
                wrap,
                letter_spacing,
                line_spacing,
                vis_y: None,
            },
            |_| {},
            |_| {},
        ) {
            Ok(out) => out,
            Err(err) => {
                self.state.borrow_mut().last_error = Some(err);
                TextMeasureOut {
                    size: liuli_core::geometry::Size::new(0, FontFace::line_height(self)),
                    ink: liuli_core::geometry::Rect::ZERO,
                    lines: 1,
                }
            }
        }
    }
}
