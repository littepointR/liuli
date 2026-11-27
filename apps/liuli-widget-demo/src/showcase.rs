use core::fmt::{self, Write};

pub mod samples;
pub use samples::{SamplePage, Samples};

use liuli::prelude::*;
use liuli_widgets::tokens::rgb8;
use liuli_widgets::{
    ChartSeries, DropdownState, GaugeSpec, MultiLineState, PageTransition, RollerState, TabLayout,
    TabPage, TableColumn, Tabs, TextSpan, Theme, TransitionKind, checkbox, data_table, dropdown,
    gauge, multi_line_editor, paint_bar_chart, paint_checkbox, paint_gauge, paint_multi_line_chart,
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
    Motion = 4,
    Widgets = 5,
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
            4 => Self::Motion,
            5 => Self::Widgets,
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
    pub const TAB_MOTION: TextKey = TextKey(24);
    pub const TAB_WIDGETS: TextKey = TextKey(25);
}

pub const EN_ENTRIES: &[&str] = &[
    "Widget Demo",
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
    "Motion",
    "Widgets",
];

pub const ZH_ENTRIES: &[&str] = &[
    "琉璃 UI 演示",
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
    "动效",
    "控件",
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
    "M1 Capacity",
    "M2 Grid",
    "M3 L565",
    "M4 Forms",
    "M5 Localization",
    "M6 Drivers",
    "M7 Figma",
    "M8 Simulator",
];

const SCROLL_CARDS: [(&str, Rgb565); 6] = [
    ("01 L565 RLE", rgb8(36, 98, 178)),
    ("02 Gradient", rgb8(28, 128, 120)),
    ("03 ScrollX", rgb8(116, 76, 176)),
    ("04 DataTable", rgb8(168, 92, 40)),
    ("05 Locale", rgb8(48, 126, 74)),
    ("06 Drivers", rgb8(146, 54, 96)),
];

