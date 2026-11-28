#![cfg(feature = "desktop")]

use liuli::prelude::*;
use liuli_sim::session::AppSession;
use liuli_widget_demo::showcase::{
    SHOWCASE_BODY_LF4, ShowcaseApp, ShowcaseModel, ShowcaseTab, showcase_fonts,
    verify_locale_entries,
};
use liuli_widgets::TransitionKind;

#[path = "showcase/tabs.rs"]
mod tabs;

#[test]
fn gallery_navigation_and_widget_samples_replay_on_both_panels() {
    use liuli_widget_demo::showcase::SamplePage;
    for size in [Size::new(800, 480), Size::new(240, 536)] {
        let mut storage = Storage::new();
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            ShowcaseModel {
                live_sample: false,
                ..ShowcaseModel::for_size(size)
            },
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        #[cfg(feature = "enhanced")]
        app.engine_mut().set_layer_budget(1024 * 1024).unwrap();
        let mut now = 0;
        for key in [12, 13, 14, 19, 23, 11, 23] {
            let button = app.engine().active_tree().node_by_key(Key(key)).unwrap();
            assert_eq!(button.kind, NodeKind::Tab);
            let header = app.engine().active_tree().node_by_key(Key(10)).unwrap();
            assert_eq!(header.kind, NodeKind::TabBar);
            assert_eq!(
                header.layout_rect.intersect(button.layout_rect),
                button.layout_rect
            );
            assert_eq!(
                app.engine()
                    .active_tree()
                    .node_by_key(Key(50))
                    .unwrap()
                    .kind,
                NodeKind::TabPanel
            );
            let parent =
                app.engine().active_tree().nodes[button.parent.get_u16().unwrap() as usize].key;
            assert_eq!(parent, Key(10));
            tap(&mut app, &mut now, key);
            now += 350;
            app.step(InstantMs(now), None, true).unwrap();
            for id in [11, 12, 13, 14, 19, 23] {
                assert_eq!(
                    app.engine()
                        .active_tree()
                        .node_by_key(Key(id))
                        .unwrap()
                        .props
                        .checked(),
                    id == key
                );
            }
        }
        tap(&mut app, &mut now, 752);
        assert_eq!(app.engine().model().samples.value, 75);
        tap(&mut app, &mut now, 716);
        assert!(app.engine().model().samples.confirm);
        tap(&mut app, &mut now, 763);
        assert!(!app.engine().model().samples.confirm);
        tap(&mut app, &mut now, 716);
        tap(&mut app, &mut now, 762);
        assert_eq!(app.engine().model().samples.value, 25);
        assert!(app.engine().active_tree().node_by_key(Key(718)).is_some());
        now += 1700;
        app.step(InstantMs(now), None, true).unwrap();
        assert!(app.engine().active_tree().node_by_key(Key(718)).is_none());
        tap(&mut app, &mut now, 702);
        assert_eq!(app.engine().model().samples.page, SamplePage::Records);
        let start = center(&app, 2006);
        let end = center(&app, 2001);
        for event in [
            RawInput::PointerDown {
                id: PointerId(2),
                pos: start,
            },
            RawInput::PointerMove {
                id: PointerId(2),
                pos: end,
            },
            RawInput::PointerUp {
                id: PointerId(2),
                pos: end,
            },
        ] {
            // Hold the strip still before releasing to select a specific visible row.
            now += if matches!(event, RawInput::PointerUp { .. }) {
                160
            } else {
                16
            };
            app.step(InstantMs(now), Some(event), true).unwrap();
        }
        assert!(app.engine().model().samples.offset >= 100);
        assert!(
            app.engine()
                .active_tree()
                .nodes
                .iter()
                .filter(|n| n.occupied && (2000..3000).contains(&n.key.0))
                .count()
                <= 12
        );
        tap(&mut app, &mut now, 2008);
        assert_eq!(app.engine().model().samples.selected, 8);
        let list_offset = app.engine().model().samples.offset;
        tap(&mut app, &mut now, 701);
        tap(&mut app, &mut now, 702);
        assert_eq!(
            app.engine()
                .active_tree()
                .node_by_key(Key(720))
                .unwrap()
                .scroll_y,
            list_offset
        );
        assert_eq!(app.engine().model().samples.selected, 8);
        tap(&mut app, &mut now, 703);
        assert_eq!(app.engine().model().samples.page, SamplePage::Graphics);
        tap(&mut app, &mut now, 734);
        assert_eq!(app.engine().model().samples.angle, 24);
        tap(&mut app, &mut now, 735);
        assert!(app.engine().model().samples.glass);
        #[cfg(feature = "enhanced")]
        {
            let at = center(&app, 732);
            let start = Point::new(at.x - 18, at.y);
            let right = Point::new(at.x + 18, at.y);
            for event in [
                RawInput::PointerDown {
                    id: PointerId(2),
                    pos: start,
                },
                RawInput::PointerDown {
                    id: PointerId(3),
                    pos: right,
                },
                RawInput::PointerMove {
                    id: PointerId(3),
                    pos: Point::new(right.x + 15, right.y + 12),
                },
                RawInput::PointerUp {
                    id: PointerId(3),
                    pos: Point::new(right.x + 15, right.y + 12),
                },
                RawInput::PointerUp {
                    id: PointerId(2),
                    pos: start,
                },
            ] {
                now += 16;
                app.step(InstantMs(now), Some(event), true).unwrap();
            }
            assert!(app.engine().model().samples.scale > 85);
            assert_ne!(app.engine().model().samples.angle, 24);
        }
        tap(&mut app, &mut now, 705);
        for (i, id) in (772..=777).enumerate() {
            tap(&mut app, &mut now, id);
            assert_eq!(app.engine().model().samples.last_button, Some(i));
        }
        tap(&mut app, &mut now, 778);
        assert_eq!(app.engine().model().samples.last_button, Some(5));
        for (index, key) in (781..=791).enumerate() {
            tap(&mut app, &mut now, key);
            assert_eq!(app.engine().model().samples.icon, index);
        }
        tap(&mut app, &mut now, 704);
        assert_eq!(app.engine().model().samples.page, SamplePage::Keyboard);
        let keyboard = app
            .engine()
            .active_tree()
            .node_by_key(Key(741))
            .unwrap()
            .layout_rect;
        let q = app
            .engine()
            .model()
            .samples
            .ime
            .key_rect(
                &liuli_widget_demo::showcase::samples::DICT,
                keyboard.size(),
                "q",
            )
            .unwrap();
        let pos = Point::new(
            keyboard.x + q.x + q.w as i32 / 2,
            keyboard.y + q.y + q.h as i32 / 2,
        );
        for event in [
            RawInput::PointerDown {
                id: PointerId(1),
                pos,
            },
            RawInput::PointerUp {
                id: PointerId(1),
                pos,
            },
        ] {
            now += 16;
            app.step(InstantMs(now), Some(event), true).unwrap();
        }
        assert_eq!(app.engine().model().samples.editor.text(), "q");
        #[cfg(feature = "enhanced")]
        {
            let pixels = app.screen().words().to_vec();
            tap(&mut app, &mut now, 744);
            assert_ne!(app.screen().words(), pixels.as_slice());
        }
    }
}

