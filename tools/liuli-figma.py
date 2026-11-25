#!/usr/bin/env python3
"""Zero-dependency offline Figma compiler, W3C Design Token generator, budget linter, and RGB565 diff tool."""

from __future__ import annotations

import argparse
import json
import math
import re
import struct
import sys
import zlib
from pathlib import Path
from typing import Any

PROFILES = {
    "default": {"max_nodes": 64, "max_depth": 16, "max_text_bytes": 2048},
    "128": {"max_nodes": 128, "max_depth": 16, "max_text_bytes": 4096},
    "256": {"max_nodes": 256, "max_depth": 16, "max_text_bytes": 8192},
}


# ---------------------------------------------------------------------------
# Color & WCAG contrast helpers
# ---------------------------------------------------------------------------

def parse_color(val: Any) -> tuple[int, int, int] | None:
    if isinstance(val, str):
        s = val.strip().lstrip("#")
        if len(s) == 3:
            return (int(s[0] * 2, 16), int(s[1] * 2, 16), int(s[2] * 2, 16))
        if len(s) in (6, 8):
            return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16))
        return None
    if isinstance(val, dict):
        if "r" in val and "g" in val and "b" in val:
            r, g, b = float(val["r"]), float(val["g"]), float(val["b"])
            if max(r, g, b) <= 1.0:
                return (round(r * 255), round(g * 255), round(b * 255))
            return (int(r), int(g), int(b))
        if "hex" in val:
            return parse_color(val["hex"])
    return None


def rgb8_to_565(rgb: tuple[int, int, int]) -> int:
    r, g, b = rgb
    return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)


def rgb565_to_rgb8(word: int) -> tuple[int, int, int]:
    r5 = (word >> 11) & 0x1F
    g6 = (word >> 5) & 0x3F
    b5 = word & 0x1F
    return ((r5 << 3) | (r5 >> 2), (g6 << 2) | (g6 >> 4), (b5 << 3) | (b5 >> 2))


def rel_luminance(rgb: tuple[int, int, int]) -> float:
    def lin(c: int) -> float:
        s = c / 255.0
        return s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4

    r, g, b = rgb
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)


def contrast_ratio(c1: tuple[int, int, int], c2: tuple[int, int, int]) -> float:
    l1 = rel_luminance(c1)
    l2 = rel_luminance(c2)
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


def rust_ident(name: str) -> str:
    s = re.sub(r"[^A-Za-z0-9_]", "_", name).upper()
    s = re.sub(r"_+", "_", s).strip("_")
    if not s or s[0].isdigit():
        s = "T_" + s
    return s


# ---------------------------------------------------------------------------
# Subcommand: tokens (W3C Design Tokens -> Rust ColorScheme + constants)
# ---------------------------------------------------------------------------

def flatten_w3c_tokens(obj: dict[str, Any], prefix: str = "", inherited_type: str | None = None) -> list[tuple[str, str | None, Any]]:
    out: list[tuple[str, str | None, Any]] = []
    cur_type = obj.get("$type", inherited_type)
    if "$value" in obj:
        out.append((prefix, cur_type, obj["$value"]))
        return out
    for k, v in obj.items():
        if k.startswith("$"):
            continue
        child_prefix = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            out.extend(flatten_w3c_tokens(v, child_prefix, cur_type))
    return out


