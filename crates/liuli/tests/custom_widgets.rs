use core::convert::Infallible;
use std::sync::Mutex;

use embedded_graphics_core::Pixel;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use liuli::core::paint::PaintCmd;
use liuli::prelude::*;

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

struct VecTarget {
    w: u32,
    h: u32,
    fb: Vec<u16>,
}

impl VecTarget {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            fb: vec![0; (w * h) as usize],
        }
    }

    fn pixel(&self, x: i32, y: i32) -> u16 {
        self.fb[(y as usize) * (self.w as usize) + (x as usize)]
    }
}

fn rect_center(r: Rect) -> Point {
    Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
}

impl OriginDimensions for VecTarget {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for VecTarget {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.x < self.w as i32 && p.y >= 0 && p.y < self.h as i32 {
                self.fb[(p.y as usize) * (self.w as usize) + (p.x as usize)] = c.into_storage();
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 1. PartStyles & VisualState & Scrollbar hide & Incremental Dirtying
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct PartsModel {
    right_icon: bool,
    hide_scrollbar: bool,
    btn_enabled: bool,
    slider_ind: Rgb565,
}

#[derive(Clone, Copy, Debug)]
enum PartsMsg {
    RightIcon(bool),
    HideScrollbar(bool),
    Enabled(bool),
    SliderInd(Rgb565),
}

struct PartsApp;

impl App for PartsApp {
    type Model = PartsModel;
    type Msg = PartsMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            PartsMsg::RightIcon(v) => model.right_icon = v,
            PartsMsg::HideScrollbar(v) => model.hide_scrollbar = v,
            PartsMsg::Enabled(v) => model.btn_enabled = v,
            PartsMsg::SliderInd(c) => model.slider_ind = c,
        }
        Change::View
    }

    fn view(model: &Self::Model) -> impl Scene {
        layout(
            Key(1),
            NodeProps {
                bg: Some(Rgb565::new(2, 4, 2)),
                width: Length::Fill(1),
                height: Length::Fill(1),
                padding: EdgeInsets::all(6),
                ..NodeProps::new()
            },
            move |ui| {
                ui.column(
                    Key(2),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        gap: 6,
                        ..NodeProps::new()
                    },
                    |col| {
                        col.slider(
                            Key(10),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(24),
                                bg: Some(Rgb565::new(6, 12, 6)),
                                accent: Rgb565::new(0, 63, 0),
                                fg: Rgb565::new(31, 63, 31),
                                value: 50,
                                ..NodeProps::new()
                            },
                        )?;
                        col.style_parts(
                            Key(10),
                            WidgetPartStyle::new()
                                .with_track_color(Rgb565::new(4, 8, 12))
                                .with_indicator_color(model.slider_ind)
                                .with_knob_color(Rgb565::new(31, 56, 0))
                                .with_knob_size(14, 14)
                                .with_knob_radius(7),
                        )?;

                        col.toggle(
                            Key(11),
                            NodeProps {
                                width: Length::Px(48),
                                height: Length::Px(26),
                                checked: true,
                                ..NodeProps::new()
                            },
                        )?;
                        col.style_parts(
                            Key(11),
                            WidgetPartStyle::new()
                                .with_indicator_color(Rgb565::new(28, 10, 8))
                                .with_knob_color(Rgb565::new(31, 63, 31))
                                .with_knob_radius(2),
                        )?;

                        col.button(
                            Key(12),
                            NodeProps {
                                width: Length::Px(140),
                                height: Length::Px(32),
                                text: TextRef::Static("Action"),
                                icon: IconKind::Check,
                                bg: Some(Rgb565::new(6, 16, 20)),
                                fg: Rgb565::new(31, 63, 31),
                                enabled: model.btn_enabled,
                                clickable: true,
                                focusable: true,
                                ..NodeProps::new()
                            },
                        )?;
                        col.style_parts(
                            Key(12),
                            WidgetPartStyle::new()
                                .with_icon_pos(if model.right_icon {
                                    IconPosition::Right
                                } else {
                                    IconPosition::Left
                                })
                                .with_icon_size(12)
                                .with_icon_gap(6)
                                .with_pressed_bg(Rgb565::new(30, 6, 4))
                                .with_pressed_fg(Rgb565::new(31, 63, 0))
                                .with_disabled_bg(Rgb565::new(5, 10, 5))
                                .with_disabled_fg(Rgb565::new(12, 24, 12)),
                        )?;

                        col.progress_bar(
                            Key(13),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(14),
                                value: 60,
                                ..NodeProps::new()
                            },
                        )?;
                        col.style_parts(
                            Key(13),
                            WidgetPartStyle::new()
                                .with_track_color(Rgb565::new(3, 6, 3))
                                .with_indicator_color(Rgb565::new(0, 48, 31))
                                .with_indicator_inset(2),
                        )?;

                        col.scroll_y(
                            Key(14),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(40),
                                accent: Rgb565::new(0, 63, 31),
                                ..NodeProps::new()
                            },
                            |sc| {
                                sc.label(
                                    Key(15),
                                    NodeProps {
                                        width: Length::Fill(1),
                                        height: Length::Px(120),
                                        text: TextRef::Static("Scrollable"),
                                        fg: Rgb565::new(31, 63, 31),
                                        ..NodeProps::new()
                                    },
                                )
                            },
                        )?;
                        col.style_parts(
                            Key(14),
                            WidgetPartStyle::new()
                                .with_knob_color(Rgb565::new(31, 63, 0))
                                .with_knob_size(5, 0)
                                .with_hide_scrollbar(model.hide_scrollbar),
                        )
                    },
                )
            },
        )
    }

    fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
        None
    }
}

