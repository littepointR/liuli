use liuli::prelude::*;

use crate::model::{SettingsModel, SettingsMsg};
use crate::pages;
use crate::theme::{DARK_THEME, LIGHT_THEME, Theme};

pub const SAMPLE_INTERVAL_MS: u64 = 1000;
pub const TOAST_DURATION_MS: u64 = 1500;

const WAVE_TEMPS_DECI_C: [i16; 16] = [
    226, 230, 236, 244, 252, 256, 248, 238, 232, 224, 234, 246, 254, 242, 230, 228,
];

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum PageId {
    #[default]
    Dashboard = 0,
    Settings = 1,
    Diagnostics = 2,
}

impl PageId {
    #[inline]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Settings,
            2 => Self::Diagnostics,
            _ => Self::Dashboard,
        }
    }

    #[inline]
    pub const fn as_usize(self) -> usize {
        self as usize
    }

    #[inline]
    pub const fn scroll_key(self) -> Key {
        match self {
            Self::Dashboard => terminal_keys::DASH_SCROLL,
            Self::Settings => terminal_keys::SET_SCROLL,
            Self::Diagnostics => terminal_keys::DIAG_SCROLL,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct PageMemory {
    pub focus_key: Option<Key>,
    pub scroll_y: i32,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct DiagnosticsSnapshot {
    pub last_bands: u32,
    pub last_pixels: u32,
    pub last_measured: u32,
    pub last_commands: u32,
    pub total_frames: u32,
    pub captured_at_ms: u64,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ToastState {
    pub text: &'static str,
    pub expires_at: InstantMs,
}

pub mod terminal_keys {
    use liuli::prelude::Key;

    pub const ROOT: Key = Key(1);
    pub const STACK: Key = Key(2);
    pub const MAIN_COL: Key = Key(3);

    pub const TAB_BAR: Key = Key(10);
    pub const TAB_DASHBOARD: Key = Key(11);
    pub const TAB_SETTINGS: Key = Key(12);
    pub const TAB_DIAGNOSTICS: Key = Key(13);

    pub const FOOTER: Key = Key(20);
    pub const FOOTER_TEXT: Key = Key(21);

    // Dashboard (1000..1099)
    pub const DASH_SCROLL: Key = Key(1000);
    pub const DASH_KPI_ROW: Key = Key(1001);
    pub const DASH_TEMP_CARD: Key = Key(1002);
    pub const DASH_TEMP_LBL: Key = Key(1003);
    pub const DASH_TEMP_VAL: Key = Key(1004);
    pub const DASH_LOAD_CARD: Key = Key(1005);
    pub const DASH_LOAD_HDR: Key = Key(1006);
    pub const DASH_LOAD_LBL: Key = Key(1007);
    pub const DASH_LOAD_VAL: Key = Key(1008);
    pub const DASH_LOAD_BAR: Key = Key(1009);
    pub const DASH_WAVE_CARD: Key = Key(1010);
    pub const DASH_WAVE_HDR: Key = Key(1011);
    pub const DASH_WAVE_TITLE: Key = Key(1012);
    pub const DASH_WAVE_META: Key = Key(1013);
    pub const DASH_SPARKLINE: Key = Key(1014);
    pub const DASH_ACT_ROW: Key = Key(1015);
    pub const DASH_BTN_PAUSE: Key = Key(1016);
    pub const DASH_BTN_DEC: Key = Key(1017);
    pub const DASH_BTN_INC: Key = Key(1018);
    pub const DASH_STATUS_LBL: Key = Key(1019);
    pub const DASH_NOTE_ROW: Key = Key(1020);
    pub const DASH_NOTE_LBL: Key = Key(1021);

    // Settings (2000..2099)
    pub const SET_SCROLL: Key = Key(2000);
    pub const SET_ROW_BRIGHTNESS: Key = Key(2010);
    pub const SET_LBL_BRIGHTNESS: Key = Key(2011);
    pub const SET_VAL_BRIGHTNESS: Key = Key(2012);
    pub const SET_BTN_BRIGHT_DEC: Key = Key(2013);
    pub const SET_SLIDER_BRIGHTNESS: Key = Key(2014);
    pub const SET_BTN_BRIGHT_INC: Key = Key(2015);

    pub const SET_ROW_SOUND: Key = Key(2020);
    pub const SET_LBL_SOUND: Key = Key(2021);
    pub const SET_STATE_SOUND: Key = Key(2022);
    pub const SET_TOGGLE_SOUND: Key = Key(2023);

    pub const SET_ROW_THEME: Key = Key(2030);
    pub const SET_LBL_THEME: Key = Key(2031);
    pub const SET_STATE_THEME: Key = Key(2032);
    pub const SET_TOGGLE_THEME: Key = Key(2033);

    pub const SET_ROW_MONITOR: Key = Key(2040);
    pub const SET_LBL_MONITOR: Key = Key(2041);
    pub const SET_STATE_MONITOR: Key = Key(2042);
    pub const SET_TOGGLE_MONITOR: Key = Key(2043);

    pub const SET_ROW_TEMP: Key = Key(2050);
    pub const SET_LBL_TEMP: Key = Key(2051);
    pub const SET_VAL_TEMP: Key = Key(2052);

    pub const SET_ROW_RESET: Key = Key(2060);
    pub const SET_LBL_RESET: Key = Key(2061);
    pub const SET_BTN_RESET: Key = Key(2062);

    // Diagnostics (3000..3099)
    pub const DIAG_SCROLL: Key = Key(3000);
    pub const DIAG_ROW_SAMPLE: Key = Key(3010);
    pub const DIAG_LBL_SAMPLE: Key = Key(3011);
    pub const DIAG_VAL_SAMPLE: Key = Key(3012);
    pub const DIAG_ROW_XFER: Key = Key(3020);
    pub const DIAG_LBL_XFER: Key = Key(3021);
    pub const DIAG_VAL_XFER: Key = Key(3022);
    pub const DIAG_ROW_FRAMES: Key = Key(3030);
    pub const DIAG_LBL_FRAMES: Key = Key(3031);
    pub const DIAG_VAL_FRAMES: Key = Key(3032);
    pub const DIAG_ROW_STORAGE: Key = Key(3040);
    pub const DIAG_LBL_STORAGE: Key = Key(3041);
    pub const DIAG_VAL_STORAGE: Key = Key(3042);
    pub const DIAG_ROW_TEMP: Key = Key(3050);
    pub const DIAG_LBL_TEMP: Key = Key(3051);
    pub const DIAG_VAL_TEMP: Key = Key(3052);
    pub const DIAG_ROW_TIME: Key = Key(3060);
    pub const DIAG_LBL_TIME: Key = Key(3061);
    pub const DIAG_VAL_TIME: Key = Key(3062);
    pub const DIAG_ACT_ROW: Key = Key(3070);
    pub const DIAG_BTN_REFRESH: Key = Key(3071);
    pub const DIAG_BTN_MONITOR: Key = Key(3072);

    // Modal (4000..4006)
    pub const MODAL: Key = Key(4000);
    pub const MODAL_CARD: Key = Key(4001);
    pub const MODAL_TITLE: Key = Key(4002);
    pub const MODAL_BODY: Key = Key(4003);
    pub const MODAL_BTNS: Key = Key(4004);
    pub const BTN_CANCEL: Key = Key(4005);
    pub const BTN_CONFIRM: Key = Key(4006);

    // Toast (4100..4102)
    pub const TOAST_LAYER: Key = Key(4100);
    pub const TOAST_BOX: Key = Key(4101);
    pub const TOAST_LABEL: Key = Key(4102);
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TerminalModel {
    pub active_page: PageId,
    pub page_memory: [PageMemory; 3],
    pub settings: SettingsModel,
    pub load_pct: i16,
    pub load_bias: i16,
    pub temp_history: SampleRing<32>,
    pub sample_seq: u64,
    pub toast: Option<ToastState>,
    pub latest_frame: DiagnosticsSnapshot,
    pub displayed_diag: DiagnosticsSnapshot,
}

impl TerminalModel {
    pub fn new(now: InstantMs) -> Self {
        let mut settings = SettingsModel::new(now);
        let temp0 = WAVE_TEMPS_DECI_C[0];
        settings.temp_deci_c = temp0;
        let mut history = SampleRing::new();
        history.push(temp0);
        Self {
            active_page: PageId::Dashboard,
            page_memory: [PageMemory::default(); 3],
            settings,
            load_pct: compute_load_pct(temp0, 0),
            load_bias: 0,
            temp_history: history,
            sample_seq: 0,
            toast: None,
            latest_frame: DiagnosticsSnapshot::default(),
            displayed_diag: DiagnosticsSnapshot::default(),
        }
    }

    pub fn record_completed_frame(&mut self, metrics: FrameMetrics, now: InstantMs) {
        self.latest_frame = DiagnosticsSnapshot {
            last_bands: metrics.bands,
            last_pixels: metrics.pixels_sent,
            last_measured: metrics.measured_nodes,
            last_commands: metrics.commands_replayed,
            total_frames: self.latest_frame.total_frames.saturating_add(1),
            captured_at_ms: now.0,
        };
    }

    pub fn sample_tick(&mut self, now: InstantMs) -> bool {
        if !self.settings.live_monitoring {
            return false;
        }
        let Some(due) = self.settings.next_sample_ms else {
            return false;
        };
        if now < due {
            return false;
        }
        let overdue = 1 + now.saturating_sub(due) / SAMPLE_INTERVAL_MS;
        let target_seq = self.sample_seq.saturating_add(overdue);
        let count = overdue.min(MAX_SPARKLINE_SAMPLES as u64);
        let first_seq = target_seq - (count - 1);
        let mut last_temp = self.settings.temp_deci_c;
        for seq in first_seq..=target_seq {
            last_temp = WAVE_TEMPS_DECI_C[(seq as usize) % WAVE_TEMPS_DECI_C.len()];
            self.temp_history.push(last_temp);
        }
        self.sample_seq = target_seq;
        self.settings.temp_deci_c = last_temp;
        self.load_pct = compute_load_pct(last_temp, self.load_bias);
        self.settings.next_sample_ms =
            Some(due.saturating_add(overdue.saturating_mul(SAMPLE_INTERVAL_MS)));
        true
    }

    pub fn show_toast(&mut self, text: &'static str, now: InstantMs) {
        self.toast = Some(ToastState {
            text,
            expires_at: now.saturating_add(TOAST_DURATION_MS),
        });
    }
}

impl Default for TerminalModel {
    fn default() -> Self {
        Self::new(InstantMs(0))
    }
}

#[inline]
const fn compute_load_pct(temp_deci_c: i16, bias: i16) -> i16 {
    let base = 35 + (((temp_deci_c - 220) * 3) / 2) + bias;
    if base < 5 {
        5
    } else if base > 100 {
        100
    } else {
        base
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TerminalMsg {
    SelectPage(PageId),
    Settings(SettingsMsg),
    AdjustBrightnessDelta(i16),
    AdjustLoadBias(i16),
    ToggleMonitoring,
    RefreshDiagnostics,
    ShowToast(&'static str),
    BackToDashboard,
}

pub struct TerminalApp;

impl TerminalApp {
    pub fn theme(model: &TerminalModel) -> &'static Theme {
        if model.settings.dark_theme {
            &DARK_THEME
        } else {
            &LIGHT_THEME
        }
    }
}

impl App for TerminalApp {
    type Model = TerminalModel;
    type Msg = TerminalMsg;

    fn page_id(model: &Self::Model) -> u8 {
        model.active_page as u8
    }

    fn is_tab_key(key: Key) -> bool {
        matches!(
            key,
            terminal_keys::TAB_DASHBOARD
                | terminal_keys::TAB_SETTINGS
                | terminal_keys::TAB_DIAGNOSTICS
        )
    }

    fn page_restore(model: &Self::Model, page: u8) -> PageRestore {
        let p = PageId::from_u8(page);
        let mem = model.page_memory[p.as_usize()];
        PageRestore {
            scroll_key: Some(p.scroll_key()),
            scroll_y: mem.scroll_y,
            focus_key: mem.focus_key,
        }
    }

    fn on_page_committed(
        model: &mut Self::Model,
        old_page: u8,
        saved_focus: Option<Key>,
        _saved_scroll_key: Option<Key>,
        saved_scroll_y: i32,
    ) {
        let idx = PageId::from_u8(old_page).as_usize();
        model.page_memory[idx] = PageMemory {
            focus_key: saved_focus,
            scroll_y: saved_scroll_y,
        };
    }

    fn update(model: &mut Self::Model, msg: Self::Msg, now: InstantMs) -> Change {
        match msg {
            TerminalMsg::SelectPage(page) => {
                if model.settings.reset_modal_open {
                    return Change::Unchanged;
                }
                if model.active_page != page {
                    model.active_page = page;
                    if page == PageId::Diagnostics {
                        model.displayed_diag = model.latest_frame;
                    }
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            TerminalMsg::Settings(smsg) => match smsg {
                SettingsMsg::SetBrightness(v) => {
                    let clamped = ((v + 2) / 5 * 5).clamp(0, 100);
                    if model.settings.brightness != clamped {
                        model.settings.brightness = clamped;
                        Change::View
                    } else {
                        Change::Unchanged
                    }
                }
                SettingsMsg::ToggleSound => {
                    model.settings.sound_enabled = !model.settings.sound_enabled;
                    Change::View
                }
                SettingsMsg::ToggleTheme => {
                    model.settings.dark_theme = !model.settings.dark_theme;
                    Change::View
                }
                SettingsMsg::ToggleMonitoring => {
                    Self::update(model, TerminalMsg::ToggleMonitoring, now)
                }
                SettingsMsg::OpenResetModal => {
                    if !model.settings.reset_modal_open {
                        model.settings.reset_modal_open = true;
                        Change::View
                    } else {
                        Change::Unchanged
                    }
                }
                SettingsMsg::CancelReset => {
                    if model.settings.reset_modal_open {
                        model.settings.reset_modal_open = false;
                        Change::View
                    } else {
                        Change::Unchanged
                    }
                }
                SettingsMsg::ConfirmReset => {
                    let cur_temp = model.settings.temp_deci_c;
                    model.settings.restore_defaults(now);
                    model.settings.temp_deci_c = cur_temp;
                    model.settings.next_sample_ms = Some(now.saturating_add(SAMPLE_INTERVAL_MS));
                    model.load_bias = 0;
                    model.load_pct = compute_load_pct(cur_temp, 0);
                    model.show_toast("已恢复默认", now);
                    Change::View
                }
            },
            TerminalMsg::AdjustBrightnessDelta(delta) => {
                let target = (model.settings.brightness + delta).clamp(0, 100);
                if target != model.settings.brightness {
                    model.settings.brightness = target;
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            TerminalMsg::AdjustLoadBias(delta) => {
                let next_bias = (model.load_bias + delta).clamp(-20, 20);
                let next_pct = compute_load_pct(model.settings.temp_deci_c, next_bias);
                if next_bias != model.load_bias || next_pct != model.load_pct {
                    model.load_bias = next_bias;
                    model.load_pct = next_pct;
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            TerminalMsg::ToggleMonitoring => {
                model.settings.live_monitoring = !model.settings.live_monitoring;
                if model.settings.live_monitoring {
                    model.settings.next_sample_ms = Some(now.saturating_add(SAMPLE_INTERVAL_MS));
                } else {
                    model.settings.next_sample_ms = None;
                }
                Change::View
            }
            TerminalMsg::RefreshDiagnostics => {
                model.displayed_diag = model.latest_frame;
                model.show_toast("诊断已刷新", now);
                Change::View
            }
            TerminalMsg::ShowToast(text) => {
                model.show_toast(text, now);
                Change::View
            }
            TerminalMsg::BackToDashboard => {
                if model.settings.reset_modal_open {
                    model.settings.reset_modal_open = false;
                    return Change::View;
                }
                if model.active_page != PageId::Dashboard {
                    model.active_page = PageId::Dashboard;
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
        }
    }

    fn on_action(model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
        use terminal_keys as tk;

        match (event.key, event.kind) {
            (tk::ROOT, ActionKind::Dismiss) => {
                if model.settings.reset_modal_open || model.active_page != PageId::Dashboard {
                    Some(TerminalMsg::BackToDashboard)
                } else {
                    None
                }
            }
            (tk::MODAL, ActionKind::Dismiss) | (tk::BTN_CANCEL, ActionKind::Activate) => {
                Some(TerminalMsg::Settings(SettingsMsg::CancelReset))
            }
            (tk::BTN_CONFIRM, ActionKind::Activate) => {
                Some(TerminalMsg::Settings(SettingsMsg::ConfirmReset))
            }
            _ if model.settings.reset_modal_open => None,
            (tk::TAB_DASHBOARD, ActionKind::Activate) => {
                Some(TerminalMsg::SelectPage(PageId::Dashboard))
            }
            (tk::TAB_SETTINGS, ActionKind::Activate) => {
                Some(TerminalMsg::SelectPage(PageId::Settings))
            }
            (tk::TAB_DIAGNOSTICS, ActionKind::Activate) => {
                Some(TerminalMsg::SelectPage(PageId::Diagnostics))
            }
            (tk::DASH_BTN_PAUSE, ActionKind::Activate) => Some(TerminalMsg::ToggleMonitoring),
            (tk::DASH_BTN_DEC, ActionKind::Activate) => Some(TerminalMsg::AdjustLoadBias(-5)),
            (tk::DASH_BTN_INC, ActionKind::Activate) => Some(TerminalMsg::AdjustLoadBias(5)),
            (tk::SET_BTN_BRIGHT_DEC, ActionKind::Activate) => {
                Some(TerminalMsg::AdjustBrightnessDelta(-5))
            }
            (tk::SET_BTN_BRIGHT_INC, ActionKind::Activate) => {
                Some(TerminalMsg::AdjustBrightnessDelta(5))
            }
            (tk::SET_SLIDER_BRIGHTNESS, ActionKind::SetValue(v)) => {
                Some(TerminalMsg::Settings(SettingsMsg::SetBrightness(v)))
            }
            (
                tk::SET_TOGGLE_SOUND | tk::SET_ROW_SOUND,
                ActionKind::Toggle | ActionKind::Activate,
            ) => Some(TerminalMsg::Settings(SettingsMsg::ToggleSound)),
            (
                tk::SET_TOGGLE_THEME | tk::SET_ROW_THEME,
                ActionKind::Toggle | ActionKind::Activate,
            ) => Some(TerminalMsg::Settings(SettingsMsg::ToggleTheme)),
            (
                tk::SET_TOGGLE_MONITOR | tk::SET_ROW_MONITOR,
                ActionKind::Toggle | ActionKind::Activate,
            ) => Some(TerminalMsg::ToggleMonitoring),
            (tk::SET_BTN_RESET | tk::SET_ROW_RESET, ActionKind::Activate) => {
                Some(TerminalMsg::Settings(SettingsMsg::OpenResetModal))
            }
            (tk::DIAG_BTN_REFRESH, ActionKind::Activate) => Some(TerminalMsg::RefreshDiagnostics),
            (tk::DIAG_BTN_MONITOR, ActionKind::Activate) => Some(TerminalMsg::ToggleMonitoring),
            _ => None,
        }
    }

    fn next_deadline(model: &Self::Model, _now: InstantMs) -> Option<InstantMs> {
        let sample_due = if model.settings.live_monitoring {
            model.settings.next_sample_ms
        } else {
            None
        };
        let toast_due = model.toast.map(|t| t.expires_at);
        match (sample_due, toast_due) {
            (Some(a), Some(b)) => Some(if a <= b { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn on_timer(model: &mut Self::Model, now: InstantMs) -> Change {
        let mut changed = false;
        if let Some(toast) = model.toast
            && now >= toast.expires_at
        {
            model.toast = None;
            changed = true;
        }
        if model.sample_tick(now) {
            changed = true;
        }
        if changed {
            Change::View
        } else {
            Change::Unchanged
        }
    }

    fn view(model: &Self::Model) -> impl Scene {
        use terminal_keys as tk;

        let t = Self::theme(model);

        let mut root_p = NodeProps::new();
        root_p.width = Length::Fill(1);
        root_p.height = Length::Fill(1);
        root_p.bg = Some(t.bg);
        root_p.theme_id = t.id;

        layout(tk::ROOT, root_p, move |ui| {
            let mut stack_p = NodeProps::new();
            stack_p.width = Length::Fill(1);
            stack_p.height = Length::Fill(1);

            ui.stack(tk::STACK, stack_p, |ui| {
                let mut col_p = NodeProps::new();
                col_p.width = Length::Fill(1);
                col_p.height = Length::Fill(1);
                col_p.align_cross = Align::Stretch;

                ui.column(tk::MAIN_COL, col_p, |ui| {
                    // 1. Tab Bar (34 px)
                    let mut tab_bar_p = NodeProps::new();
                    tab_bar_p.width = Length::Fill(1);
                    tab_bar_p.height = Length::Px(34);
                    tab_bar_p.padding = EdgeInsets::symmetric(8, 4);
                    tab_bar_p.gap = 6;
                    tab_bar_p.bg = Some(t.header_bg);
                    tab_bar_p.border_color = Some(t.border);
                    tab_bar_p.border_width = 1;
                    tab_bar_p.align_cross = Align::Center;
                    ui.row(tk::TAB_BAR, tab_bar_p, |ui| {
                        build_tab_btn(
                            ui,
                            tk::TAB_DASHBOARD,
                            "仪表盘",
                            IconKind::Gauge,
                            model.active_page == PageId::Dashboard,
                            t,
                        )?;
                        build_tab_btn(
                            ui,
                            tk::TAB_SETTINGS,
                            "设置",
                            IconKind::Sliders,
                            model.active_page == PageId::Settings,
                            t,
                        )?;
                        build_tab_btn(
                            ui,
                            tk::TAB_DIAGNOSTICS,
                            "诊断",
                            IconKind::Diagnostics,
                            model.active_page == PageId::Diagnostics,
                            t,
                        )
                    })?;

                    // 2. Active page content (186 px via Fill(1))
                    match ui.variant(
                        model.active_page,
                        &[PageId::Dashboard, PageId::Settings, PageId::Diagnostics],
                    ) {
                        PageId::Dashboard => pages::dashboard::build_dashboard(model, ui, t)?,
                        PageId::Settings => pages::settings::build_settings(model, ui, t)?,
                        PageId::Diagnostics => pages::diagnostics::build_diagnostics(model, ui, t)?,
                    }

                    // 3. Footer (20 px, Small12)
                    let mut footer_p = NodeProps::new();
                    footer_p.width = Length::Fill(1);
                    footer_p.height = Length::Px(20);
                    footer_p.padding = EdgeInsets::symmetric(8, 2);
                    footer_p.bg = Some(t.header_bg);
                    footer_p.border_color = Some(t.border);
                    footer_p.border_width = 1;
                    footer_p.align_cross = Align::Center;
                    ui.row(tk::FOOTER, footer_p, |ui| {
                        let mut f_lbl = NodeProps::new();
                        f_lbl.width = Length::Fill(1);
                        f_lbl.height = Length::Content;
                        f_lbl.fg = t.text_muted;
                        f_lbl.font = FontId::Small12;
                        f_lbl.text = TextRef::Static("页签 · 监测设置诊断 · 状态正常");
                        f_lbl.max_lines = 1;
                        ui.label(tk::FOOTER_TEXT, f_lbl)
                    })
                })?;

                // 4. Optional non-focusable click-through Toast
                if ui.optional(model.toast.is_some()) {
                    let text = model.toast.map_or("", |toast| toast.text);
                    let mut layer_p = NodeProps::new();
                    layer_p.width = Length::Fill(1);
                    layer_p.height = Length::Fill(1);
                    layer_p.padding = EdgeInsets {
                        top: 178,
                        right: 0,
                        bottom: 0,
                        left: 0,
                    };
                    layer_p.align_cross = Align::Center;
                    ui.column(tk::TOAST_LAYER, layer_p, |ui| {
                        let mut box_p = NodeProps::new();
                        box_p.width = Length::Px(156);
                        box_p.height = Length::Px(30);
                        box_p.padding = EdgeInsets::symmetric(12, 5);
                        box_p.bg = Some(t.accent);
                        box_p.border_color = Some(t.border);
                        box_p.border_width = 1;
                        box_p.radius = 8;
                        box_p.align_cross = Align::Center;
                        ui.column(tk::TOAST_BOX, box_p, |ui| {
                            let mut tl_p = NodeProps::new();
                            tl_p.width = Length::Fill(1);
                            tl_p.height = Length::Content;
                            tl_p.fg = t.on_accent;
                            tl_p.font = FontId::Body16;
                            tl_p.text_align = TextAlign::Center;
                            tl_p.text = TextRef::Static(text);
                            tl_p.max_lines = 1;
                            ui.label(tk::TOAST_LABEL, tl_p)
                        })
                    })?;
                }

                // 5. Optional Confirm Reset Modal
                if ui.optional(model.settings.reset_modal_open) {
                    let mut modal_p = NodeProps::new();
                    modal_p.width = Length::Fill(1);
                    modal_p.height = Length::Fill(1);
                    modal_p.overlay_color = t.overlay;
                    modal_p.overlay_alpha = 9;
                    ui.modal(tk::MODAL, modal_p, |ui| {
                        let mut card_p = NodeProps::new();
                        card_p.width = Length::Px(252);
                        card_p.height = Length::Content;
                        card_p.padding = EdgeInsets::all(14);
                        card_p.gap = 8;
                        card_p.align_cross = Align::Stretch;
                        card_p.bg = Some(t.surface);
                        card_p.border_color = Some(t.border);
                        card_p.border_width = 1;
                        card_p.radius = 10;
                        ui.column(tk::MODAL_CARD, card_p, |ui| {
                            let mut mt_p = NodeProps::new();
                            mt_p.width = Length::Fill(1);
                            mt_p.height = Length::Content;
                            mt_p.fg = t.text;
                            mt_p.text = TextRef::Static("恢复默认设置？");
                            mt_p.max_lines = 1;
                            ui.label(tk::MODAL_TITLE, mt_p)?;

                            let mut mb_p = NodeProps::new();
                            mb_p.width = Length::Fill(1);
                            mb_p.height = Length::Content;
                            mb_p.fg = t.text_muted;
                            mb_p.text = TextRef::Static("将恢复出厂默认值。");
                            mb_p.max_lines = 2;
                            ui.label(tk::MODAL_BODY, mb_p)?;

                            let mut btn_row = NodeProps::new();
                            btn_row.width = Length::Fill(1);
                            btn_row.height = Length::Px(40);
                            btn_row.padding = EdgeInsets::all(4);
                            btn_row.gap = 10;
                            btn_row.align_cross = Align::Stretch;
                            ui.row(tk::MODAL_BTNS, btn_row, |ui| {
                                let mut cancel_p = t.control();
                                cancel_p.width = Length::Fill(1);
                                cancel_p.height = Length::Px(32);
                                cancel_p.padding = EdgeInsets::symmetric(8, 4);
                                cancel_p.bg = Some(t.surface_alt);
                                cancel_p.border_color = Some(t.border);
                                cancel_p.border_width = 1;
                                cancel_p.radius = 6;
                                cancel_p.fg = t.text;
                                cancel_p.accent = t.surface_alt;
                                cancel_p.text = TextRef::Static("取消");
                                cancel_p.max_lines = 1;
                                cancel_p.focusable = true;
                                ui.button(tk::BTN_CANCEL, cancel_p)?;

                                let mut confirm_p = t.control();
                                confirm_p.width = Length::Fill(1);
                                confirm_p.height = Length::Px(32);
                                confirm_p.padding = EdgeInsets::symmetric(8, 4);
                                confirm_p.bg = Some(t.danger);
                                confirm_p.radius = 6;
                                confirm_p.fg = t.on_accent;
                                confirm_p.accent = t.danger;
                                confirm_p.text = TextRef::Static("恢复");
                                confirm_p.max_lines = 1;
                                confirm_p.focusable = true;
                                ui.button(tk::BTN_CONFIRM, confirm_p)
                            })
                        })
                    })?;
                }

                Ok(())
            })
        })
    }
}

fn build_tab_btn(
    ui: &mut Children<'_, '_>,
    key: Key,
    label: &'static str,
    icon: IconKind,
    active: bool,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut bp = t.control();
    bp.width = Length::Fill(1);
    bp.height = Length::Px(26);
    bp.padding = EdgeInsets::symmetric(6, 3);
    bp.bg = Some(if active { t.accent } else { t.surface_alt });
    bp.border_color = Some(if active { t.accent } else { t.border });
    bp.border_width = 1;
    bp.radius = 6;
    bp.fg = if active { t.on_accent } else { t.text };
    bp.accent = if active { t.accent } else { t.surface_alt };
    bp.icon = icon;
    bp.text = TextRef::Static(label);
    bp.max_lines = 1;
    bp.focusable = true;
    ui.button(key, bp)
}
