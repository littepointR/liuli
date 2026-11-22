use std::fs;
use std::path::PathBuf;

use liuli::prelude::*;
use liuli_settings::{SettingsApp, SettingsModel, settings_font};
use liuli_sim::screen::MemoryScreen;

fn main() {
    let font = settings_font();
    let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    let mut engine = Engine::<SettingsApp>::new_in_place(
        &mut storage,
        SettingsModel::new(InstantMs(0)),
        font,
        screen_rect,
        InstantMs(0),
    )
    .expect("init settings app");
    engine.set_focus_ring_visible(true);
    engine
        .present_sync(&mut screen, &mut scratch)
        .expect("present");

    let out_dir = PathBuf::from("target/liuli-demo");
    fs::create_dir_all(&out_dir).expect("create target/liuli-demo");
    let ppm_path = out_dir.join("initial.ppm");
    fs::write(&ppm_path, screen.to_ppm()).expect("write initial.ppm");
    println!(
        "Wrote {} ({}x{}, {} bands, {} px)",
        ppm_path.display(),
        screen.width(),
        screen.height(),
        engine.last_metrics().bands,
        engine.last_metrics().pixels_sent
    );
}
