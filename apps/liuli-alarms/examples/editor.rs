use liuli::{
    enhanced::{
        ShapingFont,
        field::{FieldStyle, TextField},
    },
    prelude::*,
};
use liuli_sim::{
    preview::{PreviewServer, replay_trace},
    session::AppSession,
};
use std::marker::PhantomData;

const FIELD: Key = Key(2);
struct Editing<'a>(PhantomData<&'a ()>);
enum Edit {
    Compose(String),
    Commit(String),
    Key(TextInput<'static>),
}

impl<'a> App for Editing<'a> {
    type Model = TextField<'a>;
    type Msg = Edit;
    fn view(field: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        out.root(Key(1), NodeProps::new(), |ui| {
            ui.custom(
                FIELD,
                NodeProps {
                    width: Length::Px(240),
                    height: Length::Px(48),
                    focusable: true,
                    clickable: true,
                    theme_id: field.revision(),
                    ..NodeProps::new()
                },
            )
        })
    }
    fn paint_model(
        field: &Self::Model,
        _key: Key,
        _props: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        field.paint(canvas)
    }
    fn on_action(_: &Self::Model, _: UiAction) -> Option<Edit> {
        None
    }
    fn accepts_text(_: &Self::Model, key: Key) -> bool {
        key == FIELD
    }
    fn on_text(_: &Self::Model, _: Key, input: TextInput<'_>) -> Option<Edit> {
        Some(match input {
            TextInput::Compose(s) => Edit::Compose(s.into()),
            TextInput::Commit(s) => Edit::Commit(s.into()),
            TextInput::Backspace => Edit::Key(TextInput::Backspace),
            TextInput::Cancel => Edit::Key(TextInput::Cancel),
            TextInput::Left => Edit::Key(TextInput::Left),
            TextInput::Right => Edit::Key(TextInput::Right),
        })
    }
    fn update(field: &mut Self::Model, msg: Edit, _: InstantMs) -> Change {
        let input = match &msg {
            Edit::Compose(s) => TextInput::Compose(s),
            Edit::Commit(s) => TextInput::Commit(s),
            Edit::Key(i) => *i,
        };
        match field.apply(input) {
            Ok(()) => Change::View,
            Err(e) => {
                eprintln!("editor: {e:?}");
                Change::Unchanged
            }
        }
    }
}

fn main() -> Result<(), String> {
    let mut cli = std::env::args().skip(1);
    let mut headless = false;
    let mut replay = None;
    let mut sources = Vec::new();
    while let Some(arg) = cli.next() {
        match arg.as_str() {
            "--headless" => headless = true,
            "--replay" => replay = Some(cli.next().ok_or("--replay needs a trace file")?),
            _ => sources.push(std::fs::read(&arg).map_err(|e| format!("{arg}: {e}"))?),
        }
    }
    let mut faces = vec![
        ShapingFont::from_bytes(include_bytes!("../assets/example.ttf"), 0)
            .ok_or("invalid product font")?,
    ];
    for source in &sources {
        faces.push(ShapingFont::from_bytes(source, 0).ok_or("invalid fallback font")?);
    }
    let field = TextField::new(
        "告警 A".into(),
        faces,
        FieldStyle {
            width: 240,
            height: 48,
            px: 24,
            foreground: Rgb565::new(31, 63, 31),
            background: Rgb565::new(1, 4, 2),
        },
        256,
    )
    .map_err(|e| format!("{e:?}"))?;
    let font =
        Lf4Font::from_bytes(include_bytes!("../assets/body.lf4")).map_err(|e| format!("{e:?}"))?;
    let mut storage = Storage::new();
    let mut app = AppSession::<Editing>::new(
        &mut storage,
        field,
        FontSet::single(font),
        Size::new(240, 48),
        InstantMs(0),
        true,
    )?;
    if let Some(path) = replay {
        return replay_trace(
            &mut app,
            &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        );
    }
    if headless {
        let before = app.screen().words().to_vec();
        app.text(InstantMs(1), TextInput::Compose("سل"), true)?;
        assert_ne!(before, app.screen().words());
        app.text(InstantMs(2), TextInput::Cancel, true)?;
        assert_eq!(before, app.screen().words());
        replay_trace(
            &mut app,
            "liuli-trace 1\n3 text compose e0a495e0a4bf\n4 text commit e0a495e0a4bf\n5 text backspace \n",
        )?;
        assert_eq!(app.engine().model().editor().text(), "告警 A");
        assert_eq!(before, app.screen().words());
        return Ok(());
    }
    let server = PreviewServer::bind(7878)?;
    println!("http://{}", server.addr());
    server.serve_app(&mut app)
}
