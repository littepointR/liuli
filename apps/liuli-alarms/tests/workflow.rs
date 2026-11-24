use liuli::prelude::*;
use liuli_alarms::{Alarms, Model, Msg, Page};
use liuli_sim::preview::replay_trace;
use liuli_sim::session::AppSession;

#[test]
fn touch_workflow_edits_only_the_selected_record_and_restores_the_list() {
    let trace = include_str!("fixtures/touch-workflow.trace");
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<Alarms>::new(
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
    let mut play = |app: &mut AppSession<'_, '_, Alarms>, count: usize| {
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
    let tap = |app: &mut AppSession<'_, '_, Alarms>, ms: u64, y: i32| {
        replay_trace(
            app,
            &format!(
                "liuli-trace 1\n{ms} kind=down&v=1&x=100&y={y}\n{} kind=up&v=1&x=100&y={y}\n",
                ms + 16
            ),
        )
        .unwrap();
    };
    tap(&mut app, 460, 80);
    assert_eq!(app.engine().model().routes.current(), Page::Detail(11));
    assert_eq!(app.engine().model().threshold, 75);
    tap(&mut app, 500, 92);
    tap(&mut app, 550, 112);
    assert_eq!(app.engine().model().routes.current(), Page::Detail(12));
    assert_eq!(app.engine().model().threshold, 50);
}
#[test]
fn bounded_scroll_edit_and_return() {
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
    let mut storage = Storage::new();
    let mut app = AppSession::<Alarms>::new(
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
    let mut app = AppSession::<Alarms>::new(
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
