//! A second consumer exercises the public Tabs contract independently of the demo layout.
use liuli::prelude::*;
use liuli_sim::session::AppSession;
use liuli_widget_demo::showcase::showcase_fonts;
use liuli_widgets::{PartRule, TabLayout, TabPage, Tabs, Theme, WidgetClass, rgb8};

const MARK: Rgb565 = rgb8(248, 0, 176);
const DOT: WidgetClass = WidgetClass("reader.dot");
const RULES: &[PartRule] = &[PartRule {
    class: DOT,
    parts: WidgetPartStyle::new()
        .with_indicator_color(MARK)
        .with_checked_fg(MARK),
}];

struct Reader;
struct Model {
    active: u8,
    layout: TabLayout,
    tools: bool,
    large: bool,
    style: WidgetPartStyle,
    fixed: bool,
    reduced: bool,
    dot_width: u8,
}
fn declarations(m: &Model) -> [TabPage<u8, Model>; 3] {
    [
        TabPage::new(0, Key(10), TextRef::Static("A"), |_, _, p| {
            p.button(Key(21), Theme::DARK.control())?;
            p.button(Key(24), Theme::DARK.control())
        }),
        TabPage::new(
            1,
            Key(11),
            TextRef::Static(if m.large {
                "Archive and recordings"
            } else {
                "Recordings"
            }),
            |_, _, p| p.button(Key(22), Theme::DARK.control()),
        ),
        TabPage::new(2, Key(12), TextRef::Static("Mix"), |_, _, p| {
            p.button(Key(23), Theme::DARK.control())
        }),
    ]
}
impl App for Reader {
    type Model = Model;
    type Msg = u8;
    fn update(m: &mut Model, id: u8, _: InstantMs) -> Change {
        m.active = id;
        Change::View
    }
    fn paint_custom(
        key: Key,
        _: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        assert_eq!(key, Key(30));
        assert_eq!(canvas.foreground(), MARK);
        assert_eq!(canvas.parts().indicator_radius(), Some(2));
        let bounds = Rect::new(0, 0, canvas.size().width, canvas.size().height);
        canvas.selection_indicator(bounds, bounds)
    }
    fn measure_custom_styled(_: Key, _: FontId, _: Size, metrics: PartMetrics) -> Size {
        Size::new(u32::from(metrics.icon_size.unwrap_or(80)), 20)
    }
    fn on_action(m: &Model, action: UiAction) -> Option<u8> {
        Tabs::new(&declarations(m), m.active)
            .unwrap()
            .selection(action)
    }
    fn view(m: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let pages = declarations(m);
        let tabs = Tabs::new(&pages, m.active)?;
        let mut t = Theme::DARK.map_parts(|p| p.with_tab(m.style).with_rules(RULES));
        t = t.with_motion(t.motion.with_reduced(m.reduced));
        if m.large {
            t.typography.body = FontId::Body16;
        } else {
            t.typography.body = FontId::Small12;
        }
        out.root(Key(1), NodeProps::new().bg(t.background), |root| {
            root.column(Key(2), NodeProps::new().width(Length::Fill(1)), |col| {
                tabs.header_with(
                    col,
                    Key(3),
                    m.layout,
                    t,
                    |bar| {
                        if m.tools {
                            bar.label(
                                Key(4),
                                t.caption()
                                    .width(Length::Content)
                                    .text(TextRef::Static("Notes")),
                            )
                        } else {
                            Ok(())
                        }
                    },
                    |bar| {
                        if m.tools {
                            bar.button(
                                Key(5),
                                t.secondary_button()
                                    .width(Length::Content)
                                    .text(TextRef::Static("Sync")),
                            )
                        } else {
                            Ok(())
                        }
                    },
                )?;
                if m.fixed {
                    col.set_layout_constraints(Key(11), Some(80), Some(80), Some(28), Some(28))?;
                    col.set_offset(Key(11), 2, 0)?;
                }
                let props = NodeProps::new()
                    .checked(true)
                    .clickable(true)
                    .state_motion(t.motion.state(MotionState::Pressed, None))
                    .with_parts(t.parts.resolve(DOT))
                    .with_parts(
                        WidgetPartStyle::new()
                            .with_indicator_radius(2)
                            .with_icon_size(m.dot_width),
                    );
                col.custom(Key(30), props)?;
                tabs.panel(col, Key(20), NodeProps::new().width(Length::Fill(1)), m, t)
            })
        })
    }
}
fn session(
    s: &mut Storage,
    w: u32,
    layout: TabLayout,
    tools: bool,
) -> AppSession<'_, 'static, Reader> {
    AppSession::new(
        s,
        Model {
            active: 0,
            layout,
            tools,
            large: false,
            style: WidgetPartStyle::ZERO,
            fixed: false,
            reduced: false,
            dot_width: 80,
        },
        showcase_fonts().unwrap(),
        Size::new(w, 260),
        InstantMs(0),
        true,
    )
    .unwrap()
}
fn rect(app: &AppSession<'_, '_, Reader>, key: u32) -> Rect {
    app.engine()
        .active_tree()
        .node_by_key(Key(key))
        .unwrap()
        .layout_rect
}
fn center(r: Rect) -> Point {
    Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
}
fn input(app: &mut AppSession<'_, '_, Reader>, now: &mut u64, e: RawInput) {
    *now += 16;
    app.step(InstantMs(*now), Some(e), true).unwrap();
}
fn verify_panel(app: &AppSession<'_, '_, Reader>, active: u8) {
    let tree = app.engine().active_tree();
    assert_eq!(
        tree.node_by_key(Key(20)).unwrap().active_tab(),
        Some(Key(10 + u32::from(active)))
    );
    assert_eq!(app.engine().model().active, active);
    for i in 0..3 {
        assert_eq!(
            tree.node_by_key(Key(21 + i)).is_some(),
            i == u32::from(active)
        );
    }
}
#[test]
fn intrinsic_tracks_wrap_at_the_measured_pixel_and_clip_tools_and_text() {
    let mut s = Storage::new();
    let wide = session(&mut s, 400, TabLayout::Wrap { max_rows: 2 }, false);
    let total = (10..=12)
        .map(|k| {
            wide.engine()
                .active_tree()
                .node_by_key(Key(k))
                .unwrap()
                .measured_size
                .width
        })
        .sum::<u32>();
    assert!(rect(&wide, 11).w > rect(&wide, 10).w);
    drop(wide);
    for width in [total - 1, total, total + 1] {
        let mut s = Storage::new();
        let app = session(&mut s, width, TabLayout::Wrap { max_rows: 2 }, false);
        assert_eq!(rect(&app, 12).y > rect(&app, 10).y, width < total);
    }
    for width in [80, 150, 400] {
        let mut s = Storage::new();
        let mut app = session(&mut s, width, TabLayout::Wrap { max_rows: 2 }, true);
        app.engine_mut().try_model_mut().unwrap().large = true;
        app.engine_mut().commit_view(InstantMs(5)).unwrap();
        app.step(InstantMs(6), None, true).unwrap();
        let tree = app.engine().active_tree();
        for key in [10, 11, 12] {
            let n = tree.node_by_key(Key(key)).unwrap();
            let visible = n.layout_rect.intersect(n.clip_rect);
            assert!(
                visible.intersect(rect(&app, 4)).is_empty()
                    && visible.intersect(rect(&app, 5)).is_empty()
            );
        }
        for cmd in app.engine().active_commands().as_slice() {
            if let liuli::core::paint::PaintCmd::Text { text, clip, .. } = cmd {
                let n = &tree.nodes[usize::from(text.0)];
                if n.kind == NodeKind::Tab {
                    let inner = n.layout_rect.inset(
                        n.props.padding.left.into(),
                        n.props.padding.top.into(),
                        n.props.padding.right.into(),
                        n.props.padding.bottom.into(),
                    );
                    assert_eq!(clip.intersect(inner).intersect(n.clip_rect), *clip);
                }
            }
        }
    }
}
#[test]
fn scroll_does_not_select_and_touch_keyboard_and_encoder_mount_one_page() {
    let mut s = Storage::new();
    let mut app = session(&mut s, 84, TabLayout::Scroll, false);
    let mut now = 0;
    let id = PointerId(1);
    let start = Point::new(76, 16);
    let end = Point::new(1, 16);
    for e in [
        RawInput::PointerDown { id, pos: start },
        RawInput::PointerMove { id, pos: end },
        RawInput::PointerUp { id, pos: end },
    ] {
        input(&mut app, &mut now, e);
    }
    verify_panel(&app, 0);
    assert!(
        app.engine()
            .active_tree()
            .node_by_key(Key(3))
            .unwrap()
            .scroll_y
            > 0
    );
    let last = app.engine().active_tree().node_by_key(Key(12)).unwrap();
    let visible = last.layout_rect.intersect(last.clip_rect);
    assert!(!visible.is_empty());
    let pos = center(visible);
    for e in [
        RawInput::PointerDown { id, pos },
        RawInput::PointerUp { id, pos },
    ] {
        input(&mut app, &mut now, e);
    }
    verify_panel(&app, 2);
    input(&mut app, &mut now, RawInput::Nav(NavDir::Right));
    verify_panel(&app, 0);
    let first = app.engine().active_tree().node_by_key(Key(10)).unwrap();
    assert_eq!(
        first.layout_rect.intersect(first.clip_rect),
        first.layout_rect
    );
    input(&mut app, &mut now, RawInput::Nav(NavDir::Next));
    assert_eq!(app.engine().focus().focused_key(), Some(Key(21)));
    input(&mut app, &mut now, RawInput::Nav(NavDir::Prev));
    assert_eq!(app.engine().focus().focused_key(), Some(Key(10)));
    input(&mut app, &mut now, RawInput::Activate);
    input(&mut app, &mut now, RawInput::Adjust(1));
    verify_panel(&app, 0);
    input(&mut app, &mut now, RawInput::Activate);
    verify_panel(&app, 1);
}

