#![no_std]
#![forbid(unsafe_code)]

pub mod controls;
pub mod data;
pub mod ime;
pub mod inputs;
mod pinyin_dict;
pub mod style;
pub mod theme;
pub mod tokens;
pub mod transition;
pub mod ui;

use liuli::prelude::*;

pub use controls::{
    arc_knob, arc_knob_from_pointer, badge_button, paint_arc_knob, paint_ticked_slider,
};
pub use data::{
    ChartSeries, GaugeSpec, TableColumn, TextSpan, data_table, gauge, paint_bar_chart, paint_gauge,
    paint_multi_line_chart, paint_rich_text, paint_spinner, rich_text, spinner,
};
pub use ime::{
    DEFAULT_PINYIN_DICT, FixedTextEditor, ImeDict, ImeEvent, ImeState, InputMode, StaticPinyinDict,
    paint_soft_keyboard, paint_text_box, soft_keyboard, soft_keyboard_on_pointer, text_box,
    text_box_on_pointer, text_box_with_focus,
};
pub use inputs::{
    DropdownState, MultiLineState, RollerState, checkbox, dropdown, multi_line_editor,
    paint_checkbox, paint_multi_line_editor, paint_radio, paint_roller, radio, roller,
};
pub use style::{ButtonVariant, Compositor, Elevation, Material, ResolvedSurface, SurfaceSpec};
pub use theme::{Style, Theme, ThemePreset};
pub use tokens::{
    ColorScheme, ComponentParts, Density, EffectTier, EffectsConfig, ElevationLevel,
    ElevationScale, GlassStyle, InteractionStyle, Motion, Shape, Typography, contrast_ratio_x10,
    rgb8,
};
pub use transition::{PageTransition, TransitionKind};
pub use ui::ThemedUi;

/// Typed route stack. Route variants carry parameters and per-page state.
/// The root always remains present; a failed push gives ownership back.
pub struct Navigator<R: Copy, const N: usize> {
    root: R,
    pages: [Option<R>; N],
    len: usize,
}
impl<R: Copy, const N: usize> Navigator<R, N> {
    pub const fn new(root: R) -> Self {
        Self {
            root,
            pages: [None; N],
            len: 0,
        }
    }
    pub fn current(&self) -> R {
        self.len
            .checked_sub(1)
            .and_then(|i| self.pages[i])
            .unwrap_or(self.root)
    }
    pub fn push(&mut self, page: R) -> Result<(), R> {
        let Some(slot) = self.pages.get_mut(self.len) else {
            return Err(page);
        };
        *slot = Some(page);
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) -> Option<R> {
        self.len = self.len.checked_sub(1)?;
        self.pages[self.len].take()
    }
}

