//! Pack PNG/PAM/PPM images as MCU-native L565 assets (v1, v2 RLE/chroma-key/A4, MaskA4, Indexed4/8) with optional dithering.
use std::collections::BTreeMap;
use std::{env, fs, path::Path};

use liuli::core::bitmap::{
    L565_FLAG_ALPHA4, L565_FLAG_CHROMA_KEY, L565_FLAG_INDEXED4, L565_FLAG_INDEXED8,
    L565_FLAG_MASK_A4, L565_FLAG_RLE,
};
use liuli::core::color::{
    Rgb888, rgb565_u16_to_rgb888, rgb888_to_gray8, rgb888_to_rgb565_round,
};
use liuli::core::dither::{gray8_to_gray4_dithered, rgb888_to_rgb565_dithered};
use liuli::core::geometry::rgb565_to_u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageMode {
    Rgb565,
    MaskA4,
    Indexed4,
    Indexed8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DitherAlg {
    None,
    Bayer4x4,
    FloydSteinberg,
}

#[derive(Clone, Copy, Debug)]
struct EncodeOptions {
    v2: bool,
    rle: bool,
    alpha4: bool,
    mode: ImageMode,
    chroma_key: Option<u16>,
    dither: DitherAlg,
}

fn parse_rgb565_hex(raw: &str) -> Result<u16, String> {
    let trimmed = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    u16::from_str_radix(trimmed, 16).map_err(|e| format!("invalid hex color '{raw}': {e}"))
}

fn quantize_a4_nibbles(a4: &[u8]) -> Vec<u8> {
    let mut packed = vec![0u8; a4.len().div_ceil(2)];
    for (idx, &a) in a4.iter().enumerate() {
        let v = a & 0x0F;
        if (idx & 1) == 0 {
            packed[idx / 2] |= v;
        } else {
            packed[idx / 2] |= v << 4;
        }
    }
    packed
}

fn quantize_rgb565_words(
    width: u16,
    height: u16,
    rgba: &[u8],
    dither: DitherAlg,
    chroma_key: Option<u16>,
) -> Vec<u16> {
    let w = usize::from(width);
    let h = usize::from(height);
    let mut words = Vec::with_capacity(w * h);
    match dither {
        DitherAlg::None => {
            for px in rgba.as_chunks::<4>().0 {
                words.push(rgb565_to_u16(rgb888_to_rgb565_round(px[0], px[1], px[2])));
            }
        }
        DitherAlg::Bayer4x4 => {
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    let round =
                        rgb565_to_u16(rgb888_to_rgb565_round(rgba[i], rgba[i + 1], rgba[i + 2]));
                    if chroma_key == Some(round) || rgba[i + 3] == 0 {
                        words.push(round);
                    } else {
                        words.push(rgb888_to_rgb565_dithered(
                            rgba[i],
                            rgba[i + 1],
                            rgba[i + 2],
                            x as i32,
                            y as i32,
                        ));
                    }
                }
            }
        }
        DitherAlg::FloydSteinberg => {
            let mut buf: Vec<[i32; 3]> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|px| [i32::from(px[0]), i32::from(px[1]), i32::from(px[2])])
                .collect();
            for y in 0..h {
                for x in 0..w {
                    let idx = y * w + x;
                    let raw = [
                        buf[idx][0].clamp(0, 255) as u8,
                        buf[idx][1].clamp(0, 255) as u8,
                        buf[idx][2].clamp(0, 255) as u8,
                    ];
                    let orig_round = rgb565_to_u16(rgb888_to_rgb565_round(
                        rgba[idx * 4],
                        rgba[idx * 4 + 1],
                        rgba[idx * 4 + 2],
                    ));
                    if chroma_key == Some(orig_round) || rgba[idx * 4 + 3] == 0 {
                        words.push(orig_round);
                        continue;
                    }
                    let qword = rgb565_to_u16(rgb888_to_rgb565_round(raw[0], raw[1], raw[2]));
                    words.push(qword);
                    let qrgb = rgb565_u16_to_rgb888(qword);
                    let err = [
                        i32::from(raw[0]) - i32::from(qrgb.r),
                        i32::from(raw[1]) - i32::from(qrgb.g),
                        i32::from(raw[2]) - i32::from(qrgb.b),
                    ];
                    let spread = |nx: usize, ny: usize, num: i32, buf: &mut [[i32; 3]]| {
                        let nidx = ny * w + nx;
                        if rgba[nidx * 4 + 3] == 0 {
                            return;
                        }
                        for ch in 0..3 {
                            buf[nidx][ch] += (err[ch] * num) / 16;
                        }
                    };
                    if x + 1 < w {
                        spread(x + 1, y, 7, &mut buf);
                    }
                    if y + 1 < h {
                        if x > 0 {
                            spread(x - 1, y + 1, 3, &mut buf);
                        }
                        spread(x, y + 1, 5, &mut buf);
                        if x + 1 < w {
                            spread(x + 1, y + 1, 1, &mut buf);
                        }
                    }
                }
            }
        }
    }
    words
}

