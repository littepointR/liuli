#![cfg(feature = "desktop")]
use liuli::prelude::*;
use liuli_sim::session::{AppSession, SessionOptions};
use liuli_widget_demo::showcase::{ShowcaseApp, ShowcaseModel, showcase_fonts};

const WHITE: Rgb565 = Rgb565::new(31, 63, 31);
const BLUE: Rgb565 = Rgb565::new(0, 0, 31);
const RED: Rgb565 = Rgb565::new(31, 0, 0);
const TRI: PreparedPath<3> = match PreparedPath::prepare(
    &[
        PathVerb::Move(0, 0),
        PathVerb::Line(1024, 0),
        PathVerb::Line(0, 1024),
        PathVerb::Close,
    ],
    1,
) {
    Ok(v) => v,
    Err(_) => panic!("triangle"),
};
const PATH: Path<'static> = TRI.path();
const TRIANGLE: WidgetGeometry = WidgetGeometry {
    hit: HitArea::Outline,
    ..WidgetGeometry::new(Outline::Path(&PATH, FillRule::NonZero))
};
const fn polygon<const N: usize>(verbs: &[PathVerb]) -> PreparedPath<N> {
    match PreparedPath::prepare(verbs, 1) {
        Ok(p) => p,
        Err(_) => panic!("invalid test path"),
    }
}
const NESTED: PreparedPath<8> = polygon(&[
    PathVerb::Move(0, 0),
    PathVerb::Line(1024, 0),
    PathVerb::Line(1024, 1024),
    PathVerb::Line(0, 1024),
    PathVerb::Close,
    PathVerb::Move(256, 256),
    PathVerb::Line(768, 256),
    PathVerb::Line(768, 768),
    PathVerb::Line(256, 768),
    PathVerb::Close,
]);
const ELBOW: PreparedPath<6> = polygon(&[
    PathVerb::Move(0, 0),
    PathVerb::Line(1024, 0),
    PathVerb::Line(1024, 256),
    PathVerb::Line(256, 256),
    PathVerb::Line(256, 1024),
    PathVerb::Line(0, 1024),
    PathVerb::Close,
]);
const NESTED_PATH: Path<'static> = NESTED.path();
const ELBOW_PATH: Path<'static> = ELBOW.path();
const fn shape_hit(outline: Outline) -> WidgetGeometry {
    WidgetGeometry {
        hit: HitArea::Outline,
        ..WidgetGeometry::new(outline)
    }
}
const WITH_HOLE: WidgetGeometry = shape_hit(Outline::Path(&NESTED_PATH, FillRule::EvenOdd));
const SOLID: WidgetGeometry = shape_hit(Outline::Path(&NESTED_PATH, FillRule::NonZero));
const CONCAVE: WidgetGeometry = shape_hit(Outline::Path(&ELBOW_PATH, FillRule::NonZero));
const BOX_HIT: WidgetGeometry = WidgetGeometry::new(TRIANGLE.outline);
const PART: FocusPart = FocusPart {
    part: WidgetPart::Content,
    bounds: PartBounds::Relative {
        left: 256,
        top: 256,
        right: 768,
        bottom: 768,
    },
    outline: Outline::Capsule,
};
const COMPOUND: WidgetGeometry = WidgetGeometry {
    focus: FocusTarget::Part(&PART),
    ..WidgetGeometry::SURFACE
};
const COLLAPSED: WidgetGeometry = WidgetGeometry {
    focus: FocusTarget::Part(&FocusPart {
        bounds: PartBounds::Inset(EdgeInsets::all(100)),
        outline: TRIANGLE.outline,
        ..PART
    }),
    ..WidgetGeometry::SURFACE
};
const CLIP: SurfaceComposition = SurfaceComposition {
    clip_content: true,
    ..SurfaceComposition::DEFAULT
};
struct ThirdParty;
impl App for ThirdParty {
    type Model = u8;
    type Msg = u8;
    fn update(m: &mut u8, next: u8, _: InstantMs) -> Change {
        *m = next;
        Change::View
    }
    fn on_action(_: &u8, _: UiAction) -> Option<u8> {
        None
    }
    fn view(m: &u8) -> impl Scene {
        layout(
            Key(0),
            NodeProps::new().bg(WHITE).padding(EdgeInsets::all(10)),
            move |ui| {
                let geom = match m {
                    0 | 3 => &TRIANGLE,
                    1 => &WidgetGeometry::DIAL,
                    4 => &WITH_HOLE,
                    5 => &SOLID,
                    6 => &CONCAVE,
                    7 => &COMPOUND,
                    8 => &COLLAPSED,
                    _ => &BOX_HIT,
                };
                ui.button(
                    Key(1),
                    NodeProps {
                        width: Length::Px(80),
                        height: Length::Px(40),
                        focusable: true,
                        clickable: true,
                        focus_color: RED,
                        focus_ring: FocusRingStyle::DEFAULT.with_gap(3),
                        bg: Some(BLUE),
                        geometry: Some(geom),
                        ..NodeProps::new()
                    },
                )?;
                if (4..=6).contains(m) {
                    ui.button(
                        Key(2),
                        NodeProps {
                            focusable: true,
                            ..NodeProps::new().size(Length::Px(20), Length::Px(12))
                        },
                    )?;
                }
                if *m == 3 {
                    ui.decorate(Key(1), SurfaceStyle::FLAT.with_composition(&CLIP))?
                }
                Ok(())
            },
        )
    }
}
fn keypad_session(
    storage: &mut Storage,
    model: u8,
    height: u32,
) -> AppSession<'_, 'static, ThirdParty> {
    AppSession::with_options(
        storage,
        model,
        showcase_fonts().unwrap(),
        Size::new(110, height),
        InstantMs(0),
        SessionOptions {
            initial_focus: InitialFocusRing::Visible,
            verify_ref: true,
            on_frame: None,
        },
    )
    .unwrap()
}
#[test]
fn third_party_focus_uses_device_pixel_offsets_and_independent_hit_policy() {
    let mut storage = Storage::new();
    let mut app = keypad_session(&mut storage, 0, 70);
    let rect = app
        .engine()
        .active_tree()
        .node_by_key(Key(1))
        .unwrap()
        .layout_rect;
    assert_eq!(rect.size(), Size::new(80, 40));
    let pixel = |app: &AppSession<'_, '_, ThirdParty>, x, y| {
        app.screen().pixel((rect.x + x) as u32, (rect.y + y) as u32)
    };
    let hit = |app: &AppSession<'_, '_, ThirdParty>, x, y| {
        let tree = app.engine().active_tree();
        tree.find_slot_by_key(Key(1))
            == liuli::core::focus::hit_test_in_scope(
                tree,
                tree.root.get_u16().unwrap(),
                Point::new(rect.x + x, rect.y + y),
            )
    };
    for mode in [0, 3] {
        app.message(InstantMs(10 + u64::from(mode)), mode, true)
            .unwrap();
        assert_eq!(pixel(&app, 20, -5), rgb565_to_u16(RED));
        assert_eq!(pixel(&app, 20, -2), rgb565_to_u16(WHITE));
        // Hypotenuse x+2y=80. (56.5,16.5) is about 4.25 px outside.
        assert_eq!(pixel(&app, 56, 16), rgb565_to_u16(RED));
        assert_eq!(pixel(&app, 50, 16), rgb565_to_u16(WHITE));
        assert_eq!(pixel(&app, 60, 16), rgb565_to_u16(WHITE));
        assert!(!hit(&app, 76, 32) && hit(&app, 6, 6));
    }
    app.message(InstantMs(20), 1, true).unwrap();
    assert_eq!(pixel(&app, 20, -5), rgb565_to_u16(WHITE));
    assert_eq!(pixel(&app, 40, -5), rgb565_to_u16(RED));
    assert_eq!(pixel(&app, 2, 20), rgb565_to_u16(WHITE));
    assert!(!hit(&app, 2, 20) && hit(&app, 40, 20));
    app.message(InstantMs(30), 2, true).unwrap();
    assert!(hit(&app, 76, 32));
}
#[test]
fn holes_and_concave_edges_keep_the_ring_gap_and_clear_on_focus_change() {
    let mut storage = Storage::new();
    let mut app = keypad_session(&mut storage, 4, 80);
    let rect = app
        .engine()
        .active_tree()
        .node_by_key(Key(1))
        .unwrap()
        .layout_rect;
    let point = |x, y| Point::new(rect.x + x, rect.y + y);
    let pixel = |app: &AppSession<'_, '_, ThirdParty>, x, y| {
        let p = point(x, y);
        app.screen().pixel(p.x as u32, p.y as u32)
    };
    let hits_body = |app: &AppSession<'_, '_, ThirdParty>, x, y| {
        let t = app.engine().active_tree();
        t.find_slot_by_key(Key(1))
            == liuli::core::focus::hit_test_in_scope(t, t.root.get_u16().unwrap(), point(x, y))
    };
    // The inner contour begins at (20,10). Both contours have the same winding.
    assert_eq!(pixel(&app, 24, 20), rgb565_to_u16(RED));
    assert_eq!(pixel(&app, 21, 20), rgb565_to_u16(WHITE));
    assert!(!hits_body(&app, 24, 20));
    app.message(InstantMs(10), 5, true).unwrap();
    assert_eq!(pixel(&app, 24, 20), rgb565_to_u16(BLUE));
    assert!(hits_body(&app, 24, 20));
    app.message(InstantMs(20), 6, true).unwrap();
    assert_eq!(pixel(&app, 24, 20), rgb565_to_u16(RED));
    assert_eq!(pixel(&app, 35, 14), rgb565_to_u16(RED));
    assert_eq!(pixel(&app, 21, 11), rgb565_to_u16(WHITE));
    assert!(!hits_body(&app, 24, 20));
    liuli_sim::preview::replay_trace(&mut app, "liuli-trace 1\n30 kind=next\n").unwrap();
    assert_eq!(app.engine().focus().focused_key(), Some(Key(2)));
    assert_eq!(pixel(&app, 24, 20), rgb565_to_u16(WHITE));
    liuli_sim::preview::replay_trace(&mut app, "liuli-trace 1\n40 kind=prev\n").unwrap();
    assert_eq!(pixel(&app, 24, 20), rgb565_to_u16(RED));
}

