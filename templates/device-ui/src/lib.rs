#![no_std]
use liuli::prelude::*;
use liuli_widgets::Theme;

pub struct Device;
impl App for Device {
    type Model = bool;
    type Msg = ();
    fn update(value: &mut bool, _: (), _: InstantMs) -> Change {
        *value = !*value;
        Change::View
    }
    fn view(value: &bool) -> impl Scene {
        let theme = if *value { Theme::LIGHT } else { Theme::DARK };
        scene(
            Key(1),
            NodeProps {
                bg: Some(theme.background),
                ..NodeProps::new()
            },
            controls(theme),
        )
    }
    fn on_action(_: &bool, event: UiAction) -> Option<()> {
        (event.key == Key(2) && event.kind == ActionKind::Activate).then_some(())
    }
}

fn controls(theme: Theme) -> impl liuli::component::Component {
    liuli::component::leaf(Key(2), NodeKind::Button, move || NodeProps {
        text: TextRef::Static("Theme"),
        ..theme.control()
    })
}