#[test]
fn returning_to_a_panel_restores_child_focus_and_encoder_cancel_keeps_selection() {
    let mut storage = Storage::new();
    let mut app = session(&mut storage, 400, TabLayout::Scroll, false);
    let mut now = 0;
    for k in [24, 11, 10] {
        let pos = center(rect(&app, k));
        for e in [
            RawInput::PointerDown {
                id: PointerId(1),
                pos,
            },
            RawInput::PointerUp {
                id: PointerId(1),
                pos,
            },
        ] {
            input(&mut app, &mut now, e);
        }
    }
    input(&mut app, &mut now, RawInput::Nav(NavDir::Next));
    assert_eq!(app.engine().focus().focused_key(), Some(Key(24)));
    let pos = center(rect(&app, 10));
    for e in [
        RawInput::PointerDown {
            id: PointerId(1),
            pos,
        },
        RawInput::PointerUp {
            id: PointerId(1),
            pos,
        },
        RawInput::Activate,
        RawInput::Adjust(2),
        RawInput::Back,
    ] {
        input(&mut app, &mut now, e);
    }
    verify_panel(&app, 0);
    assert_eq!(app.engine().focus().focused_key(), Some(Key(10)));
}

fn restyle(app: &mut AppSession<'_, '_, Reader>, now: u64, f: impl FnOnce(&mut Model)) {
    f(app.engine_mut().try_model_mut().unwrap());
    app.engine_mut().commit_view(InstantMs(now)).unwrap();
    app.step(InstantMs(now), None, true).unwrap();
}

