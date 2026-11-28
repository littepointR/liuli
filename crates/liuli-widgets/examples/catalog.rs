use liuli::prelude::*;
use liuli_sim::{preview::PreviewServer, session::AppSession};
use liuli_widgets::{Theme, confirm_dialog, number_editor, paint_meter, selector, tabs, toast};
struct Catalog;
struct State {
    tab: Key,
    choice: Key,
    value: i16,
    confirm: bool,
    saved: bool,
}
impl App for Catalog {
    type Model = State;
    type Msg = UiAction;
    fn view(m: &State) -> impl Scene {
        let t = Theme::DARK;
        layout(
            Key(1),
            NodeProps {
                bg: Some(t.background),
                ..NodeProps::new()
            },
            move |root| {
                root.column(
                    Key(2),
                    NodeProps {
                        width: Length::Fill(1),
                        gap: t.spacing,
                        ..NodeProps::new()
                    },
                    |ui| {
                        tabs(
                            ui,
                            Key(3),
                            &[
                                (Key(4), TextRef::Static("Controls")),
                                (Key(5), TextRef::Static("Meter")),
                            ],
                            m.tab,
                            t,
                        )?;
                        if ui.variant(m.tab == Key(4), &[true, false]) {
                            selector(
                                ui,
                                Key(6),
                                &[
                                    (Key(7), TextRef::Static("Quiet")),
                                    (Key(8), TextRef::Static("Audible")),
                                ],
                                m.choice,
                                t,
                            )?;
                            number_editor(ui, Key(9), 0..=100, m.value, 5, t)?;
                            ui.button(
                                Key(10),
                                NodeProps {
                                    text: TextRef::Static("Save"),
                                    ..t.control()
                                },
                            )?;
                            if ui.optional(m.saved) {
                                toast(ui, Key(11), TextRef::Static("Saved"), t)?;
                            }
                        } else {
                            ui.custom(
                                Key(12),
                                NodeProps {
                                    value: m.value,
                                    bg: Some(t.surface),
                                    accent: t.accent,
                                    width: Length::Fill(1),
                                    ..NodeProps::new()
                                },
                            )?;
                        }
                        Ok(())
                    },
                )?;
                if root.optional(m.confirm) {
                    confirm_dialog(
                        root,
                        [Key(20), Key(21), Key(22), Key(23)],
                        [TextRef::Static("Confirm"), TextRef::Static("Cancel")],
                        t,
                    )?;
                }
                Ok(())
            },
        )
    }
    fn measure_custom(_: Key, _: FontId, available: Size) -> Size {
        Size::new(available.width, 16)
    }
    fn paint_custom(_: Key, p: &StoredProps, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
        paint_meter(p, c)
    }
    fn on_action(_: &State, a: UiAction) -> Option<UiAction> {
        Some(a)
    }
    fn update(m: &mut State, msg: UiAction, _: InstantMs) -> Change {
        match (msg.key.0, msg.kind) {
            (_, ActionKind::Dismiss) | (23, ActionKind::Activate) => m.confirm = false,
            (4 | 5, ActionKind::Activate) => m.tab = msg.key,
            (7 | 8, ActionKind::Activate) => {
                m.choice = msg.key;
                m.saved = false;
            }
            (9, ActionKind::SetValue(v)) => {
                m.value = v;
                m.saved = false;
            }
            (10, ActionKind::Activate) => m.confirm = true,
            (22, ActionKind::Activate) => {
                m.confirm = false;
                m.saved = true;
            }
            _ => return Change::Unchanged,
        }
        Change::View
    }
}
fn main() -> Result<(), String> {
    let model = State {
        tab: Key(4),
        choice: Key(7),
        value: 60,
        confirm: false,
        saved: false,
    };
    #[cfg(feature = "scene-planner")]
    if std::env::var_os("LIULI_PRINT_CAPACITY").is_some() {
        let d = liuli::scene::capacity::<Catalog>(&model).map_err(|e| format!("{e:?}"))?;
        println!("liuli-capacity-v1 {} {} {}", d.nodes, d.depth, d.text_slots);
        return Ok(());
    }
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: catalog font.lf4 [--headless]")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let font = Lf4Font::from_bytes(&bytes).map_err(|e| format!("{e:?}"))?;
    let mut store = Storage::new();
    let mut app = AppSession::<Catalog>::new(
        &mut store,
        model,
        FontSet::single(font),
        Size::new(240, 536),
        InstantMs(0),
        true,
    )?;
    if std::env::args_os().any(|arg| arg == "--headless") {
        return Ok(());
    }
    let server = PreviewServer::bind(7878)?;
    println!("http://{}", server.addr());
    server.serve_app(&mut app)
}
