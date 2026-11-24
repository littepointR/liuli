//! Reproduce three inspection problems using only the public SDK.
use liuli::prelude::*;
use liuli_sim::session::AppSession;
use liuli_widgets::Theme;

struct Fixture;

#[derive(Debug)]
enum Message {
    Redraw,
    Overflow,
}

#[derive(Default)]
struct Model {
    revision: u16,
    extra_rows: u32,
}

impl App for Fixture {
    type Model = Model;
    type Msg = Message;

    fn update(model: &mut Model, msg: Message, _: InstantMs) -> Change {
        match msg {
            Message::Redraw => model.revision += 1,
            Message::Overflow => model.extra_rows = 64,
        }
        Change::View
    }

    fn view(model: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let fill = NodeProps {
            width: Length::Fill(1),
            height: Length::Fill(1),
            ..NodeProps::new()
        };
        out.root(
            Key(1),
            NodeProps {
                theme_id: model.revision,
                ..fill
            },
            |ui| {
                ui.stack(Key(2), fill, |layers| {
                    layers.button(Key(11), Theme::DARK.control())?;
                    // A transparent overlay mistakenly claims hits meant for button 11.
                    layers.label(
                        Key(12),
                        NodeProps {
                            clickable: true,
                            ..fill
                        },
                    )
                })?;
                for i in 0..model.extra_rows {
                    ui.label(Key(100 + i), NodeProps::new())?;
                }
                Ok(())
            },
        )
    }

    fn on_action(_: &Model, _: UiAction) -> Option<Message> {
        None
    }

    fn page_id(_: &Model) -> u8 {
        7
    }
}

fn main() -> Result<(), String> {
    let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4"))
        .map_err(|err| format!("{err:?}"))?;
    let mut storage = Storage::new();
    let mut app = AppSession::<Fixture>::new(
        &mut storage,
        Model::default(),
        FontSet::single(font),
        Size::new(96, 48),
        InstantMs(0),
        true,
    )?;
    let tree = app.engine().active_tree();
    let pos = Point::new(8, 8);
    let scope = liuli::core::focus::active_scope_root(tree).unwrap();
    let hit = liuli::core::focus::hit_test_in_scope(tree, scope, pos).unwrap();
    assert_eq!(tree.nodes[hit as usize].key, Key(12));
    println!(
        "point={pos:?} intended=11 actual=12 clip={:?}",
        tree.nodes[hit as usize].clip_rect
    );

    app.message(InstantMs(1), Message::Redraw, true)?;
    let id = app.snapshot().frame_id;
    let report = app.frame_report(id).unwrap();
    assert!(report.contains("trigger=message Redraw") && report.contains("changed_pixels=0 "));
    assert!(app.engine().last_metrics().pixels_sent > 0);
    println!("{report}");

    let error = app
        .message(InstantMs(2), Message::Overflow, true)
        .unwrap_err();
    let (page, key, cause) = app.engine().build_failure().unwrap();
    assert_eq!((page, key, cause), (7, Some(Key(160)), BuildError::Nodes));
    assert_eq!(app.snapshot().frame_id, id);
    println!("{error}");
    app.recover(InstantMs(3), |model| model.extra_rows = 1)?;
    assert!(app.engine().active_tree().node_by_key(Key(100)).is_some());
    Ok(())
}
