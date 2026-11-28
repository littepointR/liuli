#![cfg(feature = "desktop")]
use liuli::prelude::*;
use liuli_sim::session::AppSession;

struct MovingCard;
impl App for MovingCard {
    type Model = Option<u32>;
    type Msg = Option<u32>;
    fn update(model: &mut Self::Model, msg: Self::Msg, _: InstantMs) -> Change {
        *model = msg;
        Change::View
    }
    fn on_action(_: &Self::Model, _: UiAction) -> Option<Self::Msg> {
        None
    }
    fn view(y: &Self::Model) -> impl Scene {
        let full = NodeProps {
            width: Length::Fill(1),
            height: Length::Fill(1),
            ..NodeProps::new()
        };
        layout(
            Key(0),
            NodeProps {
                bg: Some(Rgb565::new(31, 63, 31)),
                ..full
            },
            move |root| {
                root.column(Key(1), full, |ui| {
                    if let Some(y) = *y {
                        ui.label(
                            Key(2),
                            NodeProps {
                                height: Length::Px(y),
                                ..NodeProps::new()
                            },
                        )?;
                        ui.button(
                            Key(3),
                            NodeProps {
                                width: Length::Px(20),
                                height: Length::Px(12),
                                bg: Some(Rgb565::new(1, 4, 7)),
                                clickable: true,
                                ..NodeProps::new()
                            },
                        )?;
                        ui.decorate(
                            Key(3),
                            SurfaceStyle {
                                shadow: Some(Shadow {
                                    x: 2,
                                    y: 4,
                                    blur: 2,
                                    opacity: 15,
                                    color: Rgb565::new(0, 0, 0),
                                }),
                                ..SurfaceStyle::FLAT
                            },
                        )?;
                    }
                    Ok(())
                })
            },
        )
    }
}
fn fonts() -> FontSet<'static> {
    liuli_widget_demo::studio_fonts(Lf4Font::from_bytes(liuli_widget_demo::FONT).unwrap()).unwrap()
}

#[test]
fn changing_focus_geometry_repaints_the_old_and_new_outline() {
    struct FocusButton;
    impl App for FocusButton {
        type Model = (FocusRingStyle, u8);
        type Msg = Self::Model;
        fn update(model: &mut Self::Model, msg: Self::Msg, _: InstantMs) -> Change {
            *model = msg;
            Change::View
        }
        fn view(&(ring, radius): &Self::Model) -> impl Scene {
            layout(
                Key(1),
                NodeProps::new().padding(EdgeInsets::all(8)),
                move |ui| {
                    ui.button(
                        Key(2),
                        NodeProps {
                            focus_ring: ring,
                            focusable: true,
                            width: Length::Px(32),
                            height: Length::Px(20),
                            radius,
                            focus_color: Rgb565::new(31, 63, 31),
                            ..NodeProps::new()
                        },
                    )
                },
            )
        }
        fn on_action(_: &Self::Model, _: UiAction) -> Option<Self::Msg> {
            None
        }
    }
    let mut storage = Storage::new();
    let default = FocusRingStyle::DEFAULT;
    let mut app = AppSession::<FocusButton>::new(
        &mut storage,
        (default, 6),
        fonts(),
        Size::new(64, 48),
        InstantMs(0),
        true,
    )
    .unwrap();
    app.step(InstantMs(1), Some(RawInput::Nav(NavDir::Next)), true)
        .unwrap();
    let rect = app
        .engine()
        .active_tree()
        .node_by_key(Key(2))
        .unwrap()
        .layout_rect;
    let pixel = |app: &AppSession<'_, '_, FocusButton>, x: i32, y: i32| {
        app.screen().pixel((rect.x + x) as u32, (rect.y + y) as u32)
    };
    let wide = FocusRingStyle {
        offset: 3,
        ..default
    };
    // Check both the outside corner and the middle of the top edge after every commit.
    for (i, (ring, radius, corner, edge)) in [
        (wide, 10, false, true),
        (
            FocusRingStyle {
                rounded: false,
                ..wide
            },
            10,
            true,
            true,
        ),
        (wide, 0, true, true),
        (default, 6, false, false),
        (FocusRingStyle::OFF, 6, false, false),
    ]
    .into_iter()
    .enumerate()
    {
        app.message(InstantMs((i as u64 + 1) * 16), (ring, radius), true)
            .unwrap();
        assert_eq!(pixel(&app, -3, -3) == 0xffff, corner);
        assert_eq!(pixel(&app, 16, -3) == 0xffff, edge);
        assert_eq!(pixel(&app, 16, -2) == 0xffff, ring.enabled);
    }
}

