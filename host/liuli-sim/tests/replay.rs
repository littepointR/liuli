use std::path::PathBuf;

use liuli_sim::replay::{ImageFormat, run_replay_file, run_replay_str};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn replay_settings_fixture_passes_and_captures_all_five_shots() {
    let report = run_replay_file(&fixture_path("settings.events"), None, ImageFormat::Ppm)
        .expect("settings.events replay failed");
    let shot_names: Vec<&str> = report.shots.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        shot_names,
        vec!["initial", "dark", "scroll", "updated", "modal"]
    );
}

#[test]
fn replay_drag_fixture_passes() {
    run_replay_file(&fixture_path("drag.events"), None, ImageFormat::Ppm)
        .expect("drag.events replay failed");
}

#[test]
fn replay_modal_fixture_passes() {
    run_replay_file(&fixture_path("modal.events"), None, ImageFormat::Ppm)
        .expect("modal.events replay failed");
}

#[test]
fn replay_background_fixture_passes() {
    run_replay_file(&fixture_path("background.events"), None, ImageFormat::Ppm)
        .expect("background.events replay failed");
}

#[test]
fn replay_idle_fixture_passes() {
    run_replay_file(&fixture_path("idle.events"), None, ImageFormat::Ppm)
        .expect("idle.events replay failed");
}

#[test]
fn replay_reports_line_numbers_on_invalid_scripts() {
    let backwards = "100 advance\n50 advance\n";
    let err = run_replay_str(backwards, None, ImageFormat::Ppm).unwrap_err();
    assert!(err.contains("line 2"), "unexpected error: {err}");
    assert!(err.contains("backwards"), "unexpected error: {err}");

    let unknown = "10 bogus\n";
    let err = run_replay_str(unknown, None, ImageFormat::Ppm).unwrap_err();
    assert!(err.contains("line 1"), "unexpected error: {err}");
    assert!(err.contains("unknown op"), "unexpected error: {err}");

    let bad_argc = "10 down 1 2\n";
    let err = run_replay_str(bad_argc, None, ImageFormat::Ppm).unwrap_err();
    assert!(err.contains("line 1"), "unexpected error: {err}");
    assert!(err.contains("expects 3 args"), "unexpected error: {err}");

    let bad_expect = "0 expect brightness 999\n";
    let err = run_replay_str(bad_expect, None, ImageFormat::Ppm).unwrap_err();
    assert!(err.contains("line 1"), "unexpected error: {err}");
    assert!(
        err.contains("expect brightness 999"),
        "unexpected error: {err}"
    );
}
