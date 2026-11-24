use std::cell::Cell;
use std::collections::BTreeMap;

use liuli_core::layout::TextMeasurer;
use liuli_core::view::{FontId, TextAlign, WrapMode};
use liuli_text::cache::{GlyphPoolCache, MetricsCache, StreamingFont};
use liuli_text::font::{
    FontFace, FontProvider, GlyphEncoding, GlyphFormat, HybridFontSet, Lf4Font,
};
use liuli_text::layout::{
    ProviderLayoutStyle, TextLayoutOptions, visit_layout_glyphs_provider,
    visit_layout_glyphs_styled,
};
use liuli_text::storage::ReadAt;

struct CountingFlash<'a> {
    data: &'a [u8],
    reads: Cell<u64>,
    bytes: Cell<u64>,
}

impl<'a> CountingFlash<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            reads: Cell::new(0),
            bytes: Cell::new(0),
        }
    }
}

impl ReadAt for &CountingFlash<'_> {
    type Error = ();

    fn byte_len(&self) -> usize {
        self.data.len()
    }

    fn read_at(&mut self, offset: usize, out: &mut [u8]) -> Result<usize, Self::Error> {
        self.reads.set(self.reads.get() + 1);
        let slice = self.data.get(offset..).unwrap_or(&[]);
        let n = out.len().min(slice.len());
        out[..n].copy_from_slice(&slice[..n]);
        self.bytes.set(self.bytes.get() + n as u64);
        Ok(n)
    }
}

fn encode_rle(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let mut r = 1;
        while r < 129 && i + r < raw.len() && raw[i + r] == raw[i] {
            r += 1;
        }
        if r >= 2 {
            out.push(128 + (r - 2) as u8);
            out.push(raw[i]);
            i += r;
        } else {
            let mut l = 1;
            while l < 128
                && i + l < raw.len()
                && !(i + l + 2 < raw.len()
                    && raw[i + l] == raw[i + l + 1]
                    && raw[i + l] == raw[i + l + 2])
            {
                l += 1;
            }
            out.push((l - 1) as u8);
            out.extend_from_slice(&raw[i..i + l]);
            i += l;
        }
    }
    out
}

fn build_large_v2_font(
    glyphs: &[(char, u16, u16, u16, &[u8])],
    kerning: &[(char, char, i16)],
    rle: bool,
    compact: bool,
) -> Vec<u8> {
    let mut sorted: Vec<_> = glyphs.to_vec();
    sorted.sort_by_key(|g| g.0 as u32);
    let count = sorted.len() as u16;
    let fallback_idx = sorted.iter().position(|g| g.0 == '□').unwrap_or(0) as u16;

    let mut kern_bytes = Vec::new();
    let kern_offset = if kerning.is_empty() {
        0u32
    } else {
        kern_bytes.extend_from_slice(&(kerning.len() as u32).to_le_bytes());
        for &(l, r, d) in kerning {
            kern_bytes.extend_from_slice(&(l as u32).to_le_bytes());
            kern_bytes.extend_from_slice(&(r as u32).to_le_bytes());
            kern_bytes.extend_from_slice(&d.to_le_bytes());
            kern_bytes.extend_from_slice(&0u16.to_le_bytes());
        }
        32
    };
    let table_off = 32 + kern_bytes.len() as u32;
    let entry_size = if compact { 12 } else { 20 };
    let bitmap_off = table_off + (sorted.len() * entry_size) as u32;
    let mut table = Vec::new();
    let mut bitmaps = Vec::new();

    for (ch, adv, w, h, raw) in sorted {
        let off = bitmaps.len() as u32;
        if w > 0 && h > 0 {
            if rle {
                let enc = encode_rle(raw);
                bitmaps.extend_from_slice(&(enc.len() as u16).to_le_bytes());
                bitmaps.extend_from_slice(&enc);
            } else {
                bitmaps.extend_from_slice(raw);
            }
        }
        table.extend_from_slice(&(ch as u32).to_le_bytes());
        if compact {
            table.push(adv as u8);
            table.push(0);
            table.push(10i8 as u8);
            table.push(w as u8);
            table.push(h as u8);
            table.push((off & 0xff) as u8);
            table.push(((off >> 8) & 0xff) as u8);
            table.push(((off >> 16) & 0xff) as u8);
        } else {
            table.extend_from_slice(&adv.to_le_bytes());
            table.extend_from_slice(&0i16.to_le_bytes());
            table.extend_from_slice(&10i16.to_le_bytes());
            table.extend_from_slice(&w.to_le_bytes());
            table.extend_from_slice(&h.to_le_bytes());
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&off.to_le_bytes());
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"LLF4");
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&12i16.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&fallback_idx.to_le_bytes());
    out.extend_from_slice(&table_off.to_le_bytes());
    out.extend_from_slice(&bitmap_off.to_le_bytes());
    out.push(GlyphFormat::A4 as u8);
    out.push(u8::from(rle));
    out.push(u8::from(compact));
    out.push(0);
    out.extend_from_slice(&kern_offset.to_le_bytes());
    out.extend_from_slice(&kern_bytes);
    out.extend_from_slice(&table);
    out.extend_from_slice(&bitmaps);
    out
}

