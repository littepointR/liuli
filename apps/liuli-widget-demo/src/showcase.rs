use core::fmt::{self, Write};

use liuli::prelude::*;
use liuli_widgets::tokens::rgb8;
use liuli_widgets::{
    ChartSeries, DropdownState, GaugeSpec, MultiLineState, PageTransition, RollerState,
    TableColumn, TextSpan, Theme, TransitionKind, checkbox, data_table, dropdown, gauge,
    multi_line_editor, paint_bar_chart, paint_checkbox, paint_gauge, paint_multi_line_chart,
    paint_multi_line_editor, paint_radio, paint_rich_text, paint_roller, paint_spinner, radio,
    rich_text, roller, spinner,
};

pub const SHOWCASE_BODY_LF4: &[u8] = include_bytes!("../assets/showcase_body.lf4");
pub const SHOWCASE_SMALL_LF4: &[u8] = include_bytes!("../assets/showcase_small.lf4");

pub fn showcase_fonts() -> Result<FontSet<'static>, liuli::text::FontError> {
    let body = Lf4Font::from_bytes(SHOWCASE_BODY_LF4)?;
    let small = Lf4Font::from_bytes(SHOWCASE_SMALL_LF4)?;
    Ok(FontSet::new(body, small))
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub enum ShowcaseTab {
    #[default]
    Overview = 0,
    Forms = 1,
    Charts = 2,
    Layout = 3,
}

impl ShowcaseTab {
    pub const fn index(self) -> u8 {
        self as u8
    }

    pub const fn from_index(idx: u8) -> Self {
        match idx {
            1 => Self::Forms,
            2 => Self::Charts,
            3 => Self::Layout,
            _ => Self::Overview,
        }
    }
}

pub mod i18n_keys {
    use liuli::prelude::TextKey;

    pub const TITLE: TextKey = TextKey(0);
    pub const TAB_OVERVIEW: TextKey = TextKey(1);
    pub const TAB_FORMS: TextKey = TextKey(2);
    pub const TAB_CHARTS: TextKey = TextKey(3);
    pub const TAB_LAYOUT: TextKey = TextKey(4);
    pub const BTN_LANG: TextKey = TextKey(5);
    pub const BTN_THEME: TextKey = TextKey(6);
    pub const CHK_LIVE: TextKey = TextKey(7);
    pub const CHK_BRIGHT: TextKey = TextKey(8);
    pub const CHK_DIAG: TextKey = TextKey(9);
    pub const RAD_SLIDE_L: TextKey = TextKey(10);
    pub const RAD_SLIDE_U: TextKey = TextKey(11);
    pub const RAD_FADE: TextKey = TextKey(12);
    pub const BTN_PAUSE_LIVE: TextKey = TextKey(13);
    pub const BTN_RESUME_LIVE: TextKey = TextKey(14);
    pub const BTN_TRANS_CYCLE: TextKey = TextKey(15);
    pub const BTN_ADD_TEXT: TextKey = TextKey(16);
    pub const BTN_ADD_LINE: TextKey = TextKey(17);
    pub const BTN_DEL: TextKey = TextKey(18);
    pub const BTN_RESET: TextKey = TextKey(19);
    pub const STATUS_FMT: TextKey = TextKey(20);
    pub const BTN_TRANS_SLIDE_L: TextKey = TextKey(21);
    pub const BTN_TRANS_SLIDE_U: TextKey = TextKey(22);
    pub const BTN_TRANS_FADE: TextKey = TextKey(23);
}

pub const EN_ENTRIES: &[&str] = &[
    "liuli M1-M6 SDK",
    "Overview",
    "Forms",
    "Charts",
    "Layout",
    "EN / ZH",
    "Theme",
    "Live Sample",
    "Brightness",
    "Diagnostics",
    "Trans: SlideLeft",
    "Trans: SlideUp",
    "Trans: Fade",
    "Pause Live",
    "Start Live",
    "Next Transition",
    "+ Text",
    "+ Line",
    "Del",
    "Reset",
    "Load: {0}% | Temp: {1}C | MaxNodes: {2}",
    "Trans: SlideL",
    "Trans: SlideU",
    "Trans: Fade",
];

pub const ZH_ENTRIES: &[&str] = &[
    "琉璃 M1-M6 演示",
    "主仪表盘",
    "表单输入",
    "数据图表",
    "网格布局",
    "中文 / EN",
    "深色/浅色",
    "实时采样",
    "自动亮度",
    "诊断日志",
    "左滑切换",
    "上滑切换",
    "淡入淡出",
    "暂停采样",
    "开启采样",
    "切换动效",
    "+ 文本",
    "+ 新行",
    "删除",
    "恢复默认",
    "负载: {0}% | 温度: {1}°C | 节点上限: {2}",
    "动效: 左滑",
    "动效: 上滑",
    "动效: 淡入",
];

pub const LOCALES: [LocaleTable; 2] = [
    LocaleTable::new("en", EN_ENTRIES),
    LocaleTable::new("zh", ZH_ENTRIES),
];

pub fn verify_locale_entries() -> [&'static [&'static str]; 2] {
    [EN_ENTRIES, ZH_ENTRIES]
}

const DRIVER_OPTIONS: &[&str] = &["ST7789 SPI", "ILI9341 8080", "GC9A01 Round", "RM67162 QSPI"];

const CLK_OPTIONS: &[&str] = &[
    "16 MHz", "33 MHz", "60 MHz", "120 MHz", "200 MHz", "400 MHz",
];

