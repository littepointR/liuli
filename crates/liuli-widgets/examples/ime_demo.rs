use liuli::prelude::*;
use liuli_sim::{preview::PreviewServer, session::AppSession};
use liuli_widgets::{
    DEFAULT_PINYIN_DICT, FixedTextEditor, ImeEvent, ImeState, InputMode, Theme,
    paint_soft_keyboard, paint_text_box, soft_keyboard, soft_keyboard_on_pointer, text_box,
    text_box_on_pointer,
};
use std::path::{Path, PathBuf};

const BODY_LF4: &[u8] = include_bytes!("assets/ime_body16.lf4");
const SMALL12_LF4: &[u8] = include_bytes!("assets/ime_small12.lf4");
const IME_CHARSET: &str = include_str!("assets/ime_charset.txt");

const KEY_INPUT: Key = Key(10);
const KEY_KEYBOARD: Key = Key(20);

enum ImeMsg {
    ExternalText(TextInputBuf),
    KeyboardPointer(PointerEvent),
    TextBoxPointer(PointerEvent),
}

enum TextInputBuf {
    Compose(String),
    Commit(String),
    Cancel,
    Backspace,
    Left,
    Right,
}

struct DemoModel {
    editor: FixedTextEditor<128>,
    ime: ImeState,
    font: Lf4Font<'static>,
    small: Lf4Font<'static>,
    done_count: u16,
    last_done: String,
    theme: Theme,
}

struct ImeDemoApp;

impl App for ImeDemoApp {
    type Model = DemoModel;
    type Msg = ImeMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            ImeMsg::ExternalText(buf) => {
                let ti = match &buf {
                    TextInputBuf::Compose(s) => TextInput::Compose(s),
                    TextInputBuf::Commit(s) => TextInput::Commit(s),
                    TextInputBuf::Cancel => TextInput::Cancel,
                    TextInputBuf::Backspace => TextInput::Backspace,
                    TextInputBuf::Left => TextInput::Left,
                    TextInputBuf::Right => TextInput::Right,
                };
                if model.editor.apply(ti) {
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            ImeMsg::KeyboardPointer(ev) => {
                let r0 = model.ime.revision();
                let e0 = model.editor.revision();
                let ime_ev = soft_keyboard_on_pointer(&mut model.ime, &DEFAULT_PINYIN_DICT, ev);
                let done = matches!(ime_ev, ImeEvent::Done);
                model.editor.apply_ime(ime_ev);
                if done {
                    model.ime.clear_composing();
                    model.done_count += 1;
                    model.last_done = model.editor.text().to_string();
                }
                if model.ime.revision() != r0 || model.editor.revision() != e0 || done {
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            ImeMsg::TextBoxPointer(ev) => {
                if text_box_on_pointer(&mut model.editor, ev, Some(&model.font)) {
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
        }
    }

    fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let t = model.theme;
        out.root(
            Key(1),
            NodeProps {
                bg: Some(t.background),
                width: Length::Fill(1),
                height: Length::Fill(1),
                padding: EdgeInsets::all(6),
                ..NodeProps::new()
            },
            |root| {
                root.column(
                    Key(2),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        gap: 6,
                        ..NodeProps::new()
                    },
                    |col| {
                        let status = col.text_fmt(format_args!(
                            "IME [{}]  OK:{}",
                            model.ime.mode().label(),
                            model.done_count
                        ))?;
                        col.label(
                            Key(3),
                            NodeProps {
                                text: status,
                                fg: t.text_muted,
                                font: FontId::Small12,
                                width: Length::Fill(1),
                                height: Length::Px(16),
                                ..NodeProps::new()
                            },
                        )?;
                        text_box(col, KEY_INPUT, &model.editor, "Type text...", t)?;
                        col.label(
                            Key(4),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Fill(1),
                                ..NodeProps::new()
                            },
                        )?;
                        soft_keyboard(col, KEY_KEYBOARD, &model.ime, 146, t)
                    },
                )
            },
        )
    }

    fn accepts_text(_model: &Self::Model, key: Key) -> bool {
        key == KEY_INPUT
    }

    fn on_text(_model: &Self::Model, _: Key, input: TextInput<'_>) -> Option<Self::Msg> {
        Some(ImeMsg::ExternalText(match input {
            TextInput::Compose(s) => TextInputBuf::Compose(s.into()),
            TextInput::Commit(s) => TextInputBuf::Commit(s.into()),
            TextInput::Cancel => TextInputBuf::Cancel,
            TextInput::Backspace => TextInputBuf::Backspace,
            TextInput::Left => TextInputBuf::Left,
            TextInput::Right => TextInputBuf::Right,
        }))
    }

    fn on_pointer(_model: &Self::Model, key: Key, ev: PointerEvent) -> Option<Self::Msg> {
        match key {
            KEY_INPUT => Some(ImeMsg::TextBoxPointer(ev)),
            KEY_KEYBOARD => Some(ImeMsg::KeyboardPointer(ev)),
            _ => None,
        }
    }

    fn paint_model(
        model: &Self::Model,
        key: Key,
        props: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        match key {
            KEY_INPUT => paint_text_box(
                &model.editor,
                "Type text...",
                props,
                canvas.state,
                Some(&model.font),
                canvas,
            ),
            KEY_KEYBOARD => {
                paint_soft_keyboard(&model.ime, &DEFAULT_PINYIN_DICT, props, model.theme, canvas)
            }
            _ => Ok(()),
        }
    }

    fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
        None
    }
}