/// A typed result delivered to the page revealed by a successful pop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteReturn<R, T> {
    pub from: R,
    pub to: R,
    pub value: T,
}
impl<R: Copy, const N: usize> Navigator<R, N> {
    pub fn pop_with<T>(&mut self, value: T) -> Result<RouteReturn<R, T>, T> {
        let Some(from) = self.pop() else {
            return Err(value);
        };
        Ok(RouteReturn {
            from,
            to: self.current(),
            value,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateCapacity {
    pub page: u8,
    pub key: Key,
}
/// Bounded component-local state kept in the application model across rebuilds.
pub struct KeyedState<T, const N: usize> {
    entries: [Option<(u8, Key, T)>; N],
}
impl<T, const N: usize> KeyedState<T, N> {
    pub const fn new() -> Self {
        Self {
            entries: [const { None }; N],
        }
    }
    pub fn get_or_insert(
        &mut self,
        page: u8,
        key: Key,
        new: impl FnOnce() -> T,
    ) -> Result<&mut T, StateCapacity> {
        let index = self
            .entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|(p, k, _)| *p == page && *k == key))
            .or_else(|| self.entries.iter().position(Option::is_none))
            .ok_or(StateCapacity { page, key })?;
        Ok(&mut self.entries[index]
            .get_or_insert_with(|| (page, key, new()))
            .2)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(u8, Key) -> bool) {
        for e in &mut self.entries {
            if e.as_ref().is_some_and(|(page, key, _)| !keep(*page, *key)) {
                *e = None;
            }
        }
    }
}
impl<T, const N: usize> Default for KeyedState<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
/// A fixed-height list reserves the offscreen extent with two spacers.
/// Item keys must be stable and distinct from the three container keys.
pub fn virtual_list(
    ui: &mut Children<'_, '_>,
    keys: [Key; 3],
    count: u32,
    row_height: core::num::NonZeroU16,
    offset: i32,
    viewport: u16,
    mut row: impl FnMut(&mut Children<'_, '_>, u32) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    let h = u32::from(row_height.get());
    let extent = count
        .checked_mul(h)
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(BuildError::InvalidRange)?;
    let y = (offset.max(0) as u32).min(extent.saturating_sub(viewport.into()));
    let first = (y / h).saturating_sub(1);
    let end = (y + u32::from(viewport))
        .div_ceil(h)
        .saturating_add(1)
        .min(count);
    let props = NodeProps {
        width: Length::Fill(1),
        height: Length::Px(viewport.into()),
        align_cross: Align::Stretch,
        ..NodeProps::new()
    };
    ui.scroll_y(keys[0], props, |list| {
        let spacer = |height| NodeProps {
            height: Length::Px(height),
            width: Length::Px(1),
            ..NodeProps::new()
        };
        list.label(keys[1], spacer(first * h))?;
        for index in first..end {
            row(list, index)?;
        }
        list.label(keys[2], spacer((count - end) * h))
    })
}

/// A threshold meter rendered entirely through the public custom-widget API.
pub fn paint_meter(props: &StoredProps, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = canvas.size();
    canvas.fill(Rect::new(0, 0, size.width, size.height), props.bg_raw)?;
    let range = i32::from(props.max) - i32::from(props.min);
    if range > 0 {
        let filled = (u64::from(size.width)
            * (i32::from(props.value) - i32::from(props.min)).clamp(0, range) as u64
            / range as u64) as u32;
        canvas.fill(Rect::new(0, 0, filled, size.height), props.accent)?;
    }
    Ok(())
}

/// Numeric editing via touch drag or rotary steps. Actions carry the clamped value.
pub fn number_editor(
    ui: &mut Children<'_, '_>,
    key: Key,
    range: core::ops::RangeInclusive<i16>,
    value: i16,
    step: i16,
    theme: Theme,
) -> Result<(), BuildError> {
    let (min, max) = range.into_inner();
    if min > max || step <= 0 {
        return Err(BuildError::InvalidRange);
    }
    ui.slider(
        key,
        NodeProps {
            min,
            max,
            step,
            ..theme.slider(value.clamp(min, max))
        },
    )
}
/// A mutually exclusive set of buttons. Each action uses the caller's stable key.
pub fn selector(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.column(
        key,
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |choices| choice_buttons(choices, items, selected, theme),
    )
}
pub fn tabs(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.row(
        key,
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |bar| choice_buttons(bar, items, selected, theme),
    )
}
fn choice_buttons(
    ui: &mut Children<'_, '_>,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    for &(key, text) in items {
        let chosen = key == selected;
        let base = if chosen {
            theme.primary_button()
        } else {
            theme.control()
        };
        ui.button(
            key,
            NodeProps {
                text,
                checked: chosen,
                ..base
            },
        )?;
    }
    Ok(())
}
/// Modal focus and Back are managed by the engine. The application handles the two action keys.
pub fn confirm_dialog(
    ui: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    ui.modal(
        keys[0],
        NodeProps {
            width: Length::Px(180),
            height: Length::Content,
            padding: EdgeInsets::all(8),
            bg: Some(theme.surface),
            border_color: Some(theme.border),
            border_width: theme.shape.border_width,
            radius: theme.radius,
            overlay_color: theme.overlay,
            overlay_alpha: theme.interaction.modal_scrim_alpha,
            ..NodeProps::new()
        },
        |modal| confirm_dialog_actions(modal, keys, labels, theme),
    )
}
/// Centered modal confirmation dialog covering the viewport with a backdrop scrim.
pub fn centered_confirm_dialog(
    ui: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    let panel = theme.resolve_surface(
        SurfaceSpec::FLOATING.with_radius(theme.shape.radius_lg),
        NodeProps {
            width: Length::Px(204),
            height: Length::Content,
            padding: EdgeInsets::all(theme.density.card_pad.saturating_add(2)),
            gap: theme.spacing,
            align_cross: Align::Stretch,
            ..NodeProps::new()
        },
    );
    ui.modal(
        keys[0],
        NodeProps {
            width: Length::Fill(1),
            height: Length::Fill(1),
            overlay_color: theme.overlay,
            overlay_alpha: theme.interaction.modal_scrim_alpha,
            ..NodeProps::new()
        },
        |modal| {
            modal.column(keys[1], panel.props, |actions| {
                actions.button(
                    keys[2],
                    NodeProps {
                        text: labels[0],
                        ..theme.primary_button()
                    },
                )?;
                actions.button(
                    keys[3],
                    NodeProps {
                        text: labels[1],
                        ..theme.secondary_button()
                    },
                )
            })?;
            if panel.effect.shadow.is_some() || panel.effect.layer().is_some() {
                modal.decorate(keys[1], panel.effect)?;
            }
            Ok(())
        },
    )
}
fn confirm_dialog_actions(
    modal: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    modal.column(
        keys[1],
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |actions| {
            actions.button(
                keys[2],
                NodeProps {
                    text: labels[0],
                    ..theme.primary_button()
                },
            )?;
            actions.button(
                keys[3],
                NodeProps {
                    text: labels[1],
                    ..theme.secondary_button()
                },
            )
        },
    )
}
/// Insert last in an overlay stack. The application owns the dismissal deadline.
pub fn toast(
    ui: &mut Children<'_, '_>,
    key: Key,
    text: TextRef,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.label(
        key,
        NodeProps {
            text,
            width: Length::Fill(1),
            height: Length::Px(theme.control_height.into()),
            padding: EdgeInsets::symmetric(
                theme.density.pad_x.saturating_add(2),
                theme.density.pad_y,
            ),
            bg: Some(theme.surface_alt),
            border_color: Some(theme.accent),
            border_width: theme.shape.border_width,
            radius: theme.radius,
            fg: theme.text,
            font: theme.font,
            align_cross: Align::Center,
            text_align: TextAlign::Center,
            ..NodeProps::new()
        },
    )
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicI16, Ordering};

    use liuli_sim::screen::MemoryScreen;

    use super::*;

    const TEST_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

    #[test]
    fn component_parts_propagate_via_themed_ui_without_wasting_default_slots() {
        let mut draft = liuli::core::view::ViewDraft::new();
        let mut vb = ViewBuilder::new(&mut draft);
        vb.root(Key(1), NodeProps::new(), |root| {
            let mut ui = ThemedUi::new(root, Theme::DARK);
            ui.slider(Key(2), 40)?;
            let custom_parts = ComponentParts::DEFAULT
                .with_slider(
                    WidgetPartStyle::new()
                        .with_track_color(rgb8(30, 40, 50))
                        .with_knob_radius(7),
                )
                .with_button(WidgetPartStyle::new().with_pressed_bg(rgb8(200, 20, 20)));
            ui.with_parts(custom_parts, |scoped| {
                scoped.slider(Key(3), 75)?;
                scoped.button(Key(4), "Apply", IconKind::Check, ButtonVariant::Primary)
            })
        })
        .unwrap();
        vb.finish().unwrap();

        assert!(draft.part_styles.get(Key(2)).is_empty());
        assert_eq!(draft.part_styles.get(Key(3)).knob_radius(), Some(7));
        assert_eq!(
            draft.part_styles.get(Key(3)).track_color(),
            Some(rgb8(30, 40, 50))
        );
        assert_eq!(
            draft.part_styles.get(Key(4)).pressed_bg(),
            Some(rgb8(200, 20, 20))
        );
    }

    #[test]
    fn arc_knob_pointer_mapping_handles_angles_deadzone_and_steps() {
        let sz = Size::new(100, 100);
        let ev = |x, y| PointerEvent {
            phase: PointerPhase::Move,
            local: Point::new(x, y),
            delta: Point::zero(),
            size: sz,
        };
        assert_eq!(arc_knob_from_pointer(ev(15, 85), 0, 100, 1), 0);
        assert_eq!(arc_knob_from_pointer(ev(50, 10), 0, 100, 1), 50);
        assert_eq!(arc_knob_from_pointer(ev(85, 85), 0, 100, 1), 100);
        assert_eq!(arc_knob_from_pointer(ev(45, 95), 0, 100, 1), 0);
        assert_eq!(arc_knob_from_pointer(ev(55, 95), 0, 100, 1), 100);
        assert_eq!(arc_knob_from_pointer(ev(50, 10), 0, 100, 25), 50);
        assert_eq!(arc_knob_from_pointer(ev(90, 50), 0, 100, 25), 75);
    }

    #[derive(Clone, Copy)]
    struct ControlsDemoModel {
        knob: i16,
        slider: i16,
        clicks: u16,
    }

    #[derive(Clone, Copy)]
    enum ControlsDemoMsg {
        SetKnob(i16),
        SetSlider(i16),
        ClickBadge,
    }

    static LAST_BADGE_HITS: AtomicI16 = AtomicI16::new(0);

    struct ControlsDemoApp;

    impl App for ControlsDemoApp {
        type Model = ControlsDemoModel;
        type Msg = ControlsDemoMsg;

        fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
            match msg {
                ControlsDemoMsg::SetKnob(v) => model.knob = v.clamp(0, 100),
                ControlsDemoMsg::SetSlider(v) => model.slider = v.clamp(0, 100),
                ControlsDemoMsg::ClickBadge => {
                    model.clicks += 1;
                    LAST_BADGE_HITS.store(model.clicks as i16, Ordering::Relaxed);
                }
            }
            Change::View
        }

        fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
            let theme = Theme::DARK;
            out.root(
                Key(1),
                NodeProps {
                    bg: Some(theme.background),
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    padding: EdgeInsets::all(8),
                    gap: 8,
                    ..NodeProps::new()
                },
                |ui| {
                    ui.column(
                        Key(2),
                        NodeProps {
                            width: Length::Fill(1),
                            height: Length::Content,
                            gap: 8,
                            ..NodeProps::new()
                        },
                        |col| {
                            arc_knob(
                                col,
                                Key(10),
                                64,
                                0..=100,
                                model.knob,
                                5,
                                TextRef::Static("Vol"),
                                theme,
                            )?;
                            col.slider(
                                Key(11),
                                NodeProps {
                                    paint_mode: PaintMode::Override,
                                    ..theme.slider(model.slider)
                                },
                            )?;
                            badge_button(
                                col,
                                [Key(12), Key(13), Key(14)],
                                TextRef::Static("Alerts"),
                                TextRef::Static("3"),
                                theme.danger,
                                theme,
                            )
                        },
                    )
                },
            )
        }

