#![forbid(unsafe_code)]

mod atlas;
mod pack;

use atlas::Atlas;
use pack::{pack_lf4, verify_lf4_invariants};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_COMBINED_LF4_BYTES: usize = 32_768;

struct FontSlot {
    name: &'static str,
    pbm: &'static str,
    tsv: &'static str,
    charset: &'static str,
    out_lf4: &'static str,
}

const SLOTS: &[FontSlot] = &[
    FontSlot {
        name: "Body16",
        pbm: "tools/liuli-fontpack/source/atlas.pbm",
        tsv: "tools/liuli-fontpack/source/glyphs.tsv",
        charset: "apps/liuli-settings/assets/charset.txt",
        out_lf4: "apps/liuli-settings/assets/settings.lf4",
    },
    FontSlot {
        name: "Small12",
        pbm: "tools/liuli-fontpack/source/atlas_small12.pbm",
        tsv: "tools/liuli-fontpack/source/glyphs_small12.tsv",
        charset: "apps/liuli-settings/assets/charset_small12.txt",
        out_lf4: "apps/liuli-settings/assets/small12.lf4",
    },
];

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("tools/liuli-fontpack must live two levels below workspace root")
        .to_path_buf()
}

fn check_charset_coverage(root: &Path, slot: &FontSlot, atlas: &Atlas) -> Result<(), String> {
    let charset_path = root.join(slot.charset);
    let charset = fs::read_to_string(&charset_path)
        .map_err(|e| format!("failed to read {}: {e}", charset_path.display()))?;
    let present: BTreeSet<u32> = atlas.glyphs.iter().map(|g| g.unicode).collect();
    for ch in charset.chars() {
        if ch == '\r' || ch == '\n' {
            continue;
        }
        if !present.contains(&(ch as u32)) {
            return Err(format!(
                "[{}] charset character '{}' (U+{:04X}) missing from {}",
                slot.name, ch, ch as u32, slot.tsv
            ));
        }
    }
    Ok(())
}

fn run_pack(root: &Path) -> Result<(), String> {
    let mut staged = Vec::with_capacity(SLOTS.len());
    let mut combined_bytes = 0usize;

    for slot in SLOTS {
        let pbm = root.join(slot.pbm);
        let tsv = root.join(slot.tsv);
        let atlas = Atlas::load(&pbm, &tsv)?;
        check_charset_coverage(root, slot, &atlas)?;
        let (bytes, stats) = pack_lf4(&atlas)?;
        if stats.cjk_with_intermediate_cov == 0 {
            return Err(format!(
                "[{}] packed atlas has no CJK glyph (U+4E00..=U+9FFF) with intermediate A4 coverage",
                slot.name
            ));
        }
        verify_lf4_invariants(&bytes, &atlas)?;
        combined_bytes += bytes.len();
        staged.push((slot, bytes, stats));
    }

    if combined_bytes > MAX_COMBINED_LF4_BYTES {
        return Err(format!(
            "combined LF4 asset size {combined_bytes} B exceeds limit {MAX_COMBINED_LF4_BYTES} B"
        ));
    }

    for (slot, bytes, stats) in staged {
        let out = root.join(slot.out_lf4);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        fs::write(&out, &bytes).map_err(|e| format!("failed to write {}: {e}", out.display()))?;
        println!(
            "[{}] Packed {} glyphs -> {} ({} bytes, {} CJK glyphs with intermediate A4 coverage)",
            slot.name,
            stats.glyph_count,
            out.display(),
            stats.total_bytes,
            stats.cjk_with_intermediate_cov
        );
    }
    println!(
        "Total packed LF4 size: {} / {} bytes",
        combined_bytes, MAX_COMBINED_LF4_BYTES
    );
    Ok(())
}