const TABLE_COLS: [TableColumn; 4] = [
    TableColumn::new("Module", Length::Fill(2), TextAlign::Start),
    TableColumn::new("Profile", Length::Fill(1), TextAlign::Center),
    TableColumn::new("Limit", Length::Fill(1), TextAlign::End),
    TableColumn::new("State", Length::Fill(1), TextAlign::Center),
];

const TABLE_ROWS: [&[&str]; 3] = [
    &["RetainedTree", "cap-128", "128 nodes", "OK"],
    &["DrawAccel", "SoftAccel", "RGB565 A4", "Active"],
    &["TouchDriver", "GT911 I2C", "5 pts", "Ready"],
];

const GRID_TILES: [&str; 8] = [
    "M1 Cap", "M2 Grid", "M3 L565", "M4 Forms", "M5 i18n", "M6 Drv", "M7 Figma", "M8 Sim",
];

const SCROLL_CARDS: [(&str, Rgb565); 6] = [
    ("01 L565 RLE", rgb8(36, 98, 178)),
    ("02 Gradient", rgb8(28, 128, 120)),
    ("03 ScrollX", rgb8(116, 76, 176)),
    ("04 DataTable", rgb8(168, 92, 40)),
    ("05 Locale", rgb8(48, 126, 74)),
    ("06 Drivers", rgb8(146, 54, 96)),
];

struct StackBuf<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> StackBuf<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> Write for StackBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let avail = N.saturating_sub(self.len);
        let mut take = s.len().min(avail);
        while take > 0 && !s.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ShowcaseModel {
    pub tab: ShowcaseTab,
    pub locale: LocaleId,
    pub dark: bool,
    pub now: InstantMs,
    pub transition: PageTransition,
    pub trans_kind: TransitionKind,
    pub live_sample: bool,
    pub load: i16,
    pub temp: i16,
    pub chk_bright: bool,
    pub chk_diag: bool,
    pub dropdown: DropdownState,
    pub roller: RollerState,
    pub editor: MultiLineState<160>,
    pub chart_a: [i16; 8],
    pub chart_b: [i16; 8],
    pub selected_row: Option<usize>,
    pub justify: Justify,
    pub grid_selected: usize,
    pub scroll_physics: ScrollPhysics,
    pub sample_phase: u16,
    pub chart_rev: u32,
    pub banner_rev: u32,
    pub rich_rev: u32,
}

impl ShowcaseModel {
    pub fn new() -> Self {
        Self {
            tab: ShowcaseTab::Overview,
            locale: LocaleId(1),
            dark: true,
            now: InstantMs(0),
            transition: PageTransition::IDLE,
            trans_kind: TransitionKind::SlideLeft,
            live_sample: true,
            load: 62,
            temp: 44,
            chk_bright: true,
            chk_diag: false,
            dropdown: DropdownState::new(0),
            roller: RollerState::new(2, true),
            editor: MultiLineState::from_text("liuli v1 SDK\nM4 MultiLine"),
            chart_a: [24, 38, 52, 46, 70, 64, 82, 62],
            chart_b: [40, 44, 48, 56, 52, 60, 58, 44],
            selected_row: Some(0),
            justify: Justify::SpaceBetween,
            grid_selected: 0,
            scroll_physics: ScrollPhysics::new(0, 460).with_bounce(44).with_snap(158),
            sample_phase: 0,
            chart_rev: 1,
            banner_rev: 1,
            rich_rev: 1,
        }
    }

    pub const fn page_index(&self) -> u8 {
        self.tab.index()
    }

    fn tr(&self, key: TextKey) -> &'static str {
        LocaleTable::resolve(&LOCALES, self.locale, key)
    }

    fn theme(&self) -> Theme {
        if self.dark { Theme::DARK } else { Theme::LIGHT }
    }

    fn bump_telemetry_rev(&mut self) {
        self.chart_rev = self.chart_rev.wrapping_add(1).max(1);
        self.banner_rev = self.banner_rev.wrapping_add(1).max(1);
    }

    fn push_sample(&mut self) {
        self.sample_phase = self.sample_phase.wrapping_add(1);
        let wave_a = ((self.sample_phase.wrapping_mul(17) % 72) + 18) as i16;
        let wave_b = ((self.sample_phase.wrapping_mul(11).wrapping_add(29) % 58) + 24) as i16;
        self.load = wave_a;
        self.temp = wave_b;
        for i in 0..7 {
            self.chart_a[i] = self.chart_a[i + 1];
            self.chart_b[i] = self.chart_b[i + 1];
        }
        self.chart_a[7] = wave_a;
        self.chart_b[7] = wave_b;
        self.bump_telemetry_rev();
    }

    fn load_gauge(&self) -> GaugeSpec {
        GaugeSpec {
            min: 0,
            max: 100,
            value: self.load,
            ticks: 9,
            diameter: 140,
            label: TextRef::Static("LOAD %"),
        }
    }

    fn temp_gauge(&self) -> GaugeSpec {
        GaugeSpec {
            min: 0,
            max: 120,
            value: self.temp,
            ticks: 7,
            diameter: 140,
            label: TextRef::Static("TEMP C"),
        }
    }

    fn switch_tab(&mut self, tab: ShowcaseTab, now: InstantMs) {
        if self.tab != tab {
            self.tab = tab;
            self.dropdown.open = false;
            self.transition.start(self.trans_kind, now, 260);
        }
    }
}

const ANIM_PERIOD_MS: u64 = 32;
const SAMPLE_PERIOD_MS: u64 = 320;

