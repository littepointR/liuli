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
    fn view(value: &bool, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let theme = if *value { Theme::LIGHT } else { Theme::DARK };
        out.root(
            Key(1),
            NodeProps {
                bg: Some(theme.background),
                ..NodeProps::new()
            },
            |ui| {
                ui.button(
                    Key(2),
                    NodeProps {
                        text: TextRef::Static("Theme"),
                        ..theme.control()
                    },
                )
            },
        )
    }
    fn on_action(_: &bool, event: UiAction) -> Option<()> {
        (event.key == Key(2) && event.kind == ActionKind::Activate).then_some(())
    }
}
