use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Atlas {
    pub width: usize,
    pub _height: usize,
    pub pixels: Vec<bool>,
    pub ascent: i16,
    pub descent: u16,
    pub line_gap: u16,
    pub glyphs: Vec<SourceGlyph>,
}

#[derive(Debug, Clone)]
pub struct SourceGlyph {
    pub unicode: u32,
    pub cell_x: usize,
    pub cell_y: usize,
    pub width_px: u16,
    pub height_px: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub advance: u16,
}

impl Atlas {
    pub fn load(pbm_path: &Path, tsv_path: &Path) -> Result<Self, String> {
        let pbm_bytes = fs::read(pbm_path)
            .map_err(|e| format!("failed to read {}: {e}", pbm_path.display()))?;
        let (width, height, pixels) = parse_pbm(&pbm_bytes)?;

        let tsv_text = fs::read_to_string(tsv_path)
            .map_err(|e| format!("failed to read {}: {e}", tsv_path.display()))?;

        let mut ascent = 14i16;
        let mut descent = 2u16;
        let mut line_gap = 2u16;
        let mut glyphs = Vec::new();
        let mut saw_header = false;

        for (line_idx, raw_line) in tsv_text.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                for part in rest.split_whitespace() {
                    if let Some(v) = part.strip_prefix("ascent=") {
                        ascent = v
                            .parse()
                            .map_err(|e| format!("line {}: bad ascent: {e}", line_idx + 1))?;
                    } else if let Some(v) = part.strip_prefix("descent=") {
                        descent = v
                            .parse()
                            .map_err(|e| format!("line {}: bad descent: {e}", line_idx + 1))?;
                    } else if let Some(v) = part.strip_prefix("line_gap=") {
                        line_gap = v
                            .parse()
                            .map_err(|e| format!("line {}: bad line_gap: {e}", line_idx + 1))?;
                    }
                }
                continue;
            }
            if !saw_header && line.starts_with("unicode_hex") {
                saw_header = true;
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() != 8 {
                return Err(format!(
                    "line {}: expected 8 tab-separated columns, got {}",
                    line_idx + 1,
                    cols.len()
                ));
            }
            let unicode = u32::from_str_radix(cols[0].trim_start_matches("0x"), 16)
                .map_err(|e| format!("line {}: bad unicode '{}': {e}", line_idx + 1, cols[0]))?;
            if char::from_u32(unicode).is_none() {
                return Err(format!(
                    "line {}: U+{unicode:04X} is not a valid Unicode scalar",
                    line_idx + 1
                ));
            }
            let cell_x: usize = cols[1]
                .parse()
                .map_err(|e| format!("line {}: bad cell_x: {e}", line_idx + 1))?;
            let cell_y: usize = cols[2]
                .parse()
                .map_err(|e| format!("line {}: bad cell_y: {e}", line_idx + 1))?;
            let width_px: u16 = cols[3]
                .parse()
                .map_err(|e| format!("line {}: bad width_px: {e}", line_idx + 1))?;
            let height_px: u16 = cols[4]
                .parse()
                .map_err(|e| format!("line {}: bad height_px: {e}", line_idx + 1))?;
            let bearing_x: i16 = cols[5]
                .parse()
                .map_err(|e| format!("line {}: bad bearing_x: {e}", line_idx + 1))?;
            let bearing_y: i16 = cols[6]
                .parse()
                .map_err(|e| format!("line {}: bad bearing_y: {e}", line_idx + 1))?;
            let advance: u16 = cols[7]
                .parse()
                .map_err(|e| format!("line {}: bad advance: {e}", line_idx + 1))?;

            let req_w = usize::from(width_px) * 4;
            let req_h = usize::from(height_px) * 4;
            if cell_x.checked_add(req_w).is_none_or(|end| end > width)
                || cell_y.checked_add(req_h).is_none_or(|end| end > height)
            {
                return Err(format!(
                    "line {}: glyph U+{unicode:04X} cell ({cell_x},{cell_y} + {req_w}x{req_h}) exceeds atlas {width}x{height}",
                    line_idx + 1
                ));
            }

            glyphs.push(SourceGlyph {
                unicode,
                cell_x,
                cell_y,
                width_px,
                height_px,
                bearing_x,
                bearing_y,
                advance,
            });
        }