impl Default for ShowcaseModel {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ShowcaseMsg {
    SelectTab(ShowcaseTab),
    ToggleLocale,
    ToggleTheme,
    LoadDelta(i16),
    ToggleLive,
    CycleTransition,
    ToggleBright,
    ToggleDiag,
    SetTransKind(TransitionKind),
    DropdownAction(Key),
    RollerStep(i32),
    EditorAppendText,
    EditorNewLine,
    EditorBackspace,
    EditorReset,
    EditorText(bool),
    PushSample,
    SelectRow(usize),
    SetJustify(Justify),
    SelectGrid(usize),
    FlingScroll(i32),
}

pub struct ShowcaseApp;

impl App for ShowcaseApp {
    type Model = ShowcaseModel;
    type Msg = ShowcaseMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, now: InstantMs) -> Change {
        model.now = now;
        match msg {
            ShowcaseMsg::SelectTab(tab) => {
                model.switch_tab(tab, now);
                Change::View
            }
            ShowcaseMsg::ToggleLocale => {
                model.locale = if model.locale.0 == 0 {
                    LocaleId(1)
                } else {
                    LocaleId(0)
                };
                model.banner_rev = model.banner_rev.wrapping_add(1).max(1);
                model.rich_rev = model.rich_rev.wrapping_add(1).max(1);
                Change::View
            }
            ShowcaseMsg::ToggleTheme => {
                model.dark = !model.dark;
                model.banner_rev = model.banner_rev.wrapping_add(1).max(1);
                model.chart_rev = model.chart_rev.wrapping_add(1).max(1);
                model.rich_rev = model.rich_rev.wrapping_add(1).max(1);
                Change::View
            }
            ShowcaseMsg::LoadDelta(delta) => {
                model.load = (model.load + delta).clamp(0, 100);
                model.banner_rev = model.banner_rev.wrapping_add(1).max(1);
                Change::View
            }
            ShowcaseMsg::ToggleLive => {
                model.live_sample = !model.live_sample;
                model.banner_rev = model.banner_rev.wrapping_add(1).max(1);
                Change::View
            }
            ShowcaseMsg::CycleTransition => {
                model.trans_kind = match model.trans_kind {
                    TransitionKind::SlideLeft => TransitionKind::SlideUp,
                    TransitionKind::SlideUp => TransitionKind::Fade,
                    _ => TransitionKind::SlideLeft,
                };
                model.transition.start(model.trans_kind, now, 260);
                Change::View
            }
            ShowcaseMsg::ToggleBright => {
                model.chk_bright = !model.chk_bright;
                Change::View
            }
            ShowcaseMsg::ToggleDiag => {
                model.chk_diag = !model.chk_diag;
                Change::View
            }
            ShowcaseMsg::SetTransKind(kind) => {
                model.trans_kind = kind;
                Change::View
            }
            ShowcaseMsg::DropdownAction(key) => {
                if model
                    .dropdown
                    .handle_action(key, Key(221), 223, DRIVER_OPTIONS.len())
                {
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            ShowcaseMsg::RollerStep(delta) => {
                if model.roller.step(delta, CLK_OPTIONS.len()) {
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            ShowcaseMsg::EditorAppendText => {
                let _ = model.editor.insert_str(" OK");
                Change::View
            }
            ShowcaseMsg::EditorNewLine => {
                let _ = model.editor.insert_newline();
                let _ = model.editor.insert_str("STM32H7");
                Change::View
            }
            ShowcaseMsg::EditorBackspace => {
                model.editor.backspace();
                Change::View
            }
            ShowcaseMsg::EditorReset => {
                model.editor = MultiLineState::from_text("liuli v1 SDK\nM4 MultiLine");
                Change::View
            }
            ShowcaseMsg::EditorText(changed) => {
                if changed {
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            ShowcaseMsg::PushSample => {
                model.push_sample();
                Change::View
            }
            ShowcaseMsg::SelectRow(row) => {
                model.selected_row = Some(row);
                Change::View
            }
            ShowcaseMsg::SetJustify(j) => {
                model.justify = j;
                Change::View
            }
            ShowcaseMsg::SelectGrid(idx) => {
                model.grid_selected = idx;
                Change::View
            }
            ShowcaseMsg::FlingScroll(vel) => {
                model.scroll_physics.release_with_velocity(vel, now);
                Change::View
            }
        }
    }

    fn accepts_text(model: &Self::Model, key: Key) -> bool {
        model.tab == ShowcaseTab::Forms && key == Key(240)
    }

    fn on_text(_model: &Self::Model, key: Key, input: TextInput<'_>) -> Option<Self::Msg> {
        if key != Key(240) {
            return None;
        }
        match input {
            TextInput::Commit(s) if !s.is_empty() => Some(ShowcaseMsg::EditorText(true)),
            TextInput::Backspace | TextInput::Left | TextInput::Right => {
                Some(ShowcaseMsg::EditorBackspace)
            }
            _ => None,
        }
    }

    fn on_action(model: &Self::Model, ev: UiAction) -> Option<Self::Msg> {
        let k = ev.key.0;
        match k {
            11 => Some(ShowcaseMsg::SelectTab(ShowcaseTab::Overview)),
            12 => Some(ShowcaseMsg::SelectTab(ShowcaseTab::Forms)),
            13 => Some(ShowcaseMsg::SelectTab(ShowcaseTab::Charts)),
            14 => Some(ShowcaseMsg::SelectTab(ShowcaseTab::Layout)),
            16 => Some(ShowcaseMsg::ToggleLocale),
            17 => Some(ShowcaseMsg::ToggleTheme),
            111 => Some(ShowcaseMsg::LoadDelta(-8)),
            112 => Some(ShowcaseMsg::LoadDelta(8)),
            121 => Some(ShowcaseMsg::ToggleLive),
            132 => Some(ShowcaseMsg::CycleTransition),
            201 => Some(ShowcaseMsg::ToggleLive),
            202 => Some(ShowcaseMsg::ToggleBright),
            203 => Some(ShowcaseMsg::ToggleDiag),
            211 => Some(ShowcaseMsg::SetTransKind(TransitionKind::SlideLeft)),
            212 => Some(ShowcaseMsg::SetTransKind(TransitionKind::SlideUp)),
            213 => Some(ShowcaseMsg::SetTransKind(TransitionKind::Fade)),
            221 | 223..=226 => Some(ShowcaseMsg::DropdownAction(ev.key)),
            230 => match ev.kind {
                ActionKind::Adjust(s) => Some(ShowcaseMsg::RollerStep(i32::from(s))),
                ActionKind::Activate => Some(ShowcaseMsg::RollerStep(1)),
                _ => None,
            },
            231 => Some(ShowcaseMsg::RollerStep(-1)),
            232 => Some(ShowcaseMsg::RollerStep(1)),
            241 => Some(ShowcaseMsg::EditorAppendText),
            242 => Some(ShowcaseMsg::EditorNewLine),
            245 => Some(ShowcaseMsg::EditorBackspace),
            246 => Some(ShowcaseMsg::EditorReset),
            310 => Some(ShowcaseMsg::PushSample),
            311 => Some(ShowcaseMsg::ToggleLive),
            401 => Some(ShowcaseMsg::SetJustify(Justify::Start)),
            402 => Some(ShowcaseMsg::SetJustify(Justify::Center)),
            403 => Some(ShowcaseMsg::SetJustify(Justify::End)),
            404 => Some(ShowcaseMsg::SetJustify(Justify::SpaceBetween)),
            405 => Some(ShowcaseMsg::SetJustify(Justify::SpaceAround)),
            406 => Some(ShowcaseMsg::SetJustify(Justify::SpaceEvenly)),
            421..=428 => Some(ShowcaseMsg::SelectGrid((k - 421) as usize)),
            448 => Some(ShowcaseMsg::FlingScroll(-1100)),
            449 => Some(ShowcaseMsg::FlingScroll(1100)),
            _ if model.tab == ShowcaseTab::Charts && k >= 420 && (k - 420).is_multiple_of(16) => {
                let row = ((k - 420) / 16) as usize;
                (row < TABLE_ROWS.len()).then_some(ShowcaseMsg::SelectRow(row))
            }
            _ => None,
        }
    }

    fn next_deadline(model: &Self::Model, now: InstantMs) -> Option<InstantMs> {
        let mut earliest: Option<InstantMs> = None;
        if let NextWakeup::At(t) = model.transition.next_wakeup(now) {
            earliest = Some(t);
        }
        match model.scroll_physics.next_wakeup(now) {
            NextWakeup::Now => return Some(now),
            NextWakeup::At(t) => {
                earliest = Some(earliest.map_or(t, |e| if t < e { t } else { e }));
            }
            NextWakeup::OnEvent => {}
        }
        if model.live_sample {
            let step = InstantMs(((now.0 / ANIM_PERIOD_MS) + 1).saturating_mul(ANIM_PERIOD_MS));
            earliest = Some(earliest.map_or(step, |e| if step < e { step } else { e }));
        }
        earliest
    }

    fn on_timer(model: &mut Self::Model, now: InstantMs) -> Change {
        let prev_now = model.now;
        let mut dirty = model.scroll_physics.advance(now);
        if model.transition.is_active(now) || model.transition.is_active(prev_now) {
            dirty = true;
        }
        if model.live_sample {
            let sample_due = (now.0 / SAMPLE_PERIOD_MS) != (prev_now.0 / SAMPLE_PERIOD_MS);
            let anim_due = (now.0 / ANIM_PERIOD_MS) != (prev_now.0 / ANIM_PERIOD_MS);
            if sample_due {
                model.push_sample();
                dirty = true;
            } else if anim_due {
                dirty = true;
            }
        }
        if dirty {
            model.now = now;
            Change::View
        } else {
            Change::Unchanged
        }
    }

    fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let theme = model.theme();
        let root_props = NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Fill(1))
            .padding(EdgeInsets::all(10))
            .bg(theme.background);

        out.root(Key(1), root_props, |ui| {
            ui.column(
                Key(2),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .gap(8),
                |col| {
                    build_topbar(model, theme, col)?;
                    build_content(model, theme, col)?;
                    build_footer(model, theme, col)?;
                    Ok(())
                },
            )
        })
    }

    fn paint_model(
        model: &Self::Model,
        key: Key,
        props: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        match key.0 {
            15 => paint_spinner(
                if model.tab == ShowcaseTab::Overview {
                    InstantMs(0)
                } else {
                    model.now
                },
                props,
                canvas,
            ),
            131 => paint_spinner(model.now, props, canvas),
            100 => paint_gradient_banner(model, props, canvas),
            110 => paint_gauge(model.load_gauge(), props, canvas),
            120 => paint_gauge(model.temp_gauge(), props, canvas),
            130 => paint_gradient_swatch(canvas),
            140 => paint_summary_rich_text(model, props, canvas),
            201..=203 => paint_checkbox(props, canvas.state, canvas),
            211..=213 => paint_radio(props, canvas.state, canvas),
            230 => paint_roller(CLK_OPTIONS, model.roller, props, canvas.state, canvas),
            240 => paint_multi_line_editor(&model.editor, props, canvas.state, canvas),
            301 => {
                let s0 = ChartSeries {
                    name: "Load",
                    color: rgb8(64, 172, 255),
                    values: &model.chart_a[..5],
                };
                let s1 = ChartSeries {
                    name: "Temp",
                    color: rgb8(255, 168, 54),
                    values: &model.chart_b[..5],
                };
                paint_bar_chart(&[s0, s1], 0, 100, props, canvas)
            }
            302 => {
                let s0 = ChartSeries {
                    name: "Load",
                    color: rgb8(64, 214, 164),
                    values: &model.chart_a,
                };
                let s1 = ChartSeries {
                    name: "Temp",
                    color: rgb8(255, 108, 136),
                    values: &model.chart_b,
                };
                paint_multi_line_chart(&[s0, s1], 0, 100, props, canvas)
            }
            _ => Ok(()),
        }
    }
}

fn tab_btn(
    ui: &mut Children<'_, '_>,
    key: Key,
    label: &'static str,
    active: bool,
    theme: Theme,
) -> Result<(), BuildError> {
    let bg = if active {
        theme.accent
    } else {
        theme.surface_alt
    };
    let fg = if active { theme.background } else { theme.text };
    ui.button(
        key,
        NodeProps::new()
            .width(Length::Px(88))
            .height(Length::Px(34))
            .bg(bg)
            .fg(fg)
            .border(theme.border, 1)
            .radius(6)
            .font(FontId::Body16)
            .text(TextRef::Static(label))
            .focusable(true)
            .clickable(true),
    )
}

fn small_btn(
    ui: &mut Children<'_, '_>,
    key: Key,
    label: &'static str,
    width: Length,
    bg: Rgb565,
    fg: Rgb565,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.button(
        key,
        NodeProps::new()
            .width(width)
            .height(Length::Px(32))
            .bg(bg)
            .fg(fg)
            .border(theme.border, 1)
            .radius(6)
            .font(FontId::Body16)
            .text(TextRef::Static(label))
            .focusable(true)
            .clickable(true),
    )
}

fn build_topbar(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.row(
        Key(10),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(46))
            .padding(EdgeInsets::symmetric(8, 5))
            .gap(5)
            .justify(Justify::SpaceBetween)
            .align_cross(Align::Center)
            .bg(theme.surface)
            .border(theme.border, 1)
            .radius(8),
        |bar| {
            let bar_spinner_now = if model.tab == ShowcaseTab::Overview {
                InstantMs(0)
            } else {
                model.now
            };
            spinner(bar, Key(15), 26, bar_spinner_now, theme)?;
            bar.label(
                Key(18),
                NodeProps::new()
                    .width(Length::Px(144))
                    .height(Length::Content)
                    .fg(theme.text)
                    .font(FontId::Body16)
                    .text(TextRef::Static(model.tr(i18n_keys::TITLE))),
            )?;
            tab_btn(
                bar,
                Key(11),
                model.tr(i18n_keys::TAB_OVERVIEW),
                model.tab == ShowcaseTab::Overview,
                theme,
            )?;
            tab_btn(
                bar,
                Key(12),
                model.tr(i18n_keys::TAB_FORMS),
                model.tab == ShowcaseTab::Forms,
                theme,
            )?;
            tab_btn(
                bar,
                Key(13),
                model.tr(i18n_keys::TAB_CHARTS),
                model.tab == ShowcaseTab::Charts,
                theme,
            )?;
            tab_btn(
                bar,
                Key(14),
                model.tr(i18n_keys::TAB_LAYOUT),
                model.tab == ShowcaseTab::Layout,
                theme,
            )?;
            small_btn(
                bar,
                Key(16),
                model.tr(i18n_keys::BTN_LANG),
                Length::Px(90),
                theme.surface_alt,
                theme.accent,
                theme,
            )?;
            small_btn(
                bar,
                Key(17),
                model.tr(i18n_keys::BTN_THEME),
                Length::Px(90),
                theme.surface_alt,
                theme.text,
                theme,
            )?;
            Ok(())
        },
    )
}

fn build_content(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.stack(
        Key(49),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Fill(1)),
        |vp| {
            vp.column(
                Key(50),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .gap(8),
                |body| match model.tab {
                    ShowcaseTab::Overview => build_tab_overview(model, theme, body),
                    ShowcaseTab::Forms => build_tab_forms(model, theme, body),
                    ShowcaseTab::Charts => build_tab_charts(model, theme, body),
                    ShowcaseTab::Layout => build_tab_layout(model, theme, body),
                },
            )?;

            model
                .transition
                .apply(vp, Key(50), Size::new(260, 140), model.now)?;

            #[cfg(feature = "enhanced")]
            if model.transition.kind() == TransitionKind::Fade
                && model.transition.is_active(model.now)
            {
                let alpha = model.transition.alpha_for(model.now);
                vp.decorate(
                    Key(50),
                    SurfaceStyle {
                        layer: Some(Layer::OPAQUE.with_opacity(alpha)),
                        ..SurfaceStyle::FLAT
                    },
                )?;
            }

            Ok(())
        },
    )
}

fn build_tab_overview(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.custom(
        Key(100),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(56))
            .fg(theme.text)
            .accent(theme.accent)
            .font(FontId::Body16),
    )?;
    ui.set_custom_rev(Key(100), model.banner_rev)?;

    ui.row(
        Key(105),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(220))
            .gap(12)
            .justify(Justify::SpaceEvenly),
        |cards| {
            cards.column(
                Key(106),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .padding(EdgeInsets::all(8))
                    .gap(6)
                    .align_cross(Align::Center)
                    .bg(theme.surface)
                    .border(theme.border, 1)
                    .radius(8),
                |c| {
                    gauge(c, Key(110), model.load_gauge(), theme)?;
                    c.row(
                        Key(113),
                        NodeProps::new()
                            .width(Length::Fill(1))
                            .height(Length::Content)
                            .justify(Justify::SpaceEvenly),
                        |r| {
                            small_btn(
                                r,
                                Key(111),
                                "-8%",
                                Length::Px(80),
                                theme.surface_alt,
                                theme.text,
                                theme,
                            )?;
                            small_btn(
                                r,
                                Key(112),
                                "+8%",
                                Length::Px(80),
                                theme.accent,
                                theme.background,
                                theme,
                            )
                        },
                    )
                },
            )?;

            cards.column(
                Key(107),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .padding(EdgeInsets::all(8))
                    .gap(6)
                    .align_cross(Align::Center)
                    .bg(theme.surface)
                    .border(theme.border, 1)
                    .radius(8),
                |c| {
                    gauge(c, Key(120), model.temp_gauge(), theme)?;
                    let lbl = if model.live_sample {
                        model.tr(i18n_keys::BTN_PAUSE_LIVE)
                    } else {
                        model.tr(i18n_keys::BTN_RESUME_LIVE)
                    };
                    small_btn(
                        c,
                        Key(121),
                        lbl,
                        Length::Px(132),
                        theme.surface_alt,
                        theme.accent,
                        theme,
                    )
                },
            )?;

            cards.column(
                Key(108),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .padding(EdgeInsets::all(8))
                    .gap(8)
                    .align_cross(Align::Center)
                    .bg(theme.surface)
                    .border(theme.border, 1)
                    .radius(8),
                |c| {
                    c.custom(
                        Key(130),
                        NodeProps::new()
                            .width(Length::Px(196))
                            .height(Length::Px(74)),
                    )?;
                    spinner(c, Key(131), 54, model.now, theme)?;
                    let trans_lbl = match model.trans_kind {
                        TransitionKind::SlideUp => model.tr(i18n_keys::BTN_TRANS_SLIDE_U),
                        TransitionKind::Fade => model.tr(i18n_keys::BTN_TRANS_FADE),
                        _ => model.tr(i18n_keys::BTN_TRANS_SLIDE_L),
                    };
                    small_btn(
                        c,
                        Key(132),
                        trans_lbl,
                        Length::Px(156),
                        theme.surface_alt,
                        theme.text,
                        theme,
                    )
                },
            )
        },
    )?;

