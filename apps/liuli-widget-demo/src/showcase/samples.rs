use super::{SHOWCASE_BODY_LF4, row_props, tab_theme};
use core::num::NonZeroU16;
use liuli::prelude::*;
use liuli_widgets::{
    self as widgets, ButtonVariant, Compositor, EffectsConfig, Elevation, FixedTextEditor,
    ImeEvent, ImeState, InputMode, Material, StaticPinyinDict, SurfaceSpec, Theme, ThemedUi,
};

pub const LIST: Key = Key(720);
pub const IMAGE: Key = Key(732);
pub const TICKS: Key = Key(712);
const KNOB: Key = Key(710);
const FIELD: Key = Key(740);
const KEYBOARD: Key = Key(741);
const FIRST_ROW: u32 = 2000;
const RECORDS: u32 = 1000;

// Every candidate in the demo dictionary has a glyph in both shipped LF4 sizes.
pub const DICT: StaticPinyinDict = StaticPinyinDict::new(&[
    ("zhong", "中"),
    ("wen", "文"),
    ("zhong'wen", "中文"),
    ("liu'li", "琉璃"),
    ("shu'ru", "输入"),
    ("dong'xiao", "动效"),
    ("tu'biao", "图表"),
]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SamplePage {
    #[default]
    Controls,
    Records,
    Graphics,
    Keyboard,
    Buttons,
}

struct SampleView<'a> {
    samples: &'a Samples,
    zh: bool,
}

fn pages<'a>(zh: bool) -> [widgets::TabPage<SamplePage, SampleView<'a>>; 5] {
    [
        widgets::TabPage::new(
            SamplePage::Controls,
            Key(701),
            TextRef::Static(if zh { "调节" } else { "Controls" }),
            |v: &SampleView, t, ui| v.samples.controls(ui, t, v.zh),
        ),
        widgets::TabPage::new(
            SamplePage::Records,
            Key(702),
            TextRef::Static(if zh { "列表" } else { "List" }),
            |v: &SampleView, t, ui| v.samples.records(ui, t, v.zh),
        ),
        widgets::TabPage::new(
            SamplePage::Graphics,
            Key(703),
            TextRef::Static(if zh { "图形" } else { "Graphics" }),
            |v: &SampleView, t, ui| v.samples.graphics(ui, t, v.zh),
        ),
        widgets::TabPage::new(
            SamplePage::Keyboard,
            Key(704),
            TextRef::Static(if zh { "输入" } else { "Text" }),
            |v: &SampleView, t, ui| v.samples.keyboard(ui, t, v.zh),
        ),
        widgets::TabPage::new(
            SamplePage::Buttons,
            Key(705),
            TextRef::Static(if zh { "按钮" } else { "Buttons" }),
            |v: &SampleView, t, ui| v.samples.buttons(ui, t, v.zh),
        ),
    ]
}

#[derive(Clone)]
pub struct Samples {
    pub page: SamplePage,
    pub value: i16,
    pub last_button: Option<usize>,
    pub icon: usize,
    pub choice: Key,
    history: [i16; 12],
    pub confirm: bool,
    pub toast_until: Option<InstantMs>,
    pub offset: i32,
    pub selected: u32,
    pub angle: u8,
    pub scale: i16,
    alpha: u8,
    pub glass: bool,
    origin: (u8, i16, Point),
    pan: Point,
    pub editor: FixedTextEditor<128>,
    pub ime: ImeState,
    font: Lf4Font<'static>,
    #[cfg(feature = "enhanced")]
    script: usize,
    #[cfg(feature = "enhanced")]
    shaped: Option<alloc::sync::Arc<liuli::enhanced::field::TextField<'static>>>,
}

impl Samples {
    pub(super) fn new() -> Self {
        Self {
            page: SamplePage::Controls,
            value: 25,
            last_button: None,
            icon: 0,
            choice: Key(751),
            history: [20, 25, 30, 40, 35, 50, 55, 65, 60, 50, 50, 25],
            confirm: false,
            toast_until: None,
            offset: 0,
            selected: 0,
            angle: 0,
            scale: 85,
            alpha: 15,
            glass: false,
            pan: Point::zero(),
            origin: (0, 85, Point::zero()),
            editor: FixedTextEditor::new(),
            ime: ImeState::new(InputMode::EnLower),
            font: Lf4Font::from_bytes(SHOWCASE_BODY_LF4).expect("gallery font"),
            #[cfg(feature = "enhanced")]
            script: 0,
            #[cfg(feature = "enhanced")]
            shaped: None,
        }
    }

