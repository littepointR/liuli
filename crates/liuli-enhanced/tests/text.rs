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