    ui.column(
        Key(139),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Fill(1))
            .padding(EdgeInsets::symmetric(12, 8))
            .bg(theme.surface)
            .border(theme.border, 1)
            .radius(8),
        |box_ui| {
            rich_text(box_ui, Key(140), Length::Fill(1), 54, theme)?;
            box_ui.set_custom_rev(Key(140), model.rich_rev)
        },
    )
}

fn build_tab_forms(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    let chk_theme = Theme {
        control_height: 28,
        ..theme
    };
    ui.row(
        Key(200),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Fill(1))
            .gap(12),
        |cols| {
            cols.column(
                Key(205),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .padding(EdgeInsets::all(8))
                    .gap(4)
                    .bg(theme.surface)
                    .border(theme.border, 1)
                    .radius(8),
                |left| {
                    checkbox(
                        left,
                        Key(201),
                        TextRef::Static(model.tr(i18n_keys::CHK_LIVE)),
                        model.live_sample,
                        chk_theme,
                    )?;
                    checkbox(
                        left,
                        Key(202),
                        TextRef::Static(model.tr(i18n_keys::CHK_BRIGHT)),
                        model.chk_bright,
                        chk_theme,
                    )?;
                    checkbox(
                        left,
                        Key(203),
                        TextRef::Static(model.tr(i18n_keys::CHK_DIAG)),
                        model.chk_diag,
                        chk_theme,
                    )?;
                    radio(
                        left,
                        Key(211),
                        TextRef::Static(model.tr(i18n_keys::RAD_SLIDE_L)),
                        model.trans_kind == TransitionKind::SlideLeft,
                        chk_theme,
                    )?;
                    radio(
                        left,
                        Key(212),
                        TextRef::Static(model.tr(i18n_keys::RAD_SLIDE_U)),
                        model.trans_kind == TransitionKind::SlideUp,
                        chk_theme,
                    )?;
                    radio(
                        left,
                        Key(213),
                        TextRef::Static(model.tr(i18n_keys::RAD_FADE)),
                        model.trans_kind == TransitionKind::Fade,
                        chk_theme,
                    )?;
                    dropdown(
                        left,
                        Key(220),
                        Key(221),
                        Key(222),
                        223,
                        DRIVER_OPTIONS,
                        model.dropdown,
                        chk_theme,
                    )
                },
            )?;

            cols.column(
                Key(229),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .padding(EdgeInsets::all(10))
                    .gap(8)
                    .bg(theme.surface)
                    .border(theme.border, 1)
                    .radius(8),
                |right| {
                    right.row(
                        Key(233),
                        NodeProps::new()
                            .width(Length::Fill(1))
                            .height(Length::Px(112))
                            .gap(10)
                            .align_cross(Align::Center),
                        |r| {
                            roller(
                                r,
                                Key(230),
                                Length::Fill(1),
                                108,
                                CLK_OPTIONS,
                                model.roller,
                                theme,
                            )?;
                            r.column(
                                Key(234),
                                NodeProps::new()
                                    .width(Length::Px(96))
                                    .height(Length::Content)
                                    .gap(8),
                                |btns| {
                                    small_btn(
                                        btns,
                                        Key(231),
                                        "Prev",
                                        Length::Fill(1),
                                        theme.surface_alt,
                                        theme.text,
                                        theme,
                                    )?;
                                    small_btn(
                                        btns,
                                        Key(232),
                                        "Next",
                                        Length::Fill(1),
                                        theme.accent,
                                        theme.background,
                                        theme,
                                    )
                                },
                            )
                        },
                    )?;
                    multi_line_editor(right, Key(240), Length::Fill(1), 124, &model.editor, theme)?;
                    right.row(
                        Key(247),
                        NodeProps::new()
                            .width(Length::Fill(1))
                            .height(Length::Content)
                            .gap(8)
                            .justify(Justify::SpaceBetween),
                        |tb| {
                            small_btn(
                                tb,
                                Key(241),
                                model.tr(i18n_keys::BTN_ADD_TEXT),
                                Length::Fill(1),
                                theme.accent,
                                theme.background,
                                theme,
                            )?;
                            small_btn(
                                tb,
                                Key(242),
                                model.tr(i18n_keys::BTN_ADD_LINE),
                                Length::Fill(1),
                                theme.surface_alt,
                                theme.text,
                                theme,
                            )?;
                            small_btn(
                                tb,
                                Key(245),
                                model.tr(i18n_keys::BTN_DEL),
                                Length::Fill(1),
                                theme.surface_alt,
                                theme.text,
                                theme,
                            )?;
                            small_btn(
                                tb,
                                Key(246),
                                model.tr(i18n_keys::BTN_RESET),
                                Length::Fill(1),
                                theme.surface_alt,
                                theme.accent,
                                theme,
                            )
                        },
                    )
                },
            )
        },
    )
}