def cmd_tokens(args: argparse.Namespace) -> int:
    raw = json.loads(args.input.read_text(encoding="utf-8"))
    flat = flatten_w3c_tokens(raw)

    colors: dict[str, tuple[int, int, int]] = {}
    dims: list[tuple[str, int]] = []
    for name, ttype, val in flat:
        if ttype == "color" or (isinstance(val, str) and val.strip().startswith("#")):
            c = parse_color(val)
            if c is not None:
                colors[name] = c
        elif ttype in ("dimension", "spacing", "borderRadius", "fontSize", "number"):
            if isinstance(val, (int, float)):
                dims.append((name, int(round(val))))
            elif isinstance(val, str) and re.fullmatch(r"\s*-?\d+(?:\.\d+)?(?:px)?\s*", val):
                dims.append((name, int(round(float(val.strip().removesuffix("px"))))))

    def pick(candidates: list[str], fallback: tuple[int, int, int]) -> tuple[int, int, int]:
        for key_sub in candidates:
            for k, v in colors.items():
                if key_sub in k.lower():
                    return v
        return fallback

    bg = pick(["background", "bg"], (12, 16, 22))
    surface = pick(["surface_alt", "card", "surface"], (22, 28, 38))
    surface_alt = pick(["surface_alt", "elevated", "surface2"], surface)
    border = pick(["border", "outline", "stroke"], (58, 72, 92))
    text = pick(["text.primary", "text", "foreground", "fg", "on_surface"], (240, 244, 248))
    text_muted = pick(["muted", "secondary", "subtext"], (156, 170, 188))
    accent = pick(["accent", "primary", "brand"], (42, 162, 248))
    focus = pick(["focus", "ring"], (250, 200, 40))
    danger = pick(["danger", "error", "alert"], (232, 72, 72))

    warnings: list[str] = []
    cr_bg = contrast_ratio(text, bg)
    cr_surf = contrast_ratio(text, surface)
    if cr_bg < 4.5:
        warnings.append(f"text vs background contrast {cr_bg:.2f} < 4.5")
    if cr_surf < 4.5:
        warnings.append(f"text vs surface contrast {cr_surf:.2f} < 4.5")

    lines = [
        "// Auto-generated by tools/liuli-figma.py tokens — do not edit by hand.",
        "use liuli_widgets::tokens::{ColorScheme, rgb8};",
        "use liuli::prelude::Rgb565;",
        "",
    ]
    for k, rgb in sorted(colors.items()):
        w565 = rgb8_to_565(rgb)
        lines.append(
            f"pub const {rust_ident(k)}: Rgb565 = rgb8({rgb[0]}, {rgb[1]}, {rgb[2]}); // 0x{w565:04X}"
        )
    if colors:
        lines.append("")
    for k, val in sorted(dims):
        lines.append(f"pub const {rust_ident(k)}: u16 = {max(0, val)};")
    if dims:
        lines.append("")

    def fmt_rgb8(c: tuple[int, int, int]) -> str:
        return f"rgb8({c[0]}, {c[1]}, {c[2]})"

    lines.extend([
        "pub const GENERATED_SCHEME: ColorScheme = ColorScheme {",
        f"    background: {fmt_rgb8(bg)},",
        f"    surface: {fmt_rgb8(surface)},",
        f"    surface_alt: {fmt_rgb8(surface_alt)},",
        f"    border: {fmt_rgb8(border)},",
        f"    text: {fmt_rgb8(text)},",
        f"    text_muted: {fmt_rgb8(text_muted)},",
        f"    accent: {fmt_rgb8(accent)},",
        f"    focus: {fmt_rgb8(focus)},",
        f"    danger: {fmt_rgb8(danger)},",
        "};",
        "",
    ])
    src = "\n".join(lines)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(src, encoding="utf-8", newline="\n")
    else:
        print(src)
    for w in warnings:
        print(f"tokens warning: {w}", file=sys.stderr)
    return 1 if (args.strict and warnings) else 0


# ---------------------------------------------------------------------------
# Shared Figma AST extraction
# ---------------------------------------------------------------------------

def unwrap_figma_root(data: dict[str, Any], node_name: str | None = None) -> dict[str, Any]:
    root = data.get("document", data)
    if root.get("type") == "DOCUMENT":
        pages = root.get("children", [])
        if pages:
            root = pages[0]
    if root.get("type") == "CANVAS":
        children = root.get("children", [])
        if children:
            root = children[0]
    if node_name:
        found = find_named_node(root, node_name)
        if found is None:
            raise ValueError(f"Figma node '{node_name}' not found")
        return found
    return root


def find_named_node(node: dict[str, Any], target: str) -> dict[str, Any] | None:
    if node.get("name") == target or node.get("id") == target:
        return node
    for ch in node.get("children", []):
        res = find_named_node(ch, target)
        if res is not None:
            return res
    return None


def solid_fill(node: dict[str, Any]) -> tuple[int, int, int] | None:
    for fill in node.get("fills", []):
        if not fill.get("visible", True):
            continue
        if fill.get("type") == "SOLID":
            return parse_color(fill.get("color"))
    return None


