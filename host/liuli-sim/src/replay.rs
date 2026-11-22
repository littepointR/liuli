use std::fs;
use std::path::Path;

use liuli::core::focus::CaptureKind;
use liuli::prelude::*;
use liuli_settings::{SettingsApp, SettingsModel, keys, settings_font};

use crate::clock::VirtualClock;
use crate::screen::{MemoryScreen, render_reference_fullscreen};

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

    let font = settings_font();
    let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut clock = VirtualClock::new(InstantMs(0));
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    let mut engine = Engine::<SettingsApp>::new_in_place(
        &mut storage,
        SettingsModel::new(InstantMs(0)),
        font,
        screen_rect,
        InstantMs(0),
    )
    .map_err(|e| format!("engine init failed: {e:?}"))?;

    // Standard replay starts in keyboard/knob navigation mode with visible focus ring.
    engine.set_focus_ring_visible(true);

    let mut shots = Vec::new();
    let mut frames_presented = 0usize;

    let present_and_check = |eng: &mut Engine<'_, '_, SettingsApp>,
                             scr: &mut MemoryScreen,
                             scratch_buf: &mut [u16],
                             frames: &mut usize|
     -> Result<(), String> {
        let st = eng
            .present_sync(scr, scratch_buf)
            .map_err(|e| format!("present_sync failed: {e:?}"))?;
        if st == PresentStatus::Presented {
            *frames += 1;
            let ref_scr =
                render_reference_fullscreen(eng.active_commands(), eng.active_tree(), eng.font());
            if let Some((x, y, got, exp)) = scr.diff_first_mismatch(&ref_scr) {
                return Err(format!(
                    "band/incremental render diverged from full-screen reference at ({x},{y}): got 0x{got:04X}, expected 0x{exp:04X}"
                ));
            }
        }
        Ok(())
    };

    present_and_check(
        &mut engine,
        &mut screen,
        &mut scratch,
        &mut frames_presented,
    )?;

    for (idx, raw_line) in script.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
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
                "line {line_no}: timestamp {time_ms} goes backwards before {}",
                clock.now().0
            ));
        }

        // Advance timers/animations up to target_now before dispatching the event at target_now.
        let _ = engine
            .advance(target_now, false)
            .map_err(|e| format!("line {line_no}: advance error: {e:?}"))?;

        let op = tokens[1];
        let args = &tokens[2..];

        match op {
            "next" => {
                require_argc(line_no, op, args, 0)?;
                engine
                    .dispatch(RawInput::Nav(NavDir::Next), target_now, false)
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "prev" => {
                require_argc(line_no, op, args, 0)?;
                engine
                    .dispatch(RawInput::Nav(NavDir::Prev), target_now, false)
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "activate" => {
                require_argc(line_no, op, args, 0)?;
                engine
                    .dispatch(RawInput::Activate, target_now, false)
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "adjust" => {
                require_argc(line_no, op, args, 1)?;
                let n: i16 = args[0]
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad adjust value: {e}"))?;
                engine
                    .dispatch(RawInput::Adjust(n), target_now, false)
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "back" => {
                require_argc(line_no, op, args, 0)?;
                engine
                    .dispatch(RawInput::Back, target_now, false)
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "down" | "move" | "up" => {
                require_argc(line_no, op, args, 3)?;
                let id: u8 = args[0]
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad pointer id: {e}"))?;
                let x: i32 = args[1]
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad x: {e}"))?;
                let y: i32 = args[2]
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad y: {e}"))?;
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
                engine
                    .dispatch(input, target_now, false)
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "cancel" => {
                require_argc(line_no, op, args, 1)?;
                let id: u8 = args[0]
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad pointer id: {e}"))?;
                engine
                    .dispatch(
                        RawInput::PointerCancel { id: PointerId(id) },
                        target_now,
                        false,
                    )
                    .map_err(|e| format!("line {line_no}: dispatch failed: {e:?}"))?;
            }
            "advance" => {
                require_argc(line_no, op, args, 0)?;
            }
            "shot" => {
                require_argc(line_no, op, args, 1)?;
                present_and_check(
                    &mut engine,
                    &mut screen,
                    &mut scratch,
                    &mut frames_presented,
                )?;
                let name = args[0].to_string();
                if let Some(dir) = out_dir {
                    let (ext, bytes) = match format {
                        ImageFormat::Ppm => ("ppm", screen.to_ppm()),
                        ImageFormat::Png => ("png", screen.to_png()),
                    };
                    let out_path = dir.join(format!("{name}.{ext}"));
                    fs::write(&out_path, bytes).map_err(|e| {
                        format!(
                            "line {line_no}: failed to write {}: {e}",
                            out_path.display()
                        )
                    })?;
                }
                shots.push(Shot {
                    name,
                    screen: screen.clone(),
                });
                continue;
            }
            "expect" => {
                require_argc(line_no, op, args, 2)?;
                present_and_check(
                    &mut engine,
                    &mut screen,
                    &mut scratch,
                    &mut frames_presented,
                )?;
                check_expectation(line_no, &engine, target_now, args[0], args[1])?;
                continue;
            }
            unknown => {
                return Err(format!("line {line_no}: unknown op '{unknown}'"));
            }
        }

        present_and_check(
            &mut engine,
            &mut screen,
            &mut scratch,
            &mut frames_presented,
        )?;
    }

    let final_now = clock.now();
    let final_wakeup = engine.next_wakeup(final_now, false);
    let last_metrics = engine.last_metrics();

    Ok(ReplayReport {
        shots,
        final_now,
        final_wakeup,
        frames_presented,
        last_metrics,
    })
}

fn require_argc(line_no: usize, op: &str, args: &[&str], expected: usize) -> Result<(), String> {
    if args.len() != expected {
        return Err(format!(
            "line {line_no}: '{op}' expects {expected} args, got {}",
            args.len()
        ));
    }
    Ok(())
}

fn check_expectation(
    line_no: usize,
    engine: &Engine<'_, '_, SettingsApp>,
    now: InstantMs,
    field: &str,
    expected: &str,
) -> Result<(), String> {
    let model = engine.model();
    match field {
        "focus" => {
            let actual = match engine.focus().focused_key {
                None => "none".to_string(),
                Some(k) if k == keys::SLIDER_BRIGHTNESS => "brightness".to_string(),
                Some(k) if k == keys::TOGGLE_SOUND => "sound".to_string(),
                Some(k) if k == keys::TOGGLE_THEME => "theme".to_string(),
                Some(k) if k == keys::TOGGLE_MONITOR => "monitor".to_string(),
                Some(k) if k == keys::BTN_RESET => "reset".to_string(),
                Some(k) if k == keys::BTN_CANCEL => "cancel".to_string(),
                Some(k) if k == keys::BTN_CONFIRM => "confirm".to_string(),
                Some(Key(n)) => n.to_string(),
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect focus '{expected}', got '{actual}'"
                ));
            }
        }
        "ring" => {
            let actual = engine.focus().ring_visible.to_string();
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect ring '{expected}', got '{actual}'"
                ));
            }
        }
        "editing" => {
            let actual = engine.focus().editing.to_string();
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect editing '{expected}', got '{actual}'"
                ));
            }
        }
        "modal" => {
            let actual = if model.reset_modal_open {
                "open"
            } else {
                "closed"
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect modal '{expected}', got '{actual}'"
                ));
            }
        }
        "capture" => {
            let actual = match engine.pointer().capture {
                CaptureKind::None => "none",
                CaptureKind::Button(_) => "button",
                CaptureKind::Slider { .. } => "slider",
                CaptureKind::ScrollCandidate { .. } => "candidate",
                CaptureKind::Scrolling { .. } => "scrolling",
            };
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect capture '{expected}', got '{actual}'"
                ));
            }
        }
        "scroll" => {
            let actual = engine
                .active_tree()
                .node_by_key(keys::SCROLL)
                .map(|n| n.scroll_y)
                .unwrap_or(0);
            let exp: i32 = expected
                .parse()
                .map_err(|e| format!("line {line_no}: bad scroll integer: {e}"))?;
            if actual != exp {
                return Err(format!("line {line_no}: expect scroll {exp}, got {actual}"));
            }
        }
        "brightness" => {
            let exp: i16 = expected
                .parse()
                .map_err(|e| format!("line {line_no}: bad brightness integer: {e}"))?;
            if model.brightness != exp {
                return Err(format!(
                    "line {line_no}: expect brightness {exp}, got {}",
                    model.brightness
                ));
            }
        }
        "sound" => {
            let actual = if model.sound_enabled { "on" } else { "off" };
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect sound '{expected}', got '{actual}'"
                ));
            }
        }
        "theme" => {
            let actual = if model.dark_theme { "dark" } else { "light" };
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect theme '{expected}', got '{actual}'"
                ));
            }
        }
        "monitoring" => {
            let actual = if model.live_monitoring { "on" } else { "off" };
            if actual != expected {
                return Err(format!(
                    "line {line_no}: expect monitoring '{expected}', got '{actual}'"
                ));
            }
        }
        "temp" => {
            let exp_deci: i16 = if let Some((w, f)) = expected.split_once('.') {
                let wi: i16 = w
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad temp '{expected}': {e}"))?;
                let fi: i16 = f
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad temp '{expected}': {e}"))?;
                wi * 10 + fi
            } else {
                expected
                    .parse()
                    .map_err(|e| format!("line {line_no}: bad temp '{expected}': {e}"))?
            };
            if model.temp_deci_c != exp_deci {
                return Err(format!(
                    "line {line_no}: expect temp {exp_deci}, got {}",
                    model.temp_deci_c
                ));
            }
        }
        "wakeup" => {
            let actual = match engine.next_wakeup(now, false) {
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
                    "line {line_no}: expect wakeup '{expected}', got '{actual}'"
                ));
            }
        }
        other => {
            return Err(format!("line {line_no}: unknown expect field '{other}'"));
        }
    }
    Ok(())
}