#[test]
fn gallery_button_focus_follows_the_button_corners() {
    for size in [Size::new(800, 480), Size::new(240, 536)] {
        let mut storage = Storage::new();
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            ShowcaseModel::for_size(size),
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        for (index, dir) in [NavDir::Next, NavDir::Next].into_iter().enumerate() {
            app.step(
                InstantMs((index as u64 + 1) * 16),
                Some(RawInput::Nav(dir)),
                true,
            )
            .unwrap();
            let key = app.engine().focus().focused_key().unwrap();
            let node = app.engine().active_tree().node_by_key(key).unwrap();
            assert!(
                node.props.radius >= 4,
                "expected a rounded gallery button at {key:?}"
            );
            let rect = node.layout_rect;
            let clip = node.clip_rect.intersect(app.engine().screen());
            let offset = (0..=u32::from(node.props.focus_ring().offset))
                .rev()
                .find(|&d| rect.expand(d).intersect(clip) == rect.expand(d))
                .expect("expected a fully visible gallery button");
            let ring = rect.expand(offset);
            assert!(app.engine().active_commands().as_slice().iter().any(|cmd| matches!(cmd,
                liuli::core::paint::PaintCmd::StrokeRoundedRect { rect: bounds, radius, color, .. }
                    if *bounds == ring && *radius == node.props.radius + offset as u8 && *color == node.props.focus_color
            )), "focus must use the button's radius and the available outward offset");
            let color = liuli::core::geometry::rgb565_to_u16(node.props.focus_color);
            let pixel = |x: i32, y: i32| app.screen().pixel(x as u32, y as u32);
            let right = ring.right64() as i32 - 1;
            let bottom = ring.bottom64() as i32 - 1;
            for (x, y) in [
                (ring.x, ring.y),
                (right, ring.y),
                (ring.x, bottom),
                (right, bottom),
            ] {
                assert_ne!(
                    pixel(x, y),
                    color,
                    "{size:?} {key:?} has a square focus corner at ({x},{y})"
                );
            }
            for (x, y) in [
                ((ring.x + right) / 2, ring.y),
                ((ring.x + right) / 2, bottom),
                (ring.x, (ring.y + bottom) / 2),
                (right, (ring.y + bottom) / 2),
            ] {
                assert_eq!(
                    pixel(x, y),
                    color,
                    "{size:?} {key:?} lost its focus edge at ({x},{y})"
                );
            }
        }
    }
}