fn build_palette(words: &[u16], max_colors: usize, chroma_key: Option<u16>) -> Vec<u16> {
    let mut hist = BTreeMap::<u16, usize>::new();
    for &w in words {
        *hist.entry(w).or_insert(0) += 1;
    }
    if hist.len() <= max_colors {
        let mut pal: Vec<u16> = hist.keys().copied().collect();
        pal.resize(max_colors, pal.first().copied().unwrap_or(0));
        return pal;
    }

    let mut pal = Vec::with_capacity(max_colors);
    let mut bucket_limit = max_colors;
    if let Some(ck) = chroma_key
        && hist.remove(&ck).is_some()
    {
        pal.push(ck);
        bucket_limit -= 1;
    }

    #[derive(Clone)]
    struct BoxEntry {
        color: u16,
        rgb: Rgb888,
        count: usize,
    }
    let mut items: Vec<BoxEntry> = hist
        .into_iter()
        .map(|(color, count)| BoxEntry {
            color,
            rgb: rgb565_u16_to_rgb888(color),
            count,
        })
        .collect();
    if items.is_empty() {
        pal.resize(max_colors, chroma_key.unwrap_or(0));
        return pal;
    }

    let mut boxes: Vec<Vec<BoxEntry>> = vec![std::mem::take(&mut items)];
    while boxes.len() < bucket_limit {
        let split_idx = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(_, b)| {
                let (mut rmin, mut rmax, mut gmin, mut gmax, mut bmin, mut bmax) =
                    (255u8, 0u8, 255u8, 0u8, 255u8, 0u8);
                let mut weight = 0usize;
                for it in *b {
                    rmin = rmin.min(it.rgb.r);
                    rmax = rmax.max(it.rgb.r);
                    gmin = gmin.min(it.rgb.g);
                    gmax = gmax.max(it.rgb.g);
                    bmin = bmin.min(it.rgb.b);
                    bmax = bmax.max(it.rgb.b);
                    weight += it.count;
                }
                let span = usize::from(
                    (rmax - rmin)
                        .max(gmax - gmin)
                        .max(bmax - bmin),
                );
                (span, weight)
            })
            .map(|(i, _)| i);
        let Some(idx) = split_idx else {
            break;
        };
        let mut bx = boxes.swap_remove(idx);
        let (mut rmin, mut rmax, mut gmin, mut gmax, mut bmin, mut bmax) =
            (255u8, 0u8, 255u8, 0u8, 255u8, 0u8);
        for it in &bx {
            rmin = rmin.min(it.rgb.r);
            rmax = rmax.max(it.rgb.r);
            gmin = gmin.min(it.rgb.g);
            gmax = gmax.max(it.rgb.g);
            bmin = bmin.min(it.rgb.b);
            bmax = bmax.max(it.rgb.b);
        }
        let rs = rmax - rmin;
        let gs = gmax - gmin;
        let bs = bmax - bmin;
        if rs >= gs && rs >= bs {
            bx.sort_by_key(|it| (it.rgb.r, it.color));
        } else if gs >= bs {
            bx.sort_by_key(|it| (it.rgb.g, it.color));
        } else {
            bx.sort_by_key(|it| (it.rgb.b, it.color));
        }
        let mid = bx.len() / 2;
        let right = bx.split_off(mid);
        boxes.push(bx);
        boxes.push(right);
    }

    for bx in boxes {
        let (mut rs, mut gs, mut bs, mut total) = (0u64, 0u64, 0u64, 0u64);
        for it in &bx {
            let c = it.count as u64;
            rs += u64::from(it.rgb.r) * c;
            gs += u64::from(it.rgb.g) * c;
            bs += u64::from(it.rgb.b) * c;
            total += c;
        }
        let w565 = rgb565_to_u16(rgb888_to_rgb565_round(
            ((rs + total / 2) / total) as u8,
            ((gs + total / 2) / total) as u8,
            ((bs + total / 2) / total) as u8,
        ));
        pal.push(w565);
    }
    pal.resize(max_colors, pal.first().copied().unwrap_or(0));
    pal
}

