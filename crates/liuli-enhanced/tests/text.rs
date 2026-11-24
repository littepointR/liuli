use liuli_enhanced::{Editor, ShapingFont, shape_paragraph};
#[test]
fn rtl_and_indic_clusters_use_fallback_fonts() {
    let fonts = [
        ShapingFont::from_bytes(include_bytes!("fonts/Arabic.ttf"), 0).unwrap(),
        ShapingFont::from_bytes(include_bytes!("fonts/Devanagari.ttf"), 0).unwrap(),
    ];
    let mut run = Vec::new();
    shape_paragraph("سلام", &fonts, 1024, |f, g| run.push((f, g)));
    assert!(run.len() > 1);
    assert!(run.iter().all(|(f, g)| *f == 0 && g.id != 0));
    assert!(
        run.windows(2)
            .all(|pair| pair[0].1.cluster >= pair[1].1.cluster)
    );
    run.clear();
    shape_paragraph("क्षि", &fonts, 1024, |f, g| run.push((f, g)));
    assert!(!run.is_empty());
    assert!(
        run.iter()
            .all(|(f, g)| *f == 1 && g.id != 0 && g.cluster == 0)
    );
}
#[test]
fn ime_cancel_and_commit_respect_graphemes() {
    let mut e = Editor::new("A中e\u{301}".into());
    e.compose("ni");
    e.compose("nih");
    e.cancel();
    assert_eq!(e.text(), "A中e\u{301}");
    e.backspace();
    assert_eq!(e.text(), "A中");
    e.compose("ni");
    e.commit("你");
    assert_eq!(e.text(), "A中你");
    assert!(e.preedit().is_none());
    e.previous();
    assert_eq!(e.cursor(), 4);
    assert!(!e.set_cursor(2));
    e.next();
    assert_eq!(e.cursor(), 7);
}

#[test]
fn paint_shaped_glyph_ids_at_a_baseline() {
    use liuli_enhanced::glyphs::{GlyphError, raster_glyph};
    let fonts = [ShapingFont::from_bytes(include_bytes!("fonts/Arabic.ttf"), 0).unwrap()];
    let mut ink = 0;
    let mut pen = 0;
    shape_paragraph("سلام", &fonts, 24, |f, g| {
        let mut count = 0;
        raster_glyph(
            &fonts[f],
            g.id,
            24,
            (pen + g.x_offset, 32 - g.y_offset),
            4096,
            |_, _, a| {
                assert!((1..=15).contains(&a));
                count += 1;
            },
        )
        .unwrap();
        if count > 0 {
            assert_eq!(
                raster_glyph(&fonts[f], g.id, 24, (0, 0), 0, |_, _, _| {}),
                Err(GlyphError::PixelBudget)
            );
        }
        ink += count;
        pen += g.advance;
    });
    assert!(ink > 50 && pen > 0);
}

#[test]
fn visual_rtl_caret_moves_as_graphemes_and_preserves_joined_marks() {
    let mut e = Editor::new("سَل".into());
    assert_eq!(liuli_enhanced::visual_carets(e.text()), vec![6, 4, 0]);
    e.move_visual(true);
    assert_eq!(e.cursor(), 4);
    e.move_visual(true);
    assert_eq!(e.cursor(), 0);
    e.move_visual(false);
    assert_eq!(e.cursor(), 4);
    e.backspace();
    assert_eq!(e.text(), "ل");
}

#[test]
fn glyph_cache_lru_matches_uncached_and_bounds_memory() {
    use liuli_enhanced::glyphs::{GlyphCache, raster_glyph};
    let font = ShapingFont::from_bytes(include_bytes!("fonts/Arabic.ttf"), 0).unwrap();
    let mut glyphs = Vec::new();
    shape_paragraph("سلام", core::slice::from_ref(&font), 20, |_, g| {
        glyphs.push(g);
    });
    assert!(glyphs.len() >= 2);
    let mut cache = GlyphCache::new(160);
    for pass in 0..2 {
        for g in &glyphs {
            let mut uncached = std::collections::BTreeMap::new();
            raster_glyph(&font, g.id, 20, (0, 0), 4096, |x, y, a| {
                uncached.insert((x, y), a);
            })
            .unwrap();
            if let Some(cg) = cache.raster_glyph_cached(0, &font, g.id, 20, 4096).unwrap() {
                let mut cached_map = std::collections::BTreeMap::new();
                for sy in 0..cg.height {
                    for sx in 0..cg.width {
                        let a = liuli_text::a4_pixel(&cg.data, cg.width, sx, sy);
                        if a > 0 {
                            cached_map.insert(
                                (
                                    i32::from(cg.left) + i32::from(sx),
                                    i32::from(sy) - i32::from(cg.top),
                                ),
                                a,
                            );
                        }
                    }
                }
                assert_eq!(cached_map, uncached);
            }
            assert!(cache.bytes_used() <= 160);
        }
        if pass == 0 {
            assert!(cache.misses() > 0);
        }
    }
    // Larger cache retains everything for pure cache hits on second pass.
    let mut big = GlyphCache::new(4096);
    for g in &glyphs {
        let _ = big.raster_glyph_cached(0, &font, g.id, 20, 4096).unwrap();
    }
    let hits_before = big.hits();
    let misses_before = big.misses();
    for g in &glyphs {
        let _ = big.raster_glyph_cached(0, &font, g.id, 20, 4096).unwrap();
    }
    assert!(big.hits() > hits_before);
    assert_eq!(big.misses(), misses_before);
}

#[test]
fn text_field_preedit_underline_gamma_and_cache_reuse() {
    use liuli_core::geometry::Rgb565;
    use liuli_core::input::TextInput;
    use liuli_core::view::TextGamma;
    use liuli_enhanced::field::{FieldStyle, TextField};

    let font = ShapingFont::from_bytes(include_bytes!("fonts/Arabic.ttf"), 0).unwrap();
    let style = FieldStyle {
        width: 120,
        height: 30,
        px: 16,
        foreground: Rgb565::new(31, 63, 31),
        background: Rgb565::new(0, 0, 0),
    };
    let mut field = TextField::new("س".into(), vec![font], style, 64).unwrap();
    let snap_pixels = |f: &TextField<'_>| -> Vec<u16> {
        let view = f.image().view();
        let mut out = Vec::with_capacity(120 * 30);
        for y in 0..30 {
            for x in 0..120 {
                out.push(view.sample(x, y, 120, 30, 0).unwrap());
            }
        }
        out
    };
    let base_px = snap_pixels(&field);
    field.apply(TextInput::Compose("لا")).unwrap();
    let comp_px = snap_pixels(&field);
    assert_ne!(base_px, comp_px);
    // Preedit draws an underline beneath the composed glyphs below the baseline.
    let baseline = (30 + 16) / 2 - 2;
    let uy = (baseline + 2) as usize;
    let underline_fg = comp_px[uy * 120..(uy + 1) * 120]
        .iter()
        .filter(|&&p| p == 0xFFFF)
        .count();
    assert!(
        underline_fg >= 3,
        "expected preedit underline pixels, found {underline_fg}"
    );

    field.apply(TextInput::Cancel).unwrap();
    assert_eq!(snap_pixels(&field), base_px);

    let hits0 = field.cache_hits();
    field.apply(TextInput::Left).unwrap();
    assert!(field.cache_hits() > hits0);

    field.set_gamma(TextGamma::Perceptual).unwrap();
    assert_eq!(field.gamma(), TextGamma::Perceptual);
    assert_ne!(snap_pixels(&field), base_px);
}