#[test]
fn packaged_widget_traces_reach_the_same_final_state() {
    use liuli_widget_demo::showcase::SamplePage;
    for (size, trace) in [
        (
            Size::new(800, 480),
            include_str!("fixtures/h743-widgets.trace"),
        ),
        (
            Size::new(240, 536),
            include_str!("fixtures/portrait-widgets.trace"),
        ),
    ] {
        let mut storage = Storage::new();
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            ShowcaseModel::for_size(size),
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        #[cfg(feature = "enhanced")]
        app.engine_mut().set_layer_budget(1024 * 1024).unwrap();
        liuli_sim::preview::replay_trace(&mut app, trace).unwrap();
        let m = app.engine().model();
        assert_eq!(m.tab, ShowcaseTab::Widgets);
        assert_eq!(m.samples.page, SamplePage::Keyboard);
        assert_eq!(m.samples.selected, 8);
        assert_eq!(m.samples.angle, 24);
        assert!(m.samples.glass);
        assert_eq!(m.samples.last_button, Some(5));
        assert_eq!(m.samples.editor.text(), "q");
    }
}

#[test]
fn gallery_replays_every_preset_including_final_damage_and_detachment() {
    let mut storage = Storage::new();
    let model = ShowcaseModel {
        live_sample: false,
        ..ShowcaseModel::new()
    };
    let mut app = AppSession::<ShowcaseApp>::new(
        &mut storage,
        model,
        showcase_fonts().unwrap(),
        Size::new(800, 480),
        InstantMs(0),
        true,
    )
    .unwrap();
    #[cfg(feature = "enhanced")]
    app.engine_mut().set_layer_budget(1024 * 1024).unwrap();
    liuli_sim::preview::replay_trace(&mut app, include_str!("fixtures/h743-motion.trace")).unwrap();
    assert_eq!(app.engine().model().preview.kind(), TransitionKind::Flip);
    let mut now = 1836;
    let stage = app
        .engine()
        .active_tree()
        .node_by_key(Key(474))
        .unwrap()
        .visible_rect;
    let crop = |app: &AppSession<'_, '_, ShowcaseApp>| {
        app.screen()
            .words()
            .chunks_exact(800)
            .skip(stage.y as usize)
            .take(stage.h as usize)
            .flat_map(|row| {
                row[stage.x as usize..stage.right64() as usize]
                    .iter()
                    .copied()
            })
            .collect::<Vec<_>>()
    };
    for kind in TransitionKind::ALL {
        tap(&mut app, &mut now, 473);
        assert_eq!(app.engine().model().preview.kind(), kind);
        let end = now + 720;
        now += 96;
        app.step(InstantMs(now), None, true).unwrap();
        let moving = crop(&app);
        while now < end {
            now = (now + 16).min(end);
            app.step(InstantMs(now), None, true).unwrap();
        }
        assert!(moving != crop(&app), "{kind:?} had no visible effect");
        assert_eq!(app.engine().wakeup(InstantMs(now)), NextWakeup::OnEvent);
        assert_eq!(
            app.engine().model().preview.frame(),
            liuli_widgets::TransitionFrame::IDENTITY
        );
    }
    tap(&mut app, &mut now, 472);
    tap(&mut app, &mut now, 11);
    assert!(!app.engine().model().preview.is_active(InstantMs(now)));
}

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