#[test]
fn part_styles_and_visual_states_customize_parts_and_dirty_incrementally() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 220, 200);
    let mut storage = Storage::new();
    let mut engine = Engine::<PartsApp>::new_in_place(
        &mut storage,
        PartsModel {
            right_icon: false,
            hide_scrollbar: false,
            btn_enabled: true,
            slider_ind: Rgb565::new(0, 63, 31),
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();

    let mut target = VecTarget::new(220, 200);
    let mut scratch = [0u16; 220 * 8];
    engine.present_sync(&mut target, &mut scratch).unwrap();

    // Check scrollbar FillRoundedRect exists when not hidden, and button icon starts left of text.
    let find_btn_icon_x = |cmds: &[PaintCmd]| {
        cmds.iter().find_map(|c| match c {
            PaintCmd::Icon { rect, .. } => Some(rect.x),
            _ => None,
        })
    };
    let find_btn_text_x = |cmds: &[PaintCmd]| {
        cmds.iter().find_map(|c| match c {
            PaintCmd::Text { pos, color, .. } => Some((pos.x, *color)),
            _ => None,
        })
    };

    let ix_left = find_btn_icon_x(engine.active_commands().as_slice()).unwrap();
    let (tx_left, _) = find_btn_text_x(engine.active_commands().as_slice()).unwrap();
    assert!(ix_left < tx_left);

    let scroll_rect = engine
        .active_tree()
        .node_by_key(Key(14))
        .unwrap()
        .layout_rect;
    let has_sb_before = engine.active_commands().as_slice().iter().any(|c| {
        matches!(
            c,
            PaintCmd::FillRoundedRect { color, rect, .. }
            if *color == Rgb565::new(31, 63, 0)
                && rect.x >= scroll_rect.x + scroll_rect.w as i32 - 8
        )
    });
    assert!(has_sb_before);

    // Pressing button triggers pressed_bg / pressed_fg from WidgetPartStyle.
    let btn_rect = engine
        .active_tree()
        .node_by_key(Key(12))
        .unwrap()
        .layout_rect;
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: rect_center(btn_rect),
            },
            InstantMs(10),
            false,
        )
        .unwrap();
    let pressed_bg_seen = engine.active_commands().as_slice().iter().any(|c| match c {
        PaintCmd::FillRect { rect, color, .. } | PaintCmd::FillRoundedRect { rect, color, .. } => {
            *rect == btn_rect && *color == Rgb565::new(30, 6, 4)
        }
        _ => false,
    });
    assert!(pressed_bg_seen);
    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert_eq!(
        target.pixel(btn_rect.x + 2, btn_rect.y + 2),
        liuli::core::geometry::rgb565_to_u16(Rgb565::new(30, 6, 4))
    );
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: rect_center(btn_rect),
            },
            InstantMs(20),
            false,
        )
        .unwrap();
    engine.present_sync(&mut target, &mut scratch).unwrap();

    // Update ONLY part styles (right icon + slider indicator color + hide scrollbar + disabled button):
    engine
        .try_message(PartsMsg::RightIcon(true), InstantMs(30), false)
        .unwrap();
    assert!(!engine.damage().is_empty());
    let ix_right = find_btn_icon_x(engine.active_commands().as_slice()).unwrap();
    let (tx_right, _) = find_btn_text_x(engine.active_commands().as_slice()).unwrap();
    assert!(ix_right > tx_right);

    engine
        .try_message(PartsMsg::HideScrollbar(true), InstantMs(40), false)
        .unwrap();
    let has_sb_after = engine.active_commands().as_slice().iter().any(|c| {
        matches!(
            c,
            PaintCmd::FillRoundedRect { color, rect, .. }
            if *color == Rgb565::new(31, 63, 0)
                && rect.x >= scroll_rect.x + scroll_rect.w as i32 - 8
        )
    });
    assert!(!has_sb_after);

    engine
        .try_message(
            PartsMsg::SliderInd(Rgb565::new(31, 0, 31)),
            InstantMs(50),
            false,
        )
        .unwrap();
    assert!(engine.active_commands().as_slice().iter().any(|c| {
        matches!(c, PaintCmd::FillRoundedRect { color, .. } if *color == Rgb565::new(31, 0, 31))
    }));

    engine
        .try_message(PartsMsg::Enabled(false), InstantMs(60), false)
        .unwrap();
    let (_, btn_fg) = find_btn_text_x(engine.active_commands().as_slice()).unwrap();
    assert_eq!(btn_fg, Rgb565::new(12, 24, 12));
}

