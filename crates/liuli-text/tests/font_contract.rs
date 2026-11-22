use liuli_core::geometry::Rgb565;
use liuli_core::layout::TextMeasurer;
use liuli_text::{
    FontError, Lf4Font, TextLayoutOptions, a4_pixel, blend_rgb565_a4, visit_layout_glyphs,
};

fn build_synthetic_lf4(glyphs: &[(char, u16, i16, i16, u16, u16, &[u8])]) -> Vec<u8> {
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
    let empty = font.measure_text("", 100, 0, true);
    assert_eq!(empty.lines, 0);
    assert_eq!(empty.size.height, 0);

    // Zero max_width => 0 lines, no drawing
    let zero_w = font.measure_text("AB", 0, 0, true);
    assert_eq!(zero_w.lines, 0);

    // Negative bearing extends ink left of 0
    let neg_b = font.measure_text("BA", 100, 1, false);
    assert_eq!(neg_b.ink.x, -2);

    // Explicit \r\n + trailing \n
    let nl = font.measure_text("A\r\nB\n", 100, 0, false);
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
        },
        |g| chars2.push(char::from_u32(g.glyph.metrics.unicode).unwrap()),
    );
    assert!(chars2.is_empty());
    assert!(ell2.ink.is_empty());
}
