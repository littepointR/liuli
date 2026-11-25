#![cfg(feature = "desktop")]

use liuli::prelude::*;
use liuli_sim::session::AppSession;
use liuli_widget_demo::showcase::{
    SHOWCASE_BODY_LF4, ShowcaseApp, ShowcaseModel, ShowcaseTab, showcase_fonts,
    verify_locale_entries,
};
use liuli_widgets::TransitionKind;

fn center(app: &AppSession<'_, '_, ShowcaseApp>, key: u32) -> Point {
    let node = app
        .engine()
        .active_tree()
        .node_by_key(Key(key))
        .unwrap_or_else(|| panic!("missing key {key}"));
    let r = node.visible_rect;
    assert!(r.w > 0 && r.h > 0, "key {key} is invisible: {r:?}");
    Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
}

fn tap(app: &mut AppSession<'_, '_, ShowcaseApp>, clock: &mut u64, key: u32) {
    let pos = center(app, key);
    let id = PointerId(1);
    *clock += 16;
    app.step(
        InstantMs(*clock),
        Some(RawInput::PointerDown { id, pos }),
        true,
    )
    .unwrap();
    *clock += 16;
    app.step(
        InstantMs(*clock),
        Some(RawInput::PointerUp { id, pos }),
        true,
    )
    .unwrap();
}

#[test]
fn all_locale_entries_exist_in_embedded_font() {
    let font = Lf4Font::from_bytes(SHOWCASE_BODY_LF4).unwrap();
    let mut cjk_min_h = u16::MAX;
    let mut cjk_max_h = 0u16;
    for (loc_idx, entries) in verify_locale_entries().iter().enumerate() {
        for (entry_idx, text) in entries.iter().enumerate() {
            for ch in text.chars() {
                let glyph = font.lookup_exact(ch).unwrap_or_else(|| {
                    panic!(
                        "missing glyph '{ch}' (U+{:04X}) in locale {loc_idx} entry {entry_idx} ({text:?})",
                        ch as u32
                    )
                });
                if ch >= '\u{4E00}' {
                    cjk_min_h = cjk_min_h.min(glyph.metrics.height);
                    cjk_max_h = cjk_max_h.max(glyph.metrics.height);
                }
            }
        }
    }
    assert!(
        cjk_max_h >= cjk_min_h && (cjk_max_h - cjk_min_h) <= 3,
        "showcase CJK glyph heights must be uniform (min={cjk_min_h}, max={cjk_max_h})"
    );
}