#[test]
fn compound_control_resolves_its_part_after_layout_and_handles_an_empty_part() {
    let mut storage = Storage::new();
    let mut app = keypad_session(&mut storage, 7, 80);
    let t = app.engine().active_tree();
    let node = t.node_by_key(Key(1)).unwrap();
    let rect = node.layout_rect;
    let ring = t.focus_outline(Key(1)).unwrap();
    assert_eq!(ring.body, Rect::new(rect.x + 20, rect.y + 10, 40, 20));
    assert_eq!(ring.shape, Outline::Capsule);
    assert_eq!(
        app.screen()
            .pixel((rect.x + 40) as u32, (rect.y + 5) as u32),
        rgb565_to_u16(RED)
    );
    let hit = liuli::core::focus::hit_test_in_scope(
        t,
        t.root.get_u16().unwrap(),
        Point::new(rect.x + 1, rect.y + 1),
    );
    assert_eq!(hit, t.find_slot_by_key(Key(1)));
    app.message(InstantMs(10), 8, true).unwrap();
    assert!(
        app.engine()
            .active_tree()
            .focus_outline(Key(1))
            .unwrap()
            .body
            .is_empty()
    );
    for y in 0..80 {
        for x in 0..110 {
            assert_ne!(app.screen().pixel(x, y), rgb565_to_u16(RED));
        }
    }
}