const CARD_WIDTH: u16 = 150;
const CARD_GAP: u16 = 8;

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
    pub compact: bool,
    pub samples: Samples,
    pub motion: liuli_widgets::Motion,
    pub local_slow: bool,
    pub indicator_on: bool,
    pub level: i16,
    pub locale: LocaleId,
    pub dark: bool,
    pub now: InstantMs,
    pub transition: PageTransition,
    pub trans_kind: TransitionKind,
    pub preview: PageTransition,
    pub preset_index: usize,
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
            compact: false,
            samples: Samples::new(),
            motion: liuli_widgets::Motion::STANDARD,
            local_slow: true,
            indicator_on: false,
            level: 35,
            locale: LocaleId(1),
            dark: true,
            now: InstantMs(0),
            transition: PageTransition::IDLE,
            trans_kind: TransitionKind::SlideLeft,
            preview: PageTransition::IDLE.with_easing(Easing::Linear),
            preset_index: 0,
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
            scroll_physics: ScrollPhysics::ZERO,
            sample_phase: 0,
            chart_rev: 1,
            banner_rev: 1,
            rich_rev: 1,
        }
    }

    pub fn for_size(size: Size) -> Self {
        Self {
            compact: size.width < 600,
            ..Self::new()
        }
    }

    pub const fn page_index(&self) -> u8 {
        self.tab.index()
    }

    fn tr(&self, key: TextKey) -> &'static str {
        LocaleTable::resolve(&LOCALES, self.locale, key)
    }

    fn theme(&self) -> Theme {
        (if self.dark { Theme::DARK } else { Theme::LIGHT }).with_motion(self.motion)
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

    fn play_page(&mut self, now: InstantMs) {
        self.transition.play_styled(
            self.motion,
            liuli_widgets::TransitionRole::Page,
            Some(self.motion.page.with_kind(self.trans_kind)),
            now,
        );
    }

    fn switch_tab(&mut self, tab: ShowcaseTab, now: InstantMs) {
        if self.tab != tab {
            self.samples.leave();
            self.tab = tab;
            if tab == ShowcaseTab::Layout {
                self.preview = PageTransition::IDLE.with_easing(Easing::Linear);
            }
            self.dropdown.open = false;
            self.play_page(now);
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
    SampleAction(UiAction),
    SamplePointer(Key, PointerEvent),
    ImageGesture(TransformGesture),
    ListOffset(i32),
    SetMotion(liuli_widgets::Motion),
    ToggleLocal,
    ToggleIndicator,
    SetLevel(i16),
    ToggleLocale,
    ToggleTheme,
    LoadDelta(i16),
    ToggleLive,
    CycleTransition,
    Preview(i8),
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
    StepScroll(i32),
}

pub struct ShowcaseApp;

impl App for ShowcaseApp {
    type Model = ShowcaseModel;
    type Msg = ShowcaseMsg;

    fn animations(model: &mut Self::Model, bind: &mut dyn FnMut(Key, &mut dyn Animation)) {
        bind(Key(50), &mut model.transition);
        bind(Key(475), &mut model.preview);
    }

    fn scroll_controller(model: &mut Self::Model, key: Key) -> Option<&mut ScrollPhysics> {
        (key == Key(450)).then_some(&mut model.scroll_physics)
    }

    fn update(model: &mut Self::Model, msg: Self::Msg, now: InstantMs) -> Change {
        match msg {
            ShowcaseMsg::SampleAction(action) => {
                let theme = model.theme();
                model.samples.action(action, now, theme)
            }
            ShowcaseMsg::SamplePointer(key, event) => model.samples.pointer(key, event),
            ShowcaseMsg::ImageGesture(gesture) => model.samples.transform(gesture),
            ShowcaseMsg::ListOffset(y) => {
                model.samples.offset = y;
                Change::View
            }
            ShowcaseMsg::SetMotion(motion) => {
                model.motion = motion;
                model.transition.apply_motion(motion, now);
                model.preview.apply_motion(motion, now);
                if motion.reduced {
                    Animation::cancel(&mut model.scroll_physics, now);
                }
                Change::View
            }
            ShowcaseMsg::SetLevel(level) => {
                model.level = level.clamp(0, 100);
                Change::View
            }
            ShowcaseMsg::ToggleLocal => {
                model.local_slow = !model.local_slow;
                Change::View
            }
            ShowcaseMsg::ToggleIndicator => {
                model.indicator_on = !model.indicator_on;
                Change::View
            }
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
                #[cfg(feature = "enhanced")]
                if model.samples.page == SamplePage::Keyboard {
                    model.samples.refresh_script(model.theme());
                }
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
            ShowcaseMsg::Preview(delta) => {
                let count = TransitionKind::ALL.len() as isize;
                model.preset_index =
                    (model.preset_index as isize + isize::from(delta)).rem_euclid(count) as usize;
                model.preview.play_styled(
                    model.motion,
                    liuli_widgets::TransitionRole::Entrance,
                    Some(
                        model
                            .motion
                            .entrance
                            .with_kind(TransitionKind::ALL[model.preset_index]),
                    ),
                    now,
                );
                Change::View
            }
            ShowcaseMsg::CycleTransition => {
                model.trans_kind = match model.trans_kind {
                    TransitionKind::SlideLeft => TransitionKind::SlideUp,
                    TransitionKind::SlideUp => TransitionKind::Fade,
                    _ => TransitionKind::SlideLeft,
                };
                model.play_page(now);
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
            ShowcaseMsg::StepScroll(direction) => {
                model
                    .scroll_physics
                    .scroll_by_stop(direction, now, !model.motion.reduced);
                Change::View
            }
        }
    }

    fn accepts_text(model: &Self::Model, key: Key) -> bool {
        (model.tab == ShowcaseTab::Forms && key == Key(240))
            || (model.tab == ShowcaseTab::Widgets && model.samples.accepts_text(key))
    }

    fn update_text(
        model: &mut Self::Model,
        key: Key,
        input: TextInput<'_>,
        now: InstantMs,
    ) -> Option<Change> {
        if model.tab == ShowcaseTab::Widgets {
            return model.samples.text(key, input);
        }
        if key != Key(240) {
            return None;
        }
        let changed = model.editor.apply_text(input);
        Some(Self::update(model, ShowcaseMsg::EditorText(changed), now))
    }

    fn on_action(model: &Self::Model, ev: UiAction) -> Option<Self::Msg> {
        let k = ev.key.0;
        if let Some(tab) = Tabs::new(&page_declarations(model), model.tab)
            .ok()?
            .selection(ev)
        {
            return Some(ShowcaseMsg::SelectTab(tab));
        }
        if model.tab == ShowcaseTab::Widgets && (k >= 700 || ev.kind == ActionKind::Dismiss) {
            return Some(ShowcaseMsg::SampleAction(ev));
        }
        match k {
            601 => Some(ShowcaseMsg::SetMotion(liuli_widgets::Motion::STANDARD)),
            602 => Some(ShowcaseMsg::SetMotion(liuli_widgets::Motion::SNAPPY)),
            603 => Some(ShowcaseMsg::SetMotion(liuli_widgets::Motion::REDUCED)),
            604 => Some(ShowcaseMsg::ToggleLocal),
            605 | 606 => Some(ShowcaseMsg::ToggleIndicator),
            610 => match ev.kind {
                ActionKind::SetValue(v) => Some(ShowcaseMsg::SetLevel(v)),
                ActionKind::Adjust(d) => Some(ShowcaseMsg::SetLevel(
                    model.level.saturating_add(d.saturating_mul(5)),
                )),
                _ => None,
            },
            612 => Some(ShowcaseMsg::SetLevel(if model.level < 50 {
                85
            } else {
                15
            })),
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
            471 => Some(ShowcaseMsg::Preview(-1)),
            472 => Some(ShowcaseMsg::Preview(0)),
            473 => Some(ShowcaseMsg::Preview(1)),
            448 => Some(ShowcaseMsg::StepScroll(-1)),
            449 => Some(ShowcaseMsg::StepScroll(1)),
            _ if model.tab == ShowcaseTab::Charts && k >= 420 && (k - 420).is_multiple_of(16) => {
                let row = ((k - 420) / 16) as usize;
                (row < TABLE_ROWS.len()).then_some(ShowcaseMsg::SelectRow(row))
            }
            _ => None,
        }
    }

    fn on_scroll(model: &Self::Model, key: Key, offset: i32) -> Option<Self::Msg> {
        (model.tab == ShowcaseTab::Widgets && key == samples::LIST)
            .then_some(ShowcaseMsg::ListOffset(offset))
    }
    fn on_pointer(model: &Self::Model, key: Key, event: PointerEvent) -> Option<Self::Msg> {
        (model.tab == ShowcaseTab::Widgets && samples::captures(key))
            .then_some(ShowcaseMsg::SamplePointer(key, event))
    }
    fn accepts_transform(model: &Self::Model, key: Key) -> bool {
        model.tab == ShowcaseTab::Widgets && key == samples::IMAGE
    }
    fn on_transform(_: &Self::Model, key: Key, gesture: TransformGesture) -> Option<Self::Msg> {
        (key == samples::IMAGE).then_some(ShowcaseMsg::ImageGesture(gesture))
    }
    fn paint_override(
        _: &Self::Model,
        key: Key,
        _: NodeKind,
        props: &StoredProps,
        state: PaintState,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<PaintOverride, BuildError> {
        if key == samples::TICKS {
            liuli_widgets::paint_ticked_slider(props, state, 5, canvas)
        } else {
            Ok(PaintOverride::Default)
        }
    }
    fn next_deadline(model: &Self::Model, now: InstantMs) -> Option<InstantMs> {
        let period = if model.motion.reduced {
            SAMPLE_PERIOD_MS
        } else {
            ANIM_PERIOD_MS
        };
        model
            .live_sample
            .then(|| InstantMs((now.0 / period).saturating_add(1).saturating_mul(period)))
            .into_iter()
            .chain(model.samples.toast_until)
            .min()
    }

    fn on_timer(model: &mut Self::Model, now: InstantMs) -> Change {
        let prev_now = model.now;
        let mut dirty = model.samples.expire(now);
        if model.live_sample {
            let sample_due = (now.0 / SAMPLE_PERIOD_MS) != (prev_now.0 / SAMPLE_PERIOD_MS);
            let anim_due = (now.0 / ANIM_PERIOD_MS) != (prev_now.0 / ANIM_PERIOD_MS);
            if sample_due {
                model.push_sample();
                dirty = true;
            } else if anim_due && !model.motion.reduced {
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
                    let pages = page_declarations(model);
                    let tabs = Tabs::new(&pages, model.tab)?;
                    build_topbar(&tabs, model, theme, col)?;
                    build_content(&tabs, model, theme, col)?;
                    if !model.compact {
                        build_footer(model, theme, col)?;
                    }
                    Ok(())
                },
            )?;
            if model.tab == ShowcaseTab::Widgets && model.samples.confirm {
                liuli_widgets::centered_confirm_dialog(
                    ui,
                    [Key(760), Key(761), Key(762), Key(763)],
                    [TextRef::Static("Reset"), TextRef::Static("Cancel")],
                    theme,
                )?;
            }
            Ok(())
        })
    }

    fn paint_model(
        model: &Self::Model,
        key: Key,
        props: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        if model.tab == ShowcaseTab::Widgets && key.0 >= 700 {
            return model.samples.paint(key, props, canvas, model.theme());
        }
        match key.0 {
            131 => paint_spinner(
                if model.motion.reduced {
                    InstantMs(0)
                } else {
                    model.now
                },
                props,
                canvas,
            ),
            606 => paint_meter(props, canvas),
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
        theme
            .button(liuli_widgets::ButtonVariant::Surface)
            .padding(EdgeInsets::ZERO)
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

fn page_declarations(m: &ShowcaseModel) -> [TabPage<ShowcaseTab, ShowcaseModel>; 6] {
    [
        TabPage::new(
            ShowcaseTab::Overview,
            Key(11),
            TextRef::Static(m.tr(i18n_keys::TAB_OVERVIEW)),
            build_tab_overview,
        ),
        TabPage::new(
            ShowcaseTab::Forms,
            Key(12),
            TextRef::Static(m.tr(i18n_keys::TAB_FORMS)),
            build_tab_forms,
        ),
        TabPage::new(
            ShowcaseTab::Charts,
            Key(13),
            TextRef::Static(m.tr(i18n_keys::TAB_CHARTS)),
            build_tab_charts,
        ),
        TabPage::new(
            ShowcaseTab::Layout,
            Key(14),
            TextRef::Static(m.tr(i18n_keys::TAB_LAYOUT)),
            build_tab_layout,
        ),
        TabPage::new(
            ShowcaseTab::Motion,
            Key(19),
            TextRef::Static(m.tr(i18n_keys::TAB_MOTION)),
            motion_page,
        ),
        TabPage::new(
            ShowcaseTab::Widgets,
            Key(23),
            TextRef::Static(m.tr(i18n_keys::TAB_WIDGETS)),
            |m, t, ui| m.samples.view(t, ui, m.locale),
        ),
    ]
}

fn tab_theme(theme: Theme, height: u16) -> Theme {
    theme
        .map_density(|d| liuli_widgets::Density {
            control_height: height,
            spacing: 4,
            pad_x: 2,
            pad_y: 2,
            ..d
        })
        .with_radius(6)
}

fn build_topbar(
    tabs: &Tabs<'_, ShowcaseTab, ShowcaseModel>,
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    let t = tab_theme(theme, 34).map_parts(|parts| {
        parts.with_tab(
            parts
                .tab
                .with_indicator_layout(IndicatorLayout {
                    span: IndicatorSpan::Content,
                    thickness: 3,
                    edge_offset: 2,
                    ..IndicatorLayout::UNDERLINE
                })
                .with_indicator_radius(2)
                .with_indicator_motion(IndicatorMotion::Slide),
        )
    });
    tabs.header_with(
        ui,
        Key(10),
        TabLayout::Wrap { max_rows: 2 },
        t,
        |bar| {
            bar.label(
                Key(15),
                NodeProps::new()
                    .font(FontId::Small12)
                    .fg(t.text_muted)
                    .text(TextRef::Static(model.tr(i18n_keys::TITLE))),
            )
        },
        |bar| {
            for (key, label) in [(16, i18n_keys::BTN_LANG), (17, i18n_keys::BTN_THEME)] {
                bar.button(
                    Key(key),
                    t.secondary_button()
                        .width(Length::Content)
                        .font(FontId::Small12)
                        .text(TextRef::Static(model.tr(label))),
                )?;
            }
            Ok(())
        },
    )
}

fn build_content(
    tabs: &Tabs<'_, ShowcaseTab, ShowcaseModel>,
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    let fill = NodeProps::new().size(Length::Fill(1), Length::Fill(1));
    ui.stack(Key(49), fill, |vp| {
        tabs.panel(vp, Key(50), fill.gap(8), model, theme)?;
        model.transition.frame().apply(vp, Key(50))
    })
}

fn build_tab_overview(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    if model.compact {
        return compact_overview(model, theme, ui);
    }
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
    if model.compact {
        return compact_forms(model, theme, ui);
    }
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
    if model.compact {
        return compact_charts(model, theme, ui);
    }
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
    if model.compact {
        return compact_layout(model, theme, ui);
    }
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
                        .width(Length::Fill(1))
                        .height(Length::Px(46))
                        .padding(EdgeInsets::symmetric(8, 4))
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

    scroll_strip(theme, ui)?;
    gallery(model, theme, ui)
}

fn scroll_strip(theme: Theme, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
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
                    .scroll_policy(theme.scroll_policy(ScrollPolicy::snap(CARD_WIDTH + CARD_GAP)))
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
                            .gap(CARD_GAP),
                        |cards| {
                            for (i, &(label, col)) in SCROLL_CARDS.iter().enumerate() {
                                cards.label(
                                    Key(452 + i as u32),
                                    NodeProps::new()
                                        .width(Length::Px(CARD_WIDTH.into()))
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
                    )
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

fn gallery(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    if model.compact {
        ui.row(Key(470), row_props(32), |r| {
            for (k, s) in [(471, "<"), (472, "Replay"), (473, ">")] {
                small_btn(
                    r,
                    Key(k),
                    s,
                    Length::Fill(1),
                    theme.surface_alt,
                    theme.text,
                    theme,
                )?;
            }
            Ok(())
        })?;
        return stage(model, theme, ui);
    }
    ui.row(
        Key(470),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(62))
            .align_cross(Align::Center)
            .gap(8),
        |row| {
            small_btn(
                row,
                Key(471),
                "<",
                Length::Px(52),
                theme.surface_alt,
                theme.text,
                theme,
            )?;
            small_btn(
                row,
                Key(472),
                "Replay",
                Length::Px(92),
                theme.accent,
                theme.background,
                theme,
            )?;
            small_btn(
                row,
                Key(473),
                ">",
                Length::Px(52),
                theme.surface_alt,
                theme.text,
                theme,
            )?;
            stage(model, theme, row)
        },
    )
}

fn stage(model: &ShowcaseModel, theme: Theme, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
    ui.stack(
        Key(474),
        NodeProps::new()
            .width(Length::Fill(1))
            .height(Length::Px(62))
            .bg(theme.surface)
            .radius(8),
        |stage| {
            stage.label(
                Key(475),
                NodeProps {
                    width: Length::Px(148),
                    height: Length::Px(36),
                    stack_align: StackAlign::Center,
                    text_align: TextAlign::Center,
                    text: TextRef::Static(TransitionKind::ALL[model.preset_index].name()),
                    ..NodeProps::new()
                        .bg(theme.accent)
                        .fg(theme.background)
                        .radius(8)
                },
            )?;
            model.preview.frame().apply(stage, Key(475))
        },
    )
}

fn row_props(height: u32) -> NodeProps {
    NodeProps::new()
        .size(Length::Fill(1), Length::Px(height))
        .gap(4)
        .align_cross(Align::Center)
}

fn motion_page(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    use liuli_widgets::{Motion, Style, ThemedUi};
    ui.row(Key(600), row_props(32), |r| {
        for (k, label, m) in [
            (601, "Standard", Motion::STANDARD),
            (602, "Snappy", Motion::SNAPPY),
            (603, "Reduced", Motion::REDUCED),
        ] {
            let active = model.motion == m;
            small_btn(
                r,
                Key(k),
                label,
                Length::Fill(1),
                if active {
                    theme.accent
                } else {
                    theme.surface_alt
                },
                if active { theme.background } else { theme.text },
                theme,
            )?;
        }
        Ok(())
    })?;
    small_btn(
        ui,
        Key(604),
        if model.local_slow && model.motion.reduced {
            "Local 480 ms / Reduced"
        } else if model.local_slow {
            "Local timing: 480 ms"
        } else {
            "Local timing: inherit"
        },
        Length::Fill(1),
        theme.surface,
        theme.text,
        theme,
    )?;
    ui.label(
        Key(607),
        NodeProps::new()
            .fg(theme.text)
            .text(TextRef::Static("Switch and custom meter")),
    )?;
    ui.row(Key(608), row_props(48), |r| {
        let mut scope = ThemedUi::new(r, theme);
        scope.toggle(Key(605), model.indicator_on)?;
        let motion = model.local_slow.then_some(
            theme
                .motion
                .with_state_ms(480)
                .with_state_easing(Easing::Linear),
        );
        scope.with_style(
            Style {
                motion,
                ..Style::default()
            },
            |local| {
                let t = local.theme();
                local.raw().custom(
                    Key(606),
                    t.control()
                        .size(Length::Fill(1), Length::Px(36))
                        .checked(model.indicator_on)
                        .clickable(true)
                        .focusable(true)
                        .state_motion(t.motion.state(MotionState::Checked, None)),
                )
            },
        )
    })?;
    gallery(model, theme, ui)?;
    let summary = ui.text_fmt(format_args!("Level {}%", model.level))?;
    ui.label(Key(609), NodeProps::new().fg(theme.text).text(summary))?;
    ui.row(Key(613), row_props(38), |r| {
        let mut controls = ThemedUi::new(r, theme);
        controls.slider_range(Key(610), 0, 100, 5, model.level)?;
        // The raised button exposes the same press timing to the surface and control painters.
        let raised = theme.with_effect_tier(liuli_widgets::EffectTier::Depth);
        let props = raised
            .button(liuli_widgets::ButtonVariant::Primary)
            .size(Length::Px(88), Length::Px(32))
            .padding(EdgeInsets::ZERO)
            .text(TextRef::Static("Step"));
        controls.raw().button(Key(612), props)?;
        controls
            .raw()
            .decorate(Key(612), raised.elevation(liuli_widgets::Elevation::Raised))
    })?;
    ThemedUi::new(ui, theme).progress_bar(Key(611), model.level)
}

/// An application-defined control uses the same retained progress as the SDK switch.
fn paint_meter(props: &StoredProps, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let sz = canvas.size();
    let w = sz.width.saturating_sub(8);
    let y = sz.height.saturating_sub(12) as i32 / 2;
    canvas.rounded_rect(
        Rect::new(4, y, w, 12),
        props.bg().unwrap_or(props.fg),
        6,
        Some(props.fg),
        1,
    )?;
    let fill = (u64::from(w.saturating_sub(4)) * canvas.state.anim_progress.clamp(0, 1000) as u64
        / 1000) as u32;
    if fill > 0 {
        canvas.rounded_rect(Rect::new(6, y + 2, fill, 8), props.accent, 4, None, 0)?;
    }
    Ok(())
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
            .padding(EdgeInsets::symmetric(10, 0))
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
    if model.compact {
        let _ = write!(line, "Load {}%  Temp {} C", model.load, model.temp);
    } else {
        let _ = format_template(
            &mut line,
            model.tr(i18n_keys::STATUS_FMT),
            &[&model.load, &model.temp, &MAX_NODES],
        );
    }
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

fn compact_overview(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.custom(Key(100), row_props(68).bg(theme.surface))?;
    ui.set_custom_rev(Key(100), model.banner_rev)?;
    gauge(ui, Key(110), model.load_gauge(), theme)?;
    ui.row(Key(109), row_props(32), |r| {
        for (k, label) in [(111, "-"), (112, "+")] {
            small_btn(
                r,
                Key(k),
                label,
                Length::Fill(1),
                theme.surface_alt,
                theme.text,
                theme,
            )?;
        }
        Ok(())
    })?;
    checkbox(
        ui,
        Key(201),
        TextRef::Static(model.tr(i18n_keys::CHK_LIVE)),
        model.live_sample,
        theme,
    )
}

fn compact_forms(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.scroll_y(
        Key(228),
        NodeProps::new()
            .scroll_policy(theme.scroll_policy(ScrollPolicy::FREE))
            .width(Length::Fill(1))
            .height(Length::Fill(1))
            .accent(theme.accent),
        |ui| {
            checkbox(
                ui,
                Key(202),
                TextRef::Static(model.tr(i18n_keys::CHK_BRIGHT)),
                model.chk_bright,
                theme,
            )?;
            radio(
                ui,
                Key(213),
                TextRef::Static(model.tr(i18n_keys::RAD_FADE)),
                model.trans_kind == TransitionKind::Fade,
                theme,
            )?;
            roller(
                ui,
                Key(230),
                Length::Fill(1),
                70,
                CLK_OPTIONS,
                model.roller,
                theme,
            )?;
            multi_line_editor(ui, Key(240), Length::Fill(1), 98, &model.editor, theme)?;
            dropdown(
                ui,
                Key(220),
                Key(221),
                Key(222),
                223,
                DRIVER_OPTIONS,
                model.dropdown,
                theme,
            )?;
            small_btn(
                ui,
                Key(241),
                model.tr(i18n_keys::BTN_ADD_TEXT),
                Length::Fill(1),
                theme.surface,
                theme.text,
                theme,
            )
        },
    )
}

fn compact_charts(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    for k in [301, 302] {
        ui.custom(Key(k), row_props(112).bg(theme.surface))?;
        ui.set_custom_rev(Key(k), model.chart_rev)?;
    }
    small_btn(
        ui,
        Key(310),
        "Sample",
        Length::Fill(1),
        theme.surface,
        theme.text,
        theme,
    )?;
    let cols = [
        TableColumn::new("Module", Length::Fill(2), TextAlign::Start),
        TableColumn::new("State", Length::Fill(1), TextAlign::End),
    ];
    let rows = TABLE_ROWS.map(|row| [row[0], row[3]]);
    data_table(
        ui,
        320,
        &cols,
        &[&rows[0], &rows[1], &rows[2]],
        model.selected_row,
        27,
        theme,
    )
}

fn compact_layout(
    model: &ShowcaseModel,
    theme: Theme,
    ui: &mut Children<'_, '_>,
) -> Result<(), BuildError> {
    ui.grid(
        Key(420),
        2,
        NodeProps::new().width(Length::Fill(1)).gap(4),
        |g| {
            for (i, label) in [
                "Grid",
                "Rows",
                "Images",
                "Forms",
                "Locale",
                "Input",
                "Paint",
                "Simulator",
            ]
            .into_iter()
            .enumerate()
            {
                small_btn(
                    g,
                    Key(421 + i as u32),
                    label,
                    Length::Fill(1),
                    if model.grid_selected == i {
                        theme.accent
                    } else {
                        theme.surface
                    },
                    theme.text,
                    theme,
                )?;
            }
            Ok(())
        },
    )?;
    scroll_strip(theme, ui)
}
