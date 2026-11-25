use liuli_core::geometry::Rgb565;
use liuli_core::layout::TextMeasurer;
use liuli_core::view::{FontId, TextAlign};
use liuli_text::{
    FontError, Lf4Font, TextLayoutOptions, a4_pixel, blend_rgb565_a4, visit_layout_glyphs,
};

type SyntheticGlyph<'a> = (char, u16, i16, i16, u16, u16, &'a [u8]);

fn build_synthetic_lf4(glyphs: &[SyntheticGlyph<'_>]) -> Vec<u8> {
    let count = glyphs.len() as u16;
    let fallback_idx = glyphs
        .iter()
        .position(|g| g.0 == '□')
        .expect("synthetic test font needs U+25A1") as u16;
    let table_offset = 24u32;
    let bitmap_offset = 24u32 + (count as u32) * 20;

    let mut table = Vec::new();
    let mut bitmaps = Vec::new();
    for &(ch, adv, bx, by, w, h, data) in glyphs {
        let off = bitmaps.len() as u32;
        bitmaps.extend_from_slice(data);
        table.extend_from_slice(&(ch as u32).to_le_bytes());
        table.extend_from_slice(&adv.to_le_bytes());
        table.extend_from_slice(&bx.to_le_bytes());
        table.extend_from_slice(&by.to_le_bytes());
        table.extend_from_slice(&w.to_le_bytes());
        table.extend_from_slice(&h.to_le_bytes());
        table.extend_from_slice(&0u16.to_le_bytes());
        table.extend_from_slice(&off.to_le_bytes());
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"LLF4");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&14i16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&fallback_idx.to_le_bytes());
    out.extend_from_slice(&table_offset.to_le_bytes());
    out.extend_from_slice(&bitmap_offset.to_le_bytes());
    out.extend_from_slice(&table);
    out.extend_from_slice(&bitmaps);
    out
}

#[test]
fn lf4_validation_rejects_corrupt_inputs() {
    let g_a: &[u8] = &[0x01, 0x7F];
    let g_fb: &[u8] = &[0xFF];
    let mut valid =
        build_synthetic_lf4(&[('A', 8, 0, 10, 3, 1, g_a), ('□', 12, 0, 10, 2, 1, g_fb)]);
    assert!(Lf4Font::from_bytes(&valid).is_ok());

    // Bad magic
    let mut bad = valid.clone();
    bad[0] = b'X';
    assert_eq!(Lf4Font::from_bytes(&bad), Err(FontError::InvalidMagic));

    // Unsorted index (swap entries)
    let mut unsorted = valid.clone();
    let e0 = unsorted[24..44].to_vec();
    let e1 = unsorted[44..64].to_vec();
    unsorted[24..44].copy_from_slice(&e1);
    unsorted[44..64].copy_from_slice(&e0);
    assert_eq!(
        Lf4Font::from_bytes(&unsorted),
        Err(FontError::UnsortedIndex)
    );

    // Truncated bitmap
    valid.pop();
    assert_eq!(Lf4Font::from_bytes(&valid), Err(FontError::TruncatedBitmap));
}

#[test]
fn odd_width_nibbles_and_a4_blend_exact() {
    // 3x2 glyph: row0 nibbles (1, 7, 15, [0 pad]), row1 nibbles (0, 15, 7, [0 pad])
    let bmp: &[u8] = &[0x17, 0xF0, 0x0F, 0x70];
    assert_eq!(a4_pixel(bmp, 3, 0, 0), 1);
    assert_eq!(a4_pixel(bmp, 3, 1, 0), 7);
    assert_eq!(a4_pixel(bmp, 3, 2, 0), 15);
    assert_eq!(a4_pixel(bmp, 3, 0, 1), 0);
    assert_eq!(a4_pixel(bmp, 3, 1, 1), 15);
    assert_eq!(a4_pixel(bmp, 3, 2, 1), 7);

    let bg = Rgb565::new(0, 0, 0);
    let fg = Rgb565::new(31, 63, 31);
    assert_eq!(blend_rgb565_a4(fg, bg, 0), bg);
    assert_eq!(blend_rgb565_a4(fg, bg, 15), fg);
    // a=1: r=(31*1+7)/15=2, g=(63*1+7)/15=4, b=2
    assert_eq!(blend_rgb565_a4(fg, bg, 1), Rgb565::new(2, 4, 2));
    // a=7: r=(31*7+7)/15=14, g=(63*7+7)/15=29, b=14
    assert_eq!(blend_rgb565_a4(fg, bg, 7), Rgb565::new(14, 29, 14));
}

