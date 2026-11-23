# Dual-Size LF4 Font Asset Authoring Recipe

## 1. Source Font & License
- **Source Font**: `C:\Windows\Fonts\NotoSansSC-VF.ttf` (`Noto Sans CJK SC`, SIL Open Font License 1.1, see `FONT-LICENSE.txt`)
- **Face Index**: `0`
- **Source File SHA-256**: `763146584CF0710223441356B4395E279021B0806C196614377A7A0174AE074A`

## 2. Rasterization & Packing Parameters
Rasterized offline on Windows using `tools/liuli-fontpack/source/rasterize-dwrite.ps1` via `dwrite.dll` COM interfaces (`DWriteCreateFactory`, `IDWriteFactory::CreateFontFileReference`, `CreateFontFace`, `CreateGlyphRunAnalysis`), with **zero system font fallback** (`GetGlyphIndices` rejects index `0`):
- **Supersampling**: `4x` (`emSize = TargetPx * 4` DIPs at `pixelsPerDip = 1.0`)
- **DirectWrite Mode**: `DWRITE_RENDERING_MODE_ALIASED` (`1`), `DWRITE_MEASURING_MODE_NATURAL` (`0`), `DWRITE_TEXTURE_ALIASED_1x1` (`0`)
- **Baseline Origin**: `(0.0, 0.0)`
- **Target Cell Trimming & Rounding**:
  - Pixel coverage threshold: `alpha >= 128`
  - Target coordinates via floor division by `4`: `tx = floor(px / 4)`, `ty = floor(py / 4)`
  - `width_px = max_tx - min_tx + 1`, `height_px = max_ty - min_ty + 1`, `bearing_x = min_tx`, `bearing_y = -min_ty`
  - `advance = round(design_metrics.advanceWidth * TargetPx / font_metrics.designUnitsPerEm, AwayFromZero)`
- **Downsampling (`liuli-fontpack`)**: 4x4 subpixel count `n in 0..=16` -> A4 nibble `cov = (15 * n + 8) / 16`

| Slot | Target Px | Em Size (4x) | Ascent | Descent | Line Gap | Line Height | Mode | Glyphs | Output Size |
|---|---|---:|---:|---:|---:|---:|---|---:|---:|
| `Body16` | `16` | `64.0` | `14` | `2` | `2` | `18` | Append-only over 187 v0.1 glyphs (187 base rows & 1024x444 base pixels untouched) | `221` | `16,331 B` |
| `Small12` | `12` | `48.0` | `10` | `2` | `2` | `14` | Fresh full charset (`charset_small12.txt`) | `178` | `10,915 B` |
| **Combined** | — | — | — | — | — | — | Dual-slot budget `<= 32,768 B` | **399** | **27,246 B** |

## 3. Reproducible SHA-256 Hashes (Verified Across Two Consecutive Runs)
- `tools/liuli-fontpack/source/atlas_small12.pbm`: `748D7100A047488D6668999CEFC1B41A1D70CCE417321C7BC49293CD212B8C7B`
- `tools/liuli-fontpack/source/glyphs_small12.tsv`: `22DED6DB93129C5B615773B157A83C9ACA0DE55E303076B92EED6E0ABEE960A4`
- `tools/liuli-fontpack/source/atlas.pbm`: `FF2A3D12B18F02D3F933601DC9D1C5A0B49AF157BA5CE89018091B388BCF469A`
- `tools/liuli-fontpack/source/glyphs.tsv`: `01823FBF65AC613EC9D4DFA52EDFA5B8DCF52C408DED8DCEFF005F3EB10F30C7`
- `apps/liuli-settings/assets/settings.lf4`: `F59EAB982B4F76203B035782CAEA82871B20114CD423A55D6398A56F6D4F407C`
- `apps/liuli-settings/assets/small12.lf4`: `BB5D667AA62450F5F4F7360E82E72BC1B4B4BB4AF4D8DA3FE42857E109309CEE`

## 4. Verification & Packaging Commands
From repository root:
```powershell
cargo run -p liuli-fontpack --offline --locked -- pack
cargo run -p liuli-fontpack --offline --locked -- verify
```