// Every step checks incremental rendering against the full-frame reference.
fn settle_strip(app: &mut AppSession<'_, '_, ShowcaseApp>, clock: &mut u64, direction: i32) {
    let card_x = |app: &AppSession<'_, '_, ShowcaseApp>| {
        app.engine()
            .active_tree()
            .node_by_key(Key(452))
            .unwrap()
            .layout_rect
            .x
    };
    let initial = app.engine().model().scroll_physics.offset();
    let origin = card_x(app) + initial;
    let mut previous = initial;
    let mut previous_speed = i32::MAX;
    for _ in 0..128 {
        if !app.engine().model().scroll_physics.is_active() {
            return;
        }
        *clock += 16;
        app.step(InstantMs(*clock), None, true).unwrap();
        let offset = app.engine().model().scroll_physics.offset();
        assert_eq!(card_x(app), origin - offset);
        let speed = (offset - previous) * direction;
        assert!(
            (0..=previous_speed.saturating_add(1)).contains(&speed),
            "arrow from {initial} at {clock} ms: {previous} -> {offset}, previous step {previous_speed} px"
        );
        if !app.engine().model().scroll_physics.is_active() {
            assert!(speed <= 1, "last frame moved {speed} px");
        }
        previous = offset;
        previous_speed = speed;
    }
    panic!("strip animation did not settle");
}

#[test]
fn strip_continues_after_a_fast_finger_release_in_mid_content() {
    use liuli_sim::preview::replay_trace;
    let trace = include_str!("fixtures/h743-strip-fling.trace");
    let (drag, ticks) = trace.split_once("588 tick").unwrap();
    for reduced in [false, true] {
        let mut storage = Storage::new();
        let size = Size::new(800, 480);
        let mut model = ShowcaseModel::for_size(size);
        model.motion.reduced = reduced;
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            model,
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        replay_trace(&mut app, drag).unwrap();
        let offset = |app: &AppSession<'_, '_, ShowcaseApp>| {
            app.engine()
                .active_tree()
                .node_by_key(Key(450))
                .unwrap()
                .scroll_y
        };
        let released = offset(&app);
        app.step(InstantMs(588), None, true).unwrap();
        if reduced {
            assert_eq!(released, 158);
            assert_eq!(offset(&app), released);
        } else {
            assert_eq!(released, 205);
            assert!(offset(&app) > released);
        }
        replay_trace(&mut app, &format!("liuli-trace 1{ticks}")).unwrap();
        assert_eq!(offset(&app), if reduced { 158 } else { 288 });
        assert!(!app.engine().model().scroll_physics.is_active());
    }
}