fn nearest_palette_index(palette: &[u16], target: u16, chroma_key: Option<u16>) -> u8 {
    if Some(target) == chroma_key
        && let Some(pos) = palette.iter().position(|&p| p == target)
    {
        return pos as u8;
    }
    let tr = rgb565_u16_to_rgb888(target);
    let mut best_idx = 0usize;
    let mut best_dist = u32::MAX;
    for (i, &p) in palette.iter().enumerate() {
        if Some(p) == chroma_key && Some(target) != chroma_key {
            continue;
        }
        if p == target {
            return i as u8;
        }
        let pr = rgb565_u16_to_rgb888(p);
        let dr = i32::from(tr.r) - i32::from(pr.r);
        let dg = i32::from(tr.g) - i32::from(pr.g);
        let db = i32::from(tr.b) - i32::from(pr.b);
        let dist = (dr * dr + dg * dg + db * db) as u32;
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }
    best_idx as u8
}

#[cfg(test)]
fn encode_l565(
    width: u16,
    height: u16,
    rgb: &[u8],
    v2: bool,
    rle: bool,
    chroma_key: Option<u16>,
) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(usize::from(width) * usize::from(height) * 4);
    for px in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    encode_l565_rgba(
        width,
        height,
        &rgba,
        EncodeOptions {
            v2,
            rle,
            alpha4: false,
            mode: ImageMode::Rgb565,
            chroma_key,
            dither: DitherAlg::None,
        },
    )
}