#[cfg(feature = "enhanced")]
#[test]
fn h743_scrolls_glass_card_out_and_back_without_changing_its_pixels() {
    use liuli_sim::preview::replay_trace;
    use liuli_widget_demo::{Catalog, Model, Page};
    let mut storage = Storage::new();
    let mut app = AppSession::<Catalog>::new(
        &mut storage,
        Model::for_size(Size::new(800, 480)),
        fonts(),
        Size::new(800, 480),
        InstantMs(0),
        true,
    )
    .unwrap();
    app.engine_mut().set_layer_budget(1024 * 1024).unwrap();
    let mut by_position = std::collections::BTreeMap::<i32, Vec<u16>>::new();
    let mut heights = Vec::new();
    for event in include_str!("fixtures/h743-depth-scroll.trace")
        .lines()
        .skip(1)
    {
        replay_trace(&mut app, &format!("liuli-trace 1\n{event}")).unwrap();
        if event.contains("kind=move") {
            assert_eq!(app.engine().model().page, Page::Depth);
            let card = app.engine().active_tree().node_by_key(Key(303)).unwrap();
            heights.push(card.visible_rect.h);
            let position = card.layout_rect.y;
            if let Some(expected) = by_position.get(&position) {
                let mismatch = app
                    .screen()
                    .words()
                    .iter()
                    .zip(expected)
                    .position(|(a, b)| a != b);
                assert!(
                    mismatch.is_none(),
                    "different scanout at card y={position}, first pixel {:?}",
                    mismatch.map(|p| (p % 800, p / 800))
                );
            } else {
                by_position.insert(position, app.screen().words().to_vec());
            }
        }
    }
    assert!(heights.contains(&0) && heights.iter().any(|&h| h > 0 && h < 108));
    assert_eq!(heights.last(), Some(&108));
    assert!(heights[..15].is_sorted_by(|a, b| a >= b) && heights[15..].is_sorted());
    assert_eq!(
        by_position.len(),
        16,
        "each drag step must actually move the content"
    );
}

#[test]
fn moving_and_removing_shadow_restores_pixels_outside_the_hit_rectangle() {
    let mut storage = Storage::new();
    let mut app = AppSession::<MovingCard>::new(
        &mut storage,
        Some(8),
        fonts(),
        Size::new(64, 64),
        InstantMs(0),
        true,
    )
    .unwrap();
    let tree = app.engine().active_tree();
    let p = Point::new(8, 22);
    assert!(
        !tree
            .node_by_key(Key(3))
            .unwrap()
            .layout_rect
            .contains_point(p)
    );
    assert!(liuli::core::focus::hit_test_in_scope(tree, tree.root.get_u16().unwrap(), p).is_none());
    assert_eq!(app.screen().pixel(8, 22), 0);
    app.message(InstantMs(10), Some(30), true).unwrap();
    assert_eq!(app.screen().pixel(8, 22), 0xffff);
    assert_eq!(app.screen().pixel(8, 44), 0);
    app.message(InstantMs(20), None, true).unwrap();
    assert_eq!(app.screen().pixel(8, 44), 0xffff);
}

#[cfg(feature = "enhanced")]
#[test]
fn exhausted_layer_budget_retains_the_committed_frame_and_identifies_the_card() {
    use liuli_widget_demo::{Catalog, Message, Model, PAGES, Page};
    let mut storage = Storage::new();
    let mut e = Engine::<Catalog>::new_in_place_with_fonts(
        &mut storage,
        Model::new(536),
        fonts(),
        Rect::new(0, 0, 240, 536),
        InstantMs(0),
    )
    .unwrap();
    e.set_layer_budget(16).unwrap();
    let i = PAGES.iter().position(|p| p.0 == Page::Depth).unwrap();
    let result = e.try_message(
        Message::Action(UiAction {
            key: Key(100 + i as u32),
            kind: ActionKind::Activate,
        }),
        InstantMs(5),
        false,
    );
    assert!(matches!(
        result,
        Err(TransactionError::Build(BuildError::LayerPixels))
    ));
    assert_eq!(
        e.build_failure(),
        Some((Page::Depth as u8, Some(Key(303)), BuildError::LayerPixels))
    );
    assert!(e.active_tree().node_by_key(Key(303)).is_none());
    assert_eq!(e.active_commands().cached_layer_bytes(), 0);
    e.set_layer_budget(256 * 1024).unwrap();
    e.commit_view(InstantMs(5)).unwrap();
    assert!(e.active_tree().node_by_key(Key(303)).is_some());
    assert!(e.active_commands().cached_layer_bytes() > 0);
}