        fn paint_custom(
            key: Key,
            props: &StoredProps,
            canvas: &mut WidgetCanvas<'_>,
        ) -> Result<(), BuildError> {
            if key == Key(10) {
                return paint_arc_knob(
                    props,
                    PaintState {
                        pressed: canvas.pressed,
                        focused: false,
                        editing: false,
                        checked: props.checked(),
                        enabled: props.enabled(),
                        anim_progress: 0,
                    },
                    canvas,
                );
            }
            Ok(())
        }

        fn paint_override(
            _model: &Self::Model,
            key: Key,
            _kind: NodeKind,
            props: &StoredProps,
            state: PaintState,
            canvas: &mut WidgetCanvas<'_>,
        ) -> Result<PaintOverride, BuildError> {
            if key == Key(11) {
                return paint_ticked_slider(props, state, 5, canvas);
            }
            Ok(PaintOverride::Default)
        }

        fn on_pointer(_model: &Self::Model, key: Key, event: PointerEvent) -> Option<Self::Msg> {
            if key == Key(10) && matches!(event.phase, PointerPhase::Down | PointerPhase::Move) {
                return Some(ControlsDemoMsg::SetKnob(arc_knob_from_pointer(
                    event, 0, 100, 5,
                )));
            }
            None
        }

        fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
            match (event.key, event.kind) {
                (Key(11), ActionKind::SetValue(v)) => Some(ControlsDemoMsg::SetSlider(v)),
                (Key(13), ActionKind::Activate) => Some(ControlsDemoMsg::ClickBadge),
                _ => None,
            }
        }
    }

    #[test]
    fn arc_knob_ticked_slider_and_badge_button_render_and_interact() {
        LAST_BADGE_HITS.store(0, Ordering::Relaxed);
        let font = Lf4Font::from_bytes(TEST_LF4).unwrap();
        let screen_rect = Rect::new(0, 0, 240, 200);
        let mut storage = Storage::new();
        let mut engine = Engine::<ControlsDemoApp>::new_in_place(
            &mut storage,
            ControlsDemoModel {
                knob: 25,
                slider: 50,
                clicks: 0,
            },
            font,
            screen_rect,
            InstantMs(0),
        )
        .unwrap();

        let mut mem = MemoryScreen::new(240, 200);
        let mut scratch = [0u16; 240 * 8];
        engine.present_sync(&mut mem, &mut scratch).unwrap();

        let knob_rect = engine
            .active_tree()
            .node_by_key(Key(10))
            .unwrap()
            .layout_rect;
        let badge_btn_rect = engine
            .active_tree()
            .node_by_key(Key(13))
            .unwrap()
            .layout_rect;
        let badge_lbl_rect = engine
            .active_tree()
            .node_by_key(Key(14))
            .unwrap()
            .layout_rect;

        assert!(badge_lbl_rect.y < badge_btn_rect.y);
        assert!(badge_lbl_rect.x > badge_btn_rect.x);

        let top_x = knob_rect.x + (knob_rect.w as i32 / 2);
        let top_y = knob_rect.y + 4;
        engine
            .dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(top_x, top_y),
                },
                InstantMs(10),
                false,
            )
            .unwrap();
        engine
            .dispatch(
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(top_x, top_y),
                },
                InstantMs(20),
                false,
            )
            .unwrap();
        assert_eq!(engine.model().knob, 50);

        let bx = badge_btn_rect.x + 20;
        let by = badge_btn_rect.y + (badge_btn_rect.h as i32 / 2);
        engine
            .dispatch(
                RawInput::PointerDown {
                    id: PointerId(2),
                    pos: Point::new(bx, by),
                },
                InstantMs(30),
                false,
            )
            .unwrap();
        engine
            .dispatch(
                RawInput::PointerUp {
                    id: PointerId(2),
                    pos: Point::new(bx, by),
                },
                InstantMs(40),
                false,
            )
            .unwrap();
        assert_eq!(LAST_BADGE_HITS.load(Ordering::Relaxed), 1);

        engine.present_sync(&mut mem, &mut scratch).unwrap();
        let non_bg = (knob_rect.y..knob_rect.y + knob_rect.h as i32)
            .flat_map(|y| {
                (knob_rect.x..knob_rect.x + knob_rect.w as i32).map(move |x| (x as u32, y as u32))
            })
            .filter(|&(x, y)| {
                mem.pixel(x, y) != liuli::core::geometry::rgb565_to_u16(Theme::DARK.background)
            })
            .count();
        assert!(non_bg > 50);
    }

    #[test]
    fn ime_fixed_editor_pinyin_t9_and_soft_keyboard_e2e() {
        // 1. UTF-8 CJK cursor, preedit, password masking, and backspace.
        let mut ed: FixedTextEditor<64> = FixedTextEditor::new();
        assert!(ed.apply(TextInput::Compose("she")));
        assert_eq!(ed.preedit(), "she");
        assert!(ed.apply(TextInput::Commit("设备")));
        assert_eq!(ed.text(), "设备");
        assert_eq!(ed.preedit(), "");
        assert!(ed.apply(TextInput::Commit("25")));
        assert!(ed.apply(TextInput::Left));
        assert!(ed.apply(TextInput::Backspace));
        assert_eq!(ed.text(), "设备5");

        let masked: FixedTextEditor<16> = FixedTextEditor::from_text("abc").with_password(true);
        let mut buf = [0u8; 32];
        let (disp, _, _, caret) = masked.format_display(&mut buf);
        assert_eq!(disp, "***");
        assert_eq!(caret, 3);

        // 2. Pluggable StaticPinyinDict (QWERTY & T9 exact + prefix + paging).
        let mut out = [""; 4];
        let (n, more) = DEFAULT_PINYIN_DICT.query("she", false, 0, &mut out);
        assert_eq!(n, 4);
        assert!(more);
        assert_eq!(out[0], "设");
        assert_eq!(out[1], "设备");
        let (n2, _) = DEFAULT_PINYIN_DICT.query("she", false, 4, &mut out);
        assert!(n2 >= 1);

        let (t9n, _) = DEFAULT_PINYIN_DICT.query("936", true, 0, &mut out);
        assert!(t9n >= 2);
        assert_eq!(out[0], "温");
        assert!(out[..t9n].contains(&"温度"));

        // 3. SoftKeyboard touch flow inside Engine: Pin("she")->"设备", T9("936")->"温度", 123->"25", En->"C".
        struct ImeTestModel {
            editor: FixedTextEditor<128>,
            ime: ImeState,
            font: Lf4Font<'static>,
            done: u16,
        }
        #[derive(Clone, Copy)]
        enum ImeTestMsg {
            Text(InputOwned),
            Kb(PointerEvent),
            Box(PointerEvent),
        }
        #[derive(Clone, Copy)]
        enum InputOwned {
            Compose(&'static str),
            Commit(&'static str),
            Cancel,
            Backspace,
            Left,
            Right,
        }
        struct ImeTestApp;
        impl App for ImeTestApp {
            type Model = ImeTestModel;
            type Msg = ImeTestMsg;

            fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
                match msg {
                    ImeTestMsg::Text(i) => {
                        let ti = match i {
                            InputOwned::Compose(s) => TextInput::Compose(s),
                            InputOwned::Commit(s) => TextInput::Commit(s),
                            InputOwned::Cancel => TextInput::Cancel,
                            InputOwned::Backspace => TextInput::Backspace,
                            InputOwned::Left => TextInput::Left,
                            InputOwned::Right => TextInput::Right,
                        };
                        if model.editor.apply(ti) {
                            Change::View
                        } else {
                            Change::Unchanged
                        }
                    }
                    ImeTestMsg::Kb(ev) => {
                        let r0 = model.ime.revision();
                        let e0 = model.editor.revision();
                        let ime_ev =
                            soft_keyboard_on_pointer(&mut model.ime, &DEFAULT_PINYIN_DICT, ev);
                        let done = matches!(ime_ev, ImeEvent::Done);
                        model.editor.apply_ime(ime_ev);
                        if done {
                            model.done += 1;
                        }
                        if model.ime.revision() != r0 || model.editor.revision() != e0 {
                            Change::View
                        } else {
                            Change::Unchanged
                        }
                    }
                    ImeTestMsg::Box(ev) => {
                        if text_box_on_pointer(&mut model.editor, ev, Some(&model.font)) {
                            Change::View
                        } else {
                            Change::Unchanged
                        }
                    }
                }
            }

            fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
                let theme = Theme::DARK;
                out.root(
                    Key(1),
                    NodeProps {
                        bg: Some(theme.background),
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        padding: EdgeInsets::all(6),
                        ..NodeProps::new()
                    },
                    |ui| {
                        ui.column(
                            Key(2),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Fill(1),
                                gap: 6,
                                ..NodeProps::new()
                            },
                            |col| {
                                text_box(col, Key(10), &model.editor, "Input...", theme)?;
                                soft_keyboard(col, Key(20), &model.ime, 136, theme)
                            },
                        )
                    },
                )
            }

            fn accepts_text(_model: &Self::Model, key: Key) -> bool {
                key == Key(10)
            }

            fn on_text(_model: &Self::Model, _: Key, input: TextInput<'_>) -> Option<Self::Msg> {
                Some(ImeTestMsg::Text(match input {
                    TextInput::Compose(_) => InputOwned::Compose(""),
                    TextInput::Commit(_) => InputOwned::Commit(""),
                    TextInput::Cancel => InputOwned::Cancel,
                    TextInput::Backspace => InputOwned::Backspace,
                    TextInput::Left => InputOwned::Left,
                    TextInput::Right => InputOwned::Right,
                }))
            }

            fn on_pointer(_model: &Self::Model, key: Key, ev: PointerEvent) -> Option<Self::Msg> {
                match key.0 {
                    10 => Some(ImeTestMsg::Box(ev)),
                    20 => Some(ImeTestMsg::Kb(ev)),
                    _ => None,
                }
            }

            fn paint_model(
                model: &Self::Model,
                key: Key,
                props: &StoredProps,
                canvas: &mut WidgetCanvas<'_>,
            ) -> Result<(), BuildError> {
                match key.0 {
                    10 => paint_text_box(
                        &model.editor,
                        "Input...",
                        props,
                        canvas.state,
                        Some(&model.font),
                        canvas,
                    ),
                    20 => paint_soft_keyboard(
                        &model.ime,
                        &DEFAULT_PINYIN_DICT,
                        props,
                        Theme::DARK,
                        canvas,
                    ),
                    _ => Ok(()),
                }
            }

            fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
                None
            }
        }

        let font = Lf4Font::from_bytes(TEST_LF4).unwrap();
        let small = Lf4Font::from_bytes(include_bytes!(
            "../../../apps/liuli-settings/assets/small12.lf4"
        ))
        .unwrap();
        let screen_rect = Rect::new(0, 0, 320, 240);
        let mut storage = Storage::new();
        let mut engine = Engine::<ImeTestApp>::new_in_place_with_fonts(
            &mut storage,
            ImeTestModel {
                editor: FixedTextEditor::new(),
                ime: ImeState::new(InputMode::ZhPinyin),
                font,
                done: 0,
            },
            FontSet::new(font, small),
            screen_rect,
            InstantMs(0),
        )
        .unwrap();

        let mut mem = MemoryScreen::new(320, 240);
        let mut scratch = [0u16; 320 * 8];
        engine.present_sync(&mut mem, &mut scratch).unwrap();

        let mut ts = 10u64;
        let mut tap_lbl = |eng: &mut Engine<'_, '_, ImeTestApp>, lbl: &str| {
            let kb = eng.active_tree().node_by_key(Key(20)).unwrap().layout_rect;
            let kr = eng
                .model()
                .ime
                .key_rect(&DEFAULT_PINYIN_DICT, kb.size(), lbl)
                .unwrap_or_else(|| {
                    panic!("missing key '{lbl}' in mode {:?}", eng.model().ime.mode())
                });
            let pt = Point::new(kb.x + kr.x + kr.w as i32 / 2, kb.y + kr.y + kr.h as i32 / 2);
            eng.dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: pt,
                },
                InstantMs(ts),
                false,
            )
            .unwrap();
            ts += 10;
            eng.dispatch(
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: pt,
                },
                InstantMs(ts),
                false,
            )
            .unwrap();
            ts += 10;
        };

        tap_lbl(&mut engine, "s");
        tap_lbl(&mut engine, "h");
        tap_lbl(&mut engine, "e");
        assert_eq!(engine.model().editor.preedit(), "she");
        tap_lbl(&mut engine, "设备");
        assert_eq!(engine.model().editor.text(), "设备");

        tap_lbl(&mut engine, "T9");
        tap_lbl(&mut engine, "9 WXYZ");
        tap_lbl(&mut engine, "3 DEF");
        tap_lbl(&mut engine, "6 MNO");
        tap_lbl(&mut engine, "温度");
        assert_eq!(engine.model().editor.text(), "设备温度");

        tap_lbl(&mut engine, "123");
        tap_lbl(&mut engine, "2");
        tap_lbl(&mut engine, "5");
        assert_eq!(engine.model().editor.text(), "设备温度25");

        tap_lbl(&mut engine, "Pin");
        tap_lbl(&mut engine, "En");
        tap_lbl(&mut engine, "Cap");
        tap_lbl(&mut engine, "C");
        tap_lbl(&mut engine, "OK");
        assert_eq!(engine.model().editor.text(), "设备温度25C");
        assert_eq!(engine.model().done, 1);

        engine.present_sync(&mut mem, &mut scratch).unwrap();
    }

    #[test]
    fn ime_dict_covers_all_mandarin_syllables_two_letter_prefixes_and_continuous_input() {
        extern crate alloc;
        use crate::pinyin_dict::{DEFAULT_PHRASES, DEFAULT_SYLLABLES};
        use alloc::collections::BTreeSet;
        use alloc::vec::Vec;

        assert!(DEFAULT_SYLLABLES.len() >= 409);

        // Every syllable must yield >= 1 exact match.
        let mut syl_chars = BTreeSet::new();
        for &(syl, chars) in DEFAULT_SYLLABLES {
            let mut out = [""; 4];
            let (n, _, consumed) = DEFAULT_PINYIN_DICT.query_with_prefix(syl, false, 0, &mut out);
            assert!(n >= 1, "syllable '{syl}' returned 0 candidates");
            assert_eq!(consumed, syl.len());
            for ch in chars.chars() {
                syl_chars.insert(ch);
            }

            // Every 2-letter prefix of every syllable must yield >= 1 candidate.
            if syl.len() >= 2 {
                let p2 = &syl[..2];
                let (pn, _) = DEFAULT_PINYIN_DICT.query(p2, false, 0, &mut out);
                assert!(
                    pn >= 1,
                    "2-letter prefix '{p2}' (from '{syl}') returned 0 candidates"
                );
            }
        }

        // Verify common 2-letter queries explicitly.
        for q in [
            "ni", "wo", "ha", "de", "bu", "ma", "ce", "be", "ge", "ke", "he", "le", "fa", "ba",
            "bi", "pi", "po", "zi", "si", "zh", "ch", "sh", "lv", "nv",
        ] {
            let mut out = [""; 4];
            let (n, _) = DEFAULT_PINYIN_DICT.query(q, false, 0, &mut out);
            assert!(n >= 1, "query '{q}' returned 0 candidates");
        }

        let mut out = [""; 4];
        let (n, _) = DEFAULT_PINYIN_DICT.query("ni", false, 0, &mut out);
        assert!(out[..n].contains(&"你"));
        assert!(out[..n].contains(&"你好"));

        let (n, _) = DEFAULT_PINYIN_DICT.query("she", false, 0, &mut out);
        assert!(out[..n].contains(&"设"));
        assert!(out[..n].contains(&"设备"));

        // Every phrase char must exist in DEFAULT_SYLLABLES and fit within InlineText (<= 12 bytes).
        for &(py, phrase) in DEFAULT_PHRASES {
            assert!(
                phrase.len() <= 12,
                "phrase '{phrase}' ({py}) exceeds 12 UTF-8 bytes"
            );
            for ch in phrase.chars() {
                assert!(
                    syl_chars.contains(&ch),
                    "phrase '{phrase}' ({py}) char '{ch}' missing from DEFAULT_SYLLABLES"
                );
            }
        }

        // Phrase initials (简拼).
        for (q, expected) in [
            ("nh", "你好"),
            ("sb", "设备"),
            ("zg", "中国"),
            ("wd", "温度"),
        ] {
            let mut out = [""; 4];
            let (n, _) = DEFAULT_PINYIN_DICT.query(q, false, 0, &mut out);
            assert!(
                out[..n].contains(&expected),
                "initials query '{q}' did not contain '{expected}', got {:?}",
                &out[..n]
            );
        }

        // Continuous multi-syllable splitting ("nishi" -> "你" + "shi" -> "你是").
        let (cn, _, consumed) = DEFAULT_PINYIN_DICT.query_with_prefix("nishi", false, 0, &mut out);
        assert_eq!(consumed, 2);
        assert!(cn >= 1);
        assert_eq!(out[0], "你");

        let mut ime = ImeState::new(InputMode::ZhPinyin);
        let mut editor = FixedTextEditor::<64>::new();
        for letter in ["n", "i", "s", "h", "i"] {
            let ev = ime.press_char(letter);
            editor.apply_ime(ev);
        }
        assert_eq!(editor.preedit(), "nishi");
        let ev1 = ime.select_candidate(0, &DEFAULT_PINYIN_DICT);
        assert_eq!(
            ev1,
            ImeEvent::CommitCompose {
                commit: "你",
                compose: "shi",
            }
        );
        editor.apply_ime(ev1);
        assert_eq!(editor.text(), "你");
        assert_eq!(editor.preedit(), "shi");

        let ev2 = ime.select_candidate(0, &DEFAULT_PINYIN_DICT);
        assert_eq!(ev2, ImeEvent::Text(TextInput::Commit("是")));
        editor.apply_ime(ev2);
        assert_eq!(editor.text(), "你是");
        assert_eq!(editor.preedit(), "");

        // Pagination past 16 candidates must produce zero duplicates.
        for q in ["s", "y", "shi"] {
            let mut seen = BTreeSet::new();
            let mut all = Vec::new();
            let mut offset = 0usize;
            loop {
                let mut page = [""; 4];
                let (n, more) = DEFAULT_PINYIN_DICT.query(q, false, offset, &mut page);
                for &cand in &page[..n] {
                    assert!(
                        seen.insert(cand),
                        "duplicate candidate '{cand}' in pagination for query '{q}' at offset {offset}"
                    );
                    all.push(cand);
                }
                if !more {
                    break;
                }
                offset += 4;
            }
            assert!(
                all.len() > 16 || q == "shi",
                "expected deep pagination for '{q}'"
            );
        }
    }

    #[test]
    fn all_preset_color_schemes_meet_readability_contrast() {
        for preset in ThemePreset::ALL {
            for dark in [false, true] {
                let t = Theme::preset(preset, dark);
                let c = t.colors;
                let text_on_bg = contrast_ratio_x10(c.text, c.background);
                let text_on_surf = contrast_ratio_x10(c.text, c.surface);
                let text_on_alt = contrast_ratio_x10(c.text, c.surface_alt);
                let muted_on_bg = contrast_ratio_x10(c.text_muted, c.background);
                let muted_on_surf = contrast_ratio_x10(c.text_muted, c.surface);
                let on_accent = contrast_ratio_x10(c.on_accent, c.accent);
                let on_danger = contrast_ratio_x10(c.on_danger, c.danger);
                assert!(
                    text_on_bg >= 45 && text_on_surf >= 45 && text_on_alt >= 45,
                    "{preset:?} dark={dark} primary text contrast too low: {text_on_bg}/{text_on_surf}/{text_on_alt}"
                );
                assert!(
                    muted_on_bg >= 30 && muted_on_surf >= 30,
                    "{preset:?} dark={dark} muted contrast too low: {muted_on_bg}/{muted_on_surf}"
                );
                assert!(
                    on_accent >= 30 && on_danger >= 30,
                    "{preset:?} dark={dark} button ink contrast too low: {on_accent}/{on_danger}"
                );
            }
        }
    }

    #[test]
    fn granular_effects_capabilities_toggle_and_clamp_independently() {
        let flat = Theme::DARK.with_effect_tier(EffectTier::Flat);
        assert_eq!(flat.elevation(Elevation::Floating), SurfaceStyle::FLAT);

        // Disable only shadows while keeping press depth & glass tint.
        let no_shadow = Theme::DARK
            .with_shadows(false)
            .with_glass_blur(false)
            .with_glass_tint(None, 8);
        assert_eq!(no_shadow.elevation(Elevation::Raised).shadow, None);

        // Cap shadow blur and scale shadow opacity independently.
        let capped = Theme::DARK.with_effects(
            EffectsConfig::GLASS
                .with_max_shadow_blur(3)
                .with_shadow_opacity_scale(8),
        );
        let sh = capped.elevation(Elevation::Floating).shadow.unwrap();
        assert_eq!(sh.blur, 3);
        assert!(
            sh.opacity
                < Theme::DARK
                    .elevation(Elevation::Floating)
                    .shadow
                    .unwrap()
                    .opacity
        );

        // Glass blur vs glass tint fallback resolution works independently of Cargo features.
        let tint_only = Theme::DARK.with_glass_blur(false).with_glass_tint(None, 9);
        let res = tint_only.resolve_surface(SurfaceSpec::GLASS, tint_only.card());
        assert_eq!(
            res.effect.press_depth.duration_ms(),
            tint_only.motion.press_depth_ms
        );

        let no_press = Theme::DARK.with_press_depth(false);
        assert!(
            !no_press
                .elevation(Elevation::Raised)
                .press_depth
                .is_enabled()
        );
    }

    #[test]
    fn legacy_theme_with_and_field_defaults_remain_compatible() {
        assert_eq!(Theme::DARK.control_height, 32);
        assert_eq!(Theme::DARK.spacing, 6);
        assert_eq!(Theme::DARK.radius, 8);
        assert_eq!(Theme::LIGHT.control_height, 32);

        let custom = Theme::DARK.with(Style {
            control_height: Some(38),
            spacing: Some(10),
            radius: Some(4),
            accent: Some(rgb8(255, 0, 128)),
            ..Style::default()
        });
        assert_eq!(custom.control_height, 38);
        assert_eq!(custom.density.control_height, 38);
        assert_eq!(custom.spacing, 10);
        assert_eq!(custom.density.spacing, 10);
        assert_eq!(custom.radius, 4);
        assert_eq!(custom.shape.radius_md, 4);
        assert_eq!(custom.accent, rgb8(255, 0, 128));
        assert_eq!(custom.colors.accent, rgb8(255, 0, 128));
    }

    #[test]
    fn compositor_and_variant_buttons_honor_individual_gates() {
        let mut draft = liuli::core::view::ViewDraft::new();
        let no_3d_no_opacity = Theme::DARK.with_effects(
            EffectsConfig::GLASS
                .with_projection_3d(false)
                .with_group_opacity(false)
                .with_glass_blur(false)
                .with_glass_tint(false),
        );
        let resolved = no_3d_no_opacity.resolve_surface(
            SurfaceSpec::GLASS.with_compositor(Compositor::isolated(6, Projection(32))),
            no_3d_no_opacity.card(),
        );
        // Glass degraded cleanly to elevated solid card and blocked 3D/opacity when gated off.
        assert!(resolved.props.bg.is_some());
        if let Some(layer) = resolved.effect.layer() {
            assert_eq!(layer.opacity, 15);
            assert_eq!(layer.projection, Projection(0));
            assert_eq!(layer.backdrop_radius, 0);
            assert_eq!(layer.tint_alpha, 0);
        }

        let theme = Theme::DARK
            .with_rounded_focus(true)
            .with_motion(Motion::SNAPPY);
        let mut vb = ViewBuilder::new(&mut draft);
        vb.root(Key(1), NodeProps::new(), |root| {
            let mut ui = ThemedUi::new(root, theme);
            ui.card(Key(2), SurfaceSpec::RAISED, |card| {
                card.button(Key(3), "Save", IconKind::Check, ButtonVariant::Primary)?;
                card.button(Key(4), "Delete", IconKind::Warning, ButtonVariant::Danger)?;
                card.button(Key(5), "Info", IconKind::Sliders, ButtonVariant::Outline)
            })
        })
        .unwrap();
        vb.finish().unwrap();
        assert!(draft.nodes[2].props.focus_ring().rounded);
        assert_eq!(draft.nodes[4].props.bg(), None);
        assert_eq!(draft.nodes[4].props.fg, theme.accent);
    }

    #[test]
    fn mature_widgets_builders_states_and_canvas_render_into_band() {
        let props = NodeProps::new()
            .width(Length::Px(120))
            .height(Length::Px(40))
            .fg(Theme::DARK.text)
            .accent(Theme::DARK.accent)
            .checked(true)
            .focusable(true)
            .clickable(true);
        assert_eq!(props.width, Length::Px(120));
        assert!(props.checked && props.focusable && props.clickable);

        let mut dd = DropdownState::new(0);
        assert!(dd.handle_action(Key(10), Key(10), 100, 3));
        assert!(dd.open);
        assert!(dd.handle_action(Key(102), Key(10), 100, 3));
        assert!(!dd.open && dd.selected == 2);

        let mut roller = RollerState::new(0, true);
        assert!(roller.handle_action(ActionKind::Adjust(-1), 4));
        assert_eq!(roller.selected, 3);

        let mut ml = MultiLineState::<64>::from_text("abc\ndefg");
        assert_eq!(ml.line_count(), 2);
        assert_eq!(ml.cursor_line_col(), (1, 4));
        assert!(ml.apply_nav(NavDir::Up));
        assert_eq!(ml.cursor_line_col(), (0, 3));
        assert!(ml.apply_text(TextInput::Commit("!")));
        assert_eq!(ml.text(), "abc!\ndefg");

        let mut trans = PageTransition::new();
        trans.start(TransitionKind::SlideLeft, InstantMs(100), 200);
        assert!(trans.is_active(InstantMs(150)));
        assert!(trans.offset_for(Size::new(320, 240), InstantMs(150)).0 > 0);
        assert_eq!(
            trans.offset_for(Size::new(320, 240), InstantMs(300)),
            (0, 0)
        );

        // Build full widget tree using ThemedUi wrappers + custom paint and rasterize a band!
        let mut draft = liuli::core::view::ViewDraft::new();
        let mut vb = ViewBuilder::new(&mut draft);
        vb.root(
            Key(1),
            NodeProps::new().bg(Theme::DARK.background).gap(4),
            |root| {
                let mut ui = ThemedUi::new(root, Theme::DARK);
                ui.checkbox(Key(2), TextRef::Static("Enable"), true)?;
                ui.radio(Key(3), TextRef::Static("Mode A"), true)?;
                ui.roller(
                    Key(4),
                    Length::Px(120),
                    60,
                    &["One", "Two", "Three"],
                    roller,
                )?;
                ui.gauge(
                    Key(5),
                    GaugeSpec {
                        min: 0,
                        max: 100,
                        value: 65,
                        ticks: 5,
                        diameter: 72,
                        label: TextRef::Static("65%"),
                    },
                )?;
                ui.spinner(Key(6), 32, InstantMs(250))?;
                ui.rich_text(Key(7), Length::Px(120), 32)?;
                ui.multi_line_editor(Key(8), Length::Px(140), 48, &ml)?;
                ui.data_table(
                    200,
                    &[
                        TableColumn::new("Col A", Length::Px(60), TextAlign::Start),
                        TableColumn::new("Col B", Length::Px(60), TextAlign::End),
                    ],
                    &[&["10", "20"], &["30", "40"]],
                    Some(1),
                    24,
                )?;
                ui.dropdown(
                    Key(500),
                    Key(501),
                    Key(502),
                    510,
                    &["Alpha", "Beta"],
                    DropdownState {
                        open: true,
                        selected: 1,
                    },
                )?;
                ui.custom(
                    Key(400),
                    NodeProps::new()
                        .size(Length::Px(120), Length::Px(60))
                        .bg(Theme::DARK.surface),
                )?;
                ui.custom(
                    Key(401),
                    NodeProps::new()
                        .size(Length::Px(120), Length::Px(60))
                        .bg(Theme::DARK.surface),
                )
            },
        )
        .unwrap();
        vb.finish().unwrap();

        let prev = liuli::core::node::RetainedTree::new();
        let mut tree = liuli::core::node::RetainedTree::new();
        let mut anims = liuli::core::time::AnimPool::new();
        let mut dmg = DamageSet::new();
        let screen = Rect::new(0, 0, 320, 480);
        let lf4 = include_bytes!("../../../apps/liuli-alarms/assets/body.lf4");
        let fonts = liuli::text::FontSet::single(liuli::text::Lf4Font::from_bytes(lf4).unwrap());
        liuli::core::reconcile::reconcile_into(
            &draft,
            &prev,
            &mut tree,
            screen,
            &fonts,
            &mut anims,
            None,
            None,
            InstantMs(250),
            &mut dmg,
        )
        .unwrap();

        let mut cmds = liuli::core::paint::CommandList::new();
        let series = [
            ChartSeries {
                name: "S1",
                color: Theme::DARK.accent,
                values: &[10, 40, 25, 60],
            },
            ChartSeries {
                name: "S2",
                color: Theme::DARK.focus,
                values: &[20, 30, 50, 40],
            },
        ];
        let spans = [
            TextSpan::new("Hello "),
            TextSpan::new("World")
                .with_color(Theme::DARK.accent)
                .with_underline(true),
        ];
        liuli::core::paint::emit_commands_with(
            &tree,
            &anims,
            liuli::core::paint::PaintFocusInfo {
                focused_slot: None,
                ring_visible: false,
                editing: false,
            },
            screen,
            &mut cmds,
            |key, stored, canvas| {
                let ps = PaintState {
                    checked: stored.checked(),
                    focused: key == Key(8),
                    enabled: true,
                    ..PaintState::default()
                };
                match key.0 {
                    2 => paint_checkbox(stored, ps, canvas),
                    3 => paint_radio(stored, ps, canvas),
                    4 => paint_roller(&["One", "Two", "Three"], roller, stored, ps, canvas),
                    5 => paint_gauge(
                        GaugeSpec {
                            min: 0,
                            max: 100,
                            value: 65,
                            ticks: 5,
                            diameter: 72,
                            label: TextRef::Static("65%"),
                        },
                        stored,
                        canvas,
                    ),
                    6 => paint_spinner(InstantMs(250), stored, canvas),
                    7 => paint_rich_text(&spans, stored, canvas),
                    8 => paint_multi_line_editor(&ml, stored, ps, canvas),
                    400 => paint_bar_chart(&series, 0, 80, stored, canvas),
                    401 => paint_multi_line_chart(&series, 0, 80, stored, canvas),
                    _ => Ok(()),
                }
            },
        )
        .unwrap();

        let mut band = [0u16; 320 * 120];
        let mut target =
            liuli::render::BandTarget::new::<()>(&mut band, Rect::new(0, 0, 320, 120)).unwrap();
        let replayed =
            liuli::render::render_band(&mut target, &cmds, &SliceSceneResources::EMPTY, &fonts);
        assert!(replayed > 0);
        let bg_raw = rgb565_to_u16(Theme::DARK.background);
        assert!(band.iter().any(|&px| px != bg_raw));
    }
}