#[test]
fn l2_pool_cache_lru_eviction_and_compaction_preserve_live_bytes() {
    let mut buf = [0u8; 64];
    let mut pool = GlyphPoolCache::<4>::new(&mut buf);

    let a = [0x11u8; 20];
    let b = [0x22u8; 20];
    let c = [0x33u8; 20];
    assert_eq!(pool.insert(0, 'A' as u32, &a, false).unwrap(), &a);
    assert_eq!(pool.insert(0, 'B' as u32, &b, false).unwrap(), &b);
    assert_eq!(pool.insert(0, 'C' as u32, &c, false).unwrap(), &c);
    // Touch 'A' so 'B' becomes LRU victim.
    assert_eq!(pool.get(0, 'A' as u32).unwrap().0, &a);

    let d = [0x44u8; 24]; // 20 (A) + 20 (C) + 24 (D) = 64 B exact fit after compacting out B!
    assert_eq!(pool.insert(0, 'D' as u32, &d, true).unwrap(), &d);
    assert_eq!(pool.evictions(), 1);
    assert!(pool.get(0, 'B' as u32).is_none());
    assert_eq!(pool.get(0, 'A' as u32).unwrap().0, &a);
    assert_eq!(pool.get(0, 'C' as u32).unwrap().0, &c);
    assert_eq!(pool.get(0, 'D' as u32).unwrap(), (&d[..], true));

    // Zero-slot metrics cache stays safe and misses cleanly.
    let mut zero_metrics = MetricsCache::<0>::new();
    zero_metrics.insert(0, 'A', None);
    assert!(zero_metrics.get(0, 'A').is_none());
}

#[test]
fn streaming_font_l0_l1_l2_rle_decode_once_and_sd_vs_nor_modes() {
    // 256-glyph synthetic CJK + ASCII RLE font.
    let mut spec = Vec::new();
    spec.push((' ', 6u16, 0u16, 0u16, Vec::new()));
    spec.push(('□', 10, 8, 8, vec![0xAA; 32]));
    for i in 0u32..254 {
        let ch = if i < 26 {
            char::from_u32('A' as u32 + i).unwrap()
        } else {
            char::from_u32(0x4E00 + i).unwrap()
        };
        let fill = ((i % 15) as u8) + 1;
        let byte = (fill << 4) | fill;
        spec.push((ch, 10, 8, 8, vec![byte; 32]));
    }
    let refs: Vec<(char, u16, u16, u16, &[u8])> = spec
        .iter()
        .map(|(c, a, w, h, v)| (*c, *a, *w, *h, v.as_slice()))
        .collect();
    let bytes = build_large_v2_font(&refs, &[('A', 'B', -2)], true, true);
    let xip = Lf4Font::from_bytes(&bytes).unwrap();

    // Mode 1: Non-XIP SPI NOR Flash (PAGES = 0, no block page buffer needed).
    let nor = CountingFlash::new(&bytes);
    let mut no_block = [];
    let mut nor_pool = [0u8; 1024];
    let mut nor_scratch = [0u8; 256];
    let stream_nor = StreamingFont::<_, 32, 64, 32, 0>::open(
        &nor,
        &mut no_block,
        &mut nor_pool,
        &mut nor_scratch,
    )
    .unwrap();
    stream_nor.reset_io_metrics();

    // First layout + glyph decode triggers reads & 1 RLE decode per unique char.
    let sample = "AB\u{4E20}\u{4E40}\u{4E80}";
    let mut expect = BTreeMap::new();
    visit_layout_glyphs_styled(
        &xip,
        sample,
        TextLayoutOptions {
            max_width: 120,
            max_lines: 2,
            ellipsis: false,
            align: TextAlign::Start,
        },
        Default::default(),
        |pg| {
            expect.insert(
                pg.glyph.metrics.unicode,
                (pg.x, pg.y, pg.glyph.data.to_vec()),
            );
        },
        |_| {},
    );

    let mut actual_raw_decoded = BTreeMap::new();
    let m1 = visit_layout_glyphs_provider(
        &stream_nor,
        FontId::Body16,
        sample,
        TextLayoutOptions {
            max_width: 120,
            max_lines: 2,
            ellipsis: false,
            align: TextAlign::Start,
        },
        ProviderLayoutStyle::default(),
        |pg| {
            FontFace::with_glyph_bytes(&stream_nor, pg.metrics, |data, enc| {
                assert_eq!(enc, GlyphEncoding::Raw);
                actual_raw_decoded.insert(pg.metrics.unicode, (pg.x, pg.y, data.to_vec()));
            })
            .unwrap();
        },
        |_| {},
    )
    .unwrap();
    assert_eq!(m1.lines, 1);
    let st1 = stream_nor.stats();
    assert_eq!(st1.rle_decodes, 5);
    let reads_pass1 = st1.io_reads;
    assert!(reads_pass1 > 0);

    // Repeat identical text 4 more times: L1 metrics + L2 decoded bitmaps hit 100% -> 0 new I/O reads, 0 RLE decodes!
    for _ in 0..4 {
        let _ = stream_nor.measure_text_styled(
            FontId::Body16,
            sample,
            120,
            2,
            false,
            TextAlign::Start,
            WrapMode::Char,
            0,
            0,
        );
        let _ = visit_layout_glyphs_provider(
            &stream_nor,
            FontId::Body16,
            sample,
            TextLayoutOptions {
                max_width: 120,
                max_lines: 2,
                ellipsis: false,
                align: TextAlign::Start,
            },
            ProviderLayoutStyle::default(),
            |pg| {
                FontFace::with_glyph_bytes(&stream_nor, pg.metrics, |data, enc| {
                    assert_eq!(enc, GlyphEncoding::Raw);
                    let raw_from_pool = actual_raw_decoded.get(&pg.metrics.unicode).unwrap();
                    assert_eq!(data, raw_from_pool.2.as_slice());
                })
                .unwrap();
            },
            |_| {},
        )
        .unwrap();
    }
    let st2 = stream_nor.stats();
    assert_eq!(st2.io_reads, reads_pass1);
    assert_eq!(st2.rle_decodes, 5);
    assert!(st2.metrics_hits > 0 && st2.bitmap_hits >= 20);
    assert!(actual_raw_decoded.len() == 5 && expect.len() == 5);
    assert_eq!(FontFace::kerning(&stream_nor, 'A', 'B').unwrap(), -2);
    assert_eq!(stream_nor.stats().io_reads, reads_pass1);

    // Mode 2: SD card 512B-sector block cache (PAGES = 4, 2048 B buffer).
    let sd = CountingFlash::new(&bytes);
    let mut sd_block = [0u8; 2048];
    let mut sd_pool = [0u8; 512];
    let mut sd_scratch = [0u8; 256];
    let stream_sd =
        StreamingFont::<_, 32, 64, 16, 4>::open(&sd, &mut sd_block, &mut sd_pool, &mut sd_scratch)
            .unwrap();
    stream_sd.reset_io_metrics();
    let _ = stream_sd.lookup_exact_metrics('\u{4E50}').unwrap().unwrap();
    let _ = stream_sd.lookup_exact_metrics('\u{4E51}').unwrap().unwrap();
    assert_eq!(stream_sd.stats().io_reads, 1);
}