// ---------------------------------------------------------------------------
// 2. Layer-2 PaintMode::Override preserves Slider drag/edit & Toggle anim
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct OverrideModel {
    slider: i16,
    checked: bool,
}

#[derive(Clone, Copy)]
enum OverrideMsg {
    SetSlider(i16),
    Toggle,
}

static TOGGLE_PROGRESS_SEEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());

struct OverrideApp;

impl App for OverrideApp {
    type Model = OverrideModel;
    type Msg = OverrideMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            OverrideMsg::SetSlider(v) => model.slider = v.clamp(0, 100),
            OverrideMsg::Toggle => model.checked = !model.checked,
        }
        Change::View
    }

    fn view(model: &Self::Model) -> impl Scene {
        layout(
            Key(1),
            NodeProps {
                bg: Some(Rgb565::new(2, 4, 2)),
                width: Length::Fill(1),
                height: Length::Fill(1),
                padding: EdgeInsets::all(8),
                ..NodeProps::new()
            },
            move |ui| {
                ui.column(
                    Key(2),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        gap: 10,
                        ..NodeProps::new()
                    },
                    |col| {
                        col.slider(
                            Key(20),
                            NodeProps {
                                width: Length::Px(140),
                                height: Length::Px(28),
                                min: 0,
                                max: 100,
                                step: 10,
                                value: model.slider,
                                fg: Rgb565::new(31, 63, 31),
                                accent: Rgb565::new(0, 56, 31),
                                focus_color: Rgb565::new(31, 63, 0),
                                clickable: true,
                                focusable: true,
                                paint_mode: PaintMode::Override,
                                ..NodeProps::new()
                            },
                        )?;
                        col.toggle(
                            Key(21),
                            NodeProps {
                                width: Length::Px(48),
                                height: Length::Px(26),
                                checked: model.checked,
                                fg: Rgb565::new(31, 63, 31),
                                accent: Rgb565::new(0, 63, 16),
                                clickable: true,
                                focusable: true,
                                paint_mode: PaintMode::Override,
                                ..NodeProps::new()
                            },
                        )
                    },
                )
            },
        )
    }

    fn paint_override(
        _model: &Self::Model,
        key: Key,
        _kind: NodeKind,
        props: &StoredProps,
        state: PaintState,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<PaintOverride, BuildError> {
        if key == Key(20) {
            canvas.paint_slider_track()?;
            let sz = canvas.size();
            let usable = sz.width.saturating_sub(12);
            let cx = 6 + ((i64::from(usable) * i64::from(props.value)) / 100) as i32;
            canvas.circle(
                Point::new(cx, (sz.height / 2) as i32),
                7,
                Some(props.fg),
                Some(if state.editing {
                    props.focus_color
                } else {
                    props.accent
                }),
                2,
            )?;
            return Ok(PaintOverride::Handled);
        }
        if key == Key(21) {
            TOGGLE_PROGRESS_SEEN
                .lock()
                .unwrap()
                .push(state.anim_progress);
            canvas.paint_default()?;
            return Ok(PaintOverride::Handled);
        }
        Ok(PaintOverride::Default)
    }

    fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
        match (event.key, event.kind) {
            (Key(20), ActionKind::SetValue(v)) => Some(OverrideMsg::SetSlider(v)),
            (Key(21), ActionKind::Toggle) => Some(OverrideMsg::Toggle),
            _ => None,
        }
    }
}

