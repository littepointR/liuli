#![no_std]
#![forbid(unsafe_code)]
#[cfg(feature = "enhanced")]
extern crate alloc;

use liuli::prelude::*;
use liuli_widgets::{Density, EffectsConfig, Shape, Theme, ThemePreset, paint_meter};
mod depth;
mod pages;
pub const FONT: &[u8] = include_bytes!("../assets/body.lf4");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Inputs,
    Data,
    Choices,
    Feedback,
    List,
    Icons,
    Image,
    #[cfg(feature = "enhanced")]
    Text,
    Depth,
}
pub const PAGES: &[(Page, &str, IconKind)] = &[
    (Page::Inputs, "Buttons & sliders", IconKind::Sliders),
    (Page::Data, "Meters & charts", IconKind::Gauge),
    (Page::Choices, "Tabs & selectors", IconKind::Check),
    (Page::Feedback, "Dialogs & toasts", IconKind::Bell),
    (Page::List, "Virtual list", IconKind::ChevronRight),
    (Page::Icons, "Icon set", IconKind::Diagnostics),
    (Page::Image, "Image transforms", IconKind::Waveform),
    #[cfg(feature = "enhanced")]
    (Page::Text, "Text field", IconKind::Sliders),
    (Page::Depth, "Surface & depth", IconKind::Gauge),
];

