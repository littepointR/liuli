#![no_std]
#![forbid(unsafe_code)]
#[cfg(feature = "enhanced")]
extern crate alloc;

use liuli::prelude::*;
use liuli_widgets::{Density, EffectsConfig, Shape, Theme, ThemePreset, paint_meter};
mod depth;
mod motion;
mod pages;
pub mod showcase;
mod studio;
mod tape;
pub const FONT: &[u8] = include_bytes!("../assets/body.lf4");

pub fn studio_fonts(body: Lf4Font<'_>) -> Result<FontSet<'_>, liuli::text::FontError> {
    let heading = Lf4Font::from_bytes(include_bytes!("../assets/heading.lf4"))?;
    Ok(FontSet::single(body).with_title(heading))
}

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
    Motion,
}
pub const PAGES: &[(Page, &str, IconKind)] = &[
    (Page::Inputs, "Mixer", IconKind::Sliders),
    (Page::Data, "Line check", IconKind::Gauge),
    (Page::Choices, "Input routing", IconKind::Check),
    (Page::Feedback, "Recording", IconKind::Bell),
    (Page::List, "Tape library", IconKind::ChevronRight),
    (Page::Icons, "Cue stamps", IconKind::Diagnostics),
    (Page::Image, "Deck artwork", IconKind::Waveform),
    #[cfg(feature = "enhanced")]
    (Page::Text, "Session notes", IconKind::Sliders),
    (Page::Depth, "Tape sleeve", IconKind::Gauge),
    (Page::Motion, "Fade preview", IconKind::Waveform),
];

impl Page {
    pub fn parent(self) -> Self {
        match self {
            Self::Data | Self::Choices | Self::Image => Self::Inputs,
            Self::Icons => Self::List,
            #[cfg(feature = "enhanced")]
            Self::Text => Self::List,
            _ => Self::Home,
        }
    }
    pub fn route_key(self) -> Key {
        if self == Self::Home {
            return Key(98);
        }
        Key(100 + PAGES.iter().position(|p| p.0 == self).unwrap() as u32)
    }
    fn is_tab(self) -> bool {
        matches!(self, Self::Home | Self::List | Self::Inputs)
    }
}