#[test]
fn layout_strip_drag_and_arrow_share_bounds_and_pixels() {
    for (width, height) in [(800, 480), (1040, 480), (1120, 480), (240, 536)] {
        let size = Size::new(width, height);
        let mut storage = Storage::new();
        let model = ShowcaseModel {
            live_sample: false,
            ..ShowcaseModel::for_size(size)
        };
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            model,
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        let mut clock = 0;
        tap(&mut app, &mut clock, 14);
        clock += 500;
        app.step(InstantMs(clock), None, true).unwrap();
        let strip = app.engine().active_tree().node_by_key(Key(450)).unwrap();
        let r = strip.layout_rect;
        let last_card = app
            .engine()
            .active_tree()
            .node_by_key(Key(457))
            .unwrap()
            .layout_rect;
        let limit = (last_card.right64() + i64::from(strip.props.padding.right) - r.right64())
            .max(0) as i32;
        let snapshot = |app: &AppSession<'_, '_, ShowcaseApp>| {
            app.screen()
                .words()
                .chunks_exact(width as usize)
                .skip(r.y as usize)
                .take(r.h as usize)
                .flat_map(|row| {
                    row[r.x as usize..r.x as usize + r.w as usize]
                        .iter()
                        .copied()
                })
                .collect::<Vec<_>>()
        };
        let check = |app: &AppSession<'_, '_, ShowcaseApp>, expected| {
            let tree = app.engine().active_tree();
            assert_eq!(tree.node_by_key(Key(450)).unwrap().scroll_y, expected);
            assert_eq!(app.engine().model().scroll_physics.offset(), expected);
            assert_eq!(
                tree.node_by_key(Key(457)).unwrap().layout_rect.x,
                last_card.x - expected
            );
        };
        let initial = snapshot(&app);
        let start = Point::new(r.x + r.w as i32 - 10, r.y + r.h as i32 / 2);
        let end = Point::new(r.x + 10, start.y);
        let id = PointerId(1);
        let down = RawInput::PointerDown { id, pos: start };
        let step = |app: &mut AppSession<'_, '_, ShowcaseApp>, clock: &mut u64, event| {
            *clock += 16;
            app.step(InstantMs(*clock), Some(event), true).unwrap();
        };
        let drag_distance = start.x - end.x;
        for _ in 0..(limit / drag_distance + 1) {
            for ev in [
                down,
                RawInput::PointerMove { id, pos: end },
                RawInput::PointerUp { id, pos: end },
            ] {
                step(&mut app, &mut clock, ev);
            }
        }
        check(&app, limit);
        let dragged = snapshot(&app);
        let snap_step = i32::from(app.engine().model().scroll_physics.snap_step);
        for (button, expected) in [(448, 0), (449, limit), (449, limit)] {
            for _ in 0..=(limit / snap_step + 1) {
                tap(&mut app, &mut clock, button);
                settle_strip(&mut app, &mut clock, if button == 448 { -1 } else { 1 });
            }
            check(&app, expected);
            assert_eq!(
                snapshot(&app),
                if expected == 0 { &initial } else { &dragged }.as_slice()
            );
        }

        if limit > 0 {
            tap(&mut app, &mut clock, 448);
            clock += 32;
            app.step(InstantMs(clock), None, true).unwrap();
            step(&mut app, &mut clock, down);
            let interrupted = app
                .engine()
                .active_tree()
                .node_by_key(Key(450))
                .unwrap()
                .scroll_y;
            assert!(interrupted < limit);
            clock += 240;
            app.step(InstantMs(clock), None, true).unwrap();
            check(&app, interrupted);
            let pos = Point::new(start.x - 12, start.y);
            step(&mut app, &mut clock, RawInput::PointerMove { id, pos });
            check(&app, (interrupted + 12).min(limit));
            step(&mut app, &mut clock, RawInput::PointerCancel { id });
        }
    }
}

