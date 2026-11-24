use liuli::prelude::*;
use liuli_alarms::{Alarms, Model, Msg, Page};
use liuli_sim::session::AppSession;
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