    pub(super) fn leave(&mut self) {
        self.confirm = false;
        self.toast_until = None;
        self.editor.apply_ime(self.ime.clear_composing());
    }

    pub(super) fn expire(&mut self, now: InstantMs) -> bool {
        if self.toast_until.is_some_and(|until| until <= now) {
            self.toast_until = None;
            return true;
        }
        false
    }

    fn set_value(&mut self, value: i16) {
        let next = value.clamp(0, 100);
        if self.value != next {
            self.value = next;
            self.history.rotate_left(1);
            self.history[11] = next;
        }
    }

    pub(super) fn action(&mut self, ev: UiAction, now: InstantMs, _theme: Theme) -> Change {
        if let Some(page) = widgets::Tabs::new(&pages(false), self.page)
            .ok()
            .and_then(|tabs| tabs.selection(ev))
        {
            if self.page == page {
                return Change::Unchanged;
            }
            self.leave();
            self.page = page;
            #[cfg(feature = "enhanced")]
            if page == SamplePage::Keyboard {
                self.refresh_script(_theme);
            }
            return Change::View;
        }
        match (ev.key, ev.kind) {
            (_, ActionKind::Dismiss) if self.confirm => self.confirm = false,
            (KNOB | TICKS | Key(713), ActionKind::Adjust(delta)) => {
                self.set_value(self.value.saturating_add(delta.saturating_mul(5)))
            }
            (TICKS | Key(713), ActionKind::SetValue(value)) => self.set_value(value),
            (key @ (Key(751) | Key(752)), ActionKind::Activate) => {
                self.choice = key;
                self.set_value(if key == Key(751) { 25 } else { 75 });
            }
            (Key(716), ActionKind::Activate) => self.confirm = true,
            (Key(762), ActionKind::Activate) if self.confirm => {
                self.confirm = false;
                self.choice = Key(751);
                self.set_value(25);
                self.toast_until = Some(now.saturating_add(1600));
            }
            (Key(763), ActionKind::Activate) => self.confirm = false,
            (Key(k), ActionKind::Activate) if (FIRST_ROW..FIRST_ROW + RECORDS).contains(&k) => {
                self.selected = k - FIRST_ROW
            }
            (Key(k @ 772..=777), ActionKind::Activate) => {
                self.last_button = Some((k - 772) as usize)
            }
            (Key(k @ 781..=791), ActionKind::Activate) => self.icon = (k - 781) as usize,
            #[cfg(feature = "enhanced")]
            (Key(744), ActionKind::Activate) => {
                self.script = (self.script + 1) % 3;
                self.refresh_script(_theme);
            }
            (Key(734), ActionKind::Activate) => self.angle = self.angle.wrapping_add(24),
            (Key(735), ActionKind::Activate) => self.glass = !self.glass,
            (Key(736), ActionKind::SetValue(v)) => self.scale = v.clamp(30, 120),
            (Key(736), ActionKind::Adjust(d)) => {
                self.scale = self
                    .scale
                    .saturating_add(d.saturating_mul(5))
                    .clamp(30, 120)
            }
            (Key(737), ActionKind::SetValue(v)) => self.alpha = v.clamp(1, 15) as u8,
            (Key(737), ActionKind::Adjust(d)) => {
                self.alpha = i16::from(self.alpha).saturating_add(d).clamp(1, 15) as u8
            }
            _ => return Change::Unchanged,
        }
        Change::View
    }

    #[cfg(feature = "enhanced")]
    pub(super) fn refresh_script(&mut self, t: Theme) {
        let text = ["Pier A / سلام कि", "سلام", "कि / A"][self.script];
        self.shaped = Some(alloc::sync::Arc::new(crate::pages::shaped_field(t, text)));
    }

    pub(super) fn accepts_text(&self, key: Key) -> bool {
        self.page == SamplePage::Keyboard && key == FIELD
    }
    pub(super) fn text(&mut self, key: Key, input: TextInput<'_>) -> Option<Change> {
        self.accepts_text(key).then(|| {
            if self.editor.apply(input) {
                Change::View
            } else {
                Change::Unchanged
            }
        })
    }
    pub(super) fn pointer(&mut self, key: Key, ev: PointerEvent) -> Change {
        match key {
            KNOB if matches!(ev.phase, PointerPhase::Down | PointerPhase::Move) => {
                self.set_value(widgets::arc_knob_from_pointer(ev, 0, 100, 5))
            }
            FIELD if widgets::text_box_on_pointer(&mut self.editor, ev, Some(&self.font)) => (),
            KEYBOARD => {
                let rev = self.ime.revision();
                let e = widgets::soft_keyboard_on_pointer(&mut self.ime, &DICT, ev);
                let done = matches!(e, ImeEvent::Done);
                let changed = self.editor.apply_ime(e);
                if done {
                    self.editor.apply_ime(self.ime.clear_composing());
                }
                if !changed && !done && rev == self.ime.revision() {
                    return Change::Unchanged;
                }
            }
            _ => return Change::Unchanged,
        }
        Change::View
    }