fn tap_key(
    session: &mut AppSession<'_, '_, ImeDemoApp>,
    clock: &mut u64,
    label: &str,
) -> Result<(), String> {
    let kb = session
        .engine()
        .active_tree()
        .node_by_key(KEY_KEYBOARD)
        .ok_or("missing soft keyboard node")?
        .layout_rect;
    let kr = session
        .engine()
        .model()
        .ime
        .key_rect(&DEFAULT_PINYIN_DICT, kb.size(), label)
        .ok_or_else(|| {
            format!(
                "missing keycap '{label}' in mode {:?}",
                session.engine().model().ime.mode()
            )
        })?;
    let pos = Point::new(kb.x + kr.x + kr.w as i32 / 2, kb.y + kr.y + kr.h as i32 / 2);
    let id = PointerId(1);
    *clock += 16;
    session.step(
        InstantMs(*clock),
        Some(RawInput::PointerDown { id, pos }),
        true,
    )?;
    *clock += 16;
    session.step(
        InstantMs(*clock),
        Some(RawInput::PointerUp { id, pos }),
        true,
    )?;
    Ok(())
}

fn save_shot(
    session: &AppSession<'_, '_, ImeDemoApp>,
    dir: Option<&Path>,
    name: &str,
) -> Result<(), String> {
    if let Some(d) = dir {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        std::fs::write(d.join(name), session.screen().to_png()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run_headless(
    session: &mut AppSession<'_, '_, ImeDemoApp>,
    shots: Option<&Path>,
) -> Result<(), String> {
    {
        let m = session.engine().model();
        for ch in IME_CHARSET.chars() {
            if matches!(ch, '\r' | '\n') {
                continue;
            }
            assert!(
                m.font.lookup_exact(ch).is_some(),
                "ime_body16.lf4 missing glyph U+{:04X} ('{ch}')",
                ch as u32
            );
            assert!(
                m.small.lookup_exact(ch).is_some(),
                "ime_small12.lf4 missing glyph U+{:04X} ('{ch}')",
                ch as u32
            );
        }
    }

    let mut clock = 0u64;
    for ch in ["n", "i"] {
        tap_key(session, &mut clock, ch)?;
    }
    assert_eq!(session.engine().model().editor.preedit(), "ni");
    assert!(
        session
            .engine()
            .model()
            .ime
            .key_rect(&DEFAULT_PINYIN_DICT, Size::new(320, 146), "你")
            .is_some()
    );
    save_shot(session, shots, "01-pinyin.png")?;

    for ch in ["h", "a", "o"] {
        tap_key(session, &mut clock, ch)?;
    }
    assert_eq!(session.engine().model().editor.preedit(), "nihao");
    tap_key(session, &mut clock, "你好")?;
    assert_eq!(session.engine().model().editor.text(), "你好");
    assert_eq!(session.engine().model().editor.preedit(), "");

    for ch in ["w", "o", "s", "h", "i"] {
        tap_key(session, &mut clock, ch)?;
    }
    assert_eq!(session.engine().model().editor.preedit(), "woshi");
    tap_key(session, &mut clock, "我")?;
    assert_eq!(session.engine().model().editor.text(), "你好我");
    assert_eq!(session.engine().model().editor.preedit(), "shi");
    save_shot(session, shots, "02-continuous.png")?;

    tap_key(session, &mut clock, "是")?;
    assert_eq!(session.engine().model().editor.text(), "你好我是");

    for ch in ["s", "h", "e"] {
        tap_key(session, &mut clock, ch)?;
    }
    tap_key(session, &mut clock, "设备")?;
    assert_eq!(session.engine().model().editor.text(), "你好我是设备");

    tap_key(session, &mut clock, "T9")?;
    for k in ["9 WXYZ", "3 DEF", "6 MNO"] {
        tap_key(session, &mut clock, k)?;
    }
    assert_eq!(session.engine().model().editor.preedit(), "936");
    save_shot(session, shots, "03-t9.png")?;

    tap_key(session, &mut clock, "温度")?;
    assert_eq!(session.engine().model().editor.text(), "你好我是设备温度");

    tap_key(session, &mut clock, "123")?;
    tap_key(session, &mut clock, "2")?;
    tap_key(session, &mut clock, "5")?;

    tap_key(session, &mut clock, "Pin")?;
    tap_key(session, &mut clock, "En")?;
    tap_key(session, &mut clock, "Cap")?;
    tap_key(session, &mut clock, "C")?;
    tap_key(session, &mut clock, "OK")?;
    assert_eq!(
        session.engine().model().editor.text(),
        "你好我是设备温度25C"
    );
    assert_eq!(session.engine().model().last_done, "你好我是设备温度25C");
    save_shot(session, shots, "04-committed.png")?;
    Ok(())
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut headless = false;
    let mut shots: Option<PathBuf> = None;
    let mut port = 7878u16;
    let mut size = Size::new(320, 240);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => headless = true,
            "--screenshots" => {
                shots = Some(PathBuf::from(
                    args.next().ok_or("missing --screenshots dir")?,
                ));
            }
            "--size" => {
                size = match args.next().as_deref() {
                    Some("320x240") => Size::new(320, 240),
                    Some("240x536") => Size::new(240, 536),
                    Some("800x480") => Size::new(800, 480),
                    _ => return Err("expected --size 320x240|240x536|800x480".into()),
                };
            }
            "--port" => {
                port = args
                    .next()
                    .ok_or("missing --port")?
                    .parse()
                    .map_err(|_| "invalid port")?;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    let body = Lf4Font::from_bytes(BODY_LF4).map_err(|e| format!("{e:?}"))?;
    let small = Lf4Font::from_bytes(SMALL12_LF4).map_err(|e| format!("{e:?}"))?;
    let mut storage = Storage::new();
    let mut session = AppSession::<ImeDemoApp>::new(
        &mut storage,
        DemoModel {
            editor: FixedTextEditor::new(),
            ime: ImeState::new(InputMode::ZhPinyin),
            font: body,
            small,
            done_count: 0,
            last_done: String::new(),
            theme: Theme::DARK,
        },
        FontSet::new(body, small),
        size,
        InstantMs(0),
        true,
    )?;

    if headless || shots.is_some() {
        return run_headless(&mut session, shots.as_deref());
    }

    let server = PreviewServer::bind(port)?;
    println!("IME Demo http://{}", server.addr());
    server.serve_app(&mut session)
}
