use std::{env, fmt::Write, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let planned = env::var("LIULI_PLANNED").as_deref() == Ok("1");
    if planned {
        for name in [
            "LIULI_MAX_NODES",
            "LIULI_MAX_DEPTH",
            "LIULI_TEXT_SLOTS_LIMIT",
        ] {
            assert!(
                env::var_os(name).is_some(),
                "incomplete application plan: missing {name}"
            );
        }
    }
    let mut output = String::new();
    let mut custom = false;
    for (name, default, min, max) in [
        ("MAX_NODES", 64, 1, 250),
        ("TEXT_SLOTS_LIMIT", 250, 0, 250),
        ("MAX_DEPTH", 12, 1, 250),
        ("MAX_TEXT_BYTES", 1024, 0, 65535),
        ("MAX_COMMANDS", 256, 1, 65534),
        ("MAX_DAMAGE", 12, 1, 255),
        ("MAX_ANIMATIONS", 8, 0, 31),
        ("MAX_SPARKLINES", 2, 0, 254),
        ("MAX_SPARKLINE_SAMPLES", 32, 1, 255),
        ("MAX_PART_STYLES", 12, 0, 254),
        ("MAX_LAYOUT_EXTRAS", 12, 0, 254),
        ("MAX_EFFECTS", 8, 0, 254),
        ("MAX_CUSTOM_CHILDREN", 16, 1, 250),
        ("MAX_CLIP_DEPTH", 4, 1, 250),
        ("FOCUS_HISTORY", 8, 1, 250),
        ("DRAG_THRESHOLD_PX", 6, 1, 32767),
        ("REPEAT_DELAY_MS", 400, 1, 65535),
        ("REPEAT_PERIOD_MS", 80, 1, 65535),
        ("SAMPLE_INTERVAL_MS", 16, 1, 65535),
        ("FRICTION_NUM", 235, 1, 255),
        ("FONT_SLOTS", 4, 1, 256),
        ("KERN_CACHE_SLOTS", 32, 0, 65535),
        ("IME_DISPLAY_BYTES", 512, 4, 65535),
        ("IME_PREEDIT_BYTES", 32, 0, 255),
        ("IME_CANDIDATES", 4, 1, 255),
    ] {
        let key = format!("LIULI_{name}");
        println!("cargo:rerun-if-env-changed={key}");
        let value = env::var_os(&key).map_or(default, |raw| {
            raw.to_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("{key} must be an integer"))
        });
        assert!(
            (min..=max).contains(&value),
            "{key} must be in {min}..={max}"
        );
        // Preserve the stock memory ceiling for tuning values or smaller pools.
        if (name.starts_with("MAX_") || name == "FOCUS_HISTORY") && value > default {
            custom = true;
        }
        if name == "FONT_SLOTS" && value > 4 {
            custom = true;
        }
        writeln!(output, "pub const {name}: usize = {value};").unwrap();
    }
    println!("cargo:rerun-if-env-changed=LIULI_PLANNED");
    writeln!(output, "pub const PLANNED: bool = {};", planned).unwrap();
    for name in [
        "MASKS",
        "SHADOWS",
        "ICONS",
        "TAB_INDICATOR",
        "SCROLL",
        "POINTER",
        "REPEAT",
        "METRICS",
        "DMA",
    ] {
        let key = format!("LIULI_{name}");
        println!("cargo:rerun-if-env-changed={key}");
        let enabled = match env::var(&key).as_deref() {
            Ok("0") => false,
            Ok("1") | Err(_) => true,
            _ => panic!("{key} must be 0 or 1"),
        };
        println!("cargo::metadata={name}={}", u8::from(enabled));
        writeln!(output, "pub const {name}: bool = {enabled};").unwrap();
    }
    writeln!(output, "pub const CUSTOM_BUDGET: bool = {custom};").unwrap();
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("capacity.rs"),
        output,
    )
    .unwrap();
}