        glyphs.sort_by_key(|g| g.unicode);
        for pair in glyphs.windows(2) {
            if pair[0].unicode == pair[1].unicode {
                return Err(format!("duplicate glyph U+{:04X}", pair[0].unicode));
            }
        }

        Ok(Self {
            width,
            _height: height,
            pixels,
            ascent,
            descent,
            line_gap,
            glyphs,
        })
    }

    pub fn pixel(&self, x: usize, y: usize) -> bool {
        self.pixels[y * self.width + x]
    }
}

pub(crate) fn parse_pbm(bytes: &[u8]) -> Result<(usize, usize, Vec<bool>), String> {
    const MAX_PBM_PIXELS: usize = 64 * 1024 * 1024;
    if bytes.len() < 3 {
        return Err("PBM file too short".to_string());
    }
    let is_p4 = &bytes[0..2] == b"P4";
    let is_p1 = &bytes[0..2] == b"P1";
    if !is_p4 && !is_p1 {
        return Err("expected P4 or P1 PBM magic".to_string());
    }

    let mut pos = 2usize;
    let width = next_pbm_token(bytes, &mut pos)?
        .parse::<usize>()
        .map_err(|e| format!("invalid PBM width: {e}"))?;
    let height = next_pbm_token(bytes, &mut pos)?
        .parse::<usize>()
        .map_err(|e| format!("invalid PBM height: {e}"))?;

    if width == 0 || height == 0 {
        return Err("PBM dimensions must be non-zero".to_string());
    }
    let total_pixels = width
        .checked_mul(height)
        .ok_or_else(|| "PBM pixel count overflow".to_string())?;
    if total_pixels > MAX_PBM_PIXELS {
        return Err(format!(
            "PBM pixel count {total_pixels} exceeds limit {MAX_PBM_PIXELS}"
        ));
    }

    if is_p4 {
        // After height token there is exactly one whitespace byte
        if pos >= bytes.len() || !bytes[pos].is_ascii_whitespace() {
            return Err("missing single whitespace after P4 header".to_string());
        }
        pos += 1;
        let row_bytes = width.div_ceil(8);
        let expected_len = row_bytes
            .checked_mul(height)
            .ok_or_else(|| "PBM byte size overflow".to_string())?;
        if bytes.len() - pos < expected_len {
            return Err(format!(
                "P4 PBM truncated: expected {expected_len} data bytes, got {}",
                bytes.len() - pos
            ));
        }
        let data = &bytes[pos..pos + expected_len];
        let mut pixels = vec![false; total_pixels];
        for y in 0..height {
            let row = &data[y * row_bytes..(y + 1) * row_bytes];
            for x in 0..width {
                let bit = (row[x / 8] >> (7 - (x % 8))) & 1;
                pixels[y * width + x] = bit != 0;
            }
        }
        Ok((width, height, pixels))
    } else {
        let mut pixels = Vec::with_capacity(total_pixels);
        while pos < bytes.len() && pixels.len() < total_pixels {
            match bytes[pos] {
                b'#' => {
                    while pos < bytes.len() && bytes[pos] != b'\n' {
                        pos += 1;
                    }
                }
                b'0' => {
                    pixels.push(false);
                    pos += 1;
                }
                b'1' => {
                    pixels.push(true);
                    pos += 1;
                }
                b if b.is_ascii_whitespace() => pos += 1,
                other => return Err(format!("unexpected byte 0x{other:02X} in P1 PBM")),
            }
        }
        if pixels.len() != total_pixels {
            return Err(format!(
                "P1 PBM truncated: expected {total_pixels} pixels, got {}",
                pixels.len()
            ));
        }
        Ok((width, height, pixels))
    }
}

fn next_pbm_token<'a>(bytes: &'a [u8], pos: &mut usize) -> Result<&'a str, String> {
    while *pos < bytes.len() {
        if bytes[*pos] == b'#' {
            while *pos < bytes.len() && bytes[*pos] != b'\n' {
                *pos += 1;
            }
        } else if bytes[*pos].is_ascii_whitespace() {
            *pos += 1;
        } else {
            break;
        }
    }
    let start = *pos;
    while *pos < bytes.len() && !bytes[*pos].is_ascii_whitespace() && bytes[*pos] != b'#' {
        *pos += 1;
    }
    if start == *pos {
        return Err("unexpected EOF in PBM header".to_string());
    }
    std::str::from_utf8(&bytes[start..*pos]).map_err(|e| format!("invalid UTF-8 in header: {e}"))
}