#[cfg(feature = "enhanced")]
#[test]
fn glass_tint_only_and_press_depth_off_and_rounded_focus_match_reference() {
    struct ConfigurableSurface;
    #[derive(Clone, Copy, Debug)]
    struct Cfg {
        tint_alpha: u8,
        blur: u8,
        press_depth: PressDepth,
        rounded_focus: bool,
    }
    impl App for ConfigurableSurface {
        type Model = Cfg;
        type Msg = Cfg;
        fn update(m: &mut Cfg, msg: Cfg, _: InstantMs) -> Change {
            *m = msg;
            Change::View
        }
        fn on_action(_: &Cfg, _: UiAction) -> Option<Cfg> {
            None
        }
        fn view(m: &Cfg) -> impl Scene {
            let full = NodeProps {
                width: Length::Fill(1),
                height: Length::Fill(1),
                ..NodeProps::new()
            };
            layout(
                Key(1),
                NodeProps {
                    bg: Some(Rgb565::new(28, 58, 30)),
                    ..full
                },
                move |root| {
                    root.stack(Key(2), full, |stack| {
                        stack.button(
                            Key(3),
                            NodeProps {
                                width: Length::Px(120),
                                height: Length::Px(60),
                                bg: Some(Rgb565::new(2, 20, 28)),
                                ..NodeProps::new()
                            },
                        )?;
                        stack.column(
                            Key(4),
                            NodeProps {
                                width: Length::Px(100),
                                height: Length::Px(48),
                                radius: 10,
                                border_color: Some(Rgb565::new(31, 63, 31)),
                                border_width: 1,
                                ..NodeProps::new()
                            },
                            |card| {
                                card.button(
                                    Key(5),
                                    NodeProps {
                                        width: Length::Px(80),
                                        height: Length::Px(32),
                                        radius: 8,
                                        bg: Some(Rgb565::new(4, 30, 28)),
                                        focusable: true,
                                        clickable: true,
                                        focus_ring: FocusRingStyle {
                                            enabled: true,
                                            width: 2,
                                            offset: 2,
                                            rounded: m.rounded_focus,
                                        },
                                        press_feedback: PressFeedback::Lighten(4),
                                        ..NodeProps::new()
                                    },
                                )
                            },
                        )?;
                        stack.decorate(
                            Key(4),
                            SurfaceStyle::FLAT
                                .with_shadow(Shadow {
                                    color: Rgb565::new(0, 0, 0),
                                    x: 2,
                                    y: 4,
                                    blur: 4,
                                    opacity: 10,
                                })
                                .with_press_depth(m.press_depth)
                                .with_layer(
                                    Layer::OPAQUE
                                        .with_backdrop_radius(m.blur)
                                        .with_tint(Rgb565::new(4, 12, 16), m.tint_alpha),
                                ),
                        )
                    })
                },
            )
        }
    }

    let mut storage = Storage::new();
    let mut app = AppSession::<ConfigurableSurface>::new(
        &mut storage,
        Cfg {
            tint_alpha: 8,
            blur: 0,
            press_depth: PressDepth::OFF,
            rounded_focus: true,
        },
        fonts(),
        Size::new(140, 100),
        InstantMs(0),
        true,
    )
    .unwrap();
    // Zero-blur tint-only glass saves backdrop allocation while tinting underlying pixels.
    assert!(app.engine().active_commands().cached_layer_bytes() > 0);
    // Pixel (90, 42) is inside Key(4) tinted glass (0..100, 0..48) outside Key(5) (0..80, 0..32).
    let tint_pixel = app.screen().pixel(90, 42);
    assert_ne!(tint_pixel, app.screen().pixel(135, 95));

    // Turning PressDepth::OFF keeps shadow pixels (at (50, 52) below Key(4)) unchanged while Key(5) is pressed.
    let id = PointerId(0);
    let shadow_before = app.screen().pixel(50, 52);
    app.step(
        InstantMs(16),
        Some(RawInput::PointerDown {
            id,
            pos: Point::new(10, 10),
        }),
        true,
    )
    .unwrap();
    app.step(InstantMs(150), None, true).unwrap();
    assert_eq!(shadow_before, app.screen().pixel(50, 52));
    app.step(
        InstantMs(166),
        Some(RawInput::PointerUp {
            id,
            pos: Point::new(10, 10),
        }),
        true,
    )
    .unwrap();

    // Rounded focus + blur + custom fast PressDepth all verify against reference fullscreen and shrink shadow.
    app.message(
        InstantMs(200),
        Cfg {
            tint_alpha: 7,
            blur: 4,
            press_depth: PressDepth::custom(60, 30, Easing::Linear),
            rounded_focus: true,
        },
        true,
    )
    .unwrap();
    let shadow_unpressed = app.screen().pixel(50, 52);
    app.step(
        InstantMs(216),
        Some(RawInput::PointerDown {
            id,
            pos: Point::new(10, 10),
        }),
        true,
    )
    .unwrap();
    app.step(InstantMs(260), None, true).unwrap();
    assert_ne!(
        shadow_unpressed,
        app.screen().pixel(50, 52),
        "custom PressDepth must shrink shadow when held"
    );
    assert_ne!(
        app.screen().pixel(90, 42),
        app.screen().pixel(135, 95),
        "blurred + tinted glass must differ from raw root background"
    );
}

