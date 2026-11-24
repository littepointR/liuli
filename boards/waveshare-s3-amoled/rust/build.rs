use std::{env, path::PathBuf};
fn main() {
    if env::var_os("CARGO_FEATURE_BENCHMARK").is_none() {
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixtures = root.join("crates/liuli-enhanced/tests/fonts");
    let fixtures = if fixtures.is_dir() {
        fixtures
    } else {
        root.join("boards/waveshare-s3-amoled/bench-fonts")
    };
    for (name, file) in [("ARABIC", "Arabic.ttf"), ("INDIC", "Devanagari.ttf")] {
        let path = fixtures
            .join(file)
            .canonicalize()
            .expect("shaping fonts for board benchmark");
        println!("cargo:rerun-if-changed={}", path.display());
        println!("cargo:rustc-env=LIULI_{name}={}", path.display());
    }
}