#[test]
fn l2_paint_override_keeps_slider_interaction_and_receives_toggle_animation() {
    TOGGLE_PROGRESS_SEEN.lock().unwrap().clear();
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 200, 140);
    let mut storage = Storage::new();
    let mut engine = Engine::<OverrideApp>::new_in_place(
        &mut storage,
        OverrideModel {
            slider: 20,
            checked: false,
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();

    // Custom Circle knob should be emitted for Slider.
    assert!(
        engine
            .active_commands()
            .as_slice()
            .iter()
            .any(|c| matches!(c, PaintCmd::Circle { radius: 7, .. }))
    );

    // Drag slider near right end -> SetValue >= 80.
    let srect = engine
        .active_tree()
        .node_by_key(Key(20))
        .unwrap()
        .layout_rect;
    let y = srect.y + (srect.h as i32 / 2);
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(srect.x + 20, y),
            },
            InstantMs(10),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(srect.x + srect.w as i32 - 8, y),
            },
            InstantMs(20),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(srect.x + srect.w as i32 - 8, y),
            },
            InstantMs(30),
            false,
        )
        .unwrap();
    let dragged_val = engine.model().slider;
    assert!(dragged_val >= 80);

    // Keyboard edit on overridden Slider.
    engine
        .dispatch(RawInput::Activate, InstantMs(40), false)
        .unwrap();
    assert!(engine.focus().editing);
    engine
        .dispatch(RawInput::Adjust(-1), InstantMs(50), false)
        .unwrap();
    assert_eq!(engine.model().slider, dragged_val - 10);
    engine
        .dispatch(RawInput::Activate, InstantMs(60), false)
        .unwrap();
    assert!(!engine.focus().editing);

    // Toggle click & mid-flight animation progress seen inside paint_override.
    TOGGLE_PROGRESS_SEEN.lock().unwrap().clear();
    let trect = engine
        .active_tree()
        .node_by_key(Key(21))
        .unwrap()
        .layout_rect;
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(2),
                pos: rect_center(trect),
            },
            InstantMs(70),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(2),
                pos: rect_center(trect),
            },
            InstantMs(80),
            false,
        )
        .unwrap();
    assert!(engine.model().checked);
    engine.advance(InstantMs(140), false).unwrap();
    engine.advance(InstantMs(260), false).unwrap();

    let seen = TOGGLE_PROGRESS_SEEN.lock().unwrap();
    assert!(seen.iter().any(|&p| p > 0 && p < 1000));
    assert_eq!(*seen.last().unwrap(), 1000);
}

// ---------------------------------------------------------------------------
// 3. Layer-3 Custom Widget: Capture LockX/Free vs ScrollY, Adjust, LongPress, Rev, Anim
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct CustomModel {
    adj_sum: i16,
    long_pressed: bool,
    rev: u32,
}