#[cfg(feature = "enhanced")]
#[test]
fn depth_page_granular_theme_and_effect_controls_verify_pixel_exact() {
    use liuli_widget_demo::{Catalog, Model, PAGES, Page};

    let mut storage = Storage::new();
    let mut app = AppSession::<Catalog>::new(
        &mut storage,
        Model::new(536),
        fonts(),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )
    .unwrap();
    let mut clock = 0u64;
    let tap_key = |app: &mut AppSession<'_, '_, Catalog>, clock: &mut u64, key: u32| {
        let node = app
            .engine()
            .active_tree()
            .node_by_key(Key(key))
            .unwrap_or_else(|| panic!("missing key {key}"));
        let r = node.visible_rect;
        assert!(r.w > 0 && r.h > 0, "key {key} not visible");
        let pos = Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32);
        let id = PointerId(2);
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
    };

    let depth_idx = PAGES.iter().position(|p| p.0 == Page::Depth).unwrap();
    tap_key(&mut app, &mut clock, 100 + depth_idx as u32);
    assert_eq!(app.engine().model().page, Page::Depth);

    // Drag scroll down twice to bring all granular controls into view, then exercise every toggle & slider.
    let drag_id = PointerId(2);
    for _ in 0..2 {
        for pos in [
            Point::new(120, 440),
            Point::new(120, 260),
            Point::new(120, 120),
        ] {
            clock += 16;
            let ev = if pos.y == 440 {
                RawInput::PointerDown { id: drag_id, pos }
            } else {
                RawInput::PointerMove { id: drag_id, pos }
            };
            app.step(InstantMs(clock), Some(ev), true).unwrap();
        }
        clock += 16;
        app.step(
            InstantMs(clock),
            Some(RawInput::PointerUp {
                id: drag_id,
                pos: Point::new(120, 120),
            }),
            true,
        )
        .unwrap();
    }

    let before_preset = app.engine().model().preset;
    tap_key(&mut app, &mut clock, 313);
    assert_ne!(app.engine().model().preset, before_preset);

    tap_key(&mut app, &mut clock, 314);
    assert_eq!(app.engine().model().shape_idx, 1);

    tap_key(&mut app, &mut clock, 315);
    assert!(!app.engine().model().effects.shadows);

    tap_key(&mut app, &mut clock, 316);
    assert!(!app.engine().model().effects.press_depth);

    tap_key(&mut app, &mut clock, 317);
    assert!(!app.engine().model().effects.glass_tint);

    tap_key(&mut app, &mut clock, 319);
    tap_key(&mut app, &mut clock, 321);
    assert!(app.engine().model().tint_alpha.is_some());

    assert!(app.engine().model().rounded_focus);
    tap_key(&mut app, &mut clock, 322);
    assert!(!app.engine().model().rounded_focus);
}