pub struct Model {
    pub page: Page,
    pub depth: depth::Depth,
    pub dark: bool,
    pub preset: ThemePreset,
    pub shape_idx: u8,
    pub effects: EffectsConfig,
    pub blur_radius: u8,
    pub tint_alpha: Option<u8>,
    pub rounded_focus: bool,
    height: u16,
    pub value: i16,
    enabled: bool,
    count: u16,
    samples: SampleRing<32>,
    tab: Key,
    choice: Key,
    confirm: bool,
    toast_until: Option<InstantMs>,
    pub offset: i32,
    selected: Option<Key>,
    icon: usize,
    angle: u8,
    alpha: i16,
    scale: i16,
    #[cfg(feature = "enhanced")]
    field: liuli::enhanced::field::TextField<'static>,
}
impl Model {
    pub fn new(height: u16) -> Self {
        let mut samples = SampleRing::default();
        for sample in [12, 25, 19, 37, 24, 36, 47, 39, 57, 50, 65, 55, 70, 68] {
            samples.push(sample);
        }
        Self {
            page: Page::Home,
            depth: depth::Depth::new(),
            dark: true,
            preset: ThemePreset::Liuli,
            shape_idx: 0,
            effects: EffectsConfig::GLASS,
            blur_radius: 5,
            tint_alpha: None,
            rounded_focus: false,
            height,
            value: 65,
            enabled: true,
            count: 0,
            samples,
            tab: Key(50),
            choice: Key(53),
            confirm: false,
            toast_until: None,
            offset: 0,
            selected: None,
            icon: 0,
            angle: 0,
            alpha: 15,
            scale: 70,
            #[cfg(feature = "enhanced")]
            field: pages::text_field(),
        }
    }
    pub fn theme(&self) -> Theme {
        let mut t = Theme::preset(self.preset, self.dark)
            .with_density(Density::TOUCH)
            .with_effects(self.effects.with_max_blur_radius(self.blur_radius))
            .with_blur_radius(self.blur_radius)
            .with_rounded_focus(self.rounded_focus);
        t = match self.shape_idx {
            1 => t.with_shape(Shape::SHARP),
            2 => t.with_shape(Shape::SOFT),
            3 => t.with_shape(Shape::ROUNDED),
            _ => t,
        };
        if let Some(alpha) = self.tint_alpha {
            t = t.with_glass_tint(None, alpha);
        }
        t
    }
    pub fn shape_name(&self) -> &'static str {
        match self.shape_idx {
            1 => "Sharp",
            2 => "Soft",
            3 => "Rounded",
            _ => "Preset",
        }
    }
}
pub enum Message {
    Action(UiAction),
    Scroll(i32),
    #[cfg(feature = "enhanced")]
    Input(alloc::string::String, bool),
    #[cfg(feature = "enhanced")]
    Edit(TextInput<'static>),
}
pub struct Catalog;
impl App for Catalog {
    type Model = Model;
    type Msg = Message;
    fn view(m: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        pages::view(m, out)
    }
    fn on_action(_: &Model, action: UiAction) -> Option<Message> {
        Some(Message::Action(action))
    }
    fn on_scroll(_: &Model, key: Key, y: i32) -> Option<Message> {
        (key == pages::LIST).then_some(Message::Scroll(y))
    }
    fn page_id(m: &Model) -> u8 {
        m.page as u8
    }
    fn page_restore(m: &Model, page: u8) -> PageRestore {
        if page == Page::List as u8 {
            PageRestore {
                scroll_key: Some(pages::LIST),
                scroll_y: m.offset,
                focus_key: m.selected,
            }
        } else {
            PageRestore::default()
        }
    }
    fn next_deadline(m: &Model, now: InstantMs) -> Option<InstantMs> {
        m.toast_until.into_iter().chain(m.depth.deadline(now)).min()
    }
    fn on_timer(m: &mut Model, now: InstantMs) -> Change {
        if m.page == Page::Depth && m.depth.advance(now) {
            return Change::View;
        }
        if m.toast_until.is_some_and(|end| now >= end) {
            m.toast_until = None;
            Change::View
        } else {
            Change::Unchanged
        }
    }
    fn paint_model(
        m: &Model,
        key: Key,
        props: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        match key.0 {
            301 => depth::backdrop(m, canvas),
            43 => paint_meter(props, canvas),
            71 => pages::paint_image(m, canvas),
            #[cfg(feature = "enhanced")]
            80 => m.field.paint(canvas),
            _ => Ok(()),
        }
    }
    #[cfg(feature = "enhanced")]
    fn accepts_text(_: &Model, key: Key) -> bool {
        key == Key(80)
    }
    #[cfg(feature = "enhanced")]
    fn on_text(_: &Model, _: Key, input: TextInput<'_>) -> Option<Message> {
        Some(match input {
            TextInput::Compose(s) => Message::Input(s.into(), false),
            TextInput::Commit(s) => Message::Input(s.into(), true),
            TextInput::Cancel => Message::Edit(TextInput::Cancel),
            TextInput::Left => Message::Edit(TextInput::Left),
            TextInput::Right => Message::Edit(TextInput::Right),
            TextInput::Backspace => Message::Edit(TextInput::Backspace),
        })
    }
    fn update(m: &mut Model, msg: Message, now: InstantMs) -> Change {
        let action = match msg {
            Message::Action(action) => action,
            Message::Scroll(y) => {
                m.offset = y;
                return Change::View;
            }
            #[cfg(feature = "enhanced")]
            Message::Input(s, commit) => {
                return edit(
                    m,
                    if commit {
                        TextInput::Commit(&s)
                    } else {
                        TextInput::Compose(&s)
                    },
                );
            }
            #[cfg(feature = "enhanced")]
            Message::Edit(input) => return edit(m, input),
        };
        match (action.key.0, action.kind) {
            (_, ActionKind::Dismiss) if m.confirm => m.confirm = false,
            (_, ActionKind::Dismiss) | (3, ActionKind::Activate) => {
                m.depth.stop(now);
                m.page = Page::Home;
                m.toast_until = None;
            }
            (5, ActionKind::Activate) => {
                m.dark = !m.dark;
                #[cfg(feature = "enhanced")]
                {
                    let t = m.theme();
                    let _ = m.field.set_colors(t.text, t.surface);
                }
            }
            (k @ 100..=108, ActionKind::Activate) => {
                let Some((page, _, _)) = PAGES.get((k - 100) as usize) else {
                    return Change::Unchanged;
                };
                m.page = *page;
            }
            (300..=329, _) if m.page == Page::Depth => match (action.key.0, action.kind) {
                (313, ActionKind::Activate) => {
                    m.preset = m.preset.next();
                    #[cfg(feature = "enhanced")]
                    {
                        let t = m.theme();
                        let _ = m.field.set_colors(t.text, t.surface);
                    }
                    return Change::View;
                }
                (314, ActionKind::Activate) => {
                    m.shape_idx = (m.shape_idx + 1) % 4;
                    return Change::View;
                }
                (315, ActionKind::Activate) => {
                    m.effects.shadows = !m.effects.shadows;
                    return Change::View;
                }
                (316, ActionKind::Activate) => {
                    m.effects.press_depth = !m.effects.press_depth;
                    return Change::View;
                }
                (317, ActionKind::Activate) => {
                    m.effects.glass_tint = !m.effects.glass_tint;
                    return Change::View;
                }
                (319, ActionKind::SetValue(v)) => {
                    m.blur_radius = v.clamp(0, 8) as u8;
                    m.effects.glass_blur = m.blur_radius > 0;
                    return Change::View;
                }
                (321, ActionKind::SetValue(v)) => {
                    let v = v.clamp(0, 15) as u8;
                    m.tint_alpha = Some(v);
                    m.effects.glass_tint = v > 0;
                    return Change::View;
                }
                (322, ActionKind::Activate) => {
                    m.rounded_focus = !m.rounded_focus;
                    return Change::View;
                }
                _ => return m.depth.action(action, now),
            },
            (30, ActionKind::Activate) => m.count = m.count.wrapping_add(1),
            (31, ActionKind::Toggle) => m.enabled = !m.enabled,
            (32 | 57, ActionKind::SetValue(value)) => m.value = value,
            (44, ActionKind::Activate) => {
                m.value = (m.value + 17) % 101;
                m.samples.push(m.value);
            }
            (50 | 51, ActionKind::Activate) => m.tab = action.key,
            (53..=55, ActionKind::Activate) => m.choice = action.key,
            (60, ActionKind::Activate) => m.confirm = true,
            (63, ActionKind::Activate) => {
                m.confirm = false;
                m.toast_until = Some(now.saturating_add(2000));
            }
            (64, ActionKind::Activate) => m.confirm = false,
            (65, ActionKind::Activate) => m.toast_until = Some(now.saturating_add(2000)),
            (1000..=1999, ActionKind::Activate) => m.selected = Some(action.key),
            (200..=210, ActionKind::Activate) => m.icon = action.key.0 as usize - 200,
            (72, ActionKind::Activate) => m.angle = m.angle.wrapping_add(16),
            (73, ActionKind::SetValue(a)) => m.alpha = a,
            (74, ActionKind::SetValue(percent)) => m.scale = percent,
            #[cfg(feature = "enhanced")]
            (81, ActionKind::Activate) => return edit(m, TextInput::Compose("告警")),
            #[cfg(feature = "enhanced")]
            (82, ActionKind::Activate) => return edit(m, TextInput::Commit("告警")),
            #[cfg(feature = "enhanced")]
            (83, ActionKind::Activate) => return edit(m, TextInput::Cancel),
            #[cfg(feature = "enhanced")]
            (84, ActionKind::Activate) => return edit(m, TextInput::Backspace),
            _ => return Change::Unchanged,
        }
        Change::View
    }
}
#[cfg(feature = "enhanced")]
fn edit(m: &mut Model, input: TextInput<'_>) -> Change {
    if m.field.apply(input).is_ok() {
        Change::View
    } else {
        Change::Unchanged
    }
}
