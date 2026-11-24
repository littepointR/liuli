use std::{env, path::Path, process::Command};

fn main() {
    let st = env::var_os("CARGO_FEATURE_ST_HAL").is_some();
    let embassy = env::var_os("CARGO_FEATURE_EMBASSY").is_some();
    assert!(
        st != embassy,
        "select exactly one board backend: st-hal or embassy"
    );
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = env::var("OUT_DIR").unwrap();
    println!("cargo:rerun-if-changed=st");
    println!("cargo:rerun-if-env-changed=FK743_CLANG");
    println!("cargo:rerun-if-env-changed=PYTHON");
    let python = env::var_os("PYTHON")
        .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into());
    let status = Command::new(python)
        .args(["st/compile.py", &out])
        .status()
        .expect("run ST driver compiler");
    assert!(status.success(), "compiling STM32Cube drivers");
    println!("cargo:rustc-link-search={root}");
    if embassy {
        std::fs::copy("embassy-memory.x", Path::new(&out).join("memory.x")).unwrap();
        println!("cargo:rustc-link-search={out}");
        println!("cargo:rerun-if-changed=embassy-memory.x");
        println!("cargo:rustc-link-arg=-Tlink.x");
    } else {
        println!("cargo:rustc-link-arg=-Tboard.x");
        println!("cargo:rerun-if-changed=board.x");
    }
}