#[test]
fn hybrid_font_set_keeps_hot_xip_zero_io_and_streams_cold_fallback() {
    // Hot XIP font has ASCII only; cold streaming font has CJK.
    let hot_raw = vec![0xFFu8; 16];
    let hot_bytes = build_large_v2_font(
        &[
            (' ', 6, 0, 0, &[]),
            ('A', 8, 4, 8, &hot_raw),
            ('□', 8, 4, 8, &hot_raw),
        ],
        &[],
        false,
        true,
    );
    let hot = Lf4Font::from_bytes(&hot_bytes).unwrap();

    let cold_raw = vec![0x55u8; 32];
    let cold_bytes = build_large_v2_font(
        &[('中', 12, 8, 8, &cold_raw), ('□', 12, 8, 8, &cold_raw)],
        &[],
        true,
        true,
    );
    let cold_flash = CountingFlash::new(&cold_bytes);
    let mut bbuf = [0u8; 512];
    let mut gpool = [0u8; 512];
    let mut scratch = [0u8; 256];
    let cold_stream =
        StreamingFont::<_, 8, 16, 16, 2>::open(&cold_flash, &mut bbuf, &mut gpool, &mut scratch)
            .unwrap();
    FontFace::clear_caches(&cold_stream);
    cold_stream.reset_io_metrics();

    let hybrid = HybridFontSet::from_xip(liuli_text::FontSet::single(hot))
        .with_stream_fallback(&cold_stream);

    // Pure ASCII hits only hot XIP -> 0 external storage reads!
    let r_a = hybrid.resolve_char(FontId::Body16, 'A').unwrap();
    assert!(!r_a.from_fallback);
    assert_eq!(hybrid.cache_stats().io_reads, 0);

    // Cold CJK char streams from fallback and caches cleanly.
    let r_cjk = hybrid.resolve_char(FontId::Body16, '中').unwrap();
    assert!(r_cjk.from_fallback);
    hybrid
        .with_glyph_bytes(
            FontId::Body16,
            r_cjk.from_fallback,
            r_cjk.metrics,
            |data, enc| {
                assert_eq!(enc, GlyphEncoding::Raw);
                assert_eq!(data, cold_raw.as_slice());
            },
        )
        .unwrap();
    let reads_after_first = hybrid.cache_stats().io_reads;
    assert!(reads_after_first > 0);

    // Second render of '中' is 100% cache hit -> 0 extra storage reads!
    let r_cjk2 = hybrid.resolve_char(FontId::Body16, '中').unwrap();
    hybrid
        .with_glyph_bytes(
            FontId::Body16,
            r_cjk2.from_fallback,
            r_cjk2.metrics,
            |data, _| {
                assert_eq!(data, cold_raw.as_slice());
            },
        )
        .unwrap();
    assert_eq!(hybrid.cache_stats().io_reads, reads_after_first);
}
