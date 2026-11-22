#![forbid(unsafe_code)]

mod atlas;
mod pack;

use atlas::Atlas;
use pack::pack_lf4;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("tools/liuli-fontpack must live two levels below workspace root")
        .to_path_buf()
}

fn run_pack(root: &Path) -> Result<(), String> {
    let pbm = root.join("tools/liuli-fontpack/source/atlas.pbm");
    let tsv = root.join("tools/liuli-fontpack/source/glyphs.tsv");
    let out = root.join("apps/liuli-settings/assets/settings.lf4");

    let atlas = Atlas::load(&pbm, &tsv)?;
    let (bytes, stats) = pack_lf4(&atlas)?;
    if stats.cjk_with_intermediate_cov == 0 {
        return Err("packed atlas has no CJK glyph with intermediate A4 coverage (1..=14)".into());
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(&out, &bytes).map_err(|e| format!("failed to write {}: {e}", out.display()))?;
    println!(
        "Packed {} glyphs -> {} ({} bytes, {} CJK glyphs with intermediate A4 coverage)",
        stats.glyph_count,
        out.display(),
        stats.total_bytes,
        stats.cjk_with_intermediate_cov
    );
    Ok(())
}

fn run_verify(root: &Path) -> Result<(), String> {
    let pbm = root.join("tools/liuli-fontpack/source/atlas.pbm");
    let tsv = root.join("tools/liuli-fontpack/source/glyphs.tsv");
    let charset_path = root.join("apps/liuli-settings/assets/charset.txt");
    let lf4_path = root.join("apps/liuli-settings/assets/settings.lf4");

    let atlas = Atlas::load(&pbm, &tsv)?;
    let (rebuilt, stats) = pack_lf4(&atlas)?;

    if stats.cjk_with_intermediate_cov == 0 {
        return Err("rebuilt LF4 has no CJK glyph with intermediate A4 coverage".into());
    }

    let charset = fs::read_to_string(&charset_path)
        .map_err(|e| format!("failed to read {}: {e}", charset_path.display()))?;
    let present: BTreeSet<u32> = atlas.glyphs.iter().map(|g| g.unicode).collect();
    for ch in charset.chars() {
        if ch == '\r' || ch == '\n' {
            continue;
        }
        if !present.contains(&(ch as u32)) {
            return Err(format!(
                "charset character '{}' (U+{:04X}) missing from glyphs.tsv",
                ch, ch as u32
            ));
        }
    }

    let existing =
        fs::read(&lf4_path).map_err(|e| format!("failed to read {}: {e}", lf4_path.display()))?;
    if existing != rebuilt {
        return Err(format!(
            "LF4 mismatch: {} ({} bytes) != rebuilt ({} bytes)",
            lf4_path.display(),
            existing.len(),
            rebuilt.len()
        ));
    }

    println!(
        "Verified {}: {} glyphs, {} bytes, {} CJK glyphs with intermediate A4 coverage, all charset scalars present.",
        lf4_path.display(),
        stats.glyph_count,
        stats.total_bytes,
        stats.cjk_with_intermediate_cov
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("verify");
    let root = workspace_root();
    let res = match cmd {
        "pack" => run_pack(&root),
        "verify" => run_verify(&root),
        other => Err(format!(
            "unknown subcommand '{other}'; expected 'pack' or 'verify'"
        )),
    };
    if let Err(err) = res {
        eprintln!("liuli-fontpack error: {err}");
        std::process::exit(1);
    }
}
