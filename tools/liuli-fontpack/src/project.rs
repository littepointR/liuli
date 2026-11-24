use crate::pack::{ENTRY_SIZE, HEADER_SIZE, LF4_MAGIC, LF4_VERSION};
use ab_glyph_rasterizer::{Point, Rasterizer, point};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

const LF4_VERSION_V2: u16 = 2;
const LF4_V2_HEADER_SIZE: usize = 32;
const LF4_COMPACT_ENTRY_SIZE: usize = 12;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
enum PackFormat {
    A1 = 1,
    A2 = 2,
    A4 = 4,
    A8 = 8,
    LcdRgb = 12,
}

impl PackFormat {
    fn parse(raw: Option<&str>) -> Result<Self, String> {
        match raw {
            None | Some("a4") => Ok(Self::A4),
            Some("a1") => Ok(Self::A1),
            Some("a2") => Ok(Self::A2),
            Some("a8") => Ok(Self::A8),
            Some("lcd_rgb") => Ok(Self::LcdRgb),
            Some(other) => Err(format!("unknown font format '{other}'")),
        }
    }

    fn row_bytes(self, width: u16) -> usize {
        let w = usize::from(width);
        match self {
            Self::A1 => w.div_ceil(8),
            Self::A2 => w.div_ceil(4),
            Self::A4 => w.div_ceil(2),
            Self::A8 => w,
            Self::LcdRgb => (w * 3).div_ceil(2),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    budget: usize,
    font: Vec<Spec>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    source: PathBuf,
    #[serde(default)]
    charset: Option<PathBuf>,
    #[serde(default)]
    unicode_ranges: Vec<String>,
    #[serde(default)]
    skip_missing: bool,
    output: PathBuf,
    px: u16,
    fallback: char,
    #[serde(default)]
    face_index: u32,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    rle: bool,
    #[serde(default)]
    compact: bool,
    #[serde(default)]
    subpixel_phases: u8,
    #[serde(default)]
    kerning: bool,
    #[serde(default)]
    y_hint: bool,
    #[serde(default)]
    stem_darken: f32,
    #[serde(default)]
    dedup: bool,
    #[serde(default)]
    v2: bool,
}

struct Outline {
    raster: Rasterizer,
    scale_x: f32,
    scale_y: f32,
    left: f32,
    top: f32,
    cursor: Point,
    start: Point,
}
impl Outline {
    fn map(&self, x: f32, y: f32) -> Point {
        point(x * self.scale_x - self.left, self.top - y * self.scale_y)
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cursor = self.map(x, y);
        self.start = self.cursor;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let end = self.map(x, y);
        self.raster.draw_line(self.cursor, end);
        self.cursor = end;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let end = self.map(x, y);
        self.raster.draw_quad(self.cursor, self.map(x1, y1), end);
        self.cursor = end;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let end = self.map(x, y);
        self.raster
            .draw_cubic(self.cursor, self.map(x1, y1), self.map(x2, y2), end);
        self.cursor = end;
    }
    fn close(&mut self) {
        self.raster.draw_line(self.cursor, self.start);
        self.cursor = self.start;
    }
}

/// Encodes `raw` using the PackBits RLE framing decoded by `liuli_text::decode_rle_rows`.
fn encode_rle(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0usize;
    while i < raw.len() {
        let mut run = 1usize;
        while run < 129 && i + run < raw.len() && raw[i + run] == raw[i] {
            run += 1;
        }
        if run >= 2 {
            out.push(128 + (run - 2) as u8);
            out.push(raw[i]);
            i += run;
            continue;
        }
        let mut lit = 1usize;
        while lit < 128 && i + lit < raw.len() {
            if i + lit + 2 < raw.len()
                && raw[i + lit] == raw[i + lit + 1]
                && raw[i + lit] == raw[i + lit + 2]
            {
                break;
            }
            lit += 1;
        }
        out.push((lit - 1) as u8);
        out.extend_from_slice(&raw[i..i + lit]);
        i += lit;
    }
    out
}

#[inline]
fn darken_cov(cov: f32, stem_darken: f32) -> f32 {
    let c = cov.clamp(0.0, 1.0);
    if stem_darken > 0.0 && c > 0.0 && c < 1.0 {
        c.powf(1.0 / (1.0 + stem_darken.clamp(0.0, 0.5)))
    } else {
        c
    }
}

fn collect_kerning(face: &Face<'_>, charset: &BTreeSet<char>, scale: f32) -> Vec<(u32, u32, i16)> {
    let Some(kern) = face.tables().kern else {
        return Vec::new();
    };
    let gids: Vec<(char, GlyphId)> = charset
        .iter()
        .filter_map(|&ch| face.glyph_index(ch).map(|gid| (ch, gid)))
        .collect();
    let mut pairs = Vec::new();
    for &(l_ch, l_gid) in &gids {
        for &(r_ch, r_gid) in &gids {
            let mut sum = 0i32;
            for sub in kern.subtables {
                if sub.horizontal
                    && !sub.variable
                    && let Some(k) = sub.glyphs_kerning(l_gid, r_gid)
                {
                    sum += i32::from(k);
                }
            }
            if sum != 0 {
                let q2 = (sum as f32 * scale * 4.0)
                    .round()
                    .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                    as i16;
                if q2 != 0 {
                    pairs.push((l_ch as u32, r_ch as u32, q2));
                }
            }
        }
    }
    pairs.sort_unstable_by_key(|&(l, r, _)| (l, r));
    pairs.dedup_by_key(|entry| (entry.0, entry.1));
    pairs
}

fn rasterize_phase(
    face: &Face<'_>,
    id: GlyphId,
    scale_x: f32,
    scale_y: f32,
    left: i32,
    top: i32,
    width: u16,
    height: u16,
    phase_dx: f32,
    format: PackFormat,
    stem_darken: f32,
) -> Result<Vec<u8>, String> {
    let stride = format.row_bytes(width);
    let mut raw = vec![0u8; stride * usize::from(height)];
    if raw.is_empty() {
        return Ok(raw);
    }
    match format {
        PackFormat::LcdRgb => {
            let overscan_w = usize::from(width) * 3;
            let mut outline = Outline {
                raster: Rasterizer::new(overscan_w, height.into()),
                scale_x: scale_x * 3.0,
                scale_y,
                left: (left as f32 - phase_dx) * 3.0,
                top: top as f32,
                cursor: point(0., 0.),
                start: point(0., 0.),
            };
            face.outline_glyph(id, &mut outline)
                .ok_or("unsupported glyph outline")?;
            let mut line_cov = vec![0.0f32; overscan_w * usize::from(height)];
            outline.raster.for_each_pixel_2d(|x, y, cov| {
                line_cov[y as usize * overscan_w + x as usize] = cov.clamp(0.0, 1.0);
            });
            for y in 0..usize::from(height) {
                let row = &line_cov[y * overscan_w..(y + 1) * overscan_w];
                let out_row = &mut raw[y * stride..(y + 1) * stride];
                for x in 0..usize::from(width) {
                    for ch in 0..3 {
                        let idx = x * 3 + ch;
                        let l = if idx > 0 { row[idx - 1] } else { 0.0 };
                        let m = row[idx];
                        let r = if idx + 1 < overscan_w {
                            row[idx + 1]
                        } else {
                            0.0
                        };
                        let filtered = darken_cov(0.25 * l + 0.5 * m + 0.25 * r, stem_darken);
                        let q = (filtered * 15.0).round() as u8;
                        let nib = x * 3 + ch;
                        out_row[nib >> 1] |= if (nib & 1) == 0 { q << 4 } else { q };
                    }
                }
            }
        }
        _ => {
            let mut outline = Outline {
                raster: Rasterizer::new(width.into(), height.into()),
                scale_x,
                scale_y,
                left: left as f32 - phase_dx,
                top: top as f32,
                cursor: point(0., 0.),
                start: point(0., 0.),
            };
            face.outline_glyph(id, &mut outline)
                .ok_or("unsupported glyph outline")?;
            outline.raster.for_each_pixel_2d(|x, y, coverage| {
                let cov = darken_cov(coverage, stem_darken);
                let row_off = y as usize * stride;
                let xu = x as usize;
                match format {
                    PackFormat::A1 => {
                        if cov >= 0.5 {
                            raw[row_off + (xu >> 3)] |= 1 << (7 - (xu & 7));
                        }
                    }
                    PackFormat::A2 => {
                        let val = (cov * 3.0).round() as u8;
                        let shift = 6 - ((xu & 3) << 1);
                        raw[row_off + (xu >> 2)] |= (val & 0x03) << shift;
                    }
                    PackFormat::A4 => {
                        let val = (cov * 15.0).round() as u8;
                        raw[row_off + (xu >> 1)] |= if (xu & 1) == 0 { val << 4 } else { val };
                    }
                    PackFormat::A8 => {
                        raw[row_off + xu] = (cov * 255.0).round() as u8;
                    }
                    PackFormat::LcdRgb => unreachable!(),
                }
            });
        }
    }
    Ok(raw)
}

fn rasterize(root: &Path, spec: &Spec, budget: usize) -> Result<Vec<u8>, String> {
    if !(1..=256).contains(&spec.px) {
        return Err("px must be in 1..=256".into());
    }
    let format = PackFormat::parse(spec.format.as_deref())?;
    let phases = if spec.subpixel_phases == 0 {
        1
    } else {
        spec.subpixel_phases
    };
    if !matches!(phases, 1 | 2 | 4) {
        return Err("subpixel_phases must be 1, 2, or 4".into());
    }
    let use_v2 =
        spec.v2 || spec.format.is_some() || spec.rle || spec.compact || phases > 1 || spec.kerning;

    let source = root.join(&spec.source);
    let bytes = fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let face =
        Face::parse(&bytes, spec.face_index).map_err(|e| format!("{}: {e:?}", source.display()))?;
    let text = if let Some(cs_path) = &spec.charset {
        fs::read_to_string(root.join(cs_path)).map_err(|e| format!("{}: {e}", cs_path.display()))?
    } else {
        if spec.unicode_ranges.is_empty() {
            return Err("spec requires `charset`, `unicode_ranges`, or both".into());
        }
        String::new()
    };
    let mut raw_charset: BTreeSet<char> = text
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n'))
        .chain([spec.fallback])
        .collect();
    for spec_range in &spec.unicode_ranges {
        parse_unicode_range(spec_range, &mut raw_charset)?;
    }
    let charset: BTreeSet<char> = if spec.skip_missing {
        if face.glyph_index(spec.fallback).is_none() {
            return Err(format!(
                "missing fallback glyph U+{:04X} in {}",
                spec.fallback as u32,
                source.display()
            ));
        }
        raw_charset
            .into_iter()
            .filter(|&ch| ch == spec.fallback || face.glyph_index(ch).is_some())
            .collect()
    } else {
        raw_charset
    };
    let scale_x = f32::from(spec.px) / f32::from(face.units_per_em());
    let scale_y = if spec.y_hint {
        let cap = face
            .capital_height()
            .filter(|&h| h > 0)
            .unwrap_or(face.ascender());
        if cap > 0 {
            (f32::from(cap) * scale_x).round().max(1.0) / f32::from(cap)
        } else {
            scale_x
        }
    } else {
        scale_x
    };
    let count = u16::try_from(charset.len()).map_err(|_| "too many glyphs")?;
    let fallback_index = charset.iter().position(|&c| c == spec.fallback).unwrap() as u16;
    let ascent = (f32::from(face.ascender()) * scale_y).ceil() as i16;
    let descent = (-f32::from(face.descender()) * scale_y).ceil() as u16;
    let gap = (f32::from(face.line_gap()) * scale_y).round() as u16;

    if !use_v2 {
        let bitmap_offset = HEADER_SIZE + charset.len() * ENTRY_SIZE;
        if bitmap_offset > budget {
            return Err("font index exceeds byte budget".into());
        }
        let mut out = Vec::with_capacity(bitmap_offset);
        out.extend_from_slice(LF4_MAGIC);
        for word in [
            LF4_VERSION,
            count,
            ascent as u16,
            descent,
            gap,
            fallback_index,
        ] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        out.extend_from_slice(&(bitmap_offset as u32).to_le_bytes());
        let mut bitmaps = Vec::new();
        let mut dedup_map: HashMap<Vec<u8>, usize> = HashMap::new();
        for ch in charset {
            let id = face.glyph_index(ch).ok_or_else(|| {
                format!("missing glyph U+{:04X} in {}", ch as u32, source.display())
            })?;
            let (left, top, width, height) =
                face.glyph_bounding_box(id).map_or((0, 0, 0, 0), |b| {
                    let left = (f32::from(b.x_min) * scale_x).floor() as i32;
                    let top = (f32::from(b.y_max) * scale_y).ceil() as i32;
                    (
                        left,
                        top,
                        (f32::from(b.x_max) * scale_x).ceil() as i32 - left,
                        top - (f32::from(b.y_min) * scale_y).floor() as i32,
                    )
                });
            let width = u16::try_from(width).map_err(|_| "glyph width out of range")?;
            let height = u16::try_from(height).map_err(|_| "glyph height out of range")?;
            let bearing_x = i16::try_from(left).map_err(|_| "bearing overflow")?;
            let bearing_y = i16::try_from(top).map_err(|_| "bearing overflow")?;
            let advance = (f32::from(face.glyph_hor_advance(id).ok_or("missing advance")?)
                * scale_x)
                .round() as u16;
            out.extend_from_slice(&(ch as u32).to_le_bytes());
            for word in [
                advance,
                bearing_x as u16,
                bearing_y as u16,
                width,
                height,
                0,
            ] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            if usize::from(width) * usize::from(height) > 4 * 1024 * 1024 {
                return Err("glyph raster too large".into());
            }
            let raw = if width == 0 || height == 0 {
                Vec::new()
            } else {
                rasterize_phase(
                    &face,
                    id,
                    scale_x,
                    scale_y,
                    left,
                    top,
                    width,
                    height,
                    0.0,
                    PackFormat::A4,
                    spec.stem_darken,
                )?
            };
            let start = if raw.is_empty() {
                bitmaps.len()
            } else if spec.dedup {
                if let Some(&existing) = dedup_map.get(&raw) {
                    existing
                } else {
                    let pos = bitmaps.len();
                    if pos + raw.len() > budget - bitmap_offset {
                        return Err(format!(
                            "glyph U+{:04X} exceeds font byte budget",
                            ch as u32
                        ));
                    }
                    bitmaps.extend_from_slice(&raw);
                    dedup_map.insert(raw, pos);
                    pos
                }
            } else {
                let pos = bitmaps.len();
                if pos + raw.len() > budget - bitmap_offset {
                    return Err(format!(
                        "glyph U+{:04X} exceeds font byte budget",
                        ch as u32
                    ));
                }
                bitmaps.extend_from_slice(&raw);
                pos
            };
            out.extend_from_slice(
                &u32::try_from(start)
                    .map_err(|_| "bitmap offset overflow")?
                    .to_le_bytes(),
            );
        }
        out.extend(bitmaps);
        return Ok(out);
    }

    let advance_q2 = phases > 1 || spec.kerning;
    let kern_pairs = if spec.kerning {
        collect_kerning(&face, &charset, scale_x)
    } else {
        Vec::new()
    };
    let kern_offset = if kern_pairs.is_empty() {
        0usize
    } else {
        LF4_V2_HEADER_SIZE
    };
    let mut kern_bytes = Vec::new();
    if !kern_pairs.is_empty() {
        kern_bytes.extend_from_slice(&(kern_pairs.len() as u32).to_le_bytes());
        for (l, r, k) in kern_pairs {
            kern_bytes.extend_from_slice(&l.to_le_bytes());
            kern_bytes.extend_from_slice(&r.to_le_bytes());
            kern_bytes.extend_from_slice(&k.to_le_bytes());
            kern_bytes.extend_from_slice(&0u16.to_le_bytes());
        }
    }
    let table_offset = LF4_V2_HEADER_SIZE + kern_bytes.len();
    let entry_size = if spec.compact {
        LF4_COMPACT_ENTRY_SIZE
    } else {
        ENTRY_SIZE
    };
    let bitmap_offset = table_offset + charset.len() * entry_size;
    if bitmap_offset > budget {
        return Err("font index exceeds byte budget".into());
    }

    let phase_bits = match phases {
        2 => 1u8 << 1,
        4 => 2u8 << 1,
        _ => 0,
    };
    let flags = u8::from(spec.compact) | phase_bits | (u8::from(advance_q2) << 3);

    let mut out = Vec::with_capacity(bitmap_offset);
    out.extend_from_slice(LF4_MAGIC);
    for word in [
        LF4_VERSION_V2,
        count,
        ascent as u16,
        descent,
        gap,
        fallback_index,
    ] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&(table_offset as u32).to_le_bytes());
    out.extend_from_slice(&(bitmap_offset as u32).to_le_bytes());
    out.push(format as u8);
    out.push(u8::from(spec.rle));
    out.push(flags);
    out.push(0);
    out.extend_from_slice(&(kern_offset as u32).to_le_bytes());
    out.extend_from_slice(&kern_bytes);

    let mut bitmaps = Vec::new();
    let mut dedup_map: HashMap<Vec<u8>, u32> = HashMap::new();
    for ch in charset {
        let id = face
            .glyph_index(ch)
            .ok_or_else(|| format!("missing glyph U+{:04X} in {}", ch as u32, source.display()))?;
        let (left, top, width, height) = face.glyph_bounding_box(id).map_or((0, 0, 0, 0), |b| {
            let left = (f32::from(b.x_min) * scale_x).floor() as i32;
            let top = (f32::from(b.y_max) * scale_y).ceil() as i32;
            let base_w = (f32::from(b.x_max) * scale_x).ceil() as i32 - left;
            let base_h = top - (f32::from(b.y_min) * scale_y).floor() as i32;
            let padded_w = if phases > 1 && base_w > 0 && base_h > 0 {
                base_w + 1
            } else {
                base_w
            };
            (left, top, padded_w, base_h)
        });
        let width = u16::try_from(width).map_err(|_| "glyph width out of range")?;
        let height = u16::try_from(height).map_err(|_| "glyph height out of range")?;
        let bearing_x = i16::try_from(left).map_err(|_| "bearing overflow")?;
        let bearing_y = i16::try_from(top).map_err(|_| "bearing overflow")?;
        let hor = f32::from(face.glyph_hor_advance(id).ok_or("missing advance")?) * scale_x;
        let advance = if advance_q2 {
            (hor * 4.0).round() as u16
        } else {
            hor.round() as u16
        };
        if usize::from(width) * usize::from(height) * usize::from(phases) > 4 * 1024 * 1024 {
            return Err("glyph raster too large".into());
        }

        let mut glyph_blob = Vec::new();
        if width > 0 && height > 0 {
            let mut raw_all = Vec::new();
            for phase in 0..phases {
                let phase_dx = f32::from(phase) / f32::from(phases);
                let mut part = rasterize_phase(
                    &face,
                    id,
                    scale_x,
                    scale_y,
                    left,
                    top,
                    width,
                    height,
                    phase_dx,
                    format,
                    spec.stem_darken,
                )?;
                raw_all.append(&mut part);
            }
            if spec.rle {
                let compressed = encode_rle(&raw_all);
                let clen = u16::try_from(compressed.len())
                    .map_err(|_| "compressed glyph exceeds 64 KiB")?;
                glyph_blob.extend_from_slice(&clen.to_le_bytes());
                glyph_blob.extend_from_slice(&compressed);
            } else {
                glyph_blob = raw_all;
            }
        }

        let offset = if glyph_blob.is_empty() {
            bitmaps.len() as u32
        } else if spec.dedup {
            if let Some(&existing) = dedup_map.get(&glyph_blob) {
                existing
            } else {
                let pos = u32::try_from(bitmaps.len()).map_err(|_| "bitmap offset overflow")?;
                if bitmaps.len() + glyph_blob.len() > budget - bitmap_offset {
                    return Err(format!(
                        "glyph U+{:04X} exceeds font byte budget",
                        ch as u32
                    ));
                }
                bitmaps.extend_from_slice(&glyph_blob);
                dedup_map.insert(glyph_blob, pos);
                pos
            }
        } else {
            let pos = u32::try_from(bitmaps.len()).map_err(|_| "bitmap offset overflow")?;
            if bitmaps.len() + glyph_blob.len() > budget - bitmap_offset {
                return Err(format!(
                    "glyph U+{:04X} exceeds font byte budget",
                    ch as u32
                ));
            }
            bitmaps.extend_from_slice(&glyph_blob);
            pos
        };

        if spec.compact {
            let adv_u8 = u8::try_from(advance).map_err(|_| "compact advance overflow")?;
            let bx_i8 = i8::try_from(bearing_x).map_err(|_| "compact bearing_x overflow")?;
            let by_i8 = i8::try_from(bearing_y).map_err(|_| "compact bearing_y overflow")?;
            let w_u8 = u8::try_from(width).map_err(|_| "compact width overflow")?;
            let h_u8 = u8::try_from(height).map_err(|_| "compact height overflow")?;
            if offset > 0xFF_FFFF {
                return Err("compact bitmap offset exceeds 24 bits".into());
            }
            out.extend_from_slice(&(ch as u32).to_le_bytes());
            out.push(adv_u8);
            out.push(bx_i8 as u8);
            out.push(by_i8 as u8);
            out.push(w_u8);
            out.push(h_u8);
            out.push((offset & 0xFF) as u8);
            out.push(((offset >> 8) & 0xFF) as u8);
            out.push(((offset >> 16) & 0xFF) as u8);
        } else {
            out.extend_from_slice(&(ch as u32).to_le_bytes());
            for word in [
                advance,
                bearing_x as u16,
                bearing_y as u16,
                width,
                height,
                0,
            ] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            out.extend_from_slice(&offset.to_le_bytes());
        }
    }
    out.extend(bitmaps);
    Ok(out)
}

pub fn run(path: &Path, verify: bool) -> Result<(), String> {
    let config = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let project: Project = toml::from_str(&config).map_err(|e| e.to_string())?;
    if project.font.is_empty() {
        return Err("at least one [[font]] required".into());
    }
    let root = path.parent().unwrap_or(Path::new("."));
    let mut staged = Vec::new();
    let mut remaining = project.budget;
    let mut outputs = BTreeSet::new();
    for spec in &project.font {
        let dest = root.join(&spec.output);
        if !outputs.insert(dest.clone()) {
            return Err(format!("duplicate output {}", dest.display()));
        }
        let data = rasterize(root, spec, remaining)?;
        remaining -= data.len();
        staged.push((dest, data));
    }
    for (dest, data) in staged {
        if verify {
            if fs::read(&dest).map_err(|e| format!("{}: {e}", dest.display()))? != data {
                return Err(format!("font mismatch: {}", dest.display()));
            }
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&dest, &data).map_err(|e| format!("{}: {e}", dest.display()))?;
        }
        println!("{} ({} bytes)", dest.display(), data.len());
    }
    Ok(())
}