fn build_tab_charts(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.row(
        Key(300),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(156))
            .gap(12),
        |r| {
            r.custom(
                Key(301),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .bg(theme.surface)
                    .border(theme.border, 1),
            )?;
            r.set_custom_rev(Key(301), model.chart_rev)?;
            r.custom(
                Key(302),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .bg(theme.surface)
                    .border(theme.border, 1),
            )?;
            r.set_custom_rev(Key(302), model.chart_rev)
        },
    )?;

    ui.row(
        Key(309),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(36))
            .gap(10)
            .justify(Justify::SpaceBetween)
            .align_cross(Align::Center),
        |bar| {
            small_btn(
                bar,
                Key(310),
                "Sample +1",
                Length::Px(130),
                theme.accent,
                theme.background,
                theme,
            )?;
            let live_lbl = if model.live_sample {
                model.tr(i18n_keys::BTN_PAUSE_LIVE)
            } else {
                model.tr(i18n_keys::BTN_RESUME_LIVE)
            };
            small_btn(
                bar,
                Key(311),
                live_lbl,
                Length::Px(130),
                theme.surface_alt,
                theme.text,
                theme,
            )?;
            bar.label(
                Key(312),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Content)
                    .fg(theme.text)
                    .font(FontId::Body16)
                    .text(TextRef::Static("BarChart + MultiLineChart + DataTable")),
            )
        },
    )?;

    data_table(
        ui,
        320,
        &TABLE_COLS,
        &TABLE_ROWS,
        model.selected_row,
        28,
        theme,
    )
}