def solid_stroke(node: dict[str, Any]) -> tuple[tuple[int, int, int], int] | None:
    strokes = [s for s in node.get("strokes", []) if s.get("visible", True) and s.get("type") == "SOLID"]
    if not strokes:
        return None
    col = parse_color(strokes[0].get("color"))
    if col is None:
        return None
    weight = max(1, int(round(float(node.get("strokeWeight", 1)))))
    return (col, min(weight, 255))


def node_dimensions(node: dict[str, Any]) -> tuple[int, int]:
    bbox = node.get("absoluteBoundingBox") or {}
    w = bbox.get("width", node.get("width", 0))
    h = bbox.get("height", node.get("height", 0))
    return (max(0, int(round(float(w)))), max(0, int(round(float(h)))))


# ---------------------------------------------------------------------------
# Subcommand: compile (Figma JSON -> idiomatic Liuli view Rust code)
# ---------------------------------------------------------------------------

def map_font_id(px: float) -> str:
    if px <= 13:
        return "FontId::Small12"
    if px <= 17:
        return "FontId::Body16"
    if px <= 23:
        return "FontId::Title20"
    return "FontId::Display28"


def map_length(sizing: str | None, px: int) -> str:
    if sizing == "FILL":
        return "Length::Fill(1)"
    if sizing == "HUG":
        return "Length::Content"
    return f"Length::Px({px})"


def escape_rs(s: str) -> str:
    return (
        s.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "")
        .replace("\t", "\\t")
    )


def props_expr(node: dict[str, Any], is_root: bool) -> str:
    w, h = node_dimensions(node)
    h_sz = node.get("layoutSizingHorizontal")
    v_sz = node.get("layoutSizingVertical")
    chain = ["NodeProps::new()"]
    if not is_root:
        chain.append(f".width({map_length(h_sz, w)})")
        chain.append(f".height({map_length(v_sz, h)})")
    else:
        chain.append(".width(Length::Fill(1)).height(Length::Fill(1))")

    pt = int(round(float(node.get("paddingTop", 0))))
    pr = int(round(float(node.get("paddingRight", 0))))
    pb = int(round(float(node.get("paddingBottom", 0))))
    pl = int(round(float(node.get("paddingLeft", 0))))
    if any((pt, pr, pb, pl)):
        if pt == pr == pb == pl:
            chain.append(f".padding(EdgeInsets::all({pt}))")
        else:
            chain.append(
                f".padding(EdgeInsets {{ top: {pt}, right: {pr}, bottom: {pb}, left: {pl} }})"
            )

    gap = int(round(float(node.get("itemSpacing", 0))))
    if gap > 0:
        chain.append(f".gap({gap})")

    p_align = node.get("primaryAxisAlignItems", "MIN")
    justify_map = {
        "CENTER": "Justify::Center",
        "MAX": "Justify::End",
        "SPACE_BETWEEN": "Justify::SpaceBetween",
    }
    if p_align in justify_map:
        chain.append(f".justify({justify_map[p_align]})")

    c_align = node.get("counterAxisAlignItems", "MIN")
    align_map = {"CENTER": "Align::Center", "MAX": "Align::End", "STRETCH": "Align::Stretch"}
    if c_align in align_map:
        chain.append(f".align_cross({align_map[c_align]})")

    ntype = node.get("type", "FRAME")
    fill = solid_fill(node)
    if ntype == "TEXT":
        if fill:
            chain.append(f".fg(rgb8({fill[0]}, {fill[1]}, {fill[2]}))")
        style = node.get("style") or {}
        fsize = float(style.get("fontSize", 16))
        chain.append(f".font({map_font_id(fsize)})")
        t_align = {"CENTER": "TextAlign::Center", "RIGHT": "TextAlign::End"}.get(
            style.get("textAlignHorizontal", "LEFT"),
            "TextAlign::Start",
        )
        if t_align != "TextAlign::Start":
            chain.append(f".text_align({t_align})")
        txt = escape_rs(str(node.get("characters", "")))
        chain.append(f'.text(TextRef::Static("{txt}"))')
    else:
        if fill:
            chain.append(f".bg(rgb8({fill[0]}, {fill[1]}, {fill[2]}))")

    if stroke := solid_stroke(node):
        col, bw = stroke
        chain.append(f".border(rgb8({col[0]}, {col[1]}, {col[2]}), {bw})")

    radius = int(round(float(node.get("cornerRadius", 0))))
    if radius > 0:
        chain.append(f".radius({min(radius, 255)})")

    return "".join(chain)