    pub(super) fn transform(&mut self, g: TransformGesture) -> Change {
        match g.phase {
            GesturePhase::Begin => self.origin = (self.angle, self.scale, self.pan),
            GesturePhase::Update => {
                self.angle = self
                    .origin
                    .0
                    .wrapping_add((g.radians * (128.0 / core::f32::consts::PI)) as i16 as u8);
                self.scale = (f32::from(self.origin.1) * g.scale).clamp(30.0, 120.0) as i16;
                self.pan = Point::new(
                    (self.origin.2.x + g.translation.x).clamp(-60, 60),
                    (self.origin.2.y + g.translation.y).clamp(-30, 30),
                );
            }
            GesturePhase::Cancel => (self.angle, self.scale, self.pan) = self.origin,
            GesturePhase::End => return Change::Unchanged,
        }
        Change::View
    }

    pub(super) fn view(
        &self,
        t: Theme,
        ui: &mut Children<'_, '_>,
        locale: LocaleId,
    ) -> Result<(), BuildError> {
        let zh = locale == LocaleId(1);
        let pages = pages(zh);
        let tabs = widgets::Tabs::new(&pages, self.page)?;
        tabs.header(
            ui,
            Key(700),
            widgets::TabLayout::Wrap { max_rows: 2 },
            tab_theme(t, 28).map_parts(|parts| {
                parts.with_tab(
                    parts
                        .tab
                        .with_indicator_layout(IndicatorLayout {
                            shape: IndicatorShape::Fill,
                            edge_offset: 2,
                            ..IndicatorLayout::UNDERLINE
                        })
                        .with_indicator_color(t.surface_alt)
                        .with_checked_fg(t.accent)
                        .with_indicator_inset(2)
                        .with_indicator_radius(6)
                        .with_track_size(0)
                        .with_indicator_motion(IndicatorMotion::Fade),
                )
            }),
        )?;
        tabs.panel(
            ui,
            Key(699),
            NodeProps::new()
                .width(Length::Fill(1))
                .height(Length::Fill(1))
                .gap(8),
            &SampleView { samples: self, zh },
            t,
        )
    }

    fn controls(&self, ui: &mut Children<'_, '_>, t: Theme, zh: bool) -> Result<(), BuildError> {
        ui.row(
            Key(719),
            row_props(68).gap(8).align_cross(Align::Center),
            |r| {
                let value = r.text_fmt(format_args!("{}", self.value))?;
                widgets::arc_knob(r, KNOB, 66, 0..=100, self.value, 5, value, t)?;
                r.sparkline(
                    Key(711),
                    &self.history,
                    NodeProps {
                        height: Length::Px(48),
                        ..t.sparkline()
                    },
                )
            },
        )?;
        ui.row(Key(706), row_props(24), |r| {
            for (k, s) in [
                (707, if zh { "刻度滑块" } else { "Tick slider" }),
                (708, if zh { "数值编辑" } else { "Numeric" }),
            ] {
                r.label(
                    Key(k),
                    t.caption().width(Length::Fill(1)).text(TextRef::Static(s)),
                )?;
            }
            Ok(())
        })?;
        ui.row(Key(709), row_props(32).gap(8), |r| {
            r.slider(
                TICKS,
                NodeProps {
                    min: 0,
                    max: 100,
                    step: 5,
                    paint_mode: PaintMode::Override,
                    ..t.slider(self.value)
                },
            )?;
            widgets::number_editor(r, Key(713), 0..=100, self.value, 5, t)
        })?;
        widgets::selector(
            ui,
            Key(750),
            &[
                (
                    Key(751),
                    TextRef::Static(if zh { "低预设" } else { "Low preset" }),
                ),
                (
                    Key(752),
                    TextRef::Static(if zh { "高预设" } else { "High preset" }),
                ),
            ],
            self.choice,
            t,
        )?;
        let badge = ui.text_fmt(format_args!("{}", self.value))?;
        widgets::badge_button(
            ui,
            [Key(715), Key(716), Key(717)],
            TextRef::Static(if zh { "恢复预设" } else { "Reset preset" }),
            badge,
            t.accent,
            t,
        )?;
        if self.toast_until.is_some() {
            widgets::toast(
                ui,
                Key(718),
                TextRef::Static(if zh { "已恢复" } else { "Preset restored" }),
                t,
            )?;
        }
        Ok(())
    }