fn build_tab_layout(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    let modes = [
        (401, Justify::Start, "Start"),
        (402, Justify::Center, "Center"),
        (403, Justify::End, "End"),
        (404, Justify::SpaceBetween, "Between"),
        (405, Justify::SpaceAround, "Around"),
        (406, Justify::SpaceEvenly, "Evenly"),
    ];

    ui.row(
        Key(400),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Content)
            .gap(6)
            .justify(Justify::SpaceBetween),
        |r| {
            for (k, mode, label) in modes {
                let sel = model.justify == mode;
                small_btn(
                    r,
                    Key(k),
                    label,
                    Length::Fill(1),
                    if sel { theme.accent } else { theme.surface_alt },
                    if sel { theme.background } else { theme.text },
                    theme,
                )?;
            }
            Ok(())
        },
    )?;

    ui.row(
        Key(410),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(48))
            .padding(EdgeInsets::symmetric(10, 6))
            .gap(4)
            .justify(model.justify)
            .align_cross(Align::Center)
            .bg(theme.surface)
            .border(theme.border, 1)
            .radius(8),
        |r| {
            let colors = [
                rgb8(42, 138, 224),
                rgb8(38, 162, 120),
                rgb8(216, 132, 40),
                rgb8(168, 72, 184),
            ];
            for (i, c) in colors.into_iter().enumerate() {
                r.label(
                    Key(411 + i as u32),
                    NodeProps::new()
                        .width(Length::Px(94))
                        .height(Length::Px(32))
                        .bg(c)
                        .fg(rgb8(250, 252, 255))
                        .radius(6)
                        .font(FontId::Body16)
                        .text(TextRef::Static(["P1", "P2", "P3", "P4"][i]))
                        .text_align(TextAlign::Center),
                )?;
            }
            Ok(())
        },
    )?;

    ui.grid(
        Key(420),
        4,
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Content)
            .gap(8),
        |g| {
            for (i, &name) in GRID_TILES.iter().enumerate() {
                let sel = model.grid_selected == i;
                g.button(
                    Key(421 + i as u32),
                    NodeProps::new()
                        .height(Length::Px(46))
                        .bg(if sel { theme.accent } else { theme.surface })
                        .fg(if sel { theme.background } else { theme.text })
                        .border(theme.border, 1)
                        .radius(8)
                        .font(FontId::Body16)
                        .text(TextRef::Static(name))
                        .focusable(true)
                        .clickable(true),
                )?;
            }
            Ok(())
        },
    )?;

    ui.row(
        Key(447),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(74))
            .gap(8)
            .align_cross(Align::Center),
        |strip| {
            small_btn(
                strip,
                Key(448),
                "<<",
                Length::Px(52),
                theme.surface_alt,
                theme.text,
                theme,
            )?;
            strip.scroll_x(
                Key(450),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .padding(EdgeInsets::all(4))
                    .bg(theme.surface)
                    .border(theme.border, 1)
                    .radius(8),
                |sx| {
                    sx.row(
                        Key(451),
                        NodeProps::new()
                            .width(Length::Content)
                            .height(Length::Fill(1))
                            .gap(8),
                        |cards| {
                            for (i, &(label, col)) in SCROLL_CARDS.iter().enumerate() {
                                cards.label(
                                    Key(452 + i as u32),
                                    NodeProps::new()
                                        .width(Length::Px(150))
                                        .height(Length::Fill(1))
                                        .bg(col)
                                        .fg(rgb8(248, 250, 252))
                                        .radius(6)
                                        .font(FontId::Body16)
                                        .text(TextRef::Static(label))
                                        .text_align(TextAlign::Center),
                                )?;
                            }
                            Ok(())
                        },
                    )?;
                    let dx = -(model.scroll_physics.offset().clamp(-500, 900) as i16);
                    sx.set_offset(Key(451), dx, 0)
                },
            )?;
            small_btn(
                strip,
                Key(449),
                ">>",
                Length::Px(52),
                theme.accent,
                theme.background,
                theme,
            )
        },
    )
}