#[test]
fn layout_wrap_newlines_negative_bearing_ellipsis_and_fallback() {
    let bmp1: &[u8] = &[0xF0];
    let bmp2: &[u8] = &[0xFF, 0xFF];
    let bytes = build_synthetic_lf4(&[
        ('A', 8, 0, 10, 1, 1, bmp1),
        ('B', 8, -2, 10, 2, 1, bmp2), // negative bearing_x = -2
        ('…', 8, 0, 10, 1, 1, bmp1),
        ('□', 12, 0, 10, 1, 1, bmp1),
    ]);
    let font = Lf4Font::from_bytes(&bytes).unwrap();

    // Empty text => 0 lines
    let empty = font.measure_text(FontId::Body16, "", 100, 0, true, TextAlign::Start);
    assert_eq!(empty.lines, 0);
    assert_eq!(empty.size.height, 0);

    // Zero max_width => 0 lines, no drawing
    let zero_w = font.measure_text(FontId::Body16, "AB", 0, 0, true, TextAlign::Start);
    assert_eq!(zero_w.lines, 0);

    // Negative bearing extends ink left of 0
    let neg_b = font.measure_text(FontId::Body16, "BA", 100, 1, false, TextAlign::Start);
    assert_eq!(neg_b.ink.x, -2);

    // Explicit \r\n + trailing \n
    let nl = font.measure_text(FontId::Body16, "A\r\nB\n", 100, 0, false, TextAlign::Start);
    assert_eq!(nl.lines, 3);

    // Over-wide single glyph (fallback '□' adv=12 > max_width=6): still advances 1 scalar
    let mut seen = 0usize;
    let wide = visit_layout_glyphs(
        &font,
        "中中",
        TextLayoutOptions {
            max_width: 6,
            max_lines: 0,
            ellipsis: false,
            align: TextAlign::Start,
        },
        |g| {
            assert_eq!(g.glyph.metrics.unicode, '□' as u32);
            seen += 1;
        },
    );
    assert_eq!(seen, 2);
    assert_eq!(wide.lines, 2);

    // Ellipsis rollback with max_lines = 1, max_width = 16 -> "A…" (8 + 8 = 16)
    let mut chars = Vec::new();
    let ell = visit_layout_glyphs(
        &font,
        "AAA",
        TextLayoutOptions {
            max_width: 16,
            max_lines: 1,
            ellipsis: true,
            align: TextAlign::Start,
        },
        |g| chars.push(char::from_u32(g.glyph.metrics.unicode).unwrap()),
    );
    assert_eq!(chars, vec!['A', '…']);
    assert_eq!(ell.lines, 1);

    // Ellipsis does not fit at all (max_width=4 < ell_adv=8) -> empty ink on last line
    let mut chars2 = Vec::new();
    let ell2 = visit_layout_glyphs(
        &font,
        "AAA",
        TextLayoutOptions {
            max_width: 4,
            max_lines: 1,
            ellipsis: true,
            align: TextAlign::Start,
        },
        |g| chars2.push(char::from_u32(g.glyph.metrics.unicode).unwrap()),
    );
    assert!(chars2.is_empty());
    assert!(ell2.ink.is_empty());
}

