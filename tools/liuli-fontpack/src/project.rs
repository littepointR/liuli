use crate::pack::{ENTRY_SIZE, HEADER_SIZE, LF4_MAGIC, LF4_VERSION};
use ab_glyph_rasterizer::{Point, Rasterizer, point};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use ttf_parser::{Face, OutlineBuilder};

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
    charset: PathBuf,
    output: PathBuf,
    px: u16,
    fallback: char,
    #[serde(default)]
    face_index: u32,
}

struct Outline {
    raster: Rasterizer,
    scale: f32,
    left: f32,
    top: f32,
    cursor: Point,
    start: Point,
}
impl Outline {
    fn map(&self, x: f32, y: f32) -> Point {
        point(x * self.scale - self.left, self.top - y * self.scale)
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

fn rasterize(root: &Path, spec: &Spec, budget: usize) -> Result<Vec<u8>, String> {
    if !(1..=256).contains(&spec.px) {
        return Err("px must be in 1..=256".into());
    }
    let source = root.join(&spec.source);
    let bytes = fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let face =
        Face::parse(&bytes, spec.face_index).map_err(|e| format!("{}: {e:?}", source.display()))?;
    let text = fs::read_to_string(root.join(&spec.charset))
        .map_err(|e| format!("{}: {e}", spec.charset.display()))?;
    let charset: BTreeSet<_> = text
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n'))
        .chain([spec.fallback])
        .collect();
    let scale = f32::from(spec.px) / f32::from(face.units_per_em());
    let count = u16::try_from(charset.len()).map_err(|_| "too many glyphs")?;
    let fallback_index = charset.iter().position(|&c| c == spec.fallback).unwrap() as u16;
    let bitmap_offset = HEADER_SIZE + charset.len() * ENTRY_SIZE;
    if bitmap_offset > budget {
        return Err("font index exceeds byte budget".into());
    }
    let mut out = Vec::with_capacity(bitmap_offset);
    out.extend_from_slice(LF4_MAGIC);
    let ascent = (f32::from(face.ascender()) * scale).ceil() as i16;
    let descent = (-f32::from(face.descender()) * scale).ceil() as u16;
    let gap = (f32::from(face.line_gap()) * scale).round() as u16;
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
    for ch in charset {
        let id = face
            .glyph_index(ch)
            .ok_or_else(|| format!("missing glyph U+{:04X} in {}", ch as u32, source.display()))?;
        let (left, top, width, height) = face.glyph_bounding_box(id).map_or((0, 0, 0, 0), |b| {
            let left = (f32::from(b.x_min) * scale).floor() as i32;
            let top = (f32::from(b.y_max) * scale).ceil() as i32;
            (
                left,
                top,
                (f32::from(b.x_max) * scale).ceil() as i32 - left,
                top - (f32::from(b.y_min) * scale).floor() as i32,
            )
        });
        let width = u16::try_from(width).map_err(|_| "glyph width out of range")?;
        let height = u16::try_from(height).map_err(|_| "glyph height out of range")?;
        let bearing_x = i16::try_from(left).map_err(|_| "bearing overflow")?;
        let bearing_y = i16::try_from(top).map_err(|_| "bearing overflow")?;
        let advance = (f32::from(face.glyph_hor_advance(id).ok_or("missing advance")?) * scale)
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
        let start = bitmaps.len();
        out.extend_from_slice(
            &u32::try_from(start)
                .map_err(|_| "bitmap offset overflow")?
                .to_le_bytes(),
        );
        let stride = usize::from(width).div_ceil(2);
        let end = start
            .checked_add(stride * usize::from(height))
            .ok_or("font too large")?;
        if end > budget - bitmap_offset {
            return Err(format!(
                "glyph U+{:04X} exceeds font byte budget",
                ch as u32
            ));
        }
        if start != end {
            if usize::from(width) * usize::from(height) > 4 * 1024 * 1024 {
                return Err("glyph raster too large".into());
            }
            bitmaps.resize(end, 0);
            let mut outline = Outline {
                raster: Rasterizer::new(width.into(), height.into()),
                scale,
                left: left as f32,
                top: top as f32,
                cursor: point(0., 0.),
                start: point(0., 0.),
            };
            face.outline_glyph(id, &mut outline)
                .ok_or("unsupported glyph outline")?;
            outline.raster.for_each_pixel_2d(|x, y, coverage| {
                let value = (coverage.clamp(0., 1.) * 15.).round() as u8;
                bitmaps[start + y as usize * stride + x as usize / 2] |=
                    if x % 2 == 0 { value << 4 } else { value };
            });
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