fn build_footer(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.row(
        Key(20),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(32))
            .padding(EdgeInsets::symmetric(10, 4))
            .justify(Justify::SpaceBetween)
            .align_cross(Align::Center)
            .bg(theme.surface)
            .border(theme.border, 1)
            .radius(6),
        |ft| {
            let mut status = StackBuf::<80>::new();
            let _ = format_template(
                &mut status,
                model.tr(i18n_keys::STATUS_FMT),
                &[&model.load, &model.temp, &MAX_NODES],
            );
            let status_ref = ft.text_fmt(format_args!("{}", status.as_str()))?;
            ft.label(
                Key(21),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Content)
                    .fg(theme.text)
                    .font(FontId::Body16)
                    .text(status_ref),
            )?;
            ft.label(
                Key(22),
                NodeProps::new()
                    .width(Length::Px(190))
                    .height(Length::Content)
                    .fg(theme.accent)
                    .font(FontId::Small12)
                    .text(TextRef::Static("FK743M2 800x480 | GT911")),
            )
        },
    )
}

fn paint_gradient_banner(
    model: &ShowcaseModel,
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let (c0, c1) = if model.dark {
        (rgb8(24, 72, 148), rgb8(18, 128, 118))
    } else {
        (rgb8(54, 124, 224), rgb8(32, 168, 148))
    };
    canvas.gradient_rect(
        Rect::new(0, 0, sz.width, sz.height),
        c0,
        c1,
        GRADIENT_HORIZONTAL,
        10,
    )?;

    let mut line = StackBuf::<96>::new();
    let _ = format_template(
        &mut line,
        model.tr(i18n_keys::STATUS_FMT),
        &[&model.load, &model.temp, &MAX_NODES],
    );
    let _ = props;
    canvas.text(
        Rect::new(14, 16, sz.width.saturating_sub(28), 24),
        line.as_str(),
        rgb8(248, 252, 255),
        FontId::Body16,
        TextAlign::Start,
    )
}