#[test]
fn widget_shapes_do_not_use_effect_slots() {
    struct Buttons;
    impl App for Buttons {
        type Model = ();
        type Msg = ();
        fn update(_: &mut (), _: (), _: InstantMs) -> Change {
            Change::View
        }
        fn on_action(_: &(), _: UiAction) -> Option<()> {
            None
        }
        fn view(_: &()) -> impl Scene {
            layout(Key(0), NodeProps::new(), move |ui| {
                for key in 1..=20 {
                    ui.button(
                        Key(key),
                        NodeProps::new()
                            .size(Length::Px(10), Length::Px(8))
                            .geometry(&WidgetGeometry::DIAL),
                    )?
                }
                Ok(())
            })
        }
    }
    let mut storage = Storage::new();
    let app = AppSession::<Buttons>::new(
        &mut storage,
        (),
        showcase_fonts().unwrap(),
        Size::new(40, 180),
        InstantMs(0),
        true,
    )
    .unwrap();
    assert!(app.engine().active_tree().effects.iter().is_empty());
    assert!(app.engine().active_tree().node_by_key(Key(20)).is_some());
}
#[test]
fn startup_focus_follows_input_and_keeps_the_selected_tab_indicator() {
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
        let tab_key = Key(11);
        assert_eq!(app.engine().focus().focused_key(), Some(tab_key));
        assert!(!app.engine().focus().ring_visible);
        let trace = include_str!("fixtures/startup-focus.trace");
        let records: Vec<_> = trace.lines().skip(1).collect();
        let expected_visibility = [false, true, true, false, false];
        assert_eq!(records.len(), expected_visibility.len());
        for (i, (event, visible)) in records.iter().zip(expected_visibility).enumerate() {
            liuli_sim::preview::replay_trace(&mut app, &format!("liuli-trace 1\n{event}\n"))
                .unwrap();
            let focus = app.engine().focus();
            let tree = app.engine().active_tree();
            let key = focus.focused_key().unwrap();
            assert_eq!(focus.ring_visible, visible, "{size:?} after {event}");
            assert_eq!(key == tab_key, i != 1, "{size:?} after {event}");
            let ring = tree.focus_outline(key).unwrap().bounds;
            let node = tree.node_by_key(key).unwrap();
            let color = tree
                .part_style(key)
                .focused_border()
                .unwrap_or(node.props.focus_color);
            assert_eq!(
                app.screen()
                    .pixel(ring.x as u32 + ring.w / 2, ring.y as u32)
                    == rgb565_to_u16(color),
                visible,
                "{size:?} painted focus after {event}"
            );
            let tab = tree.node_by_key(tab_key).unwrap();
            assert!(tab.props.checked());
            let style = tree.part_style(tab_key);
            let underline = style.indicator_layout();
            let rect = tab.layout_rect;
            let y = rect.bottom64() as u32 - u32::from(underline.edge_offset) - 2;
            assert_eq!(
                app.screen().pixel(rect.x as u32 + rect.w / 2, y),
                rgb565_to_u16(style.indicator_color().unwrap_or(tab.props.accent)),
                "{size:?} selected tab indicator after {event}"
            );
        }
    }
}