#[derive(Clone, Copy, Debug)]
enum CustomMsg {
    Pointer(Key, PointerEvent),
    Adjust(i16),
    LongPressed,
    BumpRev,
}

static POINTER_LOG: Mutex<Vec<(Key, PointerPhase, Point, Point)>> = Mutex::new(Vec::new());

struct CustomApp;

impl App for CustomApp {
    type Model = CustomModel;
    type Msg = CustomMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            CustomMsg::Pointer(k, ev) => {
                POINTER_LOG
                    .lock()
                    .unwrap()
                    .push((k, ev.phase, ev.local, ev.delta));
                Change::Unchanged
            }
            CustomMsg::Adjust(d) => {
                model.adj_sum += d;
                Change::View
            }
            CustomMsg::LongPressed => {
                model.long_pressed = true;
                Change::View
            }
            CustomMsg::BumpRev => {
                model.rev += 1;
                Change::View
            }
        }
    }

    fn view(model: &Self::Model) -> impl Scene {
        layout(
            Key(1),
            NodeProps {
                bg: Some(Rgb565::new(2, 4, 2)),
                width: Length::Fill(1),
                height: Length::Fill(1),
                ..NodeProps::new()
            },
            move |ui| {
                ui.scroll_y(
                    Key(30),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        gap: 6,
                        ..NodeProps::new()
                    },
                    |sc| {
                        sc.custom(
                            Key(33),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(44),
                                pointer_capture: PointerCaptureMode::LockX,
                                editable: true,
                                focusable: true,
                                clickable: true,
                                long_press: true,
                                fg: Rgb565::new(31, 63, 31),
                                ..NodeProps::new()
                            },
                        )?;
                        sc.set_custom_rev(Key(33), model.rev)?;
                        sc.custom(
                            Key(34),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(44),
                                pointer_capture: PointerCaptureMode::Free,
                                focusable: true,
                                clickable: true,
                                fg: Rgb565::new(0, 63, 31),
                                ..NodeProps::new()
                            },
                        )?;
                        sc.label(
                            Key(31),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(140),
                                text: TextRef::Static("Tail"),
                                fg: Rgb565::new(31, 63, 31),
                                ..NodeProps::new()
                            },
                        )
                    },
                )
            },
        )
    }

    fn paint_model(
        model: &Self::Model,
        key: Key,
        props: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        let color = if model.rev > 0 {
            Rgb565::new(31, 0, 0)
        } else {
            props.fg
        };
        if key == Key(33) {
            canvas.circle(Point::new(22, 22), 10, Some(color), None, 0)?;
        } else if key == Key(34) {
            let r = (8 + canvas.state.anim_progress / 100) as u16;
            canvas.circle(Point::new(22, 22), r, Some(props.fg), None, 0)?;
        }
        Ok(())
    }

    fn on_pointer(_model: &Self::Model, key: Key, event: PointerEvent) -> Option<Self::Msg> {
        Some(CustomMsg::Pointer(key, event))
    }

    fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
        match (event.key, event.kind) {
            (Key(33), ActionKind::Adjust(d)) => Some(CustomMsg::Adjust(d)),
            (Key(33), ActionKind::LongPress) => Some(CustomMsg::LongPressed),
            _ => None,
        }
    }
}

