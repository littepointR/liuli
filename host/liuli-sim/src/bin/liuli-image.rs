//! Pack a binary PPM (RGB8) as an MCU-native L565 asset (v1 or v2 with optional RLE / chroma key).
use std::{env, fs, path::Path};

fn parse_rgb565_hex(raw: &str) -> Result<u16, String> {
    let trimmed = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    u16::from_str_radix(trimmed, 16).map_err(|e| format!("invalid hex color '{raw}': {e}"))
}

fn encode_l565(
    width: u16,
    height: u16,
    rgb: &[u8],
    v2: bool,
    rle: bool,
    chroma_key: Option<u16>,
) -> Result<Vec<u8>, String> {
    let mut words = Vec::with_capacity(usize::from(width) * usize::from(height));
    for px in rgb.as_chunks::<3>().0 {
        words.push(
            (u16::from(px[0] >> 3) << 11) | (u16::from(px[1] >> 2) << 5) | u16::from(px[2] >> 3),
        );
    }

    if !v2 && !rle && chroma_key.is_none() {
        let mut asset = b"L565\x01\x00".to_vec();
        asset.extend(width.to_le_bytes());
        asset.extend(height.to_le_bytes());
        for w in words {
            asset.extend(w.to_le_bytes());
        }
        return Ok(asset);
    }

    let mut flags = 0u8;
    if chroma_key.is_some() {
        flags |= liuli::core::bitmap::L565_FLAG_CHROMA_KEY;
    }
    if rle {
        flags |= liuli::core::bitmap::L565_FLAG_RLE;
    }

    let mut asset = Vec::new();
    asset.extend_from_slice(b"L565");
    asset.push(2);
    asset.push(flags);
    asset.extend(width.to_le_bytes());
    asset.extend(height.to_le_bytes());
    asset.extend(chroma_key.unwrap_or(0).to_le_bytes());

    if !rle {
        for w in words {
            asset.extend(w.to_le_bytes());
        }
        return Ok(asset);
    }

    let row_tbl_start = asset.len();
    asset.resize(row_tbl_start + usize::from(height) * 4, 0);
    let payload_start = asset.len();

    for sy in 0..usize::from(height) {
        let row_off =
            u32::try_from(asset.len() - payload_start).map_err(|_| "RLE payload exceeds 4 GiB")?;
        asset[row_tbl_start + sy * 4..row_tbl_start + sy * 4 + 4]
            .copy_from_slice(&row_off.to_le_bytes());
        let row = &words[sy * usize::from(width)..(sy + 1) * usize::from(width)];
        let mut i = 0usize;
        while i < row.len() {
            let mut rep = 1usize;
            while i + rep < row.len() && rep < 128 && row[i + rep] == row[i] {
                rep += 1;
            }
            if rep >= 2 {
                asset.push(0x80 | ((rep - 1) as u8));
                asset.extend(row[i].to_le_bytes());
                i += rep;
            } else {
                let start = i;
                let mut lit = 0usize;
                while i < row.len() && lit < 128 {
                    if i + 2 <= row.len() && row[i] == row[i + 1] {
                        break;
                    }
                    i += 1;
                    lit += 1;
                }
                if lit == 0 {
                    lit = 1;
                    i += 1;
                }
                asset.push((lit - 1) as u8);
                for &w in &row[start..start + lit] {
                    asset.extend(w.to_le_bytes());
                }
            }
        }
    }
    Ok(asset)
}

fn pack(
    source: &Path,
    output: &Path,
    v2: bool,
    rle: bool,
    chroma_key: Option<u16>,
) -> Result<(), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let (width, height, rgb) = liuli_sim::image::decode_ppm(&bytes)?;
    let w = u16::try_from(width).map_err(|_| "width exceeds 65535")?;
    let h = u16::try_from(height).map_err(|_| "height exceeds 65535")?;
    if w == 0 || h == 0 {
        return Err("image dimensions must be positive".into());
    }
    let asset = encode_l565(w, h, &rgb, v2, rle, chroma_key)?;
    fs::write(output, asset).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut v2 = false;
    let mut rle = false;
    let mut chroma_key = None;
    let mut pos = Vec::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--v2" => v2 = true,
            "--rle" => {
                v2 = true;
                rle = true;
            }
            "--chroma-key" => {
                idx += 1;
                let val = args
                    .get(idx)
                    .ok_or("missing hex value after --chroma-key")?;
                chroma_key = Some(parse_rgb565_hex(val)?);
                v2 = true;
            }
            arg if arg.starts_with("--") => {
                return Err(format!("unknown option '{arg}'"));
            }
            _ => pos.push(args[idx].clone()),
        }
        idx += 1;
    }
    let [source, output] = pos.as_slice() else {
        return Err(
            "usage: liuli-image [--v2] [--rle] [--chroma-key HEX] INPUT.ppm OUTPUT.l565".into(),
        );
    };
    pack(Path::new(source), Path::new(output), v2, rle, chroma_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use liuli::core::bitmap::BitmapView;

    #[test]
    fn encodes_v1_and_v2_rle_with_chroma_key_roundtrip() {
        // 4x2 pixels: row0 solid red (#F800), row1 mixed with chroma-key (#F81F)
        let rgb = [
            255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 255, 255, 0,
            255,
        ];
        let v1 = encode_l565(4, 2, &rgb, false, false, None).unwrap();
        assert_eq!(&v1[..6], b"L565\x01\x00");
        assert!(BitmapView::from_asset(&v1).is_some());

        let v2_rle = encode_l565(4, 2, &rgb, true, true, Some(0xF81F)).unwrap();
        let bmp = BitmapView::from_asset(&v2_rle).unwrap();
        assert!(bmp.is_rle());
        assert!(bmp.has_alpha());
        assert_eq!(bmp.chroma_key(), Some(0xF81F));
        assert_eq!(bmp.sample_rgba(0, 0, 4, 2, 0), Some((0xF800, 15)));
        assert_eq!(bmp.sample_rgba(0, 1, 4, 2, 0), Some((0xF81F, 0)));
        assert_eq!(bmp.sample(0, 1, 4, 2, 0), None);
        assert_eq!(bmp.sample(1, 1, 4, 2, 0), Some(0x07E0));
    }
}
