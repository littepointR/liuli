use liuli::prelude::*;
use liuli_alarms::{AlarmApp, LIST_VIEWPORT, Model, Msg, Page};
use liuli_sim::preview::replay_trace;
use liuli_sim::session::AppSession;

#[test]
fn virtual_list_tracks_inertia_and_stops_on_touch_hold_reverse_and_cancel() {
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<AlarmApp<LIST_VIEWPORT>>::new(
        &mut storage,
        Model::new(536),
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )
    .unwrap();
    let id = PointerId(1);
    let p = |y| Point::new(120, y);
    let down = |y| RawInput::PointerDown { id, pos: p(y) };
    let mv = |y| RawInput::PointerMove { id, pos: p(y) };
    let up = |y| RawInput::PointerUp { id, pos: p(y) };
    let mut step = |ms, event| {
        app.step(InstantMs(ms), event, true).unwrap();
        let offset = app.engine().model().offset;
        let tree = app.engine().active_tree();
        assert_eq!(tree.node_by_key(Key(10)).unwrap().scroll_y, offset);
        assert!(tree.node_by_key(Key(100 + offset as u32 / 32)).is_some());
        assert!(tree.active_count <= 6 + 504u16.div_ceil(32) + 1 + 2);
        offset
    };
    step(10, Some(down(400)));
    step(26, Some(mv(376)));
    step(42, Some(mv(340)));
    let release = step(46, Some(up(334)));
    assert_eq!(release, 66); // The lift itself carries six new pixels.
    let first = step(62, None);
    assert!(first > release);
    let next = step(78, None);
    assert!(next >= first);
    let stopped = step(82, Some(down(240)));
    assert_eq!(step(242, None), stopped);
    step(258, Some(mv(210)));
    let held = step(420, Some(up(210)));
    assert_eq!(step(468, None), held);
    step(500, Some(down(310)));
    step(516, Some(mv(280)));
    step(532, Some(mv(250)));
    step(548, Some(mv(276)));
    let reversed = step(552, Some(up(276)));
    assert!(step(568, None) < reversed);
    step(574, Some(down(280)));
    step(590, Some(mv(254)));
    let cancelled = step(594, Some(RawInput::PointerCancel { id }));
    assert_eq!(step(750, None), cancelled);
    assert_eq!(app.engine().wakeup(InstantMs(750)), NextWakeup::OnEvent);
}

#[test]
fn board_trace_opens_the_held_record_before_the_finger_lifts() {
    let trace = include_str!("fixtures/s3-long-press.trace");
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<AlarmApp<LIST_VIEWPORT>>::new(
        &mut storage,
        Model::new(536),
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )
    .unwrap();
    let (before_hold, tail) = trace.split_once("36717 kind=down").unwrap();
    replay_trace(&mut app, before_hold).unwrap();
    assert_eq!(app.engine().model().routes.current(), Page::List);
    // The recorded drags cover 415 pixels; free scrolling also advances after each lift.
    assert_eq!(app.engine().model().offset, 438);
    let (hold, release) = tail.split_once("37617 kind=up").unwrap();
    // The 900 ms hold has no move events. Only the captured time advances can open detail.
    replay_trace(&mut app, &format!("liuli-trace 1\n36717 kind=down{hold}")).unwrap();
    assert_eq!(app.engine().model().routes.current(), Page::Detail(24));
    replay_trace(&mut app, &format!("liuli-trace 1\n37617 kind=up{release}")).unwrap();
    assert_eq!(app.engine().model().routes.current(), Page::Detail(24));
}