fn run_verify(root: &Path) -> Result<(), String> {
    let mut combined_bytes = 0usize;

    for slot in SLOTS {
        let pbm = root.join(slot.pbm);
        let tsv = root.join(slot.tsv);
        let lf4_path = root.join(slot.out_lf4);

        let atlas = Atlas::load(&pbm, &tsv)?;
        check_charset_coverage(root, slot, &atlas)?;
        let (rebuilt, stats) = pack_lf4(&atlas)?;

        if stats.cjk_with_intermediate_cov == 0 {
            return Err(format!(
                "[{}] rebuilt LF4 has no CJK glyph (U+4E00..=U+9FFF) with intermediate A4 coverage",
                slot.name
            ));
        }

        let existing = fs::read(&lf4_path)
            .map_err(|e| format!("failed to read {}: {e}", lf4_path.display()))?;
        verify_lf4_invariants(&existing, &atlas)?;
        if existing != rebuilt {
            return Err(format!(
                "[{}] LF4 mismatch: {} ({} bytes) != rebuilt ({} bytes)",
                slot.name,
                lf4_path.display(),
                existing.len(),
                rebuilt.len()
            ));
        }

        combined_bytes += existing.len();
        println!(
            "[{}] Verified {}: {} glyphs, {} bytes, {} CJK glyphs with intermediate A4 coverage, all charset scalars present.",
            slot.name,
            lf4_path.display(),
            stats.glyph_count,
            stats.total_bytes,
            stats.cjk_with_intermediate_cov
        );
    }

    if combined_bytes > MAX_COMBINED_LF4_BYTES {
        return Err(format!(
            "combined LF4 asset size {combined_bytes} B exceeds limit {MAX_COMBINED_LF4_BYTES} B"
        ));
    }
    println!(
        "Combined LF4 budget OK: {} / {} bytes",
        combined_bytes, MAX_COMBINED_LF4_BYTES
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

#[cfg(test)]
mod tests {
    use super::*;
    use atlas::{SourceGlyph, parse_pbm};

    #[test]
    fn pbm_dimension_overflow_is_rejected_without_allocation() {
        let p4_ovf = parse_pbm(b"P4\n4294967296 4294967296\n");
        assert!(p4_ovf.is_err());

        let p1_huge = parse_pbm(b"P1\n10000 10000\n");
        assert!(p1_huge.is_err());
    }

    #[test]
    fn lf4_verify_checks_length_odd_nibble_and_cjk_range() {
        // 2 glyphs: fallback '□' (3x1 odd width, cell 0,0) and '设' U+8BBE (1x1 cell 12,0)
        // Make 12x4 atlas so both 4x cells fit cleanly: cell1=4x (12x4 -> step 4), cell2 at (12,0) => width=16
        let mut pixels = vec![false; 16 * 4];
        // For '□' at (0,0) width=3px (12x4 subpixels): 8 lit subpixels in col 0 => cov=8 (even/odd OK)
        for sx in 0..4 {
            for sy in 0..2 {
                pixels[sy * 16 + sx] = true;
            }
        }
        // For '设' at (12,0) width=1px (4x4 subpixels): 6 lit subpixels => intermediate cov=6
        for sy in 0..2 {
            for sx in 12..15 {
                pixels[sy * 16 + sx] = true;
            }
        }

        let atlas = Atlas {
            width: 16,
            _height: 4,
            pixels,
            ascent: 10,
            descent: 2,
            line_gap: 2,
            glyphs: vec![
                SourceGlyph {
                    unicode: 0x25A1,
                    cell_x: 0,
                    cell_y: 0,
                    width_px: 3,
                    height_px: 1,
                    bearing_x: 0,
                    bearing_y: 1,
                    advance: 4,
                },
                SourceGlyph {
                    unicode: 0x8BBE,
                    cell_x: 12,
                    cell_y: 0,
                    width_px: 1,
                    height_px: 1,
                    bearing_x: 0,
                    bearing_y: 1,
                    advance: 12,
                },
            ],
        };

        let (bytes, stats) = pack_lf4(&atlas).unwrap();
        assert_eq!(stats.cjk_with_intermediate_cov, 1);
        verify_lf4_invariants(&bytes, &atlas).unwrap();

        // Non-zero low trailing nibble on odd-width glyph must fail
        let mut bad_nibble = bytes.clone();
        let last_glyph1_byte = pack::HEADER_SIZE + 2 * pack::ENTRY_SIZE + 1;
        bad_nibble[last_glyph1_byte] |= 0x05;
        assert!(verify_lf4_invariants(&bad_nibble, &atlas).is_err());

        // Length formula mismatch (trailing byte) must fail
        let mut extra_byte = bytes.clone();
        extra_byte.push(0);
        assert!(verify_lf4_invariants(&extra_byte, &atlas).is_err());

        // Non-CJK >= 0xF900 scalar must NOT satisfy CJK range check (0x4E00..=0x9FFF)
        let mut only_ff = atlas.clone();
        only_ff.glyphs[1].unicode = 0xFF1F;
        let (_bytes_ff, stats_ff) = pack_lf4(&only_ff).unwrap();
        assert_eq!(stats_ff.cjk_with_intermediate_cov, 0);
    }
}