#[test]
fn text_align_center_end_crlf_negative_bearing_ellipsis_overwide_and_font_set() {
    use liuli_core::geometry::Rect;
    use liuli_text::FontSet;

    let bmp1 = &[0xFF]; // 1x1
    let bmp2 = &[0xFF]; // 2x1 packed A4 = 1 row byte
    let bytes16 = build_synthetic_lf4(&[
        ('A', 8, 0, 10, 1, 1, bmp1),
        ('B', 8, -2, 10, 2, 1, bmp2), // negative bearing_x = -2
        ('…', 8, 0, 10, 1, 1, bmp1),
        ('□', 12, 0, 10, 1, 1, bmp1),
    ]);
    // Distinct small header (ascent=10, descent=2, line_gap=2 -> line_height=14)
    let mut bytes12 = bytes16.clone();
    bytes12[8..10].copy_from_slice(&10i16.to_le_bytes());

    let f16 = Lf4Font::from_bytes(&bytes16).unwrap();
    let f12 = Lf4Font::from_bytes(&bytes12).unwrap();
    let font_set = FontSet::new(f16, f12);
    assert_eq!(font_set.line_height(FontId::Body16), 18);
    assert_eq!(font_set.line_height(FontId::Small12), 14);
    assert_eq!(
        font_set
            .measure_text(FontId::Small12, "AA", 100, 1, false, TextAlign::Center)
            .size
            .height,
        14
    );

    // 1. Multi-line different widths "AAAA\nBB" under Start, Center, End in max_width=40
    let collect = |align| {
        let mut pts = Vec::new();
        let out = visit_layout_glyphs(
            &f16,
            "AAAA\nBB",
            TextLayoutOptions {
                max_width: 40,
                max_lines: 0,
                ellipsis: false,
                align,
            },
            |pg| {
                pts.push((
                    char::from_u32(pg.glyph.metrics.unicode).unwrap(),
                    pg.x,
                    pg.y,
                    pg.line,
                ));
            },
        );
        (pts, out)
    };

    let (pts_start, out_start) = collect(TextAlign::Start);
    assert_eq!(out_start.size.width, 32);
    assert_eq!(pts_start[0], ('A', 0, 4, 0));
    assert_eq!(pts_start[4], ('B', -2, 22, 1));
    assert_eq!(out_start.ink, Rect::new(-2, 4, 27, 19));

    let (pts_center, out_center) = collect(TextAlign::Center);
    // Line 0 adv=32 -> slack=8 -> align_x=4; Line 1 adv=16 -> slack=24 -> align_x=12 (with bearing_x=-2 => 10)
    assert_eq!(
        pts_center,
        vec![
            ('A', 4, 4, 0),
            ('A', 12, 4, 0),
            ('A', 20, 4, 0),
            ('A', 28, 4, 0),
            ('B', 10, 22, 1),
            ('B', 18, 22, 1),
        ]
    );
    assert_eq!(out_center.ink, Rect::new(4, 4, 25, 19));

    let (pts_end, out_end) = collect(TextAlign::End);
    // Line 0 slack=8 -> align_x=8; Line 1 slack=24 -> align_x=24 (with bearing_x=-2 => 22)
    assert_eq!(
        pts_end,
        vec![
            ('A', 8, 4, 0),
            ('A', 16, 4, 0),
            ('A', 24, 4, 0),
            ('A', 32, 4, 0),
            ('B', 22, 22, 1),
            ('B', 30, 22, 1),
        ]
    );
    assert_eq!(out_end.ink, Rect::new(8, 4, 25, 19));

    // 2. CRLF + trailing empty line under End
    let mut crlf_pts = Vec::new();
    let crlf_out = visit_layout_glyphs(
        &f16,
        "AA\r\nB\n",
        TextLayoutOptions {
            max_width: 40,
            max_lines: 0,
            ellipsis: false,
            align: TextAlign::End,
        },
        |pg| crlf_pts.push((pg.x, pg.y, pg.line)),
    );
    assert_eq!(crlf_out.lines, 3);
    assert_eq!(crlf_out.size.height, 54);
    assert_eq!(crlf_pts, vec![(24, 4, 0), (32, 4, 0), (30, 22, 1)]);

    // 3. Ellipsis rollback under Center/End: "AAA" in max_width=20 => "A…" (adv=16, slack=4)
    let mut ell_center = Vec::new();
    let _ = visit_layout_glyphs(
        &f16,
        "AAA",
        TextLayoutOptions {
            max_width: 20,
            max_lines: 1,
            ellipsis: true,
            align: TextAlign::Center,
        },
        |pg| ell_center.push((char::from_u32(pg.glyph.metrics.unicode).unwrap(), pg.x)),
    );
    assert_eq!(ell_center, vec![('A', 2), ('…', 10)]);

    let mut ell_end = Vec::new();
    let _ = visit_layout_glyphs(
        &f16,
        "AAA",
        TextLayoutOptions {
            max_width: 20,
            max_lines: 1,
            ellipsis: true,
            align: TextAlign::End,
        },
        |pg| ell_end.push((char::from_u32(pg.glyph.metrics.unicode).unwrap(), pg.x)),
    );
    assert_eq!(ell_end, vec![('A', 4), ('…', 12)]);

    // 4. Over-wide first glyph '□' (adv=12 > max_width=6): align_x clamps cleanly to 0
    let mut wide_end = Vec::new();
    let _ = visit_layout_glyphs(
        &f16,
        "□",
        TextLayoutOptions {
            max_width: 6,
            max_lines: 1,
            ellipsis: false,
            align: TextAlign::End,
        },
        |pg| wide_end.push(pg.x),
    );
    assert_eq!(wide_end, vec![0]);
}