#[test]
fn showcase_800x480_tabs_interactions_and_incremental_rendering_pass() {
    let fonts = showcase_fonts().unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<ShowcaseApp>::new(
        &mut storage,
        ShowcaseModel::new(),
        fonts,
        Size::new(800, 480),
        InstantMs(0),
        true,
    )
    .unwrap();
    #[cfg(feature = "enhanced")]
    app.engine_mut().set_layer_budget(1024 * 1024).unwrap();
    let mut clock = 0u64;

    // Tab 0: Overview -> toggle lang, theme, gauge load, live pause, cycle transition
    assert_eq!(app.engine().model().tab, ShowcaseTab::Overview);
    tap(&mut app, &mut clock, 112); // +8% load
    assert_eq!(app.engine().model().load, 70);
    tap(&mut app, &mut clock, 16); // EN / ZH
    assert_eq!(app.engine().model().locale, LocaleId(0));
    tap(&mut app, &mut clock, 17); // Light theme
    assert!(!app.engine().model().dark);
    tap(&mut app, &mut clock, 121); // Pause live
    assert!(!app.engine().model().live_sample);

    // Cycle transition 3 times: SlideLeft -> SlideUp -> Fade -> SlideLeft
    tap(&mut app, &mut clock, 132); // 1st tap -> SlideUp
    assert_eq!(app.engine().model().trans_kind, TransitionKind::SlideUp);
    let vp_rect = app.engine().active_tree().node_by_key(Key(49)).unwrap().layout_rect;
    let body_up = app.engine().active_tree().node_by_key(Key(50)).unwrap();
    assert!(body_up.layout_rect.y > vp_rect.y);
    assert_eq!(body_up.visible_rect, body_up.ink_rect.intersect(vp_rect));
    assert!(body_up.visible_rect.y >= vp_rect.y);
    assert!(body_up.visible_rect.bottom64() <= vp_rect.bottom64());
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    assert_eq!(
        app.engine().active_tree().node_by_key(Key(50)).unwrap().layout_rect,
        vp_rect
    );

    tap(&mut app, &mut clock, 132); // 2nd tap -> Fade (must not blank screen at t=0)
    assert_eq!(app.engine().model().trans_kind, TransitionKind::Fade);
    let alpha0 = app.engine().model().transition.alpha_for(InstantMs(clock));
    assert!(alpha0 >= 3, "fade opacity at frame 0 must stay visible, got {alpha0}");
    let bg_u16 = rgb565_to_u16(liuli_widgets::Theme::LIGHT.background);
    let fb = app.screen().words();
    let has_content_px = (vp_rect.y..vp_rect.bottom64() as i32).any(|y| {
        let row = &fb[(y as usize) * 800..(y as usize + 1) * 800];
        row[vp_rect.x as usize..vp_rect.right64() as usize]
            .iter()
            .any(|&px| px != bg_u16)
    });
    assert!(has_content_px, "fade frame 0 must not blank the viewport");
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }

    tap(&mut app, &mut clock, 132); // 3rd tap -> SlideLeft
    assert_eq!(app.engine().model().trans_kind, TransitionKind::SlideLeft);
    let body_left = app.engine().active_tree().node_by_key(Key(50)).unwrap();
    assert!(body_left.layout_rect.x > vp_rect.x);
    assert!(body_left.visible_rect.right64() <= vp_rect.right64());
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    assert_eq!(
        app.engine().active_tree().node_by_key(Key(50)).unwrap().layout_rect,
        vp_rect
    );

    // Tab 1: Forms -> checkboxes, radios, dropdown, roller, multi-line editor
    tap(&mut app, &mut clock, 12);
    assert_eq!(app.engine().model().tab, ShowcaseTab::Forms);
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    tap(&mut app, &mut clock, 202); // brightness toggle
    tap(&mut app, &mut clock, 212); // SlideUp radio
    tap(&mut app, &mut clock, 221); // Open dropdown
    assert!(app.engine().model().dropdown.open);
    tap(&mut app, &mut clock, 224); // Select ILI9341 8080
    assert_eq!(app.engine().model().dropdown.selected, 1);
    assert!(!app.engine().model().dropdown.open);
    tap(&mut app, &mut clock, 232); // Roller next
    assert_eq!(app.engine().model().roller.selected, 3);
    tap(&mut app, &mut clock, 241); // + Text
    assert!(app.engine().model().editor.text().ends_with(" OK"));
    tap(&mut app, &mut clock, 242); // + Line
    assert!(app.engine().model().editor.line_count() >= 3);

    // Tab 2: Charts -> push sample, select data_table row
    tap(&mut app, &mut clock, 13);
    assert_eq!(app.engine().model().tab, ShowcaseTab::Charts);
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    let rev_before = app.engine().model().chart_rev;
    tap(&mut app, &mut clock, 310);
    assert!(app.engine().model().chart_rev > rev_before);
    tap(&mut app, &mut clock, 436); // row index 1 (320 + 100 + 16 = 436)
    assert_eq!(app.engine().model().selected_row, Some(1));

    // Tab 3: Layout -> Justify, Grid, ScrollX + ScrollPhysics
    tap(&mut app, &mut clock, 14);
    assert_eq!(app.engine().model().tab, ShowcaseTab::Layout);
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    tap(&mut app, &mut clock, 405); // SpaceAround
    assert_eq!(app.engine().model().justify, Justify::SpaceAround);
    tap(&mut app, &mut clock, 424); // Grid tile #3
    assert_eq!(app.engine().model().grid_selected, 3);
    let card0_x_before = app
        .engine()
        .active_tree()
        .node_by_key(Key(452))
        .unwrap()
        .layout_rect
        .x;
    tap(&mut app, &mut clock, 449); // Fling right
    for _ in 0..24 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    assert!(app.engine().model().scroll_physics.offset() > 0);
    let card0_x_after = app
        .engine()
        .active_tree()
        .node_by_key(Key(452))
        .unwrap()
        .layout_rect
        .x;
    assert!(
        card0_x_after < card0_x_before,
        "ScrollX set_offset must shift child card left ({card0_x_before} -> {card0_x_after})"
    );
}
