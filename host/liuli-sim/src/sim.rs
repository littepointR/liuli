use liuli::core::focus::{FocusState, PointerState};
use liuli::core::node::RetainedTree;
use liuli::core::paint::CommandList;
use liuli::prelude::*;

use liuli_settings::{
    PageId, SettingsApp, SettingsModel, TerminalApp, TerminalModel, TerminalMsg, app_fonts,
};

use crate::screen::MemoryScreen;
use crate::session::rects_to_json;

pub use crate::session::{AppSession, FrameLookupError, FrameSnapshot, MAX_PNG_HISTORY};
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SimScenario {
    Settings,
    Terminal,
}

pub enum SimEngine<'a, 's> {
    Settings(&'a Engine<'s, 'static, SettingsApp>),
    Terminal(&'a Engine<'s, 'static, TerminalApp>),
}
#[allow(clippy::large_enum_variant)]
pub enum SimSession<'s> {
    Settings(AppSession<'s, 'static, SettingsApp>),
    Terminal(AppSession<'s, 'static, TerminalApp>),
}
impl<'s> SimSession<'s> {
    pub fn new(
        scenario: SimScenario,
        storage: &'s mut Storage,
        now: InstantMs,
        verify: bool,
    ) -> Result<Self, String> {
        let size = Size::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
        match scenario {
            SimScenario::Settings => Ok(Self::Settings(AppSession::new(
                storage,
                SettingsModel::new(now),
                app_fonts(),
                size,
                now,
                verify,
            )?)),
            SimScenario::Terminal => Ok(Self::Terminal(AppSession::with_frame_callback(
                storage,
                TerminalModel::new(now),
                app_fonts(),
                size,
                now,
                verify,
                Some(TerminalModel::record_completed_frame),
            )?)),
        }
    }
    pub fn scenario(&self) -> SimScenario {
        match self {
            Self::Settings(_) => SimScenario::Settings,
            Self::Terminal(_) => SimScenario::Terminal,
        }
    }
    pub fn engine(&self) -> SimEngine<'_, 's> {
        match self {
            Self::Settings(s) => SimEngine::Settings(s.engine()),
            Self::Terminal(s) => SimEngine::Terminal(s.engine()),
        }
    }
    pub fn screen(&self) -> &MemoryScreen {
        match self {
            Self::Settings(s) => s.screen(),
            Self::Terminal(s) => s.screen(),
        }
    }
    pub fn snapshot(&self) -> &FrameSnapshot {
        match self {
            Self::Settings(s) => s.snapshot(),
            Self::Terminal(s) => s.snapshot(),
        }
    }
    pub fn png_for_frame(&self, id: Option<u64>) -> Result<(u64, &[u8]), FrameLookupError> {
        match self {
            Self::Settings(s) => s.png_for_frame(id),
            Self::Terminal(s) => s.png_for_frame(id),
        }
    }
    pub fn step(
        &mut self,
        now: InstantMs,
        input: Option<RawInput>,
        verify: bool,
    ) -> Result<&FrameSnapshot, String> {
        match self {
            Self::Settings(s) => s.step(now, input, verify),
            Self::Terminal(s) => s.step(now, input, verify),
        }
    }
    pub fn active_tree(&self) -> &RetainedTree {
        match self.engine() {
            SimEngine::Settings(e) => e.active_tree(),
            SimEngine::Terminal(e) => e.active_tree(),
        }
    }

    pub fn active_commands(&self) -> &CommandList {
        match self.engine() {
            SimEngine::Settings(e) => e.active_commands(),
            SimEngine::Terminal(e) => e.active_commands(),
        }
    }

    pub fn focus(&self) -> &FocusState {
        match self.engine() {
            SimEngine::Settings(e) => e.focus(),
            SimEngine::Terminal(e) => e.focus(),
        }
    }

    pub fn pointer(&self) -> &PointerState {
        match self.engine() {
            SimEngine::Settings(e) => e.pointer(),
            SimEngine::Terminal(e) => e.pointer(),
        }
    }

    pub fn damage(&self) -> &DamageSet {
        match self.engine() {
            SimEngine::Settings(e) => e.damage(),
            SimEngine::Terminal(e) => e.damage(),
        }
    }

    pub fn fonts(&self) -> &FontSet<'static> {
        match self.engine() {
            SimEngine::Settings(e) => e.fonts(),
            SimEngine::Terminal(e) => e.fonts(),
        }
    }

    pub fn band_height(&self) -> u32 {
        match self.engine() {
            SimEngine::Settings(e) => e.band_height(),
            SimEngine::Terminal(e) => e.band_height(),
        }
    }