def emit_node(
    node: dict[str, Any],
    ui_var: str,
    indent: int,
    next_key: list[int],
    is_root: bool,
    lines: list[str],
    sep: str,
) -> None:
    if not node.get("visible", True):
        return
    key_id = next_key[0]
    next_key[0] += 1
    pad = " " * indent
    name = str(node.get("name", "node"))
    ntype = node.get("type", "FRAME")
    props = props_expr(node, is_root)

    if is_root:
        lines.append(f"{pad}// Figma frame: {name}")
        lines.append(f"{pad}vb.root(Key({key_id}), {props}, |ui| {{")
        children = [c for c in node.get("children", []) if c.get("visible", True)]
        if not children:
            lines.append(f"{pad}    Ok(())")
        else:
            for idx, ch in enumerate(children):
                is_last = idx + 1 == len(children)
                emit_node(ch, "ui", indent + 4, next_key, False, lines, "" if is_last else ";")
        lines.append(f"{pad}}}){sep}")
        return

    children = [c for c in node.get("children", []) if c.get("visible", True)]
    if ntype in ("FRAME", "GROUP", "COMPONENT", "INSTANCE") and children:
        lmode = node.get("layoutMode", "NONE")
        if lmode == "HORIZONTAL":
            method = "wrap" if node.get("layoutWrap") == "WRAP" else "row"
        elif lmode == "VERTICAL":
            method = "column"
        else:
            method = "stack"
        child_ui = f"c{key_id}"
        lines.append(f"{pad}// {name} ({ntype})")
        lines.append(f"{pad}{ui_var}.{method}(Key({key_id}), {props}, |{child_ui}| {{")
        for idx, ch in enumerate(children):
            is_last = idx + 1 == len(children)
            emit_node(ch, child_ui, indent + 4, next_key, False, lines, "" if is_last else ";")
        lines.append(f"{pad}}}){sep}")
    elif ntype == "TEXT":
        lines.append(f"{pad}{ui_var}.label(Key({key_id}), {props}){sep} // {name}")
    elif "button" in name.lower() or "btn" in name.lower():
        lines.append(f"{pad}{ui_var}.button(Key({key_id}), {props}.clickable(true).focusable(true)){sep} // {name}")
    else:
        lines.append(f"{pad}{ui_var}.custom(Key({key_id}), {props}){sep} // {name} ({ntype})")


def cmd_compile(args: argparse.Namespace) -> int:
    data = json.loads(args.input.read_text(encoding="utf-8"))
    root = unwrap_figma_root(data, args.frame)
    fn_name = args.fn_name or "build_figma_view"

    lines = [
        "// Auto-generated by tools/liuli-figma.py compile",
        "use liuli::prelude::*;",
        "use liuli_widgets::tokens::rgb8;",
        "",
        f"pub fn {fn_name}(vb: &mut ViewBuilder<'_>) -> Result<(), BuildError> {{",
    ]
    emit_node(root, "vb", 4, [1], True, lines, "")
    lines.append("}")
    lines.append("")
    src = "\n".join(lines)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(src, encoding="utf-8", newline="\n")
    else:
        print(src)
    return 0


# ---------------------------------------------------------------------------
# Subcommand: lint (Resource Budget, Touch Target & WCAG Checker)
# ---------------------------------------------------------------------------