fn paint_gradient_swatch(canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let sz = canvas.size();
    let half = (sz.width.saturating_sub(8)) / 2;
    if half == 0 {
        return Ok(());
    }
    canvas.gradient_rect(
        Rect::new(0, 0, half, sz.height),
        rgb8(220, 72, 116),
        rgb8(64, 120, 240),
        GRADIENT_DIAGONAL,
        8,
    )?;
    canvas.gradient_rect(
        Rect::new((half + 8) as i32, 0, half, sz.height),
        rgb8(42, 184, 128),
        rgb8(24, 72, 128),
        GRADIENT_VERTICAL,
        8,
    )?;
    canvas.text(
        Rect::new(4, 24, half.saturating_sub(8), 20),
        "DIAG",
        rgb8(255, 255, 255),
        FontId::Small12,
        TextAlign::Center,
    )?;
    canvas.text(
        Rect::new((half + 12) as i32, 24, half.saturating_sub(8), 20),
        "VERT",
        rgb8(255, 255, 255),
        FontId::Small12,
        TextAlign::Center,
    )
}

fn paint_summary_rich_text(
    model: &ShowcaseModel,
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let cap_label = if MAX_NODES >= 250 {
        "capacity-256 "
    } else if MAX_NODES >= 128 {
        "capacity-128 "
    } else {
        "default-64 "
    };
    let spans = [
        TextSpan::new("SDK Showcase: ").with_color(props.accent),
        TextSpan::new(cap_label).with_underline(true),
        TextSpan::new("| Justify/Grid/ScrollX | GradientRect | LocaleTable("),
        TextSpan::new(if model.locale.0 == 0 { "EN" } else { "ZH" }).with_color(rgb8(64, 214, 152)),
        TextSpan::new(") | Drivers(ST7789/GT911)"),
    ];
    paint_rich_text(&spans, props, canvas)
}