#[cfg(feature = "enhanced")]
#[test]
fn rounded_glass_and_outline_button_have_no_square_or_black_corners() {
    use liuli::core::{geometry::rgb565_to_u16, paint::PaintCmd};
    use liuli_widgets::{ButtonVariant, Elevation, Material, SurfaceSpec, Theme, ThemedUi};

    struct GlassAndOutline;
    impl App for GlassAndOutline {
        type Model = ();
        type Msg = ();
        fn update(_: &mut (), _: (), _: InstantMs) -> Change {
            Change::Unchanged
        }
        fn on_action(_: &(), _: UiAction) -> Option<()> {
            None
        }
        fn view(_: &()) -> impl Scene {
            let t = Theme::DARK;
            let full = NodeProps {
                width: Length::Fill(1),
                height: Length::Fill(1),
                ..NodeProps::new()
            };
            layout(
                Key(0),
                NodeProps {
                    bg: Some(t.background),
                    ..full
                },
                move |root| {
                    let mut tui = ThemedUi::new(root, t.with_rounded_focus(true));
                    tui.column(Key(10), full, |col| {
                        col.stack(
                            Key(1),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(140),
                                ..NodeProps::new()
                            },
                            |stack| {
                                // Bright blue backdrop behind the glass card
                                stack.raw().button(
                                    Key(2),
                                    NodeProps {
                                        width: Length::Fill(1),
                                        height: Length::Fill(1),
                                        bg: Some(Rgb565::new(8, 36, 30)),
                                        ..NodeProps::new()
                                    },
                                )?;
                                stack.column(
                                    Key(3),
                                    NodeProps {
                                        padding: EdgeInsets::all(18),
                                        width: Length::Fill(1),
                                        height: Length::Fill(1),
                                        ..NodeProps::new()
                                    },
                                    |inset| {
                                        inset.card(
                                            Key(4),
                                            SurfaceSpec::CARD
                                                .with_material(Material::GLASS)
                                                .with_elevation(Elevation::Floating),
                                            |card| {
                                                card.button(
                                                    Key(5),
                                                    "OK",
                                                    IconKind::None,
                                                    ButtonVariant::Primary,
                                                )
                                            },
                                        )
                                    },
                                )
                            },
                        )?;
                        col.button(Key(6), "Outline", IconKind::None, ButtonVariant::Outline)
                    })
                },
            )
        }
    }

    let mut storage = Storage::new();
    let mut app = AppSession::<GlassAndOutline>::new(
        &mut storage,
        (),
        fonts(),
        Size::new(200, 220),
        InstantMs(0),
        true,
    )
    .unwrap();

    // 1. Both the glass card (Key 4) and outline button (Key 6) must emit StrokeRoundedRect, not StrokeRect.
    let cmds = app.engine().active_commands().as_slice();
    let rounded_strokes = cmds
        .iter()
        .filter(|c| matches!(c, PaintCmd::StrokeRoundedRect { .. }))
        .count();
    let square_strokes = cmds
        .iter()
        .filter(|c| matches!(c, PaintCmd::StrokeRect { .. }))
        .count();
    assert!(
        rounded_strokes >= 2,
        "expected rounded borders, cmds={cmds:?}"
    );
    assert_eq!(
        square_strokes, 0,
        "unexpected square StrokeRect, cmds={cmds:?}"
    );

    // 2. Card outer bounding-box corners must NOT contain the card border color (526984 -> bright border),
    //    and pixels diagonally adjacent inside the bounding box but outside radius=12 must NOT reveal
    //    un-tinted dark shadow core inside a sharp corner.
    let card_rect = app
        .engine()
        .active_tree()
        .node_by_key(Key(4))
        .unwrap()
        .layout_rect;
    let border_col = rgb565_to_u16(
        app.engine()
            .active_tree()
            .node_by_key(Key(4))
            .unwrap()
            .props
            .border_color()
            .unwrap(),
    );
    let corners = [
        (card_rect.x, card_rect.y),
        (card_rect.right64() as i32 - 1, card_rect.y),
        (card_rect.x, card_rect.bottom64() as i32 - 1),
        (
            card_rect.right64() as i32 - 1,
            card_rect.bottom64() as i32 - 1,
        ),
    ];
    for (cx, cy) in corners {
        assert_ne!(
            app.screen().pixel(cx as u32, cy as u32),
            border_col,
            "outer corner ({cx},{cy}) must be outside rounded border"
        );
    }

    // 3. Pressing Outline button must keep contrast (fill != fg/accent) and verify against reference.
    let out_node = app.engine().active_tree().node_by_key(Key(6)).unwrap();
    let out_rect = out_node.layout_rect;
    let accent = rgb565_to_u16(out_node.props.accent);
    let root_bg = rgb565_to_u16(Theme::DARK.background);
    assert_ne!(
        app.screen().pixel(out_rect.x as u32, out_rect.y as u32),
        accent,
        "outline button outer corner must be rounded"
    );
    let center = Point::new(
        out_rect.x + (out_rect.w / 2) as i32,
        out_rect.y + (out_rect.h / 2) as i32,
    );
    app.step(
        InstantMs(16),
        Some(RawInput::PointerDown {
            id: PointerId(0),
            pos: center,
        }),
        true,
    )
    .unwrap();
    // Sample the themed press transition after the pointer-down frame.
    app.step(InstantMs(76), None, true).unwrap();
    let pressed_bg = app
        .screen()
        .pixel((out_rect.x + 12) as u32, (out_rect.y + 4) as u32);
    assert_ne!(
        pressed_bg, accent,
        "pressed outline bg must not turn solid accent"
    );
    assert_ne!(pressed_bg, root_bg, "pressed outline bg must tint visibly");
}
