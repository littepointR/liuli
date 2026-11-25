#!/usr/bin/env python3
"""Self-contained regression suite for tools/liuli-figma.py."""

from __future__ import annotations

import json
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / "tools" / "liuli-figma.py"
sys.path.insert(0, str(CLI.parent))
import importlib

figma = importlib.import_module("liuli-figma")


def run_cli(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(CLI), *args],
        text=True,
        capture_output=True,
        check=check,
    )


def sample_figma_doc() -> dict:
    return {
        "document": {
            "type": "DOCUMENT",
            "children": [
                {
                    "type": "CANVAS",
                    "children": [
                        {
                            "id": "1:2",
                            "name": "DashboardScreen",
                            "type": "FRAME",
                            "layoutMode": "VERTICAL",
                            "paddingTop": 16,
                            "paddingRight": 16,
                            "paddingBottom": 16,
                            "paddingLeft": 16,
                            "itemSpacing": 12,
                            "primaryAxisAlignItems": "SPACE_BETWEEN",
                            "counterAxisAlignItems": "STRETCH",
                            "absoluteBoundingBox": {"x": 0, "y": 0, "width": 320, "height": 240},
                            "fills": [{"type": "SOLID", "color": {"r": 0.05, "g": 0.07, "b": 0.1}}],
                            "children": [
                                {
                                    "id": "1:3",
                                    "name": "HeaderTitle",
                                    "type": "TEXT",
                                    "characters": "System Ready",
                                    "layoutSizingHorizontal": "FILL",
                                    "layoutSizingVertical": "HUG",
                                    "style": {"fontSize": 20, "textAlignHorizontal": "CENTER"},
                                    "fills": [{"type": "SOLID", "color": {"r": 0.95, "g": 0.96, "b": 0.98}}],
                                },
                                {
                                    "id": "1:4",
                                    "name": "PrimaryBtn",
                                    "type": "FRAME",
                                    "cornerRadius": 8,
                                    "layoutSizingHorizontal": "FIXED",
                                    "layoutSizingVertical": "FIXED",
                                    "absoluteBoundingBox": {"x": 0, "y": 0, "width": 120, "height": 40},
                                    "fills": [{"type": "SOLID", "color": {"r": 0.16, "g": 0.63, "b": 0.97}}],
                                    "strokes": [{"type": "SOLID", "color": {"r": 1.0, "g": 1.0, "b": 1.0}}],
                                    "strokeWeight": 1,
                                    "children": [],
                                },
                            ],
                        }
                    ],
                }
            ],
        }
    }


def test_tokens_compile_lint_and_diff() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)

        # 1. W3C Tokens generation
        tokens_json = {
            "color": {
                "$type": "color",
                "background": {"$value": "#0C1016"},
                "surface": {"$value": "#161C26"},
                "text": {"$value": "#F0F4F8"},
                "accent": {"$value": "#2AA2F8"},
            },
            "spacing": {"$type": "dimension", "md": {"$value": "12px"}},
        }
        tok_in = d / "tokens.json"
        tok_out = d / "tokens.rs"
        tok_in.write_text(json.dumps(tokens_json), encoding="utf-8")
        run_cli("tokens", "--input", str(tok_in), "--output", str(tok_out), "--strict")
        tok_rs = tok_out.read_text(encoding="utf-8")
        assert "pub const GENERATED_SCHEME: ColorScheme = ColorScheme {" in tok_rs
        assert "pub const SPACING_MD: u16 = 12;" in tok_rs

        # 2. Figma compile
        fig_in = d / "screen.json"
        fig_out = d / "view.rs"
        fig_in.write_text(json.dumps(sample_figma_doc()), encoding="utf-8")
        run_cli("compile", "--input", str(fig_in), "--output", str(fig_out), "--fn-name", "build_dash")
        view_rs = fig_out.read_text(encoding="utf-8")
        assert "pub fn build_dash(vb: &mut ViewBuilder<'_>)" in view_rs
        assert ".justify(Justify::SpaceBetween)" in view_rs
        assert ".align_cross(Align::Stretch)" in view_rs
        assert "ui.label(Key(2)," in view_rs
        assert "ui.button(Key(3)," in view_rs

        # 3. Figma lint: passing doc vs low-contrast & node-overflow doc
        ok_lint = run_cli("lint", "--input", str(fig_in), "--json")
        ok_rep = json.loads(ok_lint.stdout)
        assert ok_rep["ok"] and ok_rep["stats"]["nodes"] == 3

        bad_doc = sample_figma_doc()
        # Low contrast dark gray on dark background + exceed 64 nodes
        frame = bad_doc["document"]["children"][0]["children"][0]
        frame["children"][0]["fills"] = [{"type": "SOLID", "color": {"r": 0.08, "g": 0.09, "b": 0.11}}]
        for i in range(70):
            frame["children"].append(
                {
                    "id": f"9:{i}",
                    "name": f"tiny_btn_{i}",
                    "type": "FRAME",
                    "absoluteBoundingBox": {"x": 0, "y": 0, "width": 16, "height": 16},
                    "effects": [{"type": "LAYER_BLUR", "visible": True}],
                }
            )
        bad_in = d / "bad.json"
        bad_in.write_text(json.dumps(bad_doc), encoding="utf-8")
        bad_lint = run_cli("lint", "--input", str(bad_in), "--json", check=False)
        assert bad_lint.returncode == 1
        bad_rep = json.loads(bad_lint.stdout)
        rules = {e["rule"] for e in bad_rep["errors"]}
        assert "contrast" in rules and "max_nodes" in rules

        # 4. Pixel diff: identical PNG & shifted RGB565 vs PNG + diff image verification
        px_ok = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)]
        px_bad = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (0, 0, 0)]
        png1 = d / "a.png"
        png2 = d / "b.png"
        diff_png = d / "diff.png"
        png1.write_bytes(figma.encode_png(2, 2, px_ok))
        png2.write_bytes(figma.encode_png(2, 2, px_bad))
        raw565 = d / "a.rgb565"
        raw565.write_bytes(b"".join(struct.pack("<H", figma.rgb8_to_565(c)) for c in px_ok))

        pass_diff = run_cli(
            "diff",
            "--expected",
            str(png1),
            "--actual",
            str(raw565),
            "--width",
            "2",
            "--height",
            "2",
            "--rgb565-quantize",
            "--json",
        )
        assert json.loads(pass_diff.stdout)["mismatched_pixels"] == 0

        fail_diff = run_cli(
            "diff",
            "--expected",
            str(png1),
            "--actual",
            str(png2),
            "--diff-out",
            str(diff_png),
            "--json",
            check=False,
        )
        assert fail_diff.returncode == 1
        rep_d = json.loads(fail_diff.stdout)
        assert rep_d["mismatched_pixels"] == 1
        dw, dh, dpx = figma.decode_png(diff_png.read_bytes())
        assert (dw, dh) == (2, 2) and dpx[3] == (255, 32, 32)


if __name__ == "__main__":
    test_tokens_compile_lint_and_diff()
    print("test-figma.py ALL PASSED")