def cmd_lint(args: argparse.Namespace) -> int:
    data = json.loads(args.input.read_text(encoding="utf-8"))
    root = unwrap_figma_root(data, args.frame)
    profile = PROFILES[args.profile]
    min_touch = args.min_touch

    errors: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    stats = {"nodes": 0, "max_depth": 0, "text_bytes": 0}

    def walk(node: dict[str, Any], depth: int, anc_bg: tuple[int, int, int] | None) -> None:
        if not node.get("visible", True):
            return
        stats["nodes"] += 1
        stats["max_depth"] = max(stats["max_depth"], depth)
        name = str(node.get("name", "<unnamed>"))
        ntype = node.get("type", "FRAME")
        w, h = node_dimensions(node)

        my_bg = anc_bg
        if ntype != "TEXT":
            if f := solid_fill(node):
                my_bg = f

        if ntype == "TEXT":
            chars = str(node.get("characters", ""))
            stats["text_bytes"] += len(chars.encode("utf-8"))
            fg = solid_fill(node)
            if fg and my_bg:
                ratio = contrast_ratio(fg, my_bg)
                fsize = float((node.get("style") or {}).get("fontSize", 16))
                req = 3.0 if fsize >= 20.0 else 4.5
                if ratio < req:
                    errors.append({
                        "rule": "contrast",
                        "node": name,
                        "detail": f"WCAG contrast {ratio:.2f} < {req:.1f} ({fg} on {my_bg})",
                    })

        if any(tok in name.lower() for tok in ("btn", "button", "toggle", "switch", "slider", "check")):
            if w > 0 and h > 0 and (w < min_touch or h < min_touch):
                warnings.append({
                    "rule": "touch_target",
                    "node": name,
                    "detail": f"touch target {w}x{h} < recommended {min_touch}x{min_touch}",
                })

        for eff in node.get("effects", []):
            if eff.get("visible", True) and eff.get("type") not in ("DROP_SHADOW",):
                warnings.append({
                    "rule": "unsupported_effect",
                    "node": name,
                    "detail": f"effect '{eff.get('type')}' is not hardware-accelerated on default MCU target",
                })

        for ch in node.get("children", []):
            walk(ch, depth + 1, my_bg)

    walk(root, 1, None)

    if stats["nodes"] > profile["max_nodes"]:
        errors.append({
            "rule": "max_nodes",
            "node": "<root>",
            "detail": f"node count {stats['nodes']} exceeds MAX_NODES={profile['max_nodes']}",
        })
    if stats["max_depth"] > profile["max_depth"]:
        errors.append({
            "rule": "max_depth",
            "node": "<root>",
            "detail": f"tree depth {stats['max_depth']} exceeds MAX_DEPTH={profile['max_depth']}",
        })
    if stats["text_bytes"] > profile["max_text_bytes"]:
        errors.append({
            "rule": "max_text_bytes",
            "node": "<root>",
            "detail": f"text bytes {stats['text_bytes']} exceeds MAX_TEXT_BYTES={profile['max_text_bytes']}",
        })

    if args.strict and warnings:
        errors.extend(warnings)
        warnings = []

    report = {
        "profile": args.profile,
        "limits": profile,
        "stats": stats,
        "errors": errors,
        "warnings": warnings,
        "ok": len(errors) == 0,
    }
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(
            f"Lint ({args.profile}): nodes={stats['nodes']}/{profile['max_nodes']} "
            f"depth={stats['max_depth']}/{profile['max_depth']} text={stats['text_bytes']}B "
            f"errors={len(errors)} warnings={len(warnings)}"
        )
        for item in errors:
            print(f"  ERROR [{item['rule']}] {item['node']}: {item['detail']}")
        for item in warnings:
            print(f"  WARN  [{item['rule']}] {item['node']}: {item['detail']}")
    return 0 if report["ok"] else 1


# ---------------------------------------------------------------------------
# Subcommand: diff (Pure-stdlib PNG / PPM / RGB565 pixel diff)
# ---------------------------------------------------------------------------

