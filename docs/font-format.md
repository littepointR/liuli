# LF4 v1 Bitmap Font Format

LF4 (`LLF4` version 1) is a little-endian binary bitmap font format designed for `include_bytes!` Flash storage without runtime allocation or rasterization.

## Header (24 bytes)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| 0 | 4 | `[u8; 4]` | `magic` | `"LLF4"` |
| 4 | 2 | `u16` | `version` | `1` |
| 6 | 2 | `u16` | `count` | Number of glyph records (`>= 1`) |
| 8 | 2 | `i16` | `ascent` | Typographic ascent above baseline (e.g. `14`) |
| 10 | 2 | `u16` | `descent` | Typographic descent below baseline (e.g. `2`) |
| 12 | 2 | `u16` | `line_gap` | Inter-line spacing (e.g. `2`) |
| 14 | 2 | `u16` | `fallback_index` | Glyph index for missing codepoints (`□`, `< count`) |
| 16 | 4 | `u32` | `table_offset` | Byte offset to glyph index table (`24`) |
| 20 | 4 | `u32` | `bitmap_offset` | Byte offset to packed A4 glyph bitmaps |

## Glyph Record (20 bytes each, sorted by ascending `unicode`)

| Offset | Size | Type | Field | Description |
|---|---|---|---|---|
| 0 | 4 | `u32` | `unicode` | Valid Unicode scalar value |
| 4 | 2 | `u16` | `advance` | Horizontal pen advance in pixels |
| 6 | 2 | `i16` | `bearing_x` | Horizontal bearing from pen x to glyph left |
| 8 | 2 | `i16` | `bearing_y` | Vertical bearing from baseline up to glyph top |
| 10 | 2 | `u16` | `width` | Bitmap width in pixels |
| 12 | 2 | `u16` | `height` | Bitmap height in pixels |
| 14 | 2 | `u16` | `reserved` | `0` |
| 16 | 4 | `u32` | `data_offset` | Offset from `bitmap_offset` |

## Pixel Encoding & Blending

Each glyph row stores `ceil(width / 2)` bytes of 4-bit coverage values (`0..=15`):

- High nibble (`byte >> 4`) is even pixel `x`; low nibble (`byte & 0x0F`) is odd pixel `x + 1`.
- For odd `width`, the trailing low nibble of each row is zero.
- Packed directly from 4x supersampled monochrome PBM (`tools/liuli-fontpack/source/atlas.pbm`): each `4x4` block with `n` (`0..=16`) set bits maps to `a = (15 * n + 8) / 16`.
- RGB565 channel blend: `out = (fg * a + bg * (15 - a) + 7) / 15` independently across R5, G6, B5.

## Dual-Slot Fonts (`FontSet` & `FontId`)

`liuli` supports two compile-time `LF4 v1` slots grouped in `FontSet<'f>` (`FontId::Body16` primary slot and `FontId::Small12` secondary slot):
- Line wrapping supports `\n` / `\r\n`, optional ellipsis truncation (`max_lines`), negative horizontal bearings, and horizontal alignment (`TextAlign::Start`, `TextAlign::Center`, `TextAlign::End`).
- Missing codepoints fall back deterministically to the font's `fallback_index` glyph (`□`).

## CLI Tool & Embedded Assets (`tools/liuli-fontpack`)

- `cargo run -p liuli-fontpack --locked -- pack`: rebuilds both `apps/liuli-settings/assets/settings.lf4` (Body16) and `apps/liuli-settings/assets/small12.lf4` (Small12) reproducibly from 4x supersampled PBM/TSV sources (`tools/liuli-fontpack/source/RECIPE.md` + `rasterize-dwrite.ps1`).
- `cargo run -p liuli-fontpack --locked -- verify`: rebuilds both slots in memory, validates charset completeness (`charset.txt` / `charset_small12.txt`) and A4 packing constraints, verifies combined size `<= 32,768` bytes, and asserts byte-for-byte equality with checked-in `.lf4` binaries:
  - `Body16` (`apps/liuli-settings/assets/settings.lf4`): `221` glyphs, `16,331` bytes, `117` CJK glyphs with intermediate A4 coverage (`SHA256: F59EAB982B4F76203B035782CAEA82871B20114CD423A55D6398A56F6D4F407C`).
  - `Small12` (`apps/liuli-settings/assets/small12.lf4`): `178` glyphs, `10,915` bytes, `74` CJK glyphs with intermediate A4 coverage (`SHA256: BB5D667AA62450F5F4F7360E82E72BC1B4B4BB4AF4D8DA3FE42857E109309CEE`).
  - **Combined LF4 payload**: `27,246 / 32,768` bytes (`26.61 KiB`).
