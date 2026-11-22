use std::fs;
use std::path::PathBuf;

use liuli_sim::image::{decode_png_rgb8, decode_ppm};
use liuli_sim::replay::{ImageFormat, run_replay_file};

fn sim_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

#[test]
fn settings_replay_matches_five_golden_ppms_and_png_roundtrip() {
    let fixture = sim_path("tests/fixtures/settings.events");
    let report = run_replay_file(&fixture, None, ImageFormat::Ppm).expect("settings replay failed");

    let expected_names = ["initial", "dark", "scroll", "updated", "modal"];
    let got_names: Vec<&str> = report.shots.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(got_names, expected_names);

    for shot in &report.shots {
        let golden_path = sim_path(&format!("tests/golden/{}.ppm", shot.name));
        let golden_bytes = fs::read(&golden_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", golden_path.display()));
        let (gw, gh, golden_rgb) = decode_ppm(&golden_bytes)
            .unwrap_or_else(|e| panic!("invalid golden PPM {}: {e}", golden_path.display()));

        let ppm_bytes = shot.screen.to_ppm();
        let (pw, ph, ppm_rgb) = decode_ppm(&ppm_bytes).expect("decode generated PPM");
        assert_eq!(
            (pw, ph),
            (gw, gh),
            "dimension mismatch on shot '{}'",
            shot.name
        );

        for y in 0..gh {
            for x in 0..gw {
                let idx = ((y as usize) * (gw as usize) + (x as usize)) * 3;
                let actual = &ppm_rgb[idx..idx + 3];
                let expected = &golden_rgb[idx..idx + 3];
                if actual != expected {
                    panic!(
                        "golden PPM mismatch on shot '{}' at ({x}, {y}): got RGB({},{},{}), expected RGB({},{},{})",
                        shot.name,
                        actual[0],
                        actual[1],
                        actual[2],
                        expected[0],
                        expected[1],
                        expected[2],
                    );
                }
            }
        }

        // Verify RGB8 PNG encoding/decoding produces identical pixels to P6 PPM
        let png_bytes = shot.screen.to_png();
        let (nw, nh, png_rgb) = decode_png_rgb8(&png_bytes).expect("decode generated PNG");
        assert_eq!((nw, nh), (pw, ph));
        assert_eq!(
            png_rgb, ppm_rgb,
            "PNG decoded pixels differ from PPM on shot '{}'",
            shot.name
        );
    }
}