fn marked(app: &AppSession<'_, '_, Reader>, x: i32, y: i32) -> bool {
    app.screen().pixel(x as u32, y as u32) == rgb565_to_u16(MARK)
}

#[test]
fn one_touch_trace_selects_the_same_page_under_three_decorations() {
    let mut baseline = None;
    for (shape, span, motion) in [
        (
            IndicatorShape::Bottom,
            IndicatorSpan::Control,
            IndicatorMotion::Reveal,
        ),
        (
            IndicatorShape::Bottom,
            IndicatorSpan::Content,
            IndicatorMotion::Slide,
        ),
        (
            IndicatorShape::Fill,
            IndicatorSpan::Control,
            IndicatorMotion::Fade,
        ),
    ] {
        let mut s = Storage::new();
        let mut app = session(&mut s, 300, TabLayout::Columns(3), false);
        restyle(&mut app, 1, |m| {
            m.style = WidgetPartStyle::new()
                .with_indicator_color(MARK)
                .with_indicator_layout(IndicatorLayout {
                    shape,
                    span,
                    thickness: 3,
                    edge_offset: 2,
                })
                .with_indicator_radius(2)
                .with_indicator_motion(motion)
        });
        assert_eq!(app.last_profile().measured_nodes, 0);
        liuli_sim::preview::replay_trace(
            &mut app,
            include_str!("../fixtures/tabs-selection.trace"),
        )
        .unwrap();
        verify_panel(&app, 1);
        let boxes = [rect(&app, 10), rect(&app, 11), rect(&app, 12)];
        assert_eq!(*baseline.get_or_insert(boxes), boxes);
        let r = boxes[1];
        let y = r.bottom64() as i32 - 4;
        assert!(marked(&app, center(r).x, y));
        assert_eq!(marked(&app, r.x + 2, y), span == IndicatorSpan::Control);
        assert_eq!(
            marked(&app, r.x + 4, r.y + 4),
            shape == IndicatorShape::Fill
        );
        assert!(!marked(&app, center(boxes[0]).x, y));
        let dot = rect(&app, 30);
        assert!(marked(&app, center(dot).x, dot.bottom64() as i32 - 2));
    }
}