fn parse_scalar_token(raw: &str) -> Result<u32, String> {
    let s = raw.trim();
    if let Some(hex) = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .or_else(|| s.strip_prefix("U+"))
        .or_else(|| s.strip_prefix("u+"))
    {
        u32::from_str_radix(hex, 16).map_err(|e| format!("invalid hex scalar '{s}': {e}"))
    } else {
        s.parse::<u32>()
            .map_err(|e| format!("invalid scalar '{s}': {e}"))
    }
}

pub fn parse_unicode_range(expr: &str, out: &mut BTreeSet<char>) -> Result<(), String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let (start_str, end_str) = if let Some((a, b)) = trimmed.split_once("..=") {
        (a, b)
    } else if let Some((a, b)) = trimmed.split_once('-') {
        (a, b)
    } else {
        let cp = parse_scalar_token(trimmed)?;
        let ch = char::from_u32(cp).ok_or_else(|| format!("invalid Unicode scalar U+{cp:04X}"))?;
        out.insert(ch);
        return Ok(());
    };
    let start = parse_scalar_token(start_str)?;
    let end = parse_scalar_token(end_str)?;
    if start > end || end > 0x10_FFFF {
        return Err(format!("invalid unicode range '{expr}'"));
    }
    for cp in start..=end {
        if let Some(ch) = char::from_u32(cp) {
            out.insert(ch);
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct Lf4InspectReport {
    pub version: u16,
    pub glyph_count: u16,
    pub ascent: i16,
    pub descent: u16,
    pub line_gap: u16,
    pub line_height: u32,
    pub format_name: &'static str,
    pub encoding_name: &'static str,
    pub compact_index: bool,
    pub subpixel_phases: u8,
    pub advance_q2: bool,
    pub kern_pairs: usize,
    pub header_bytes: usize,
    pub kern_bytes: usize,
    pub index_bytes: usize,
    pub bitmap_bytes: usize,
    pub total_bytes: usize,
    pub uncompressed_bitmap_bytes: usize,
    pub max_glyph_bytes: usize,
}

pub fn inspect_bytes(bytes: &[u8]) -> Result<Lf4InspectReport, String> {
    if bytes.len() < HEADER_SIZE || &bytes[..4] != LF4_MAGIC {
        return Err("not a valid LF4 asset".into());
    }
    let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let i16_at = |o: usize| i16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32_at =
        |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);

    let version = u16_at(4);
    let glyph_count = u16_at(6);
    let ascent = i16_at(8);
    let descent = u16_at(10);
    let line_gap = u16_at(12);
    let line_height = (ascent.max(0) as u32) + u32::from(descent) + u32::from(line_gap);
    let table_offset = u32_at(16) as usize;
    let bitmap_offset = u32_at(20) as usize;

    let (format, is_rle, compact_index, subpixel_phases, advance_q2, kern_offset, header_bytes) =
        if version == LF4_VERSION_V2 {
            if bytes.len() < LF4_V2_HEADER_SIZE {
                return Err("truncated LF4 v2 header".into());
            }
            let fmt = match bytes[24] {
                1 => PackFormat::A1,
                2 => PackFormat::A2,
                4 => PackFormat::A4,
                8 => PackFormat::A8,
                12 => PackFormat::LcdRgb,
                other => return Err(format!("unknown format {other}")),
            };
            let is_rle = bytes[25] == 1;
            let flags = bytes[26];
            let compact = (flags & 0x01) != 0;
            let phases = match (flags >> 1) & 0x03 {
                0 => 1,
                1 => 2,
                2 => 4,
                _ => return Err("unsupported subpixel phases".into()),
            };
            let q2 = (flags & 0x08) != 0;
            let ko = u32_at(28) as usize;
            (fmt, is_rle, compact, phases, q2, ko, LF4_V2_HEADER_SIZE)
        } else if version == LF4_VERSION {
            (PackFormat::A4, false, false, 1, false, 0, HEADER_SIZE)
        } else {
            return Err(format!("unsupported LF4 version {version}"));
        };

    let kern_pairs = if kern_offset != 0 {
        u32_at(kern_offset) as usize
    } else {
        0
    };
    let kern_bytes = if kern_offset != 0 {
        4 + kern_pairs * 12
    } else {
        0
    };
    let entry_size = if compact_index {
        LF4_COMPACT_ENTRY_SIZE
    } else {
        ENTRY_SIZE
    };
    let index_bytes = usize::from(glyph_count) * entry_size;
    let mut uncompressed_bitmap_bytes = 0usize;
    let mut max_glyph_bytes = 0usize;
    for i in 0..usize::from(glyph_count) {
        let off = table_offset + i * entry_size;
        let (w, h) = if compact_index {
            (u16::from(bytes[off + 7]), u16::from(bytes[off + 8]))
        } else {
            (u16_at(off + 10), u16_at(off + 12))
        };
        let raw_len = format.row_bytes(w) * usize::from(h) * usize::from(subpixel_phases);
        uncompressed_bitmap_bytes += raw_len;
        max_glyph_bytes = max_glyph_bytes.max(raw_len);
    }

    let format_name = match format {
        PackFormat::A1 => "A1",
        PackFormat::A2 => "A2",
        PackFormat::A4 => "A4",
        PackFormat::A8 => "A8",
        PackFormat::LcdRgb => "LcdRgb",
    };

    Ok(Lf4InspectReport {
        version,
        glyph_count,
        ascent,
        descent,
        line_gap,
        line_height,
        format_name,
        encoding_name: if is_rle { "RLE" } else { "Raw" },
        compact_index,
        subpixel_phases,
        advance_q2,
        kern_pairs,
        header_bytes,
        kern_bytes,
        index_bytes,
        bitmap_bytes: bytes.len().saturating_sub(bitmap_offset),
        total_bytes: bytes.len(),
        uncompressed_bitmap_bytes,
        max_glyph_bytes,
    })
}

pub fn inspect_path(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let r = inspect_bytes(&bytes)?;
    let ratio = if r.uncompressed_bitmap_bytes > 0 {
        (r.bitmap_bytes as f64) * 100.0 / (r.uncompressed_bitmap_bytes as f64)
    } else {
        100.0
    };
    let avg_glyph = if r.glyph_count > 0 {
        r.uncompressed_bitmap_bytes / usize::from(r.glyph_count)
    } else {
        0
    };
    let l2_64 = (avg_glyph.max(64) * 64).max(r.max_glyph_bytes * 4);
    let l2_128 = (avg_glyph.max(64) * 128).max(r.max_glyph_bytes * 4);
    Ok(format!(
        "{path}: LF4 v{v} | glyphs={cnt} | fmt={fmt} enc={enc} compact={cmp} phases={ph} q2={q2} kern={kn}\n\
         metrics: ascent={asc} descent={desc} gap={gap} line_height={lh}\n\
         sizes:   total={tot} B (header={hdr} B, kern={kb} B, index={ib} B, bitmaps={bb} B / raw={raw} B, {ratio:.1}%)\n\
         glyphs:  avg_raw={avg} B, max_raw={maxg} B\n\
         non-XIP cache budget guide:\n\
         - tiny (32 glyphs):  L0=32*6B (192B), L1=64 slots, L2={l2_32} B pool, scratch>={scr} B\n\
         - norm (64 glyphs):  L0=64*6B (384B), L1=128 slots, L2={l2_64} B pool, scratch>={scr} B\n\
         - rich (128 glyphs): L0=64*6B (384B), L1=256 slots, L2={l2_128} B pool, scratch>={scr} B",
        path = path.display(),
        v = r.version,
        cnt = r.glyph_count,
        fmt = r.format_name,
        enc = r.encoding_name,
        cmp = r.compact_index,
        ph = r.subpixel_phases,
        q2 = r.advance_q2,
        kn = r.kern_pairs,
        asc = r.ascent,
        desc = r.descent,
        gap = r.line_gap,
        lh = r.line_height,
        tot = r.total_bytes,
        hdr = r.header_bytes,
        kb = r.kern_bytes,
        ib = r.index_bytes,
        bb = r.bitmap_bytes,
        raw = r.uncompressed_bitmap_bytes,
        ratio = ratio,
        avg = avg_glyph,
        maxg = r.max_glyph_bytes,
        l2_32 = (avg_glyph.max(64) * 32).max(r.max_glyph_bytes * 2),
        l2_64 = l2_64,
        l2_128 = l2_128,
        scr = r.max_glyph_bytes.max(128),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_rle_test(rle: &[u8], expected_len: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < rle.len() {
            let ctrl = rle[pos];
            pos += 1;
            if ctrl < 128 {
                let len = usize::from(ctrl) + 1;
                out.extend_from_slice(&rle[pos..pos + len]);
                pos += len;
            } else {
                let len = usize::from(ctrl - 128) + 2;
                let v = rle[pos];
                pos += 1;
                out.extend(std::iter::repeat_n(v, len));
            }
        }
        assert_eq!(out.len(), expected_len);
        out
    }

    #[test]
    fn rle_encoder_roundtrips_edge_patterns() {
        for raw in [
            vec![0u8; 300],
            (0..260u16).map(|i| (i % 17) as u8).collect(),
            vec![0, 0, 1, 2, 3, 255, 255, 255, 4, 0, 0],
        ] {
            let encoded = encode_rle(&raw);
            let decoded = decode_rle_test(&encoded, raw.len());
            assert_eq!(decoded, raw);
        }
    }

    #[test]
    fn pack_v2_multiformat_rle_compact_phases_hint_darken_and_dedup() {
        let tmp = std::env::temp_dir().join("liuli-fontpack-v2-test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let font_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/liuli-widget-demo/assets/example.ttf");
        let charset = tmp.join("charset.txt");
        fs::write(&charset, " .01Aa\n").unwrap();

        for (fmt, phases, rle, compact) in [
            ("a1", 1, true, true),
            ("a2", 2, true, true),
            ("a4", 4, false, true),
            ("a8", 1, true, false),
            ("lcd_rgb", 2, true, true),
        ] {
            let spec = Spec {
                source: font_src.clone(),
                charset: Some(charset.clone()),
                unicode_ranges: Vec::new(),
                skip_missing: false,
                output: tmp.join(format!("out-{fmt}.lf4")),
                px: 16,
                fallback: '0',
                face_index: 0,
                format: Some(fmt.into()),
                rle,
                compact,
                subpixel_phases: phases,
                kerning: true,
                y_hint: true,
                stem_darken: 0.25,
                dedup: true,
                v2: true,
            };
            let bytes = rasterize(&tmp, &spec, 65536).unwrap();
            assert_eq!(&bytes[..4], b"LLF4");
            assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), LF4_VERSION_V2);
            let expected_fmt = PackFormat::parse(Some(fmt)).unwrap() as u8;
            assert_eq!(bytes[24], expected_fmt);
            assert_eq!(bytes[25], u8::from(rle));
            assert_eq!(bytes[26] & 0x01, u8::from(compact));
            let phase_code = (bytes[26] >> 1) & 0x03;
            assert_eq!(
                phase_code,
                match phases {
                    2 => 1,
                    4 => 2,
                    _ => 0,
                }
            );
            let rep = inspect_bytes(&bytes).unwrap();
            assert_eq!(rep.version, LF4_VERSION_V2);
            assert_eq!(rep.compact_index, compact);
            assert_eq!(rep.subpixel_phases, phases);
        }

        // Test unicode_ranges + skip_missing without an external charset file.
        let range_spec = Spec {
            source: font_src,
            charset: None,
            unicode_ranges: vec!["U+0030-U+0032".into(), "U+F8F0".into()],
            skip_missing: true,
            output: tmp.join("range.lf4"),
            px: 14,
            fallback: '0',
            face_index: 0,
            format: Some("a4".into()),
            rle: true,
            compact: true,
            subpixel_phases: 1,
            kerning: false,
            y_hint: false,
            stem_darken: 0.0,
            dedup: true,
            v2: true,
        };
        let range_bytes = rasterize(&tmp, &range_spec, 65536).unwrap();
        let rep = inspect_bytes(&range_bytes).unwrap();
        assert_eq!(rep.glyph_count, 3);
        let _ = fs::remove_dir_all(&tmp);
    }
}
