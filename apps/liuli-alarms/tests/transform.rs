#![cfg(feature = "enhanced")]
use liuli::prelude::*;
use liuli_sim::session::AppSession;

struct GestureApp;
#[derive(Default)]
struct Model {
    modal: bool,
    removed: bool,
    page: u8,
    cancelled: usize,
    activated: usize,
}
#[derive(Debug, Clone, Copy)]
enum Msg {
    Modal,
    Remove,
    Navigate,
    Cancel,
    Activate,
}
impl App for GestureApp {
    type Model = Model;
    type Msg = Msg;
    fn page_id(m: &Model) -> u8 {
        m.page
    }
    fn update(m: &mut Model, msg: Msg, _: InstantMs) -> Change {
        match msg {
            Msg::Modal => m.modal = true,
            Msg::Remove => m.removed = true,
            Msg::Navigate => m.page += 1,
            Msg::Cancel => m.cancelled += 1,
            Msg::Activate => m.activated += 1,
        }
        Change::View
    }
    fn view(m: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let props = NodeProps {
            width: Length::Px(64),
            height: Length::Px(64),
            focusable: true,
            clickable: true,
            ..NodeProps::new()
        };
        out.root(Key(1), NodeProps::new(), |ui| {
            if !m.removed {
                ui.button(Key(2), props)?;
            }
            if m.modal {
                ui.modal(Key(3), props, |dialog| dialog.button(Key(4), props))?;
            }
            Ok(())
        })
    }
    fn accepts_transform(_: &Model, k: Key) -> bool {
        k == Key(2)
    }
    fn on_transform(_: &Model, key: Key, g: TransformGesture) -> Option<Msg> {
        assert_eq!(key, Key(2));
        (g.phase == GesturePhase::Cancel).then_some(Msg::Cancel)
    }
    fn on_action(_: &Model, a: UiAction) -> Option<Msg> {
        (a.kind == ActionKind::Activate).then_some(Msg::Activate)
    }
}
#[test]
fn view_commit_cancels_pair_before_next_input_and_drains_releases() {
    for mutation in [Msg::Modal, Msg::Remove, Msg::Navigate] {
        let mut storage = Storage::new();
        let font = Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).unwrap();
        let mut s = AppSession::<GestureApp>::new(
            &mut storage,
            Model::default(),
            FontSet::single(font),
            Size::new(64, 64),
            InstantMs(0),
            true,
        )
        .unwrap();
        for id in [2, 3] {
            s.step(
                InstantMs(id.into()),
                Some(RawInput::PointerDown {
                    id: PointerId(id),
                    pos: Point::new(5 * i32::from(id), 10),
                }),
                true,
            )
            .unwrap();
        }
        s.message(InstantMs(4), mutation, true).unwrap();
        assert_eq!(s.engine().model().cancelled, 1);
        for id in [3, 2] {
            s.step(
                InstantMs(6),
                Some(RawInput::PointerUp {
                    id: PointerId(id),
                    pos: Point::new(12, 10),
                }),
                true,
            )
            .unwrap();
        }
        assert_eq!(s.engine().model().activated, 0);
        assert_eq!(s.engine().model().cancelled, 1);
    }
}