#[allow(clippy::too_many_arguments)]
fn build_synthetic_lf4_v2(
    format: liuli_text::GlyphFormat,
    encoding: liuli_text::GlyphEncoding,
    compact: bool,
    phases: u8,
    advance_q2: bool,
    kern_pairs: &[(char, char, i16)],
    glyphs: &[SyntheticGlyph<'_>],
) -> Vec<u8> {
    let count = glyphs.len() as u16;
    let fallback_idx = glyphs
        .iter()
        .position(|g| g.0 == '□')
        .expect("needs U+25A1") as u16;
    let kern_offset = if kern_pairs.is_empty() { 0u32 } else { 32u32 };
    let table_offset = 32u32
        + if kern_pairs.is_empty() {
            0
        } else {
            4 + (kern_pairs.len() as u32) * 12
        };
    let entry_size = if compact { 12u32 } else { 20u32 };
    let bitmap_offset = table_offset + (count as u32) * entry_size;

    let mut kern_bytes = Vec::new();
    if !kern_pairs.is_empty() {
        kern_bytes.extend_from_slice(&(kern_pairs.len() as u32).to_le_bytes());
        for &(l, r, dx) in kern_pairs {
            kern_bytes.extend_from_slice(&(l as u32).to_le_bytes());
            kern_bytes.extend_from_slice(&(r as u32).to_le_bytes());
            kern_bytes.extend_from_slice(&dx.to_le_bytes());
            kern_bytes.extend_from_slice(&0u16.to_le_bytes());
        }
    }

    let mut table = Vec::new();
    let mut bitmaps = Vec::new();
    for &(ch, adv, bx, by, w, h, data) in glyphs {
        let off = bitmaps.len() as u32;
        if encoding == liuli_text::GlyphEncoding::Rle && w > 0 && h > 0 {
            bitmaps.extend_from_slice(&(data.len() as u16).to_le_bytes());
        }
        bitmaps.extend_from_slice(data);
        if compact {
            table.extend_from_slice(&(ch as u32).to_le_bytes());
            table.push(adv as u8);
            table.push((bx as i8) as u8);
            table.push((by as i8) as u8);
            table.push(w as u8);
            table.push(h as u8);
            table.push((off & 0xFF) as u8);
            table.push(((off >> 8) & 0xFF) as u8);
            table.push(((off >> 16) & 0xFF) as u8);
        } else {
            table.extend_from_slice(&(ch as u32).to_le_bytes());
            table.extend_from_slice(&adv.to_le_bytes());
            table.extend_from_slice(&bx.to_le_bytes());
            table.extend_from_slice(&by.to_le_bytes());
            table.extend_from_slice(&w.to_le_bytes());
            table.extend_from_slice(&h.to_le_bytes());
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&off.to_le_bytes());
        }
    }

    let phase_bits = match phases {
        2 => 1u8 << 1,
        4 => 2u8 << 1,
        _ => 0,
    };
    let flags = u8::from(compact) | phase_bits | (u8::from(advance_q2) << 3);
    let mut out = Vec::new();
    out.extend_from_slice(b"LLF4");
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&14i16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&fallback_idx.to_le_bytes());
    out.extend_from_slice(&table_offset.to_le_bytes());
    out.extend_from_slice(&bitmap_offset.to_le_bytes());
    out.push(format as u8);
    out.push(encoding as u8);
    out.push(flags);
    out.push(0);
    out.extend_from_slice(&kern_offset.to_le_bytes());
    out.extend_from_slice(&kern_bytes);
    out.extend_from_slice(&table);
    out.extend_from_slice(&bitmaps);
    out
}