    pub fn last_metrics(&self) -> FrameMetrics {
        match self.engine() {
            SimEngine::Settings(e) => e.last_metrics(),
            SimEngine::Terminal(e) => e.last_metrics(),
        }
    }

    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        match self.engine() {
            SimEngine::Settings(e) => e.next_wakeup(now, false),
            SimEngine::Terminal(e) => e.next_wakeup(now, false),
        }
    }

    pub fn select_terminal_page(
        &mut self,
        page: PageId,
        now: InstantMs,
        verify: bool,
    ) -> Result<&FrameSnapshot, String> {
        match self {
            Self::Terminal(s) => s.message(now, TerminalMsg::SelectPage(page), verify),
            Self::Settings(_) => Err("'page' op requires 'scenario terminal'".into()),
        }
    }
    pub fn state_json(&self, paused: bool, rate_x: u32) -> String {
        let s = self.snapshot();
        let status_str = match s.status {
            PresentStatus::Idle => "idle",
            PresentStatus::Pending => "pending",
            PresentStatus::Presented => "presented",
        };
        let wakeup_str = match s.wakeup {
            NextWakeup::Now => "now".to_string(),
            NextWakeup::OnEvent => "on_event".to_string(),
            NextWakeup::At(t) => format!("at:{}", t.0),
        };
        let focus_key = self.focus().focused_key().map(|k| k.0).unwrap_or(0);
        let editing = self.focus().editing;

        let dmg_json = rects_to_json(&s.damage);
        let bands_json = rects_to_json(&s.band_rects);

        match self.engine() {
            SimEngine::Settings(e) => {
                let m = e.model();
                let scroll_y = e
                    .active_tree()
                    .node_by_key(liuli_settings::keys::SCROLL)
                    .map(|n| n.scroll_y)
                    .unwrap_or(0);
                format!(
                    "{{\"scenario\":\"settings\",\"page\":\"settings\",\"frame_id\":{},\"last_completed_frame_id\":{},\"now_ms\":{},\"paused\":{},\"rate_x\":{},\"status\":\"{}\",\"brightness\":{},\"sound\":{},\"dark_theme\":{},\"live_monitoring\":{},\"temp_deci_c\":{},\"load_pct\":0,\"samples\":0,\"toast\":\"\",\"modal_open\":{},\"focus_key\":{},\"editing\":{},\"scroll_y\":{},\"wakeup\":\"{}\",\"measured_nodes\":{},\"bands\":{},\"pixels_sent\":{},\"commands_replayed\":{},\"last_bands\":{},\"last_pixels\":{},\"damage\":{},\"band_rects\":{}}}",
                    s.frame_id,
                    s.last_completed_frame_id,
                    s.now_ms,
                    paused,
                    rate_x,
                    status_str,
                    m.brightness,
                    m.sound_enabled,
                    m.dark_theme,
                    m.live_monitoring,
                    m.temp_deci_c,
                    m.reset_modal_open,
                    focus_key,
                    editing,
                    scroll_y,
                    wakeup_str,
                    s.measured_nodes,
                    s.bands,
                    s.pixels_sent,
                    s.commands_replayed,
                    s.last_completed_metrics.bands,
                    s.last_completed_metrics.pixels_sent,
                    dmg_json,
                    bands_json,
                )
            }
            SimEngine::Terminal(e) => {
                let m = e.model();
                let page_str = match m.active_page {
                    PageId::Dashboard => "dashboard",
                    PageId::Settings => "settings",
                    PageId::Diagnostics => "diagnostics",
                };
                let scroll_y = e
                    .active_tree()
                    .node_by_key(m.active_page.scroll_key())
                    .map(|n| n.scroll_y)
                    .unwrap_or(0);
                let toast_str = m.toast.map(|t| t.text).unwrap_or("");
                format!(
                    "{{\"scenario\":\"terminal\",\"page\":\"{}\",\"frame_id\":{},\"last_completed_frame_id\":{},\"now_ms\":{},\"paused\":{},\"rate_x\":{},\"status\":\"{}\",\"brightness\":{},\"sound\":{},\"dark_theme\":{},\"live_monitoring\":{},\"temp_deci_c\":{},\"load_pct\":{},\"samples\":{},\"toast\":\"{}\",\"modal_open\":{},\"focus_key\":{},\"editing\":{},\"scroll_y\":{},\"wakeup\":\"{}\",\"measured_nodes\":{},\"bands\":{},\"pixels_sent\":{},\"commands_replayed\":{},\"last_bands\":{},\"last_pixels\":{},\"damage\":{},\"band_rects\":{}}}",
                    page_str,
                    s.frame_id,
                    s.last_completed_frame_id,
                    s.now_ms,
                    paused,
                    rate_x,
                    status_str,
                    m.settings.brightness,
                    m.settings.sound_enabled,
                    m.settings.dark_theme,
                    m.settings.live_monitoring,
                    m.settings.temp_deci_c,
                    m.load_pct,
                    m.temp_history.len(),
                    toast_str,
                    m.settings.reset_modal_open,
                    focus_key,
                    editing,
                    scroll_y,
                    wakeup_str,
                    s.measured_nodes,
                    s.bands,
                    s.pixels_sent,
                    s.commands_replayed,
                    s.last_completed_metrics.bands,
                    s.last_completed_metrics.pixels_sent,
                    dmg_json,
                    bands_json,
                )
            }
        }
    }
}