#[test]
fn touch_workflow_edits_only_the_selected_record_and_restores_the_list() {
    let trace = include_str!("fixtures/touch-workflow.trace");
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<AlarmApp<LIST_VIEWPORT>>::new(
        &mut storage,
        Model::new(536),
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )
    .unwrap();
    let mut lines = trace.lines();
    assert_eq!(lines.next(), Some("liuli-trace 1"));
    let mut play = |app: &mut AppSession<'_, '_, AlarmApp<LIST_VIEWPORT>>, count: usize| {
        let mut part = String::from("liuli-trace 1\n");
        for _ in 0..count {
            let line = lines.next().expect("complete raw pointer gesture");
            assert!(line.split_once(' ').unwrap().1.starts_with("kind="));
            part.push_str(line);
            part.push('\n');
        }
        replay_trace(app, &part).unwrap();
    };
    play(&mut app, 3);
    let offset = app.engine().model().offset;
    assert_eq!(offset, 320);
    play(&mut app, 2);
    assert_eq!(app.engine().model().routes.current(), Page::Detail(11));
    play(&mut app, 2);
    assert_eq!(app.engine().model().routes.current(), Page::Edit(11));
    play(&mut app, 3);
    assert_eq!(app.engine().model().draft, 75);
    play(&mut app, 2);
    assert!(app.engine().model().confirm);
    play(&mut app, 2);
    assert!(
        !app.engine().model().confirm,
        "confirm hit rect {:?}",
        app.engine()
            .active_tree()
            .node_by_key(Key(32))
            .map(|n| n.visible_rect)
    );
    assert_eq!(app.engine().model().routes.current(), Page::Detail(11));
    assert_eq!(app.engine().model().threshold, 75);
    play(&mut app, 2);
    assert!(lines.next().is_none());
    assert_eq!(app.engine().model().routes.current(), Page::List);
    assert_eq!(app.engine().model().offset, offset);
    assert_eq!(
        app.engine()
            .active_tree()
            .node_by_key(Key(10))
            .unwrap()
            .scroll_y,
        offset
    );
    assert_eq!(app.engine().focus().focused_key(), Some(Key(111)));
    // Reopen both records by touch to verify the stored values independently of the editor.
    let tap = |app: &mut AppSession<'_, '_, AlarmApp<LIST_VIEWPORT>>, ms: u64, y: i32| {
        replay_trace(
            app,
            &format!(
                "liuli-trace 1\n{ms} kind=down&v=1&x=100&y={y}\n{} kind=up&v=1&x=100&y={y}\n",
                ms + 16
            ),
        )
        .unwrap();
    };
    tap(&mut app, 580, 80);
    assert_eq!(app.engine().model().routes.current(), Page::Detail(11));
    assert_eq!(app.engine().model().threshold, 75);
    tap(&mut app, 620, 92);
    tap(&mut app, 670, 112);
    assert_eq!(app.engine().model().routes.current(), Page::Detail(12));
    assert_eq!(app.engine().model().threshold, 50);
}
#[test]
fn bounded_scroll_edit_and_return() {
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<AlarmApp<LIST_VIEWPORT>>::new(
        &mut storage,
        Model::new(536),
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )
    .unwrap();
    let id = PointerId(1);
    for n in 0..12u64 {
        for (i, input) in [
            RawInput::PointerDown {
                id,
                pos: Point::new(120, 480),
            },
            RawInput::PointerMove {
                id,
                pos: Point::new(120, 240),
            },
            RawInput::PointerMove {
                id,
                pos: Point::new(120, 60),
            },
            RawInput::PointerUp {
                id,
                pos: Point::new(120, 60),
            },
        ]
        .into_iter()
        .enumerate()
        {
            app.step(InstantMs(n * 4 + i as u64 + 1), Some(input), true)
                .unwrap();
            // Six fixed nodes, both partial rows, and one rotary focus target at each edge.
            assert!(app.engine().active_tree().active_count <= 6 + 504u16.div_ceil(32) + 1 + 2);
        }
    }
    let offset = app.engine().model().offset;
    assert!(offset > 4000, "offset {offset}");
    let key = app
        .engine()
        .active_tree()
        .nodes
        .iter()
        .find(|n| n.occupied && n.key.0 >= 100 && n.layout_rect.y > 40 && n.layout_rect.y < 480)
        .unwrap()
        .key;
    let rect = app
        .engine()
        .active_tree()
        .node_by_key(key)
        .unwrap()
        .visible_rect;
    let pos = Point::new(rect.x + 8, rect.y + 8);
    app.step(InstantMs(51), Some(RawInput::PointerDown { id, pos }), true)
        .unwrap();
    app.step(InstantMs(52), Some(RawInput::PointerUp { id, pos }), true)
        .unwrap();
    assert_eq!(
        app.engine().model().routes.current(),
        Page::Detail(key.0 - 100)
    );
    for (n, msg) in [Msg::Edit, Msg::Adjust(75), Msg::Ask, Msg::Save, Msg::Back]
        .into_iter()
        .enumerate()
    {
        app.message(InstantMs(54 + n as u64), msg, true).unwrap();
    }
    assert_eq!(app.engine().model().routes.current(), Page::List);
    assert_eq!(app.engine().model().threshold, 75);
    assert_eq!(app.engine().model().offset, offset);
    assert_eq!(
        app.engine()
            .active_tree()
            .node_by_key(Key(10))
            .unwrap()
            .scroll_y,
        offset
    );
    assert_eq!(app.engine().focus().focused_key(), Some(key));
}

#[test]
fn rotary_navigation_crosses_materialized_window() {
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<AlarmApp<LIST_VIEWPORT>>::new(
        &mut storage,
        Model::new(536),
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )
    .unwrap();
    for time in 1..=45 {
        app.step(InstantMs(time), Some(RawInput::Nav(NavDir::Next)), true)
            .unwrap();
    }
    assert!(app.engine().model().offset > 650);
    assert!(app.engine().focus().focused_key().unwrap().0 > 140);
}