    fn buttons(&self, ui: &mut Children<'_, '_>, t: Theme, zh: bool) -> Result<(), BuildError> {
        use ButtonVariant::*;
        const VARIANTS: [(ButtonVariant, IconKind, &str); 6] = [
            (Surface, IconKind::Sliders, "Surface"),
            (Primary, IconKind::Check, "Primary"),
            (Secondary, IconKind::Waveform, "Secondary"),
            (Outline, IconKind::Gauge, "Outline"),
            (Ghost, IconKind::Moon, "Ghost"),
            (Danger, IconKind::Warning, "Danger"),
        ];
        let mut panel = ThemedUi::new(ui, tab_theme(t, 32));
        panel.wrap(
            Key(770),
            NodeProps::new()
                .width(Length::Fill(1))
                .height(Length::Content)
                .gap(6),
            |wrap| {
                for (i, (variant, icon, name)) in VARIANTS.into_iter().enumerate() {
                    wrap.button_props(
                        Key(772 + i as u32),
                        variant,
                        t.button(variant)
                            .width(Length::Px(102))
                            .padding(EdgeInsets::symmetric(2, 2))
                            .icon(icon)
                            .text(TextRef::Static(name)),
                    )?;
                }
                Ok(())
            },
        )?;
        panel.button_enabled(
            Key(778),
            if zh { "禁用" } else { "Disabled" },
            IconKind::Bell,
            Secondary,
            false,
        )?;
        let last = self.last_button.map_or("-", |i| VARIANTS[i].2);
        let label = panel.text_fmt(format_args!(
            "{}  {}",
            if zh { "最近操作" } else { "Last pressed" },
            last
        ))?;
        panel.caption(Key(779), label)?;
        panel.wrap(
            Key(780),
            NodeProps::new()
                .width(Length::Fill(1))
                .height(Length::Content)
                .gap(4),
            |wrap| {
                for (i, &(icon, _)) in crate::pages::ICONS.iter().enumerate() {
                    let variant = if self.icon == i { Primary } else { Surface };
                    wrap.button_props(
                        Key(781 + i as u32),
                        variant,
                        t.button(variant)
                            .size(Length::Px(32), Length::Px(32))
                            .padding(EdgeInsets::ZERO)
                            .icon(icon)
                            .checked(self.icon == i),
                    )?;
                }
                Ok(())
            },
        )?;
        let icon = panel.text_fmt(format_args!(
            "IconKind::{:?}",
            crate::pages::ICONS[self.icon].0
        ))?;
        panel.caption(Key(792), icon)
    }

    fn records(&self, ui: &mut Children<'_, '_>, t: Theme, zh: bool) -> Result<(), BuildError> {
        let summary = ui.text_fmt(format_args!(
            "{} {:04} / 1000",
            if zh { "选中" } else { "Selected" },
            self.selected + 1
        ))?;
        ui.label(Key(724), t.paragraph().text(summary))?;
        widgets::virtual_list(
            ui,
            [LIST, Key(721), Key(722)],
            RECORDS,
            NonZeroU16::new(30).unwrap(),
            self.offset,
            246,
            |rows, id| {
                let style = if id == self.selected {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Surface
                };
                let label = rows.text_fmt(format_args!(
                    "{}  {:04}",
                    if zh { "记录" } else { "Record" },
                    id + 1
                ))?;
                rows.button(
                    Key(FIRST_ROW + id),
                    t.button(style)
                        .height(Length::Px(30))
                        .text(label)
                        .checked(id == self.selected),
                )
            },
        )
    }

