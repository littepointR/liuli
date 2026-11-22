use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    for file in ["memory.x", "link.x"] {
        let content = fs::read(file).unwrap_or_else(|e| panic!("failed to read {file}: {e}"));
        fs::write(out.join(file), content)
            .unwrap_or_else(|e| panic!("failed to write {file}: {e}"));
        println!("cargo:rerun-if-changed={file}");
    }
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
}
