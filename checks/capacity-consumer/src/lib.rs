use liuli::{
    component::{Bounded, Component, Either, container, leaf},
    prelude::*,
};
use liuli_widgets::{Theme, VirtualRows};
pub struct Product;
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Model {
    pub first: u32,
    pub count: u32,
    pub confirm: bool,
    pub history: bool,
    pub offset: i32,
}
fn screen(m: &Model) -> impl Component + use<> {
    if m.history {
        Either::First(VirtualRows::<1000, 20, 80>::rows(
            [Key(20), Key(21), Key(22)],
            1000,
            m.offset,
            80,
            ScrollPolicy::FREE,
            |id| {
                leaf(Key(1000 + id), NodeKind::Button, || {
                    NodeProps::new().height(Length::Px(20))
                })
            },
        ))
    } else {
        Either::Second((
            container(
                Key(2),
                NodeKind::Column,
                NodeProps::new,
                Bounded::<3>::rows(Key(2), m.first..m.first + m.count, product_controls::record),
            ),
            m.confirm.then(|| {
                liuli_widgets::confirmation(
                    [Key(30), Key(31), Key(32), Key(33)],
                    [TextRef::Static("OK"), TextRef::Static("Back")],
                    &Theme::DARK,
                )
            }),
        ))
    }
}

impl App for Product {
    type Model = Model;
    type Msg = Model;
    fn update(model: &mut Model, msg: Model, _: InstantMs) -> Change {
        *model = msg;
        Change::View
    }
    fn view(m: &Model) -> impl Scene {
        scene(Key(1), NodeProps::new(), screen(m))
    }
    fn on_action(_: &Model, _: UiAction) -> Option<Model> {
        None
    }
}