#[test]
fn l3_custom_widget_arbitration_adjust_longpress_rev_and_anim() {
    POINTER_LOG.lock().unwrap().clear();
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 180, 120);
    let mut storage = Storage::new();
    let mut engine = Engine::<CustomApp>::new_in_place(
        &mut storage,
        CustomModel {
            adj_sum: 0,
            long_pressed: false,
            rev: 0,
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();
    let mut target = VecTarget::new(180, 120);
    let mut scratch = [0u16; 180 * 8];
    engine.present_sync(&mut target, &mut scratch).unwrap();

    let lockx_rect = engine
        .active_tree()
        .node_by_key(Key(33))
        .unwrap()
        .layout_rect;
    let free_rect = engine
        .active_tree()
        .node_by_key(Key(34))
        .unwrap()
        .layout_rect;

    // 1. Horizontal drag on LockX wins against ScrollY.
    let lx = lockx_rect.x + 20;
    let ly = lockx_rect.y + 20;
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(lx, ly),
            },
            InstantMs(10),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(lx + 24, ly + 1),
            },
            InstantMs(20),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(lx + 24, ly + 1),
            },
            InstantMs(30),
            false,
        )
        .unwrap();
    assert_eq!(
        engine.active_tree().node_by_key(Key(30)).unwrap().scroll_y,
        0
    );
    {
        let log = POINTER_LOG.lock().unwrap();
        assert!(
            log.iter()
                .any(|&(k, p, _, d)| k == Key(33) && p == PointerPhase::Down && d.x == 24)
        );
        assert!(
            log.iter()
                .any(|&(k, p, _, _)| k == Key(33) && p == PointerPhase::Up)
        );
    }

    // 2. Vertical drag on LockX yields to ScrollY.
    POINTER_LOG.lock().unwrap().clear();
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(2),
                pos: Point::new(lx, ly),
            },
            InstantMs(40),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerMove {
                id: PointerId(2),
                pos: Point::new(lx, ly - 24),
            },
            InstantMs(50),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(2),
                pos: Point::new(lx, ly - 24),
            },
            InstantMs(60),
            false,
        )
        .unwrap();
    assert!(engine.active_tree().node_by_key(Key(30)).unwrap().scroll_y > 0);
    assert!(POINTER_LOG.lock().unwrap().is_empty());

    // Scroll back to 0 for Free test & edit test.
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(3),
                pos: Point::new(lx, ly),
            },
            InstantMs(70),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerMove {
                id: PointerId(3),
                pos: Point::new(lx, ly + 40),
            },
            InstantMs(80),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(3),
                pos: Point::new(lx, ly + 40),
            },
            InstantMs(90),
            false,
        )
        .unwrap();

    // 3. Free capture receives even vertical drag immediately without scrolling.
    POINTER_LOG.lock().unwrap().clear();
    let fx = free_rect.x + 20;
    let fy = free_rect.y + 20;
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(4),
                pos: Point::new(fx, fy),
            },
            InstantMs(100),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerMove {
                id: PointerId(4),
                pos: Point::new(fx, fy - 24),
            },
            InstantMs(110),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(4),
                pos: Point::new(fx, fy - 24),
            },
            InstantMs(120),
            false,
        )
        .unwrap();
    assert_eq!(
        engine.active_tree().node_by_key(Key(30)).unwrap().scroll_y,
        0
    );
    {
        let log = POINTER_LOG.lock().unwrap();
        assert_eq!(log[0].0, Key(34));
        assert_eq!(log[0].1, PointerPhase::Down);
        assert_eq!(log[1].1, PointerPhase::Move);
        assert_eq!(log[2].1, PointerPhase::Up);
    }

    // 4. Long press & Editable Adjust & custom_rev & start_custom_anim.
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(5),
                pos: Point::new(lx, ly),
            },
            InstantMs(200),
            false,
        )
        .unwrap();
    engine
        .advance(InstantMs(200 + REPEAT_DELAY_MS + 10), false)
        .unwrap();
    assert!(engine.model().long_pressed);
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(5),
                pos: Point::new(lx, ly),
            },
            InstantMs(200 + REPEAT_DELAY_MS + 20),
            false,
        )
        .unwrap();

    // Key(33) was focused by pointer down above; Activate enters editing -> Adjust -> Activate exits.
    engine
        .dispatch(RawInput::Activate, InstantMs(1000), false)
        .unwrap();
    assert!(engine.focus().editing);
    engine
        .dispatch(RawInput::Adjust(3), InstantMs(1010), false)
        .unwrap();
    assert_eq!(engine.model().adj_sum, 3);
    engine
        .dispatch(RawInput::Activate, InstantMs(1020), false)
        .unwrap();
    assert!(!engine.focus().editing);

    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert!(engine.damage().is_empty());
    engine
        .try_message(CustomMsg::BumpRev, InstantMs(1030), false)
        .unwrap();
    assert!(!engine.damage().is_empty());
    assert!(engine.active_commands().as_slice().iter().any(|c| {
        matches!(c, PaintCmd::Circle { fill: Some(f), .. } if *f == Rgb565::new(31, 0, 0))
    }));

    engine
        .start_custom_anim(Key(34), 0, 0, 1000, 200, Easing::Linear, InstantMs(1100))
        .unwrap();
    engine.advance(InstantMs(1200), false).unwrap();
    assert_eq!(engine.custom_anim_value(Key(34), 0), Some(500));
    engine.advance(InstantMs(1300), false).unwrap();
    assert!(
        engine
            .active_commands()
            .as_slice()
            .iter()
            .any(|c| { matches!(c, PaintCmd::Circle { radius: 18, .. }) })
    );
}