#[test]
fn sliding_marker_retargets_across_the_middle_tab_and_reduced_motion_snaps() {
    let mut s = Storage::new();
    let mut app = session(&mut s, 300, TabLayout::Columns(3), false);
    restyle(&mut app, 1, |m| {
        m.style = WidgetPartStyle::new()
            .with_indicator_color(MARK)
            .with_indicator_motion(IndicatorMotion::Slide)
    });
    let first = rect(&app, 10);
    let y = first.bottom64() as i32 - 2;
    let extent = |app: &AppSession<'_, '_, Reader>| {
        let x: Vec<_> = (0..300).filter(|&x| marked(app, x, y)).collect();
        assert_eq!(x.len(), 100);
        assert_eq!(x[99] - x[0], 99);
        x[0]
    };
    let mut now = 4;
    let tap = |app: &mut AppSession<'_, '_, Reader>, now: &mut u64, x| {
        let pos = Point::new(x, 12);
        input(
            app,
            now,
            RawInput::PointerDown {
                id: PointerId(1),
                pos,
            },
        );
        input(
            app,
            now,
            RawInput::PointerUp {
                id: PointerId(1),
                pos,
            },
        );
    };
    tap(&mut app, &mut now, 250);
    now += 45;
    app.step(InstantMs(now), None, true).unwrap();
    assert!((1..200).contains(&extent(&app)));
    tap(&mut app, &mut now, 150);
    verify_panel(&app, 1);
    app.step(InstantMs(now + 400), None, true).unwrap();
    assert_eq!(extent(&app), 100);
    now += 410;
    restyle(&mut app, now, |m| m.reduced = true);
    tap(&mut app, &mut now, 250);
    assert_eq!(extent(&app), 200);
}

#[test]
fn one_tab_keeps_fixed_bounds_and_offset_across_layout_policies() {
    for layout in [
        TabLayout::Columns(3),
        TabLayout::Scroll,
        TabLayout::Wrap { max_rows: 2 },
    ] {
        let mut s = Storage::new();
        let mut app = session(&mut s, 300, layout, false);
        restyle(&mut app, 1, |m| m.fixed = true);
        let r = rect(&app, 11);
        assert_eq!(r.size(), Size::new(80, 28));
        if layout == TabLayout::Columns(3) {
            assert_eq!(r.x, 112);
        }
        let mut now = 2;
        let pos = Point::new(r.x + 1, r.y + 1);
        input(
            &mut app,
            &mut now,
            RawInput::PointerDown {
                id: PointerId(3),
                pos,
            },
        );
        input(
            &mut app,
            &mut now,
            RawInput::PointerUp {
                id: PointerId(3),
                pos,
            },
        );
        verify_panel(&app, 1);
    }
}

#[test]
fn custom_component_uses_resolved_metrics_for_layout_and_pixels() {
    let mut s = Storage::new();
    let mut app = session(&mut s, 300, TabLayout::Columns(3), false);
    assert_eq!(rect(&app, 30).w, 80);
    restyle(&mut app, 5, |m| m.dot_width = 44);
    assert!(app.last_profile().measured_nodes > 0);
    let r = rect(&app, 30);
    assert_eq!(r.w, 44);
    let y = r.bottom64() as i32 - 2;
    assert!(marked(&app, r.x + 40, y));
    assert!(!marked(&app, r.x + 55, y));
    let mut now = 10;
    input(
        &mut app,
        &mut now,
        RawInput::PointerDown {
            id: PointerId(2),
            pos: center(r),
        },
    );
    app.step(InstantMs(now + 20), None, true).unwrap();
    assert!(
        app.engine()
            .active_tree()
            .node_by_key(Key(30))
            .unwrap()
            .pressed
    );
    assert!(
        marked(&app, r.x + 40, y),
        "a press transition cannot shrink a checked decoration"
    );
}

#[test]
fn theme_styles_share_capacity_across_widgets_and_local_overrides_are_isolated() {
    use liuli::core::part_style::MAX_PART_STYLES;
    use liuli::core::view::ViewDraft;
    let mut draft = ViewDraft::new();
    let shared = WidgetPartStyle::new().with_indicator_color(MARK);
    let theme = Theme::DARK.map_parts(|p| p.with_progress(shared));
    let mut builder = ViewBuilder::new(&mut draft);
    builder
        .root(Key(1), NodeProps::new(), |ui| {
            for k in 10..42 {
                ui.progress_bar(Key(k), theme.progress_bar(50))?;
            }
            for n in 1..MAX_PART_STYLES {
                ui.style_parts(
                    Key(10 + n as u32),
                    WidgetPartStyle::new().with_indicator_radius(n as u8),
                )?;
            }
            // Replacing the last reference to a distinct style can reuse its slot even when full.
            ui.style_parts(Key(11), WidgetPartStyle::new().with_indicator_radius(70))?;
            Ok(())
        })
        .unwrap();
    builder.finish().unwrap();
    assert_eq!(draft.part_style(Key(11)).indicator_radius(), Some(70));
    assert_eq!(*draft.part_style(Key(41)), shared);

    let mut b = ViewBuilder::new(&mut draft);
    let err = b.root(Key(1), NodeProps::new(), |ui| {
        for n in 0..=MAX_PART_STYLES {
            ui.custom(
                Key(10 + n as u32),
                NodeProps::new().with_parts(shared.with_indicator_radius(n as u8)),
            )?;
        }
        Ok(())
    });
    assert_eq!(err, Err(BuildError::PartStyles));
}