def _paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def decode_png(data: bytes) -> tuple[int, int, list[tuple[int, int, int]]]:
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("invalid PNG signature")
    pos = 8
    w = h = 0
    ctype = bit_depth = 0
    idat = bytearray()
    while pos < len(data):
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        ctype_chunk = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if ctype_chunk == b"IHDR":
            w, h, bit_depth, ctype, _comp, _filt, interlace = struct.unpack(">IIBBBBB", chunk)
            if bit_depth != 8 or ctype not in (2, 6) or interlace != 0:
                raise ValueError(f"unsupported PNG format bit_depth={bit_depth} color={ctype}")
        elif ctype_chunk == b"IDAT":
            idat.extend(chunk)
        elif ctype_chunk == b"IEND":
            break
    bpp = 3 if ctype == 2 else 4
    raw = zlib.decompress(bytes(idat))
    stride = w * bpp
    pixels: list[tuple[int, int, int]] = []
    prev_row = bytearray(stride)
    idx = 0
    for _ in range(h):
        ftype = raw[idx]
        idx += 1
        row = bytearray(raw[idx : idx + stride])
        idx += stride
        for i in range(stride):
            a = row[i - bpp] if i >= bpp else 0
            b = prev_row[i]
            c = prev_row[i - bpp] if i >= bpp else 0
            if ftype == 1:
                row[i] = (row[i] + a) & 0xFF
            elif ftype == 2:
                row[i] = (row[i] + b) & 0xFF
            elif ftype == 3:
                row[i] = (row[i] + ((a + b) >> 1)) & 0xFF
            elif ftype == 4:
                row[i] = (row[i] + _paeth(a, b, c)) & 0xFF
        for px in range(0, stride, bpp):
            pixels.append((row[px], row[px + 1], row[px + 2]))
        prev_row = row
    return w, h, pixels


def encode_png(w: int, h: int, pixels: list[tuple[int, int, int]]) -> bytes:
    def chunk(tag: bytes, payload: bytes) -> bytes:
        crc = zlib.crc32(tag + payload) & 0xFFFFFFFF
        return struct.pack(">I", len(payload)) + tag + payload + struct.pack(">I", crc)

    raw = bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            r, g, b = pixels[y * w + x]
            raw.extend((r, g, b))
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
        + chunk(b"IEND", b"")
    )


def decode_ppm(data: bytes) -> tuple[int, int, list[tuple[int, int, int]]]:
    idx = 0

    def next_tok() -> bytes:
        nonlocal idx
        while idx < len(data) and (data[idx] in b" \t\r\n" or data[idx : idx + 1] == b"#"):
            if data[idx : idx + 1] == b"#":
                while idx < len(data) and data[idx : idx + 1] != b"\n":
                    idx += 1
            else:
                idx += 1
        start = idx
        while idx < len(data) and data[idx] not in b" \t\r\n#":
            idx += 1
        return data[start:idx]

    magic = next_tok()
    if magic != b"P6":
        raise ValueError("expected P6 PPM")
    w = int(next_tok())
    h = int(next_tok())
    maxval = int(next_tok())
    if maxval != 255:
        raise ValueError("expected PPM maxval 255")
    idx += 1
    payload = data[idx:]
    if len(payload) < w * h * 3:
        raise ValueError("truncated PPM payload")
    pixels = [(payload[i], payload[i + 1], payload[i + 2]) for i in range(0, w * h * 3, 3)]
    return w, h, pixels


def load_image(path: Path, width: int | None, height: int | None) -> tuple[int, int, list[tuple[int, int, int]]]:
    data = path.read_bytes()
    ext = path.suffix.lower()
    if ext == ".png" or data[:8] == b"\x89PNG\r\n\x1a\n":
        return decode_png(data)
    if ext == ".ppm" or data[:2] == b"P6":
        return decode_ppm(data)
    if ext == ".rgb565":
        if width is None or height is None:
            raise ValueError("--width and --height are required for raw .rgb565 files")
        expected = width * height * 2
        if len(data) != expected:
            raise ValueError(f"{path}: expected {expected} bytes for {width}x{height}, got {len(data)}")
        pixels = [rgb565_to_rgb8(struct.unpack_from("<H", data, i)[0]) for i in range(0, expected, 2)]
        return width, height, pixels
    raise ValueError(f"unsupported image format: {path}")