#[test]
fn showcase_labels_fit_in_both_locales_at_panel_size() {
    use liuli::core::{layout::TextMeasurer, view::NodeKind};
    let mut clipped = Vec::new();
    for size in [Size::new(800, 480), Size::new(240, 536)] {
        for locale in [LocaleId(0), LocaleId(1)] {
            use liuli_widget_demo::showcase::SamplePage;
            for (tab, sample) in [
                (ShowcaseTab::Overview, SamplePage::Controls),
                (ShowcaseTab::Forms, SamplePage::Controls),
                (ShowcaseTab::Charts, SamplePage::Controls),
                (ShowcaseTab::Layout, SamplePage::Controls),
                (ShowcaseTab::Motion, SamplePage::Controls),
                (ShowcaseTab::Widgets, SamplePage::Controls),
                (ShowcaseTab::Widgets, SamplePage::Records),
                (ShowcaseTab::Widgets, SamplePage::Graphics),
                (ShowcaseTab::Widgets, SamplePage::Keyboard),
                (ShowcaseTab::Widgets, SamplePage::Buttons),
            ] {
                let mut model = ShowcaseModel::for_size(size);
                model.tab = tab;
                model.samples.page = sample;
                model.locale = locale;
                model.dropdown.open = true;
                let mut storage = Storage::new();
                let app = AppSession::<ShowcaseApp>::new(
                    &mut storage,
                    model,
                    showcase_fonts().unwrap(),
                    size,
                    InstantMs(0),
                    true,
                )
                .unwrap();
                let tree = app.engine().active_tree();
                for node in tree.nodes.iter().filter(|n| {
                    n.occupied
                        && matches!(n.kind, NodeKind::Button | NodeKind::Tab | NodeKind::Label)
                }) {
                    let p = &node.props;
                    let label = tree.text_str(p.text());
                    if label.is_empty() {
                        continue;
                    }
                    let size = app
                        .engine()
                        .fonts()
                        .measure_text(p.font(), label, 4096, 1, false, TextAlign::Start)
                        .size;
                    let width = node.layout_rect.w.saturating_sub(p.padding.horizontal());
                    let height = node.layout_rect.h.saturating_sub(p.padding.vertical());
                    let mut parent = node.parent.get_u16();
                    let mut in_scroll = false;
                    while let Some(slot) = parent {
                        let ancestor = &tree.nodes[slot as usize];
                        in_scroll |= matches!(ancestor.kind, NodeKind::ScrollX | NodeKind::ScrollY);
                        parent = ancestor.parent.get_u16();
                    }
                    if size.width > width
                        || size.height > height
                        || (!in_scroll
                            && node.layout_rect.intersect(node.clip_rect) != node.layout_rect)
                    {
                        clipped.push(format!(
                            "{tab:?}/{sample:?} {label:?}: {}x{} in {width}x{height}",
                            size.width, size.height
                        ));
                    }
                }
            }
        }
    }
    assert!(clipped.is_empty(), "{}", clipped.join("\n"));
}

#[test]
fn forms_text_events_edit_the_same_buffer_as_the_buttons() {
    let mut storage = Storage::new();
    let mut app = AppSession::<ShowcaseApp>::new(
        &mut storage,
        ShowcaseModel::new(),
        showcase_fonts().unwrap(),
        Size::new(800, 480),
        InstantMs(0),
        true,
    )
    .unwrap();
    let mut clock = 0;
    tap(&mut app, &mut clock, 12);
    clock += 320;
    app.step(InstantMs(clock), None, true).unwrap();
    tap(&mut app, &mut clock, 240);
    let initial = app.engine().model().editor.text().to_owned();
    for (input, suffix) in [
        (TextInput::Commit("界A"), "界A"),
        (TextInput::Left, "界A"),
        (TextInput::Commit("中"), "界中A"),
        (TextInput::Right, "界中A"),
        (TextInput::Backspace, "界中"),
    ] {
        clock += 16;
        app.text(InstantMs(clock), input, true).unwrap();
        assert_eq!(
            app.engine().model().editor.text(),
            format!("{initial}{suffix}")
        );
    }
    tap(&mut app, &mut clock, 245);
    assert_eq!(app.engine().model().editor.text(), format!("{initial}界"));
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
    let vp_rect = app
        .engine()
        .active_tree()
        .node_by_key(Key(49))
        .unwrap()
        .layout_rect;
    let body_up = app.engine().active_tree().node_by_key(Key(50)).unwrap();
    assert_eq!(i64::from(body_up.layout_rect.y), vp_rect.bottom64());
    assert!(body_up.visible_rect.is_empty());
    clock += 32;
    app.step(InstantMs(clock), None, true).unwrap();
    let body_up = app.engine().active_tree().node_by_key(Key(50)).unwrap();
    assert!(!body_up.visible_rect.is_empty() && body_up.visible_rect.h < vp_rect.h);
    assert_eq!(body_up.visible_rect, body_up.ink_rect.intersect(vp_rect));
    assert!(body_up.visible_rect.y >= vp_rect.y);
    assert!(body_up.visible_rect.bottom64() <= vp_rect.bottom64());
    for _ in 0..18 {
        clock += 16;
        app.step(InstantMs(clock), None, true).unwrap();
    }
    assert_eq!(
        app.engine()
            .active_tree()
            .node_by_key(Key(50))
            .unwrap()
            .layout_rect,
        vp_rect
    );

    tap(&mut app, &mut clock, 132); // 2nd tap -> Fade (must not blank screen at t=0)
    assert_eq!(app.engine().model().trans_kind, TransitionKind::Fade);
    let alpha0 = app.engine().model().transition.alpha_for(InstantMs(clock));
    assert!(
        alpha0 >= 3,
        "fade opacity at frame 0 must stay visible, got {alpha0}"
    );
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
        app.engine()
            .active_tree()
            .node_by_key(Key(50))
            .unwrap()
            .layout_rect,
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
    let grid = app.engine().active_tree().node_by_key(Key(420)).unwrap();
    let cell_width = (grid.layout_rect.w - 3 * u32::from(grid.props.gap)) / 4;
    for key in 421..=428 {
        let tile = app.engine().active_tree().node_by_key(Key(key)).unwrap();
        assert!(
            tile.layout_rect.w >= cell_width,
            "tile {key} leaves its grid cell empty"
        );
        assert!(
            app.engine()
                .active_tree()
                .text_cache(usize::from(
                    app.engine()
                        .active_tree()
                        .find_slot_by_key(Key(key))
                        .unwrap()
                ))
                .size
                .width
                + 16
                <= tile.layout_rect.w
        );
        let pos = Point::new(
            tile.layout_rect.x + cell_width as i32 - 6,
            tile.layout_rect.y + 23,
        );
        for input in [
            RawInput::PointerDown {
                id: PointerId(1),
                pos,
            },
            RawInput::PointerUp {
                id: PointerId(1),
                pos,
            },
        ] {
            clock += 16;
            app.step(InstantMs(clock), Some(input), true).unwrap();
        }
        assert_eq!(app.engine().model().grid_selected, (key - 421) as usize);
    }
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
        "the viewport must scroll its cards left ({card0_x_before} -> {card0_x_after})"
    );
}