// ---------------------------------------------------------------------------
// 4. Stack 9-grid + offset, Wrap multi-row, and CustomContainer
// ---------------------------------------------------------------------------

static LAST_LAYOUT_CLICK: Mutex<Option<Key>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct LayoutModel;

struct LayoutApp;

impl App for LayoutApp {
    type Model = LayoutModel;
    type Msg = Key;

    fn update(_model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        *LAST_LAYOUT_CLICK.lock().unwrap() = Some(msg);
        Change::Unchanged
    }

    fn view(_model: &Self::Model) -> impl Scene {
        layout(
            Key(1),
            NodeProps {
                bg: Some(Rgb565::new(2, 4, 2)),
                width: Length::Fill(1),
                height: Length::Fill(1),
                ..NodeProps::new()
            },
            move |ui| {
                ui.column(
                    Key(2),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        gap: 4,
                        ..NodeProps::new()
                    },
                    |col| {
                        // A: Stack 200x70 with 9-grid & set_offset
                        col.stack(
                            Key(40),
                            NodeProps {
                                width: Length::Px(200),
                                height: Length::Px(70),
                                ..NodeProps::new()
                            },
                            |s| {
                                s.label(
                                    Key(41),
                                    NodeProps {
                                        width: Length::Px(20),
                                        height: Length::Px(10),
                                        stack_align: StackAlign::TopStart,
                                        ..NodeProps::new()
                                    },
                                )?;
                                s.label(
                                    Key(42),
                                    NodeProps {
                                        width: Length::Px(20),
                                        height: Length::Px(10),
                                        stack_align: StackAlign::BottomEnd,
                                        ..NodeProps::new()
                                    },
                                )?;
                                s.button(
                                    Key(43),
                                    NodeProps {
                                        width: Length::Px(30),
                                        height: Length::Px(20),
                                        stack_align: StackAlign::Center,
                                        clickable: true,
                                        focusable: true,
                                        ..NodeProps::new()
                                    },
                                )?;
                                s.set_offset(Key(43), 25, -10)
                            },
                        )?;

                        // B: Wrap 150px wide with 4 chips: 50, 60, 50, 40 -> 2 rows
                        col.wrap(
                            Key(50),
                            NodeProps {
                                width: Length::Px(150),
                                height: Length::Content,
                                gap: 8,
                                line_spacing: 6,
                                ..NodeProps::new()
                            },
                            |w| {
                                for (k, cw) in [(51, 50), (52, 60), (53, 50), (54, 40)] {
                                    w.label(
                                        Key(k),
                                        NodeProps {
                                            width: Length::Px(cw),
                                            height: Length::Px(24),
                                            ..NodeProps::new()
                                        },
                                    )?;
                                }
                                Ok(())
                            },
                        )?;

                        // C: CustomContainer with custom layout callback
                        col.custom_container(
                            Key(60),
                            NodeProps {
                                width: Length::Px(140),
                                height: Length::Content,
                                ..NodeProps::new()
                            },
                            |cc| {
                                cc.button(
                                    Key(61),
                                    NodeProps {
                                        width: Length::Px(60),
                                        height: Length::Px(24),
                                        clickable: true,
                                        focusable: true,
                                        ..NodeProps::new()
                                    },
                                )?;
                                cc.button(
                                    Key(62),
                                    NodeProps {
                                        width: Length::Px(60),
                                        height: Length::Px(24),
                                        clickable: true,
                                        focusable: true,
                                        ..NodeProps::new()
                                    },
                                )
                            },
                        )
                    },
                )
            },
        )
    }

    fn layout_custom(
        key: Key,
        _child_sizes: &[Size],
        _available: Size,
        out_rects: &mut [Rect],
    ) -> Size {
        if key == Key(60) && out_rects.len() >= 2 {
            out_rects[0] = Rect::new(4, 4, 60, 24);
            out_rects[1] = Rect::new(44, 34, 60, 24);
            return Size::new(108, 62);
        }
        Size::zero()
    }

    fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
        Some(event.key)
    }
}