fn encode_l565_rgba(
    width: u16,
    height: u16,
    rgba: &[u8],
    opts: EncodeOptions,
) -> Result<Vec<u8>, String> {
    let expected = usize::from(width) * usize::from(height) * 4;
    if rgba.len() != expected {
        return Err(format!(
            "RGBA buffer length mismatch: expected {expected}, got {}",
            rgba.len()
        ));
    }
    if opts.mode == ImageMode::MaskA4 && (opts.rle || opts.alpha4 || opts.chroma_key.is_some()) {
        return Err("--mask-a4 cannot combine with --rle, --alpha4, or --chroma-key".into());
    }
    if matches!(opts.mode, ImageMode::Indexed4 | ImageMode::Indexed8) && opts.rle {
        return Err("--indexed4 / --indexed8 cannot combine with --rle".into());
    }

    if !opts.v2
        && !opts.rle
        && !opts.alpha4
        && opts.chroma_key.is_none()
        && opts.mode == ImageMode::Rgb565
    {
        let words = quantize_rgb565_words(width, height, rgba, opts.dither, None);
        let mut asset = b"L565\x01\x00".to_vec();
        asset.extend(width.to_le_bytes());
        asset.extend(height.to_le_bytes());
        for w in words {
            asset.extend(w.to_le_bytes());
        }
        return Ok(asset);
    }

    let mut flags = 0u8;
    if opts.chroma_key.is_some() {
        flags |= L565_FLAG_CHROMA_KEY;
    }
    if opts.alpha4 {
        flags |= L565_FLAG_ALPHA4;
    }
    if opts.rle {
        flags |= L565_FLAG_RLE;
    }
    match opts.mode {
        ImageMode::Rgb565 => {}
        ImageMode::MaskA4 => flags |= L565_FLAG_MASK_A4,
        ImageMode::Indexed4 => flags |= L565_FLAG_INDEXED4,
        ImageMode::Indexed8 => flags |= L565_FLAG_INDEXED8,
    }

    let mut asset = Vec::new();
    asset.extend_from_slice(b"L565");
    asset.push(2);
    asset.push(flags);
    asset.extend(width.to_le_bytes());
    asset.extend(height.to_le_bytes());
    asset.extend(opts.chroma_key.unwrap_or(0).to_le_bytes());

    let a4_values: Vec<u8> = if opts.alpha4 || opts.mode == ImageMode::MaskA4 {
        let rgba_px = rgba.as_chunks::<4>().0;
        let has_varying_alpha = rgba_px.iter().any(|px| px[3] < 255);
        let w = usize::from(width);
        let mut out = Vec::with_capacity(usize::from(width) * usize::from(height));
        for (idx, px) in rgba_px.iter().enumerate() {
            let a8 = if opts.mode == ImageMode::MaskA4 && !has_varying_alpha {
                rgb888_to_gray8(Rgb888::new(px[0], px[1], px[2])).0
            } else {
                px[3]
            };
            let a4 = if opts.dither == DitherAlg::Bayer4x4 {
                let x = (idx % w) as i32;
                let y = (idx / w) as i32;
                gray8_to_gray4_dithered(a8, x, y)
            } else {
                ((u16::from(a8) * 15 + 127) / 255) as u8
            };
            out.push(a4.min(15));
        }
        out
    } else {
        Vec::new()
    };

    if opts.mode == ImageMode::MaskA4 {
        asset.extend_from_slice(&quantize_a4_nibbles(&a4_values));
        return Ok(asset);
    }

    let words = quantize_rgb565_words(width, height, rgba, opts.dither, opts.chroma_key);

    if matches!(opts.mode, ImageMode::Indexed4 | ImageMode::Indexed8) {
        let max_colors = if opts.mode == ImageMode::Indexed4 { 16 } else { 256 };
        let palette = build_palette(&words, max_colors, opts.chroma_key);
        for &p in &palette {
            asset.extend_from_slice(&p.to_le_bytes());
        }
        let indices: Vec<u8> = words
            .iter()
            .map(|&w| nearest_palette_index(&palette, w, opts.chroma_key))
            .collect();
        if opts.mode == ImageMode::Indexed4 {
            asset.extend_from_slice(&quantize_a4_nibbles(&indices));
        } else {
            asset.extend_from_slice(&indices);
        }
        if opts.alpha4 {
            asset.extend_from_slice(&quantize_a4_nibbles(&a4_values));
        }
        return Ok(asset);
    }

    if !opts.rle {
        for w in words {
            asset.extend(w.to_le_bytes());
        }
        if opts.alpha4 {
            asset.extend_from_slice(&quantize_a4_nibbles(&a4_values));
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
        let row_start = sy * usize::from(width);
        let row = &words[row_start..row_start + usize::from(width)];
        let row_a = if opts.alpha4 {
            Some(&a4_values[row_start..row_start + usize::from(width)])
        } else {
            None
        };
        let px_eq = |i: usize, j: usize| -> bool {
            row[i] == row[j] && row_a.is_none_or(|a| a[i] == a[j])
        };
        let push_px = |dst: &mut Vec<u8>, idx: usize| {
            dst.extend_from_slice(&row[idx].to_le_bytes());
            if let Some(a) = row_a {
                dst.push(a[idx] & 0x0F);
            }
        };

        let mut i = 0usize;
        while i < row.len() {
            let mut rep = 1usize;
            while i + rep < row.len() && rep < 128 && px_eq(i, i + rep) {
                rep += 1;
            }
            if rep >= 2 {
                asset.push(0x80 | ((rep - 1) as u8));
                push_px(&mut asset, i);
                i += rep;
            } else {
                let start = i;
                let mut lit = 0usize;
                while i < row.len() && lit < 128 {
                    if i + 2 <= row.len() && px_eq(i, i + 1) {
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
                for j in start..start + lit {
                    push_px(&mut asset, j);
                }
            }
        }
    }
    Ok(asset)
}

fn pack(source: &Path, output: &Path, opts: EncodeOptions) -> Result<(), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let (width, height, rgba) = liuli_sim::image::decode_image_rgba8(&bytes)?;
    let w = u16::try_from(width).map_err(|_| "width exceeds 65535")?;
    let h = u16::try_from(height).map_err(|_| "height exceeds 65535")?;
    if w == 0 || h == 0 {
        return Err("image dimensions must be positive".into());
    }
    let asset = encode_l565_rgba(w, h, &rgba, opts)?;
    fs::write(output, asset).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut opts = EncodeOptions {
        v2: false,
        rle: false,
        alpha4: false,
        mode: ImageMode::Rgb565,
        chroma_key: None,
        dither: DitherAlg::None,
    };
    let mut pos = Vec::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--v2" => opts.v2 = true,
            "--rle" => {
                opts.v2 = true;
                opts.rle = true;
            }
            "--alpha4" => {
                opts.v2 = true;
                opts.alpha4 = true;
            }
            "--mask-a4" => {
                opts.v2 = true;
                opts.mode = ImageMode::MaskA4;
            }
            "--indexed4" => {
                opts.v2 = true;
                opts.mode = ImageMode::Indexed4;
            }
            "--indexed8" => {
                opts.v2 = true;
                opts.mode = ImageMode::Indexed8;
            }
            "--chroma-key" => {
                idx += 1;
                let val = args
                    .get(idx)
                    .ok_or("missing hex value after --chroma-key")?;
                opts.chroma_key = Some(parse_rgb565_hex(val)?);
                opts.v2 = true;
            }
            "--dither" => {
                idx += 1;
                let val = args
                    .get(idx)
                    .ok_or("missing dither mode after --dither (none|bayer|floyd-steinberg)")?;
                opts.dither = match val.as_str() {
                    "none" => DitherAlg::None,
                    "bayer" | "bayer4x4" => DitherAlg::Bayer4x4,
                    "floyd-steinberg" | "fs" => DitherAlg::FloydSteinberg,
                    other => return Err(format!("unknown dither mode '{other}'")),
                };
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
            "usage: liuli-image [--v2] [--rle] [--alpha4] [--mask-a4|--indexed4|--indexed8] [--chroma-key HEX] [--dither none|bayer|floyd-steinberg] INPUT.(png|pam|ppm) OUTPUT.l565".into(),
        );
    };
    pack(Path::new(source), Path::new(output), opts)
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

    #[test]
    fn encodes_png_alpha4_mask_indexed4_indexed8_and_dither_roundtrips() {
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 0, 255,
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 0, 200,
        ];
        let png = liuli_sim::image::encode_png_rgba8(4, 2, &rgba);
        let (w, h, dec) = liuli_sim::image::decode_image_rgba8(&png).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(dec, rgba);

        // Alpha4 planar
        let a4_asset = encode_l565_rgba(
            4,
            2,
            &dec,
            EncodeOptions {
                v2: true,
                rle: false,
                alpha4: true,
                mode: ImageMode::Rgb565,
                chroma_key: None,
                dither: DitherAlg::None,
            },
        )
        .unwrap();
        let b_a4 = BitmapView::from_asset(&a4_asset).unwrap();
        assert_eq!(b_a4.pixel_rgba(0, 0), Some((0xF800, 15)));
        assert_eq!(b_a4.pixel_rgba(1, 0), Some((0x07E0, 8)));
        assert_eq!(b_a4.pixel_rgba(2, 0), Some((0x001F, 0)));

        // MaskA4
        let mask_asset = encode_l565_rgba(
            4,
            2,
            &dec,
            EncodeOptions {
                v2: true,
                rle: false,
                alpha4: false,
                mode: ImageMode::MaskA4,
                chroma_key: None,
                dither: DitherAlg::None,
            },
        )
        .unwrap();
        let b_mask = BitmapView::from_asset(&mask_asset).unwrap();
        assert!(b_mask.is_alpha_mask());
        assert_eq!(b_mask.sample_alpha(0, 0), Some(15));
        assert_eq!(b_mask.sample_alpha(1, 0), Some(8));
        assert_eq!(b_mask.sample_alpha(2, 0), Some(0));

        // Indexed4 exact lossless for 4 unique colors
        let i4_asset = encode_l565_rgba(
            4,
            2,
            &dec,
            EncodeOptions {
                v2: true,
                rle: false,
                alpha4: false,
                mode: ImageMode::Indexed4,
                chroma_key: None,
                dither: DitherAlg::None,
            },
        )
        .unwrap();
        let b_i4 = BitmapView::from_asset(&i4_asset).unwrap();
        assert_eq!(b_i4.pixel_rgba(0, 0), Some((0xF800, 15)));
        assert_eq!(b_i4.pixel_rgba(3, 0), Some((0xFFE0, 15)));

        // Indexed8 + Bayer/FS dithering on gradient > 256 pixels
        let mut grad = Vec::with_capacity(32 * 16 * 4);
        for y in 0..16u8 {
            for x in 0..32u8 {
                grad.extend_from_slice(&[x * 8, y * 16, 128, 255]);
            }
        }
        let i8_bayer = encode_l565_rgba(
            32,
            16,
            &grad,
            EncodeOptions {
                v2: true,
                rle: false,
                alpha4: false,
                mode: ImageMode::Indexed8,
                chroma_key: None,
                dither: DitherAlg::Bayer4x4,
            },
        )
        .unwrap();
        assert!(BitmapView::from_asset(&i8_bayer).is_some());

        let fs_v2 = encode_l565_rgba(
            32,
            16,
            &grad,
            EncodeOptions {
                v2: true,
                rle: true,
                alpha4: false,
                mode: ImageMode::Rgb565,
                chroma_key: None,
                dither: DitherAlg::FloydSteinberg,
            },
        )
        .unwrap();
        let b_fs = BitmapView::from_asset(&fs_v2).unwrap();
        assert!(b_fs.is_rle());
    }
}
