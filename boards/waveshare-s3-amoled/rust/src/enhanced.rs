use core::alloc::{GlobalAlloc, Layout};
use liuli::prelude::*;
use liuli::text::GlyphBitmap;
extern crate alloc;
unsafe extern "C" {
    fn board_alloc(size: usize, alignment: usize) -> *mut u8;
    fn board_free(ptr: *mut u8);
    fn board_read_font(offset: usize, data: *mut u8, len: usize) -> i32;
    fn board_font_metrics(reads: u64, bytes: u64, hits: u64, cache_size: usize);
}
struct Psram;
// ESP-IDF heap_caps_aligned_alloc honors the supplied alignment and supports
// concurrent callers. All live allocations belong to this allocator.
unsafe impl GlobalAlloc for Psram {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { board_alloc(layout.size().max(1), layout.align().max(4)) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        unsafe { board_free(ptr) }
    }
}
#[global_allocator]
static ALLOCATOR: Psram = Psram;
struct Partition(usize);
impl ReadAt for Partition {
    type Error = i32;
    fn byte_len(&self) -> usize {
        self.0
    }
    fn read_at(&mut self, offset: usize, out: &mut [u8]) -> Result<usize, i32> {
        let n = out.len().min(self.0.saturating_sub(offset));
        let result = unsafe { board_read_font(offset, out.as_mut_ptr(), n) };
        if result == 0 { Ok(n) } else { Err(result) }
    }
}
pub struct Reader<'a> {
    font: StoredFont<BlockCache<'a, Partition, 8>>,
    scratch: [u8; 512],
    #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
    band: u32,
    #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
    seen: alloc::vec::Vec<(u32, u32)>,
    cross_band: u32,
    requests: u32,
    baseline: IoMetrics,
}
impl<'a> Reader<'a> {
    pub fn new(len: usize, cache: &'a mut [u8]) -> Result<Self, ReadError<i32>> {
        let cache_size = cache.len();
        let font = StoredFont::open(BlockCache::new(Partition(len), cache))?;
        let m = font.source().metrics();
        unsafe {
            board_font_metrics(m.reads, m.bytes, m.cache_hits, cache_size);
        }
        #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
        let seen = alloc::vec::Vec::with_capacity(font.glyph_count().into());
        Ok(Self {
            font,
            scratch: [0; 512],
            #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
            band: 0,
            #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
            seen,
            cross_band: 0,
            requests: 0,
            baseline: m,
        })
    }
}
impl super::FrameGlyphs for Reader<'_> {
    fn start(&mut self) {
        self.baseline = self.font.source().metrics();
        self.cross_band = 0;
        self.requests = 0;
        #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
        {
            self.seen.clear();
            self.band = 0;
        }
    }
    fn finish(&self) {
        let m = self.font.source().metrics();
        unsafe {
            board_font_frame(
                self.requests,
                self.cross_band,
                (m.reads - self.baseline.reads) as u32,
                (m.bytes - self.baseline.bytes) as u32,
            );
        }
    }
}
unsafe extern "C" {
    fn board_font_frame(requests: u32, repeated: u32, reads: u32, bytes: u32);
}
impl GlyphPixels for Reader<'_> {
    type Error = ReadError<i32>;
    fn begin_band(&mut self, _: Rect) {
        #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
        {
            self.band += 1;
        }
    }
    fn pixels<'a>(
        &'a mut self,
        _slot: FontId,
        g: GlyphBitmap<'a>,
    ) -> Result<&'a [u8], Self::Error> {
        // Heading glyph offsets belong to the demo's linked asset, body offsets to the partition.
        #[cfg(all(feature = "widget-demo", not(feature = "benchmark")))]
        if _slot == FontId::Title20 {
            return Ok(g.data);
        }
        self.requests += 1;
        #[cfg(any(feature = "benchmark", feature = "effects-benchmark"))]
        {
            let key = g.metrics.unicode;
            if let Some((_, last)) = self.seen.iter_mut().find(|(ch, _)| *ch == key) {
                if *last != self.band {
                    self.cross_band += 1;
                    *last = self.band;
                }
            } else {
                self.seen.push((key, self.band));
            }
        }
        Ok(self.font.read_bitmap(g.metrics, &mut self.scratch)?.data)
    }
}
pub fn cache() -> alloc::vec::Vec<u8> {
    alloc::vec![0;4096]
}