#[test]
fn stack_9grid_wrap_and_custom_container_layout_and_hit_test() {
    *LAST_LAYOUT_CLICK.lock().unwrap() = None;
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 240, 240);
    let mut storage = Storage::new();
    let mut engine =
        Engine::<LayoutApp>::new_in_place(&mut storage, LayoutModel, font, screen, InstantMs(0))
            .unwrap();

    let t = engine.active_tree();
    let s40 = t.node_by_key(Key(40)).unwrap().layout_rect;
    let r41 = t.node_by_key(Key(41)).unwrap().layout_rect;
    let r42 = t.node_by_key(Key(42)).unwrap().layout_rect;
    let r43 = t.node_by_key(Key(43)).unwrap().layout_rect;

    assert_eq!(r41, Rect::new(s40.x, s40.y, 20, 10));
    assert_eq!(r42, Rect::new(s40.x + 180, s40.y + 60, 20, 10));
    // Center(200x70 for 30x20) = (85, 25); + offset (25, -10) = (110, 15).
    assert_eq!(r43, Rect::new(s40.x + 110, s40.y + 15, 30, 20));

    let w50 = t.node_by_key(Key(50)).unwrap().layout_rect;
    let r51 = t.node_by_key(Key(51)).unwrap().layout_rect;
    let r52 = t.node_by_key(Key(52)).unwrap().layout_rect;
    let r53 = t.node_by_key(Key(53)).unwrap().layout_rect;
    let r54 = t.node_by_key(Key(54)).unwrap().layout_rect;
    assert_eq!(w50.h, 54);
    assert_eq!(r51, Rect::new(w50.x, w50.y, 50, 24));
    assert_eq!(r52, Rect::new(w50.x + 58, w50.y, 60, 24));
    assert_eq!(r53, Rect::new(w50.x, w50.y + 30, 50, 24));
    assert_eq!(r54, Rect::new(w50.x + 58, w50.y + 30, 40, 24));

    let cc = t.node_by_key(Key(60)).unwrap().layout_rect;
    let c1 = t.node_by_key(Key(61)).unwrap().layout_rect;
    let c2 = t.node_by_key(Key(62)).unwrap().layout_rect;
    assert_eq!(cc.h, 62);
    assert_eq!(c1, Rect::new(cc.x + 4, cc.y + 4, 60, 24));
    assert_eq!(c2, Rect::new(cc.x + 44, cc.y + 34, 60, 24));

    // Verify clicking shifted Stack child hits only at shifted position, not unshifted Center.
    let unshifted_center = Point::new(s40.x + 90, s40.y + 35);
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: unshifted_center,
            },
            InstantMs(10),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: unshifted_center,
            },
            InstantMs(20),
            false,
        )
        .unwrap();
    assert_eq!(*LAST_LAYOUT_CLICK.lock().unwrap(), None);

    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(2),
                pos: rect_center(r43),
            },
            InstantMs(30),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(2),
                pos: rect_center(r43),
            },
            InstantMs(40),
            false,
        )
        .unwrap();
    assert_eq!(*LAST_LAYOUT_CLICK.lock().unwrap(), Some(Key(43)));

    // Verify clicking CustomContainer children routes to Key(61) and Key(62).
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(3),
                pos: rect_center(c2),
            },
            InstantMs(50),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(3),
                pos: rect_center(c2),
            },
            InstantMs(60),
            false,
        )
        .unwrap();
    assert_eq!(*LAST_LAYOUT_CLICK.lock().unwrap(), Some(Key(62)));
}
