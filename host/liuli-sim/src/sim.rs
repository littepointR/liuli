use std::collections::VecDeque;

use liuli::core::focus::{FocusState, PointerState};
use liuli::core::node::RetainedTree;
use liuli::core::paint::CommandList;
use liuli::prelude::*;
use liuli::render::BandIter;
use liuli_settings::{
    PageId, SettingsApp, SettingsModel, TerminalApp, TerminalModel, TerminalMsg, app_fonts,
};

use crate::screen::{MemoryScreen, render_reference_fullscreen};

pub const MAX_PNG_HISTORY: usize = 16;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SimScenario {
    Settings,
    Terminal,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FrameLookupError {
    Expired,
    NotFound,
}

#[derive(Clone, Debug)]
pub struct FrameSnapshot {
    pub frame_id: u64,
    pub now_ms: u64,
    pub status: PresentStatus,
    pub damage: Vec<Rect>,
    pub band_rects: Vec<Rect>,
    pub bands: u32,
    pub pixels_sent: u32,
    pub measured_nodes: u32,
    pub commands_replayed: u32,
    pub wakeup: NextWakeup,
    pub last_completed_frame_id: u64,
    pub last_completed_metrics: FrameMetrics,
}

pub enum SimEngine<'s> {
    Settings(Engine<'s, 'static, SettingsApp>),
    Terminal(Engine<'s, 'static, TerminalApp>),
}

pub struct SimSession<'s> {
    scenario: SimScenario,
    engine: SimEngine<'s>,
    screen_rect: Rect,
    screen: MemoryScreen,
    scratch: [u16; DEFAULT_BAND_PIXELS],
    last_now: InstantMs,
    completed_frame_id: u64,
    oldest_retained_frame: u64,
    last_completed_metrics: FrameMetrics,
    png_history: VecDeque<(u64, Vec<u8>)>,
    snapshot: FrameSnapshot,
}