def cmd_diff(args: argparse.Namespace) -> int:
    w1, h1, px1 = load_image(args.expected, args.width, args.height)
    w2, h2, px2 = load_image(args.actual, args.width, args.height)
    if (w1, h1) != (w2, h2):
        raise ValueError(f"dimension mismatch: {w1}x{h1} != {w2}x{h2}")

    total = len(px1)
    mismatched = 0
    max_delta = 0
    sum_dr = sum_dg = sum_db = 0
    diff_vis: list[tuple[int, int, int]] | None = [] if args.diff_out else None

    for a, b in zip(px1, px2):
        a565 = rgb565_to_rgb8(rgb8_to_565(a)) if args.rgb565_quantize else a
        b565 = rgb565_to_rgb8(rgb8_to_565(b)) if args.rgb565_quantize else b
        dr = abs(a565[0] - b565[0])
        dg = abs(a565[1] - b565[1])
        db = abs(a565[2] - b565[2])
        sum_dr += dr
        sum_dg += dg
        sum_db += db
        d = max(dr, dg, db)
        max_delta = max(max_delta, d)
        if d > args.channel_tolerance:
            mismatched += 1
            if diff_vis is not None:
                diff_vis.append((255, 32, 32))
        elif diff_vis is not None:
            luma = (a[0] + a[1] + a[2]) // 6
            diff_vis.append((luma, luma, luma))

    if args.diff_out and diff_vis is not None:
        args.diff_out.parent.mkdir(parents=True, exist_ok=True)
        args.diff_out.write_bytes(encode_png(w1, h1, diff_vis))

    pct = (mismatched * 100.0 / total) if total else 0.0
    mae = ((sum_dr + sum_dg + sum_db) / (3.0 * total)) if total else 0.0
    passed = pct <= args.max_mismatch_pct
    report = {
        "width": w1,
        "height": h1,
        "total_pixels": total,
        "mismatched_pixels": mismatched,
        "mismatch_pct": round(pct, 4),
        "mae": round(mae, 4),
        "max_channel_delta": max_delta,
        "ok": passed,
    }
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(
            f"Diff {w1}x{h1}: mismatch={mismatched}/{total} ({pct:.3f}%) "
            f"MAE={mae:.3f} max_delta={max_delta} -> {'PASS' if passed else 'FAIL'}"
        )
    return 0 if passed else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Offline Figma & Design Token toolkit for Liuli.")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_tok = sub.add_parser("tokens", help="Compile W3C Design Tokens JSON into #![no_std] Rust.")
    p_tok.add_argument("--input", type=Path, required=True)
    p_tok.add_argument("--output", type=Path)
    p_tok.add_argument("--strict", action="store_true")

    p_cmp = sub.add_parser("compile", help="Compile Figma REST JSON into Rust view() code.")
    p_cmp.add_argument("--input", type=Path, required=True)
    p_cmp.add_argument("--output", type=Path)
    p_cmp.add_argument("--frame", help="Optional target frame name or node id")
    p_cmp.add_argument("--fn-name", help="Generated Rust function name")

    p_lnt = sub.add_parser("lint", help="Check Figma JSON against MCU memory & WCAG budgets.")
    p_lnt.add_argument("--input", type=Path, required=True)
    p_lnt.add_argument("--frame")
    p_lnt.add_argument("--profile", choices=tuple(PROFILES), default="default")
    p_lnt.add_argument("--min-touch", type=int, default=32)
    p_lnt.add_argument("--strict", action="store_true")
    p_lnt.add_argument("--json", action="store_true")

    p_diff = sub.add_parser("diff", help="Compare rendered PNG/PPM/RGB565 images pixel-by-pixel.")
    p_diff.add_argument("--expected", type=Path, required=True)
    p_diff.add_argument("--actual", type=Path, required=True)
    p_diff.add_argument("--width", type=int)
    p_diff.add_argument("--height", type=int)
    p_diff.add_argument("--channel-tolerance", type=int, default=0)
    p_diff.add_argument("--max-mismatch-pct", type=float, default=0.0)
    p_diff.add_argument("--rgb565-quantize", action="store_true")
    p_diff.add_argument("--diff-out", type=Path)
    p_diff.add_argument("--json", action="store_true")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    handlers = {
        "tokens": cmd_tokens,
        "compile": cmd_compile,
        "lint": cmd_lint,
        "diff": cmd_diff,
    }
    return handlers[args.cmd](args)


if __name__ == "__main__":
    raise SystemExit(main())