pub struct Model {
    pub page: Page,
    pub depth: depth::Depth,
    motion: motion::Motion,
    pub dark: bool,
    pub preset: ThemePreset,
    pub shape_idx: u8,
    pub effects: EffectsConfig,
    pub blur_radius: u8,
    pub tint_alpha: Option<u8>,
    pub rounded_focus: bool,
    pub text_gamma: TextGamma,
    height: u16,
    landscape: bool,
    deck: tape::Deck,
    pub value: i16,
    level: i16,
    enabled: bool,
    count: u16,
    last_cue: Option<(usize, u32)>,
    samples: SampleRing<32>,
    tab: Key,
    choice: Key,
    confirm: Option<InstantMs>,
    #[cfg(feature = "enhanced")]
    room_tone_logged: bool,
    capture_saved: bool,
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
        Self::for_size(Size::new(240, height.into()))
    }
    pub fn for_size(size: Size) -> Self {
        let mut samples = SampleRing::default();
        for sample in [12, 25, 19, 37, 24, 36, 47, 39, 57, 50, 65, 55, 70, 68] {
            samples.push(sample);
        }
        #[cfg(feature = "enhanced")]
        let field = {
            let mut f = pages::text_field(studio::palette(Theme::DARK));
            let _ = f.set_gamma(TextGamma::Perceptual);
            f
        };
        Self {
            page: Page::Home,
            depth: depth::Depth::new(),
            motion: motion::Motion::new(),
            dark: true,
            preset: ThemePreset::Liuli,
            shape_idx: 0,
            effects: EffectsConfig::GLASS,
            blur_radius: 5,
            tint_alpha: None,
            rounded_focus: FocusRingStyle::DEFAULT.rounded,
            text_gamma: TextGamma::Perceptual,
            height: size.height.min(u16::MAX.into()) as u16,
            landscape: size.width >= 540,
            deck: tape::Deck::new(),
            value: 65,
            level: 68,
            enabled: true,
            count: 0,
            last_cue: None,
            samples,
            tab: Key(50),
            choice: Key(53),
            confirm: None,
            #[cfg(feature = "enhanced")]
            room_tone_logged: false,
            capture_saved: false,
            toast_until: None,
            offset: 0,
            selected: Some(Key(1000)),
            icon: 0,
            angle: 0,
            alpha: 15,
            scale: 100,
            #[cfg(feature = "enhanced")]
            field,
        }
    }
    pub fn theme(&self) -> Theme {
        let mut t = studio::palette(Theme::preset(self.preset, self.dark))
            .with_typography(liuli_widgets::Typography {
                title: FontId::Title20,
                ..liuli_widgets::Typography::STANDARD
            })
            .with_density(Density::TOUCH)
            .with_effects(self.effects.with_max_blur_radius(self.blur_radius))
            .with_blur_radius(self.blur_radius)
            .with_rounded_focus(self.rounded_focus)
            .with_text_gamma(self.text_gamma);
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
    fn mark(&mut self) {
        self.count = self.count.saturating_add(1);
        self.last_cue = Some((
            self.icon,
            if self.deck.recording.is_some() {
                self.deck.recording_seconds
            } else {
                self.deck.elapsed()
            },
        ));
    }
    fn sample_input(&mut self, phase: u64) {
        const WAVE: [i16; 12] = [52, 77, 63, 93, 70, 44, 82, 96, 57, 71, 55, 83];
        let phase = (phase % WAVE.len() as u64) as usize + (self.choice.0 - 53) as usize;
        self.level = self.value * WAVE[phase % WAVE.len()] / 100;
        self.samples.push(self.level);
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
    fn view(m: &Model) -> impl Scene {
        pages::view(m)
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
    fn animations(m: &mut Model, bind: &mut dyn FnMut(Key, &mut dyn Animation)) {
        bind(Key(300), &mut m.depth);
        bind(Key(400), &mut m.motion.tween);
    }
    fn next_deadline(m: &Model, now: InstantMs) -> Option<InstantMs> {
        m.toast_until
            .into_iter()
            .chain(m.deck.deadline(now).filter(|_| m.confirm.is_none()))
            .min()
    }
    fn on_timer(m: &mut Model, now: InstantMs) -> Change {
        let seconds = (m.deck.elapsed(), m.deck.recording_seconds);
        let transport_changed = m.confirm.is_none() && m.deck.advance(now);
        if seconds != (m.deck.elapsed(), m.deck.recording_seconds) {
            m.sample_input(now.0 / 1000);
        }
        if m.toast_until.is_some_and(|end| now >= end) {
            m.toast_until = None;
            Change::View
        } else if transport_changed
            && matches!(
                m.page,
                Page::Home | Page::Feedback | Page::Data | Page::List
            )
        {
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
            450 => studio::paint_tape(m, canvas),
            456 => studio::waveform(props, canvas),
            402 | 404 => motion::paint(props, canvas),
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
            (_, ActionKind::Dismiss) if m.confirm.is_some() => {
                if let (Some(end), Some(start)) = (m.confirm.take(), m.deck.recording) {
                    m.deck.recording = Some(start.saturating_add(now.saturating_sub(end)));
                }
            }
            (_, ActionKind::Dismiss) | (3, ActionKind::Activate) => {
                m.depth.stop(now);
                m.motion.reset();
                if m.page == Page::Feedback && m.deck.recording.is_some() {
                    m.deck.advance(now);
                    m.confirm = Some(now);
                    return Change::View;
                }
                m.page = m.page.parent();
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
            (98, ActionKind::Activate) => m.page = Page::Home,
            (90, ActionKind::Activate) => m.deck.play_pause(now, m.motion.duration_ms()),
            (k @ 100..=199, ActionKind::Activate) => {
                let Some((page, _, _)) = PAGES.get((k - 100) as usize) else {
                    return Change::Unchanged;
                };
                m.page = *page;
                if *page == Page::Feedback {
                    m.capture_saved = false;
                    m.deck.recording_seconds = 0;
                }
            }
            (406 | 407 | 410..=412, ActionKind::Activate) if m.page == Page::Motion => {
                return m.motion.action(action.key, now);
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
            (30, ActionKind::Activate) if m.enabled => m.mark(),
            (31, ActionKind::Toggle) => m.enabled = !m.enabled,
            (32 | 57, ActionKind::SetValue(value)) => {
                m.value = value;
                m.sample_input(now.0 / 1000);
            }
            (44, ActionKind::Activate) => {
                m.sample_input(now.0 / 16);
            }
            (50 | 51, ActionKind::Activate) => m.tab = action.key,
            (53..=55, ActionKind::Activate) => m.choice = action.key,
            (60, ActionKind::Activate) if m.deck.recording.is_some() => {
                m.deck.advance(now);
                m.confirm = Some(now);
            }
            (60, ActionKind::Activate) => {
                m.capture_saved = false;
                m.deck.record(now);
            }
            (63, ActionKind::Activate) if m.confirm.is_some() => {
                m.deck.save(m.confirm.unwrap());
                m.capture_saved = true;
                m.selected = Some(m.deck.loaded.key);
                m.offset = 0;
                m.confirm = None;
                m.toast_until = Some(now.saturating_add(2000));
            }
            (64, ActionKind::Activate) => {
                m.confirm = None;
                m.deck.recording = None;
                m.page = Page::Home;
            }
            (65, ActionKind::Activate) if m.enabled => {
                m.mark();
                m.toast_until = Some(now.saturating_add(2000));
            }
            (1000..=2007, ActionKind::Activate) => {
                m.deck.load(action.key);
                m.selected = Some(action.key);
                m.page = Page::Home;
            }
            (200..=210, ActionKind::Activate) => m.icon = action.key.0 as usize - 200,
            (72, ActionKind::Activate) => m.angle = m.angle.wrapping_add(16),
            (73, ActionKind::SetValue(a)) => m.alpha = a,
            (74, ActionKind::SetValue(percent)) => m.scale = percent,
            #[cfg(feature = "enhanced")]
            (81, ActionKind::Activate) => return edit(m, TextInput::Compose(" wind")),
            #[cfg(feature = "enhanced")]
            (82, ActionKind::Activate) => return edit(m, TextInput::Commit(" wind")),
            #[cfg(feature = "enhanced")]
            (83, ActionKind::Activate) => return edit(m, TextInput::Cancel),
            #[cfg(feature = "enhanced")]
            (84, ActionKind::Activate) => return edit(m, TextInput::Backspace),
            #[cfg(feature = "enhanced")]
            (863, ActionKind::Toggle) => m.room_tone_logged = !m.room_tone_logged,
            #[cfg(feature = "enhanced")]
            (85, ActionKind::Activate) => {
                m.text_gamma = if m.text_gamma == TextGamma::Perceptual {
                    TextGamma::Linear
                } else {
                    TextGamma::Perceptual
                };
                let _ = m.field.set_gamma(m.text_gamma);
            }
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