impl<'s> SimSession<'s> {
    pub fn new(
        scenario: SimScenario,
        storage: &'s mut Storage,
        start_now: InstantMs,
        verify_ref: bool,
    ) -> Result<Self, String> {
        let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
        let fonts = app_fonts();
        let mut engine = match scenario {
            SimScenario::Settings => SimEngine::Settings(
                Engine::<SettingsApp>::new_in_place_with_fonts(
                    storage,
                    SettingsModel::new(start_now),
                    fonts,
                    screen_rect,
                    start_now,
                )
                .map_err(|e| format!("SettingsApp init failed: {e:?}"))?,
            ),
            SimScenario::Terminal => SimEngine::Terminal(
                Engine::<TerminalApp>::new_in_place_with_fonts(
                    storage,
                    TerminalModel::new(start_now),
                    fonts,
                    screen_rect,
                    start_now,
                )
                .map_err(|e| format!("TerminalApp init failed: {e:?}"))?,
            ),
        };

        match &mut engine {
            SimEngine::Settings(e) => e.set_focus_ring_visible(true),
            SimEngine::Terminal(e) => e.set_focus_ring_visible(true),
        }

        let placeholder_snap = FrameSnapshot {
            frame_id: 0,
            now_ms: start_now.0,
            status: PresentStatus::Idle,
            damage: Vec::new(),
            band_rects: Vec::new(),
            bands: 0,
            pixels_sent: 0,
            measured_nodes: 0,
            commands_replayed: 0,
            wakeup: NextWakeup::OnEvent,
            last_completed_frame_id: 0,
            last_completed_metrics: FrameMetrics::default(),
        };

        let mut session = Self {
            scenario,
            engine,
            screen_rect,
            screen: MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT),
            scratch: [0u16; DEFAULT_BAND_PIXELS],
            last_now: start_now,
            completed_frame_id: 0,
            oldest_retained_frame: 1,
            last_completed_metrics: FrameMetrics::default(),
            png_history: VecDeque::with_capacity(MAX_PNG_HISTORY),
            snapshot: placeholder_snap,
        };
        session.present_and_record(start_now, verify_ref)?;
        Ok(session)
    }

    #[inline]
    pub const fn scenario(&self) -> SimScenario {
        self.scenario
    }

    #[inline]
    pub fn engine(&self) -> &SimEngine<'s> {
        &self.engine
    }

    #[inline]
    pub fn screen(&self) -> &MemoryScreen {
        &self.screen
    }

    #[inline]
    pub fn snapshot(&self) -> &FrameSnapshot {
        &self.snapshot
    }

    pub fn active_tree(&self) -> &RetainedTree {
        match &self.engine {
            SimEngine::Settings(e) => e.active_tree(),
            SimEngine::Terminal(e) => e.active_tree(),
        }
    }

    pub fn active_commands(&self) -> &CommandList {
        match &self.engine {
            SimEngine::Settings(e) => e.active_commands(),
            SimEngine::Terminal(e) => e.active_commands(),
        }
    }

    pub fn focus(&self) -> &FocusState {
        match &self.engine {
            SimEngine::Settings(e) => e.focus(),
            SimEngine::Terminal(e) => e.focus(),
        }
    }

    pub fn pointer(&self) -> &PointerState {
        match &self.engine {
            SimEngine::Settings(e) => e.pointer(),
            SimEngine::Terminal(e) => e.pointer(),
        }
    }

    pub fn damage(&self) -> &DamageSet {
        match &self.engine {
            SimEngine::Settings(e) => e.damage(),
            SimEngine::Terminal(e) => e.damage(),
        }
    }

    pub fn fonts(&self) -> &FontSet<'static> {
        match &self.engine {
            SimEngine::Settings(e) => e.fonts(),
            SimEngine::Terminal(e) => e.fonts(),
        }
    }

    pub fn band_height(&self) -> u32 {
        match &self.engine {
            SimEngine::Settings(e) => e.band_height(),
            SimEngine::Terminal(e) => e.band_height(),
        }
    }

    pub fn last_metrics(&self) -> FrameMetrics {
        match &self.engine {
            SimEngine::Settings(e) => e.last_metrics(),
            SimEngine::Terminal(e) => e.last_metrics(),
        }
    }

    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        match &self.engine {
            SimEngine::Settings(e) => e.next_wakeup(now, false),
            SimEngine::Terminal(e) => e.next_wakeup(now, false),
        }
    }

    /// Shared deterministic simulation step:
    /// 1. Advance virtual time (`advance`)
    /// 2. Dispatch optional input (`dispatch`)
    /// 3. Capture pre-present damage and band rectangles
    /// 4. Present synchronous bands (and verify against fullscreen reference if requested)
    /// 5. Save `FrameSnapshot`, PNG ring history, and next wakeup
    pub fn step(
        &mut self,
        target_now: InstantMs,
        input: Option<RawInput>,
        verify_ref: bool,
    ) -> Result<&FrameSnapshot, String> {
        if target_now < self.last_now {
            return Err(format!(
                "timestamp {} goes backwards before {}",
                target_now.0, self.last_now.0
            ));
        }
        self.last_now = target_now;

        match &mut self.engine {
            SimEngine::Settings(e) => {
                e.advance(target_now, false)
                    .map_err(|err| format!("advance error: {err:?}"))?;
                if let Some(inp) = input {
                    e.dispatch(inp, target_now, false)
                        .map_err(|err| format!("dispatch error: {err:?}"))?;
                }
            }
            SimEngine::Terminal(e) => {
                e.advance(target_now, false)
                    .map_err(|err| format!("advance error: {err:?}"))?;
                if let Some(inp) = input {
                    e.dispatch(inp, target_now, false)
                        .map_err(|err| format!("dispatch error: {err:?}"))?;
                }
            }
        }

        self.present_and_record(target_now, verify_ref)?;
        Ok(&self.snapshot)
    }

    pub fn select_terminal_page(
        &mut self,
        page: PageId,
        target_now: InstantMs,
        verify_ref: bool,
    ) -> Result<&FrameSnapshot, String> {
        if target_now < self.last_now {
            return Err(format!(
                "timestamp {} goes backwards before {}",
                target_now.0, self.last_now.0
            ));
        }
        self.last_now = target_now;
        match &mut self.engine {
            SimEngine::Terminal(e) => {
                e.advance(target_now, false)
                    .map_err(|err| format!("advance error: {err:?}"))?;
                e.message(TerminalMsg::SelectPage(page), target_now, false)
                    .map_err(|err| format!("select page failed: {err:?}"))?;
            }
            SimEngine::Settings(_) => {
                return Err("'page' op requires 'scenario terminal'".into());
            }
        }
        self.present_and_record(target_now, verify_ref)?;
        Ok(&self.snapshot)
    }

    fn present_and_record(&mut self, now: InstantMs, verify_ref: bool) -> Result<(), String> {
        let pre_damage: Vec<Rect> = self.damage().as_slice().to_vec();
        let band_rects: Vec<Rect> = if pre_damage.is_empty() {
            Vec::new()
        } else {
            BandIter::new::<()>(
                &pre_damage,
                self.screen_rect,
                self.band_height(),
                DEFAULT_BAND_PIXELS,
            )
            .map_err(|e| format!("BandIter error: {e:?}"))?
            .collect()
        };

        let (status, metrics) = match &mut self.engine {
            SimEngine::Settings(e) => {
                let st = e
                    .present_sync(&mut self.screen, &mut self.scratch)
                    .map_err(|err| format!("present_sync failed: {err:?}"))?;
                (st, e.last_metrics())
            }
            SimEngine::Terminal(e) => {
                let st = e
                    .present_sync(&mut self.screen, &mut self.scratch)
                    .map_err(|err| format!("present_sync failed: {err:?}"))?;
                (st, e.last_metrics())
            }
        };

        if status == PresentStatus::Presented {
            if verify_ref {
                let ref_scr = render_reference_fullscreen(
                    self.active_commands(),
                    self.active_tree(),
                    self.fonts(),
                );
                if let Some((x, y, got, exp)) = self.screen.diff_first_mismatch(&ref_scr) {
                    return Err(format!(
                        "band/incremental render diverged from full-screen reference at ({x},{y}): got 0x{got:04X}, expected 0x{exp:04X}"
                    ));
                }
            }

            self.completed_frame_id += 1;
            self.last_completed_metrics = metrics;
            let png = self.screen.to_png();
            if self.png_history.len() == MAX_PNG_HISTORY {
                self.png_history.pop_front();
            }
            self.png_history.push_back((self.completed_frame_id, png));
            self.oldest_retained_frame = self
                .png_history
                .front()
                .map(|(id, _)| *id)
                .unwrap_or(self.completed_frame_id + 1);

            if let SimEngine::Terminal(e) = &mut self.engine {
                let _ = e.try_with_model(|m| m.record_completed_frame(metrics, now));
            }
        }

        let wakeup = self.next_wakeup(now);
        self.snapshot = FrameSnapshot {
            frame_id: self.completed_frame_id,
            now_ms: now.0,
            status,
            damage: pre_damage,
            band_rects,
            bands: metrics.bands,
            pixels_sent: metrics.pixels_sent,
            measured_nodes: metrics.measured_nodes,
            commands_replayed: metrics.commands_replayed,
            wakeup,
            last_completed_frame_id: self.completed_frame_id,
            last_completed_metrics: self.last_completed_metrics,
        };
        Ok(())
    }

    pub fn png_for_frame(&self, frame_id: Option<u64>) -> Result<(u64, &[u8]), FrameLookupError> {
        match frame_id {
            None => self
                .png_history
                .back()
                .map(|(id, bytes)| (*id, bytes.as_slice()))
                .ok_or(FrameLookupError::NotFound),
            Some(id) => {
                if let Some((_, bytes)) = self.png_history.iter().find(|(fid, _)| *fid == id) {
                    Ok((id, bytes.as_slice()))
                } else if id > 0 && id < self.oldest_retained_frame {
                    Err(FrameLookupError::Expired)
                } else {
                    Err(FrameLookupError::NotFound)
                }
            }
        }
    }

    pub fn state_json(&self, paused: bool, rate_x: u32) -> String {
        let s = &self.snapshot;
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

        match &self.engine {
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

fn rects_to_json(rects: &[Rect]) -> String {
    let mut out = String::from("[");
    for (i, r) in rects.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            r.x, r.y, r.w, r.h
        ));
    }
    out.push(']');
    out
}