#[test]
fn lf4_v2_multibpp_rle_kerning_subpixel_word_wrap_and_storage() {
    use liuli_core::view::WrapMode;
    use liuli_text::storage::{Cached, StoredFont};
    use liuli_text::{
        FontSet, GlyphEncoding, GlyphFormat, TextLayoutStyle, visit_layout_glyphs_styled,
    };

    // Compact 12-byte index, RLE, 2 subpixel phases, Q2 advance, kerning A+V -> -4 q2 (-1 px).
    // 2x1 A2 => 1 row byte * 2 phases = 2 raw bytes; RLE [1, 0xF0, 0x50] -> literal 2 bytes.
    let rle2 = &[1u8, 0xF0, 0x50];
    let bytes = build_synthetic_lf4_v2(
        GlyphFormat::A2,
        GlyphEncoding::Rle,
        true,
        2,
        true,
        &[('A', 'V', -4)],
        &[
            (' ', 16, 0, 10, 0, 0, &[]),
            ('A', 22, 0, 10, 2, 1, rle2), // 5.5 px advance in Q2
            ('V', 20, 0, 10, 2, 1, rle2),
            ('…', 32, 0, 10, 2, 1, rle2),
            ('□', 32, 0, 10, 2, 1, rle2),
        ],
    );
    let font = Lf4Font::from_bytes(&bytes).unwrap();
    assert_eq!(font.version(), 2);
    assert!(font.compact_index());
    assert_eq!(font.format(), GlyphFormat::A2);
    assert_eq!(font.encoding(), GlyphEncoding::Rle);
    assert_eq!(font.subpixel_phases(), 2);
    assert!(font.advance_is_q2());
    assert_eq!(font.kerning('A', 'V'), -4);
    assert_eq!(font.kerning('V', 'A'), 0);

    // StoredFont opens LF4 v2 cleanly and decodes RLE length prefix.
    let mut cache_buf = [0u8; 256];
    let mut stored = StoredFont::open(Cached::new(bytes.as_slice(), &mut cache_buf)).unwrap();
    let m_a = stored.lookup('A').unwrap().unwrap();
    let mut scratch = [0u8; 32];
    let bmp_a = stored.read_bitmap(m_a, &mut scratch).unwrap();
    assert_eq!(bmp_a.data, rle2);
    let mut row_buf = [0u8; 4];
    let mut decoded = Vec::new();
    liuli_text::decode_rle_rows(bmp_a.data, 1, 2, 0, 2, &mut row_buf, |row, bytes| {
        decoded.push((row, bytes.to_vec()));
    })
    .unwrap();
    assert_eq!(decoded, vec![(0, vec![0xF0]), (1, vec![0x50])]);

    // Check Q2 phase stepping + kerning: 'A' at x=0 (phase 0), pen becomes 22 - 4 = 18 q2 (x=4, phase=1)
    let mut out = Vec::new();
    visit_layout_glyphs_styled(
        &font,
        "AV",
        TextLayoutOptions {
            max_width: 80,
            max_lines: 1,
            ellipsis: false,
            align: TextAlign::Start,
        },
        TextLayoutStyle::default(),
        |pg| out.push((pg.glyph.metrics.unicode, pg.x, pg.phase)),
        |_| {},
    );
    assert_eq!(out, vec![('A' as u32, 0, 0), ('V' as u32, 4, 1)]);

    // Word wrap & CJK kinsoku test with v1 font.
    let raw1 = &[0xFF];
    let word_bytes = build_synthetic_lf4(&[
        (' ', 6, 0, 10, 0, 0, &[]),
        ('A', 6, 0, 10, 1, 1, raw1),
        ('B', 6, 0, 10, 1, 1, raw1),
        ('□', 12, 0, 10, 1, 1, raw1),
    ]);
    let wf = Lf4Font::from_bytes(&word_bytes).unwrap();
    let mut lines = Vec::new();
    visit_layout_glyphs_styled(
        &wf,
        "AA BBB",
        TextLayoutOptions {
            max_width: 24,
            max_lines: 4,
            ellipsis: false,
            align: TextAlign::Start,
        },
        TextLayoutStyle {
            wrap: WrapMode::Word,
            ..TextLayoutStyle::default()
        },
        |pg| lines.push((pg.line, char::from_u32(pg.glyph.metrics.unicode).unwrap())),
        |_| {},
    );
    // "AA BBB" in width 24 = 4 chars -> breaks cleanly before "BBB" instead of breaking mid-word.
    assert_eq!(
        lines,
        vec![(0, 'A'), (0, 'A'), (1, 'B'), (1, 'B'), (1, 'B')]
    );

    // 4-slot FontSet + fallback chain test.
    let fb_bytes = build_synthetic_lf4(&[('Z', 9, 0, 10, 1, 1, raw1), ('□', 9, 0, 10, 1, 1, raw1)]);
    let fb = Lf4Font::from_bytes(&fb_bytes).unwrap();
    let set = FontSet::single(wf).with_title(font).with_fallback(fb);
    assert_eq!(set.get(FontId::Title20).version(), 2);
    let mut seen_chars = Vec::new();
    visit_layout_glyphs_styled(
        set.get(FontId::Body16),
        "AZ",
        TextLayoutOptions {
            max_width: 80,
            max_lines: 1,
            ellipsis: false,
            align: TextAlign::Start,
        },
        TextLayoutStyle {
            fallback: set.fallback(),
            ..TextLayoutStyle::default()
        },
        |pg| {
            seen_chars.push((
                char::from_u32(pg.glyph.metrics.unicode).unwrap(),
                pg.from_fallback,
            ))
        },
        |_| {},
    );
    assert_eq!(seen_chars, vec![('A', false), ('Z', true)]);
}