#[test]
fn scoped_widget_motion_replays_on_both_screen_sizes() {
    use liuli_widgets::Motion;
    for size in [Size::new(240, 536), Size::new(800, 480)] {
        let model = ShowcaseModel {
            live_sample: false,
            ..ShowcaseModel::for_size(size)
        };
        let mut storage = Storage::new();
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            model,
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        #[cfg(feature = "enhanced")]
        app.engine_mut().set_layer_budget(1024 * 1024).unwrap();
        let mut now = 0;
        tap(&mut app, &mut now, 19);
        now += 320;
        app.step(InstantMs(now), None, true).unwrap();
        assert_eq!(app.engine().model().tab, ShowcaseTab::Motion);
        let pixels = |app: &AppSession<'_, '_, ShowcaseApp>| {
            let n = app.engine().active_tree().node_by_key(Key(606)).unwrap();
            let r = n.layout_rect;
            let y = (r.y + r.h as i32 / 2) as usize;
            app.screen().words()[y * size.width as usize + r.x as usize
                ..y * size.width as usize + r.right64() as usize]
                .iter()
                .filter(|&&p| p == rgb565_to_u16(n.props.accent))
                .count()
        };
        let empty = pixels(&app);
        tap(&mut app, &mut now, 605);
        assert!(app.engine().model().indicator_on);
        now += 160;
        app.step(InstantMs(now), None, true).unwrap();
        let partial = pixels(&app);
        assert!(partial > empty);
        // The switch has settled, while the external control's local 480ms linear curve is still running.
        let switch = |app: &AppSession<'_, '_, ShowcaseApp>| {
            let r = app
                .engine()
                .active_tree()
                .node_by_key(Key(605))
                .unwrap()
                .layout_rect;
            let y = (r.y + r.h as i32 / 2) as usize * size.width as usize;
            app.screen().words()[y + r.x as usize..y + r.right64() as usize].to_vec()
        };
        let on = switch(&app);
        now += 48;
        app.step(InstantMs(now), None, true).unwrap();
        assert_eq!(switch(&app), on);
        assert!(pixels(&app) > partial);
        tap(&mut app, &mut now, 606);
        assert!(!app.engine().model().indicator_on);
        let reversed = pixels(&app);
        now += 48;
        app.step(InstantMs(now), None, true).unwrap();
        assert!(pixels(&app) < reversed && pixels(&app) > empty);
        tap(&mut app, &mut now, 603);
        assert_eq!(pixels(&app), empty);
        assert_eq!(app.engine().model().motion, Motion::REDUCED);
        assert_eq!(app.engine().wakeup(InstantMs(now)), NextWakeup::OnEvent);
        tap(&mut app, &mut now, 606);
        let full = pixels(&app);
        assert!(full > partial * 2 && switch(&app) == on);
        assert_eq!(app.engine().wakeup(InstantMs(now)), NextWakeup::OnEvent);
        tap(&mut app, &mut now, 473);
        assert_eq!(
            app.engine().model().preview.next_wakeup(InstantMs(now)),
            NextWakeup::OnEvent
        );
        tap(&mut app, &mut now, 604);
        tap(&mut app, &mut now, 602);
        tap(&mut app, &mut now, 605);
        now += 90;
        app.step(InstantMs(now), None, true).unwrap();
        assert_eq!(pixels(&app), empty);
        tap(&mut app, &mut now, 612);
        assert_eq!(app.engine().model().level, 85);
        now += 120;
        app.step(InstantMs(now), None, true).unwrap();
        assert_eq!(
            app.engine()
                .active_tree()
                .node_by_key(Key(611))
                .unwrap()
                .props
                .value,
            85
        );
        if size.width < 600 {
            for key in [11, 12, 13, 14] {
                tap(&mut app, &mut now, key);
                now += 350;
                app.step(InstantMs(now), None, true).unwrap();
            }
            tap(&mut app, &mut now, 424);
            assert_eq!(app.engine().model().grid_selected, 3);
        }
        drop(app);
        let model = ShowcaseModel {
            live_sample: false,
            ..ShowcaseModel::for_size(size)
        };
        let mut session = AppSession::<ShowcaseApp>::new(
            &mut storage,
            model,
            showcase_fonts().unwrap(),
            size,
            InstantMs(0),
            true,
        )
        .unwrap();
        let trace = if size.width < 600 {
            include_str!("fixtures/portrait-motion.trace")
        } else {
            include_str!("fixtures/h743-style.trace")
        };
        liuli_sim::preview::replay_trace(&mut session, trace).unwrap();
        let result = session.engine().model();
        assert!(result.tab == ShowcaseTab::Motion && result.indicator_on && result.motion.reduced);
        assert_eq!(result.preview.kind(), TransitionKind::SlideRight);
        assert_eq!(pixels(&session), full);
        assert_eq!(
            session.engine().wakeup(InstantMs(1000)),
            NextWakeup::OnEvent
        );
    }
}