#[test]
fn demo_dial_replay() {
    let mut storage = Storage::new();
    let size = Size::new(800, 480);
    let mut app = AppSession::<ShowcaseApp>::new(
        &mut storage,
        ShowcaseModel::for_size(size),
        showcase_fonts().unwrap(),
        size,
        InstantMs(0),
        true,
    )
    .unwrap();
    let trace = include_str!("fixtures/h743-focus-shapes.trace");
    let (before, after) = trace.split_once("1200 kind=prev").unwrap();
    liuli_sim::preview::replay_trace(&mut app, before).unwrap();
    assert_eq!(app.engine().focus().focused_key(), Some(Key(712)));
    let slider = app.engine().active_tree().node_by_key(Key(712)).unwrap();
    let track = app
        .engine()
        .active_tree()
        .focus_outline(Key(712))
        .unwrap()
        .body;
    assert_eq!(track.w, slider.layout_rect.w);
    assert!(track.h < slider.layout_rect.h);
    liuli_sim::preview::replay_trace(&mut app, &format!("liuli-trace 1\n1200 kind=prev{after}"))
        .unwrap();
    assert_eq!(app.engine().focus().focused_key(), Some(Key(710)));
    let dial = app.engine().active_tree().node_by_key(Key(710)).unwrap();
    let ring = app.engine().active_tree().focus_outline(Key(710)).unwrap();
    assert_eq!(ring.shape, Outline::Circle);
    let color = rgb565_to_u16(dial.props.focus_color);
    let r = ring.bounds;
    let pixel = |x: i32, y: i32| app.screen().pixel(x as u32, y as u32);
    for (x, y) in [
        (r.x, r.y),
        (r.right64() as i32 - 1, r.y),
        (r.x, r.bottom64() as i32 - 1),
        (r.right64() as i32 - 1, r.bottom64() as i32 - 1),
    ] {
        assert_ne!(pixel(x, y), color);
    }
    assert_eq!(pixel(r.x + (r.w / 2) as i32, r.y), color);
    assert_eq!(pixel(r.x, r.y + (r.h / 2) as i32), color);
}
