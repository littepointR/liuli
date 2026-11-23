use std::collections::HashSet;
use std::fs;
use std::path::Path;

use liuli::core::focus::CaptureKind;
use liuli::prelude::*;
use liuli_settings::{PageId, keys, terminal_keys as tk};

use crate::clock::VirtualClock;
use crate::screen::MemoryScreen;
use crate::sim::{SimEngine, SimScenario, SimSession};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImageFormat {
    Ppm,
    Png,
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub name: String,
    pub screen: MemoryScreen,
}

#[derive(Clone, Debug)]
pub struct ReplayReport {
    pub scenario: SimScenario,
    pub shots: Vec<Shot>,
    pub final_now: InstantMs,
    pub final_wakeup: NextWakeup,
    pub frames_presented: usize,
    pub last_metrics: FrameMetrics,
}

pub fn run_replay_file(
    path: &Path,
    out_dir: Option<&Path>,
    format: ImageFormat,
) -> Result<ReplayReport, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    run_replay_str(&text, out_dir, format)
}

pub fn run_replay_str(
    script: &str,
    out_dir: Option<&Path>,
    format: ImageFormat,
) -> Result<ReplayReport, String> {
    if let Some(dir) = out_dir {
        fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create output dir {}: {e}", dir.display()))?;
    }

    let mut parsed_lines: Vec<(usize, &str)> = Vec::new();
    let mut scenario = SimScenario::Settings;
    let mut scenario_line_seen = false;

    for (idx, raw_line) in script.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if parsed_lines.is_empty() && !scenario_line_seen {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.first() == Some(&"scenario") {
                if tokens.len() != 2 {
                    return Err(format!(
                        "line {line_no}: 'scenario' expects 1 arg (terminal|settings), got {}",
                        tokens.len().saturating_sub(1)
                    ));
                }
                scenario = match tokens[1] {
                    "terminal" => SimScenario::Terminal,
                    "settings" => SimScenario::Settings,
                    other => {
                        return Err(format!(
                            "line {line_no}: unknown scenario '{other}' (expected terminal or settings)"
                        ));
                    }
                };
                scenario_line_seen = true;
                continue;
            }
        }
        parsed_lines.push((line_no, line));
    }

    let mut storage = Box::new(Storage::new());
    let mut session = SimSession::new(scenario, &mut storage, InstantMs(0), true)?;
    let mut clock = VirtualClock::new(InstantMs(0));

    let mut shots = Vec::new();
    let mut seen_shots: HashSet<String> = HashSet::new();
    let mut frames_presented = 1usize; // initial frame presented inside SimSession::new
    let mut last_step_time = InstantMs(0);

    for (line_no, line) in parsed_lines {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.first() == Some(&"scenario") {
            return Err(format!(
                "line {line_no}: 'scenario' directive must appear before any events"
            ));
        }
        if tokens.len() < 2 {
            return Err(format!(
                "line {line_no}: expected '<time_ms> <op> ...', got '{line}'"
            ));
        }

        let time_ms: u64 = tokens[0]
            .parse()
            .map_err(|e| format!("line {line_no}: invalid timestamp '{}': {e}", tokens[0]))?;
        let target_now = InstantMs(time_ms);
        if clock.advance_to(target_now).is_err() {
            return Err(format!(
                "line {line_no} (t={time_ms} ms): timestamp {time_ms} goes backwards before {}",
                clock.now().0
            ));
        }

        let op = tokens[1];
        let args = &tokens[2..];

        match op {
            "next" => {
                require_argc(line_no, time_ms, op, args, 0)?;
                let snap = session
                    .step(target_now, Some(RawInput::Nav(NavDir::Next)), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "prev" => {
                require_argc(line_no, time_ms, op, args, 0)?;
                let snap = session
                    .step(target_now, Some(RawInput::Nav(NavDir::Prev)), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "activate" => {
                require_argc(line_no, time_ms, op, args, 0)?;
                let snap = session
                    .step(target_now, Some(RawInput::Activate), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "adjust" => {
                require_argc(line_no, time_ms, op, args, 1)?;
                let n: i16 = args[0].parse().map_err(|e| {
                    format!("line {line_no} (t={time_ms} ms, op={op}): bad adjust value: {e}")
                })?;
                let snap = session
                    .step(target_now, Some(RawInput::Adjust(n)), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "back" => {
                require_argc(line_no, time_ms, op, args, 0)?;
                let snap = session
                    .step(target_now, Some(RawInput::Back), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "key_down" | "key_up" => {
                require_argc(line_no, time_ms, op, args, 1)?;
                let hk = match args[0] {
                    "activate" => HoldKey::Activate,
                    "adjust_prev" => HoldKey::AdjustPrev,
                    "adjust_next" => HoldKey::AdjustNext,
                    other => {
                        return Err(format!(
                            "line {line_no} (t={time_ms} ms, op={op}): unknown hold key '{other}'"
                        ));
                    }
                };
                let inp = if op == "key_down" {
                    RawInput::KeyDown(hk)
                } else {
                    RawInput::KeyUp(hk)
                };
                let snap = session
                    .step(target_now, Some(inp), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "down" | "move" | "up" => {
                require_argc(line_no, time_ms, op, args, 3)?;
                let id: u8 = args[0].parse().map_err(|e| {
                    format!("line {line_no} (t={time_ms} ms, op={op}): bad pointer id: {e}")
                })?;
                let x: i32 = args[1]
                    .parse()
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): bad x: {e}"))?;
                let y: i32 = args[2]
                    .parse()
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): bad y: {e}"))?;
                let pos = Point::new(x, y);
                let input = match op {
                    "down" => RawInput::PointerDown {
                        id: PointerId(id),
                        pos,
                    },
                    "move" => RawInput::PointerMove {
                        id: PointerId(id),
                        pos,
                    },
                    _ => RawInput::PointerUp {
                        id: PointerId(id),
                        pos,
                    },
                };
                let snap = session
                    .step(target_now, Some(input), true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "cancel" => {
                require_argc(line_no, time_ms, op, args, 1)?;
                let id: u8 = args[0].parse().map_err(|e| {
                    format!("line {line_no} (t={time_ms} ms, op={op}): bad pointer id: {e}")
                })?;
                let snap = session
                    .step(
                        target_now,
                        Some(RawInput::PointerCancel { id: PointerId(id) }),
                        true,
                    )
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "page" => {
                require_argc(line_no, time_ms, op, args, 1)?;
                let page = match args[0] {
                    "dashboard" => PageId::Dashboard,
                    "settings" => PageId::Settings,
                    "diagnostics" => PageId::Diagnostics,
                    other => {
                        return Err(format!(
                            "line {line_no} (t={time_ms} ms, op={op}): unknown page '{other}'"
                        ));
                    }
                };
                let snap = session
                    .select_terminal_page(page, target_now, true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "advance" => {
                require_argc(line_no, time_ms, op, args, 0)?;
                let snap = session
                    .step(target_now, None, true)
                    .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                if snap.status == PresentStatus::Presented {
                    frames_presented += 1;
                }
            }
            "shot" => {
                require_argc(line_no, time_ms, op, args, 1)?;
                let name = args[0].to_string();
                validate_shot_name(line_no, time_ms, &name)?;
                if !seen_shots.insert(name.clone()) {
                    return Err(format!(
                        "line {line_no} (t={time_ms} ms, op=shot): duplicate shot name '{name}'"
                    ));
                }
                if target_now > last_step_time || !session.damage().is_empty() {
                    let snap = session
                        .step(target_now, None, true)
                        .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                    if snap.status == PresentStatus::Presented {
                        frames_presented += 1;
                    }
                }
                if let Some(dir) = out_dir {
                    let (ext, bytes) = match format {
                        ImageFormat::Ppm => ("ppm", session.screen().to_ppm()),
                        ImageFormat::Png => ("png", session.screen().to_png()),
                    };
                    let out_path = dir.join(format!("{name}.{ext}"));
                    fs::write(&out_path, bytes).map_err(|e| {
                        format!(
                            "line {line_no} (t={time_ms} ms, op=shot): failed to write {}: {e}",
                            out_path.display()
                        )
                    })?;
                }
                shots.push(Shot {
                    name,
                    screen: session.screen().clone(),
                });
            }
            "expect" => {
                require_argc(line_no, time_ms, op, args, 2)?;
                if target_now > last_step_time || !session.damage().is_empty() {
                    let snap = session
                        .step(target_now, None, true)
                        .map_err(|e| format!("line {line_no} (t={time_ms} ms, op={op}): {e}"))?;
                    if snap.status == PresentStatus::Presented {
                        frames_presented += 1;
                    }
                }
                check_expectation(line_no, time_ms, &session, target_now, args[0], args[1])?;
            }
            unknown => {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms): unknown op '{unknown}'"
                ));
            }
        }

        last_step_time = target_now;
    }

    let final_now = clock.now();
    let final_wakeup = session.next_wakeup(final_now);
    let last_metrics = session.last_metrics();

    Ok(ReplayReport {
        scenario,
        shots,
        final_now,
        final_wakeup,
        frames_presented,
        last_metrics,
    })
}

fn validate_shot_name(line_no: usize, time_ms: u64, name: &str) -> Result<(), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(format!(
            "line {line_no} (t={time_ms} ms, op=shot): invalid single-segment shot name '{name}'; only [A-Za-z0-9_-] allowed"
        ));
    }
    Ok(())
}

fn require_argc(
    line_no: usize,
    time_ms: u64,
    op: &str,
    args: &[&str],
    expected: usize,
) -> Result<(), String> {
    if args.len() != expected {
        return Err(format!(
            "line {line_no} (t={time_ms} ms, op={op}): '{op}' expects {expected} args, got {}",
            args.len()
        ));
    }
    Ok(())
}

fn focus_key_label(key: Option<Key>) -> String {
    match key {
        None => "none".to_string(),
        Some(k) if k == keys::SLIDER_BRIGHTNESS || k == tk::SET_SLIDER_BRIGHTNESS => {
            "brightness".to_string()
        }
        Some(k) if k == keys::TOGGLE_SOUND || k == tk::SET_TOGGLE_SOUND => "sound".to_string(),
        Some(k) if k == keys::TOGGLE_THEME || k == tk::SET_TOGGLE_THEME => "theme".to_string(),
        Some(k) if k == keys::TOGGLE_MONITOR || k == tk::SET_TOGGLE_MONITOR => {
            "monitor".to_string()
        }
        Some(k) if k == keys::BTN_RESET || k == tk::SET_BTN_RESET => "reset".to_string(),
        Some(k) if k == keys::BTN_CANCEL || k == tk::BTN_CANCEL => "cancel".to_string(),
        Some(k) if k == keys::BTN_CONFIRM || k == tk::BTN_CONFIRM => "confirm".to_string(),
        Some(k) if k == tk::TAB_DASHBOARD => "tab_dashboard".to_string(),
        Some(k) if k == tk::TAB_SETTINGS => "tab_settings".to_string(),
        Some(k) if k == tk::TAB_DIAGNOSTICS => "tab_diagnostics".to_string(),
        Some(k) if k == tk::DASH_BTN_PAUSE => "dash_pause".to_string(),
        Some(k) if k == tk::DASH_BTN_DEC => "dash_dec".to_string(),
        Some(k) if k == tk::DASH_BTN_INC => "dash_inc".to_string(),
        Some(k) if k == tk::SET_BTN_BRIGHT_DEC => "bright_dec".to_string(),
        Some(k) if k == tk::SET_BTN_BRIGHT_INC => "bright_inc".to_string(),
        Some(k) if k == tk::DIAG_BTN_REFRESH => "diag_refresh".to_string(),
        Some(k) if k == tk::DIAG_BTN_MONITOR => "diag_monitor".to_string(),
        Some(Key(n)) => n.to_string(),
    }
}

fn check_expectation(
    line_no: usize,
    time_ms: u64,
    session: &SimSession<'_>,
    now: InstantMs,
    field: &str,
    expected: &str,
) -> Result<(), String> {
    let settings = match session.engine() {
        SimEngine::Settings(e) => e.model(),
        SimEngine::Terminal(e) => &e.model().settings,
    };
    let scroll_key = match session.engine() {
        SimEngine::Settings(_) => keys::SCROLL,
        SimEngine::Terminal(e) => e.model().active_page.scroll_key(),
    };
    let snap = session.snapshot();

    match field {
        "focus" => {
            let actual = focus_key_label(session.focus().focused_key());
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect focus '{expected}', got '{actual}'"
                ));
            }
        }
        "ring" => {
            let actual = session.focus().ring_visible.to_string();
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect ring '{expected}', got '{actual}'"
                ));
            }
        }
        "editing" => {
            let actual = session.focus().editing.to_string();
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect editing '{expected}', got '{actual}'"
                ));
            }
        }
        "modal" => {
            let actual = if settings.reset_modal_open {
                "open"
            } else {
                "closed"
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect modal '{expected}', got '{actual}'"
                ));
            }
        }
        "capture" => {
            let actual = match session.pointer().capture {
                CaptureKind::None => "none",
                CaptureKind::Button(_) => "button",
                CaptureKind::Slider { .. } => "slider",
                CaptureKind::ScrollCandidate { .. } => "candidate",
                CaptureKind::Scrolling { .. } => "scrolling",
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect capture '{expected}', got '{actual}'"
                ));
            }
        }
        "scroll" => {
            let actual = session
                .active_tree()
                .node_by_key(scroll_key)
                .map(|n| n.scroll_y)
                .unwrap_or(0);
            let exp: i32 = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad scroll integer: {e}")
            })?;
            if actual != exp {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect scroll {exp}, got {actual}"
                ));
            }
        }
        "brightness" => {
            let exp: i16 = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad brightness integer: {e}")
            })?;
            if settings.brightness != exp {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect brightness {exp}, got {}",
                    settings.brightness
                ));
            }
        }
        "sound" => {
            let actual = if settings.sound_enabled { "on" } else { "off" };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect sound '{expected}', got '{actual}'"
                ));
            }
        }
        "theme" => {
            let actual = if settings.dark_theme { "dark" } else { "light" };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect theme '{expected}', got '{actual}'"
                ));
            }
        }
        "monitoring" => {
            let actual = if settings.live_monitoring {
                "on"
            } else {
                "off"
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect monitoring '{expected}', got '{actual}'"
                ));
            }
        }
        "temp" => {
            let exp_deci: i16 = if let Some((w, f)) = expected.split_once('.') {
                let wi: i16 = w.parse().map_err(|e| {
                    format!(
                        "line {line_no} (t={time_ms} ms, op=expect): bad temp '{expected}': {e}"
                    )
                })?;
                let fi: i16 = f.parse().map_err(|e| {
                    format!(
                        "line {line_no} (t={time_ms} ms, op=expect): bad temp '{expected}': {e}"
                    )
                })?;
                wi * 10 + fi
            } else {
                expected.parse().map_err(|e| {
                    format!(
                        "line {line_no} (t={time_ms} ms, op=expect): bad temp '{expected}': {e}"
                    )
                })?
            };
            if settings.temp_deci_c != exp_deci {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect temp {exp_deci}, got {}",
                    settings.temp_deci_c
                ));
            }
        }
        "wakeup" => {
            let actual = match session.next_wakeup(now) {
                NextWakeup::Now => "now".to_string(),
                NextWakeup::OnEvent => "on_event".to_string(),
                NextWakeup::At(t) => format!("at:{}", t.0),
            };
            let ok = if expected == "at" {
                actual.starts_with("at:")
            } else {
                actual == expected
            };
            if !ok {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect wakeup '{expected}', got '{actual}'"
                ));
            }
        }
        "page" => {
            let actual = match session.engine() {
                SimEngine::Settings(_) => "settings",
                SimEngine::Terminal(e) => match e.model().active_page {
                    PageId::Dashboard => "dashboard",
                    PageId::Settings => "settings",
                    PageId::Diagnostics => "diagnostics",
                },
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect page '{expected}', got '{actual}'"
                ));
            }
        }
        "toast" => {
            let actual = match session.engine() {
                SimEngine::Settings(_) => "none",
                SimEngine::Terminal(e) => e.model().toast.map(|t| t.text).unwrap_or("none"),
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect toast '{expected}', got '{actual}'"
                ));
            }
        }
        "samples" => {
            let actual = match session.engine() {
                SimEngine::Settings(_) => 0usize,
                SimEngine::Terminal(e) => e.model().temp_history.len(),
            };
            let exp: usize = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad samples integer: {e}")
            })?;
            if actual != exp {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect samples {exp}, got {actual}"
                ));
            }
        }
        "load" => {
            let actual = match session.engine() {
                SimEngine::Settings(_) => 0i16,
                SimEngine::Terminal(e) => e.model().load_pct,
            };
            let exp: i16 = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad load integer: {e}")
            })?;
            if actual != exp {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect load {exp}, got {actual}"
                ));
            }
        }
        "measured" => {
            let exp: u32 = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad measured integer: {e}")
            })?;
            if snap.measured_nodes != exp {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): expect measured {exp}, got {}",
                    snap.measured_nodes
                ));
            }
        }
        "bands_le" => {
            let limit: u32 = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad bands_le integer: {e}")
            })?;
            if snap.bands > limit {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): bands {} exceeded limit {limit}",
                    snap.bands
                ));
            }
        }
        "pixels_le" => {
            let limit: u32 = expected.parse().map_err(|e| {
                format!("line {line_no} (t={time_ms} ms, op=expect): bad pixels_le integer: {e}")
            })?;
            if snap.pixels_sent > limit {
                return Err(format!(
                    "line {line_no} (t={time_ms} ms, op=expect): pixels_sent {} exceeded limit {limit}",
                    snap.pixels_sent
                ));
            }
        }
        other => {
            return Err(format!(
                "line {line_no} (t={time_ms} ms, op=expect): unknown expect field '{other}'"
            ));
        }
    }
    Ok(())
}