    fn graphics(&self, ui: &mut Children<'_, '_>, t: Theme, zh: bool) -> Result<(), BuildError> {
        let mut surface = ThemedUi::new(ui, t.with_effects(EffectsConfig::GLASS));
        surface.stack(
            Key(730),
            row_props(132).padding(EdgeInsets::all(6)),
            |stage| {
                stage.custom(
                    Key(729),
                    NodeProps::new()
                        .width(Length::Fill(1))
                        .height(Length::Fill(1))
                        .bg(t.surface)
                        .accent(t.accent),
                )?;
                let spec = SurfaceSpec::CARD
                    .with_elevation(Elevation::Floating)
                    .with_material(if self.glass {
                        Material::GLASS
                    } else {
                        Material::Solid
                    })
                    .with_compositor(if self.glass {
                        Compositor::isolated(13, Projection(12))
                    } else {
                        Compositor::default()
                    });
                stage.surface_column(
                    Key(731),
                    spec,
                    NodeProps::new()
                        .size(Length::Px(196), Length::Px(112))
                        .padding(EdgeInsets::all(8))
                        .stack_align(StackAlign::Center),
                    |card| {
                        card.custom(
                            IMAGE,
                            NodeProps::new()
                                .size(Length::Fill(1), Length::Fill(1))
                                .clickable(true),
                        )?;
                        card.set_custom_rev(IMAGE, self.image_rev())
                    },
                )
            },
        )?;
        ui.row(Key(733), row_props(32).gap(5), |r| {
            r.button(
                Key(734),
                t.button(ButtonVariant::Outline)
                    .icon(IconKind::Reset)
                    .text(TextRef::Static(if zh { "旋转" } else { "Rotate" })),
            )?;
            r.button(
                Key(735),
                t.button(ButtonVariant::Secondary)
                    .icon(IconKind::Moon)
                    .text(TextRef::Static(if self.glass { "Glass" } else { "Solid" })),
            )
        })?;
        let info = ui.text_fmt(format_args!(
            "{} {}%    {} {}/15",
            if zh { "缩放" } else { "Scale" },
            self.scale,
            if zh { "透明" } else { "Alpha" },
            self.alpha
        ))?;
        ui.label(Key(738), t.caption().text(info))?;
        widgets::number_editor(ui, Key(736), 30..=120, self.scale, 5, t)?;
        widgets::number_editor(ui, Key(737), 1..=15, self.alpha.into(), 1, t)
    }

    fn image_rev(&self) -> u32 {
        // Retained image placement also changes when a two-contact gesture pans the preview.
        u32::from(self.angle)
            | ((self.scale as u32 - 30) << 8)
            | (u32::from(self.alpha) << 15)
            | (((self.pan.x + 60) as u32) << 19)
            | (((self.pan.y + 30) as u32) << 26)
    }

    fn keyboard(&self, ui: &mut Children<'_, '_>, t: Theme, zh: bool) -> Result<(), BuildError> {
        widgets::text_box_with_focus(
            ui,
            FIELD,
            &self.editor,
            if zh { "输入文本" } else { "Type here" },
            true,
            t,
        )?;
        ui.label(
            Key(742),
            t.caption().text(TextRef::Static(if zh {
                "拼音示例  zhong wen"
            } else {
                "Pinyin sample: zhong wen"
            })),
        )?;
        widgets::soft_keyboard(ui, KEYBOARD, &self.ime, 154, t)?;
        #[cfg(feature = "enhanced")]
        if self.shaped.is_some() {
            ui.custom(Key(743), row_props(44).bg(t.surface))?;
            ui.set_custom_rev(Key(743), self.script as u32)?;
            ui.button(
                Key(744),
                t.secondary_button()
                    .height(Length::Px(28))
                    .padding(EdgeInsets::ZERO)
                    .text(TextRef::Static(if zh {
                        "切换文字样本"
                    } else {
                        "Next script sample"
                    })),
            )?;
        }
        Ok(())
    }

    pub(super) fn paint(
        &self,
        key: Key,
        p: &StoredProps,
        c: &mut WidgetCanvas<'_>,
        t: Theme,
    ) -> Result<(), BuildError> {
        match key {
            KNOB => widgets::paint_arc_knob(p, c.state, c),
            FIELD => widgets::paint_text_box(&self.editor, "", p, c.state, Some(&self.font), c),
            KEYBOARD => widgets::paint_soft_keyboard(&self.ime, &DICT, p, t, c),
            Key(729) => c.gradient_rect(
                Rect::new(0, 0, c.size().width, c.size().height),
                p.bg_raw,
                p.accent,
                GRADIENT_HORIZONTAL,
                8,
            ),
            IMAGE => {
                let s = c.size();
                let w = s.width.min(s.height * 240 / 136) * self.scale as u32 / 100;
                let h = w * 136 / 240;
                c.bitmap(
                    Rect::new(
                        (s.width as i32 - w as i32) / 2 + self.pan.x,
                        (s.height as i32 - h as i32) / 2 + self.pan.y,
                        w,
                        h,
                    ),
                    &crate::studio::CASSETTE,
                    self.angle,
                    self.alpha,
                )
            }
            #[cfg(feature = "enhanced")]
            Key(743) => {
                if let Some(f) = &self.shaped {
                    f.paint(c)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}

pub(super) fn captures(key: Key) -> bool {
    matches!(key, KNOB | FIELD | KEYBOARD)
}