#[test]
fn custom_style_curve_drives_the_same_switch_without_an_app_timer() {
    static HOLD: StyleCurve =
        StyleCurve::Samples(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1000]);
    let mut storage = Storage::new();
    let model = ShowcaseModel {
        tab: ShowcaseTab::Motion,
        live_sample: false,
        local_slow: false,
        motion: liuli_widgets::Motion::STANDARD
            .with_state_ms(320)
            .with_state_curve(&HOLD),
        ..ShowcaseModel::new()
    };
    let mut app = AppSession::<ShowcaseApp>::new(
        &mut storage,
        model,
        showcase_fonts().unwrap(),
        Size::new(800, 480),
        InstantMs(0),
        true,
    )
    .unwrap();
    let rect = app
        .engine()
        .active_tree()
        .node_by_key(Key(606))
        .unwrap()
        .layout_rect;
    let p = Point::new(rect.x + rect.w as i32 / 2, rect.y + rect.h as i32 / 2);
    let sample = |s: &AppSession<'_, '_, ShowcaseApp>| s.screen().pixel(p.x as u32, p.y as u32);
    let off = sample(&app);
    let mut clock = 0;
    tap(&mut app, &mut clock, 606);
    app.step(InstantMs(clock + 288), None, true).unwrap();
    assert!(app.engine().model().indicator_on);
    assert_eq!(sample(&app), off);
    app.step(InstantMs(clock + 320), None, true).unwrap();
    assert_ne!(sample(&app), off);
    assert_eq!(
        app.engine().wakeup(InstantMs(clock + 320)),
        NextWakeup::OnEvent
    );
}
