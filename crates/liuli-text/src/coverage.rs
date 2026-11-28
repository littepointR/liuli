use liuli_core::geometry::{IntoStorage, RawU16, Rgb565};

/// Reads the 1-bit A1 coverage (`0` or `15`) at `(x, y)` from an MSB-first packed bitmap.
#[inline]
pub fn a1_pixel(bitmap: &[u8], width: u16, x: u16, y: u16) -> u8 {
    let row_bytes = (width as usize).div_ceil(8);
    let idx = (y as usize) * row_bytes + (x as usize) / 8;
    let bit = (bitmap[idx] >> (7 - (x & 7))) & 1;
    if bit != 0 { 15 } else { 0 }
}

/// Reads the 2-bit A2 coverage (`0, 5, 10, 15`) at `(x, y)` from an MSB-first packed bitmap.
#[inline]
pub fn a2_pixel(bitmap: &[u8], width: u16, x: u16, y: u16) -> u8 {
    let row_bytes = (width as usize).div_ceil(4);
    let idx = (y as usize) * row_bytes + (x as usize) / 4;
    let shift = 6 - ((x & 3) * 2);
    ((bitmap[idx] >> shift) & 0x03) * 5
}

/// Reads the 4-bit A4 coverage (`0..=15`) at `(x, y)` from a tightly packed row-major nibble bitmap.
///
/// High nibble stores the even column pixel, low nibble stores the odd column pixel.
#[inline]
pub fn a4_pixel(bitmap: &[u8], width: u16, x: u16, y: u16) -> u8 {
    let w = width as usize;
    let row_bytes = w.div_ceil(2);
    let idx = (y as usize) * row_bytes + (x as usize) / 2;
    let byte = bitmap[idx];
    if x.is_multiple_of(2) {
        (byte >> 4) & 0x0F
    } else {
        byte & 0x0F
    }
}

/// Reads the 8-bit A8 coverage (`0..=255`) at `(x, y)`.
#[inline]
pub fn a8_pixel(bitmap: &[u8], width: u16, x: u16, y: u16) -> u8 {
    bitmap[(y as usize) * (width as usize) + (x as usize)]
}

/// Reads packed 3×A4 `(r, g, b)` subpixel coverage (`0..=15` each) at `(x, y)`.
#[inline]
pub fn lcd_rgb_pixel(bitmap: &[u8], width: u16, x: u16, y: u16) -> (u8, u8, u8) {
    let row_bytes = (width as usize * 3).div_ceil(2);
    let row_off = (y as usize) * row_bytes;
    let nib = (x as usize) * 3;
    let nibble = |k: usize| -> u8 {
        let b = bitmap[row_off + k / 2];
        if k.is_multiple_of(2) {
            b >> 4
        } else {
            b & 0x0F
        }
    };
    (nibble(nib), nibble(nib + 1), nibble(nib + 2))
}

/// Blends an A4 foreground pixel (`a` in `0..=15`) over an RGB565 background using exact integer rounding:
/// `out = (fg * a + bg * (15 - a) + 7) / 15` per 5/6/5 channel.
#[inline]
pub fn blend_rgb565_a4(fg: Rgb565, bg: Rgb565, alpha_a4: u8) -> Rgb565 {
    let a = alpha_a4.min(15) as u16;
    if a == 0 {
        return bg;
    }
    if a == 15 {
        return fg;
    }
    let inv = 15 - a;
    let f = fg.into_storage();
    let b = bg.into_storage();

    let fr = (f >> 11) & 0x1F;
    let fg_c = (f >> 5) & 0x3F;
    let fb = f & 0x1F;

    let br = (b >> 11) & 0x1F;
    let bg_c = (b >> 5) & 0x3F;
    let bb = b & 0x1F;

    let rr = (fr * a + br * inv + 7) / 15;
    let rg = (fg_c * a + bg_c * inv + 7) / 15;
    let rb = (fb * a + bb * inv + 7) / 15;

    Rgb565::from(RawU16::new((rr << 11) | (rg << 5) | rb))
}

/// Blends raw RGB565 `fg_raw` (`alpha_a4` in `0..=15`) over raw RGB565 `bg_raw` using exact
/// zero-division arithmetic (`* 4370 >> 16`) matching [`blend_rgb565_a4`].
#[inline(always)]
pub fn blend_rgb565_a4_u16(fg_raw: u16, bg_raw: u16, alpha_a4: u8) -> u16 {
    let a = alpha_a4.min(15) as u32;
    if a == 0 {
        return bg_raw;
    }
    if a == 15 {
        return fg_raw;
    }
    let inv = 15 - a;
    let f = u32::from(fg_raw);
    let b = u32::from(bg_raw);
    let rr = (((f >> 11) * a + (b >> 11) * inv + 7) * 4370) >> 16;
    let rg = ((((f >> 5) & 63) * a + ((b >> 5) & 63) * inv + 7) * 4370) >> 16;
    let rb = (((f & 31) * a + (b & 31) * inv + 7) * 4370) >> 16;
    ((rr << 11) | (rg << 5) | rb) as u16
}

/// Porter-Duff Source-Over compositing for straight RGB565 + 4-bit alpha (`0..=15`).
#[inline(always)]
pub fn blend_over_rgb565_a4(dst_rgb: u16, dst_a: u8, src_rgb: u16, src_a: u8) -> (u16, u8) {
    let sa = src_a.min(15);
    if sa == 0 {
        return (dst_rgb, dst_a.min(15));
    }
    let da = dst_a.min(15);
    if sa == 15 || da == 0 {
        return (src_rgb, sa);
    }
    if da == 15 {
        return (blend_rgb565_a4_u16(src_rgb, dst_rgb, sa), 15);
    }
    let sa32 = u32::from(sa);
    let da32 = u32::from(da);
    let inv = 15 - sa32;
    let out_a = ((sa32 * 15 + da32 * inv + 7) * 4370) >> 16;
    let w_src = sa32 * 15;
    let w_dst = da32 * inv;
    let total = w_src + w_dst;
    let half = total >> 1;
    let s = u32::from(src_rgb);
    let d = u32::from(dst_rgb);
    let rr = ((s >> 11) * w_src + (d >> 11) * w_dst + half) / total;
    let rg = (((s >> 5) & 63) * w_src + ((d >> 5) & 63) * w_dst + half) / total;
    let rb = ((s & 31) * w_src + (d & 31) * w_dst + half) / total;
    (((rr << 11) | (rg << 5) | rb) as u16, out_a as u8)
}

/// Blends an 8-bit A8 pixel (`alpha_a8` in `0..=255`) over `bg_raw` with zero division.
/// Uses exact `num / 255` rounding: `(num + 1 + (num >> 8)) >> 8` where `num <= 16_192`.
#[inline(always)]
pub fn blend_rgb565_a8_u16(fg_raw: u16, bg_raw: u16, alpha_a8: u8) -> u16 {
    if alpha_a8 == 0 {
        return bg_raw;
    }
    if alpha_a8 == 255 {
        return fg_raw;
    }
    let a = u32::from(alpha_a8);
    let inv = 255 - a;
    let f = u32::from(fg_raw);
    let b = u32::from(bg_raw);
    let div255 = |n: u32| -> u32 { (n + 1 + (n >> 8)) >> 8 };
    let rr = div255((f >> 11) * a + (b >> 11) * inv + 127);
    let rg = div255(((f >> 5) & 63) * a + ((b >> 5) & 63) * inv + 127);
    let rb = div255((f & 31) * a + (b & 31) * inv + 127);
    ((rr << 11) | (rg << 5) | rb) as u16
}

/// Blends RGB565 `fg_raw` using per-channel 4-bit LCD subpixel coverages (`0..=15`).
#[inline(always)]
pub fn blend_rgb565_lcd_a4_u16(
    fg_raw: u16,
    bg_raw: u16,
    r_a4: u8,
    g_a4: u8,
    b_a4: u8,
    bgr: bool,
) -> u16 {
    let (ra, ba) = if bgr { (b_a4, r_a4) } else { (r_a4, b_a4) };
    let ra = ra.min(15) as u32;
    let ga = g_a4.min(15) as u32;
    let ba = ba.min(15) as u32;
    if ra == 0 && ga == 0 && ba == 0 {
        return bg_raw;
    }
    if ra == 15 && ga == 15 && ba == 15 {
        return fg_raw;
    }
    let f = u32::from(fg_raw);
    let b = u32::from(bg_raw);
    let rr = (((f >> 11) * ra + (b >> 11) * (15 - ra) + 7) * 4370) >> 16;
    let rg = ((((f >> 5) & 63) * ga + ((b >> 5) & 63) * (15 - ga) + 7) * 4370) >> 16;
    let rb = (((f & 31) * ba + (b & 31) * (15 - ba) + 7) * 4370) >> 16;
    ((rr << 11) | (rg << 5) | rb) as u16
}

/// Integer luminance proxy for RGB565 (`0..=187`): `2*R + G6 + 2*B`.
#[inline(always)]
pub const fn rgb565_luma(raw: u16) -> u16 {
    ((raw >> 11) << 1) + ((raw >> 5) & 63) + ((raw & 31) << 1)
}

/// Perceptual sRGB gamma boost LUT (`~gamma 1/1.45`) for light text on dark background.
pub const GAMMA_LIGHT_ON_DARK_A4: [u8; 16] =
    [0, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 12, 13, 14, 14, 15];

/// Perceptual sRGB gamma suppression LUT (`~gamma 1.45`) for dark text on light background.
pub const GAMMA_DARK_ON_LIGHT_A4: [u8; 16] = [0, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15];

/// Maps raw 4-bit coverage through contrast-aware perceptual gamma LUTs (zero heap/float).
#[inline(always)]
pub const fn gamma_remap_a4(cov: u8, fg_raw: u16, bg_raw: u16) -> u8 {
    let idx = (if cov > 15 { 15 } else { cov }) as usize;
    if rgb565_luma(fg_raw) >= rgb565_luma(bg_raw) {
        GAMMA_LIGHT_ON_DARK_A4[idx]
    } else {
        GAMMA_DARK_ON_LIGHT_A4[idx]
    }
}

const GAMMA_LIGHT_ON_DARK_A8: [u8; 17] = [
    0, 28, 49, 67, 83, 98, 113, 127, 140, 153, 166, 178, 191, 203, 216, 232, 255,
];
const GAMMA_DARK_ON_LIGHT_A8: [u8; 17] = [
    0, 7, 15, 25, 36, 48, 62, 77, 93, 110, 129, 148, 169, 190, 212, 233, 255,
];

/// Maps raw 8-bit coverage through 17-point piecewise linear perceptual gamma curves.
#[inline(always)]
pub const fn gamma_remap_a8(cov: u8, fg_raw: u16, bg_raw: u16) -> u8 {
    if cov == 0 || cov == 255 {
        return cov;
    }
    let table = if rgb565_luma(fg_raw) >= rgb565_luma(bg_raw) {
        &GAMMA_LIGHT_ON_DARK_A8
    } else {
        &GAMMA_DARK_ON_LIGHT_A8
    };
    let i = (cov >> 4) as usize;
    let frac = (cov & 0x0F) as u16;
    let a = table[i] as u16;
    let b = table[i + 1] as u16;
    let v = if i == 15 {
        a + ((b - a) * frac) / 15
    } else {
        a + (((b - a) * frac + 8) >> 4)
    };
    v as u8
}

/// Validates that `rle` PackBits stream decodes cleanly to exact `expected_bytes`.
pub fn validate_rle(rle: &[u8], expected_bytes: usize) -> bool {
    let mut pos = 0usize;
    let mut decoded = 0usize;
    while pos < rle.len() {
        let ctrl = rle[pos];
        pos += 1;
        if ctrl < 128 {
            let run = usize::from(ctrl) + 1;
            if pos + run > rle.len() || decoded + run > expected_bytes {
                return false;
            }
            pos += run;
            decoded += run;
        } else {
            let run = usize::from(ctrl - 128) + 2;
            if pos >= rle.len() || decoded + run > expected_bytes {
                return false;
            }
            pos += 1;
            decoded += run;
        }
    }
    decoded == expected_bytes
}

/// Streams decoded rows from a PackBits RLE blob (`0..height`) into `row_buf`, skipping
/// rows before `skip_rows` and stopping early after `max_rows` visible rows are emitted.
#[inline]
#[allow(clippy::result_unit_err)]
pub fn decode_rle_rows(
    rle: &[u8],
    row_bytes: usize,
    total_rows: usize,
    skip_rows: usize,
    max_rows: usize,
    row_buf: &mut [u8],
    mut on_row: impl FnMut(usize, &[u8]),
) -> Result<(), ()> {
    if row_bytes > row_buf.len() {
        return Err(());
    }
    decode_rle_chunks(
        rle,
        row_bytes,
        total_rows,
        skip_rows,
        max_rows,
        row_buf,
        |row, _, data| on_row(row, data),
    )
}

/// Visit visible rows in byte-aligned chunks. The caller chooses the working-set size.
#[allow(clippy::result_unit_err)]
pub fn decode_rle_chunks(
    data: &[u8],
    row_bytes: usize,
    total_rows: usize,
    skip_rows: usize,
    max_rows: usize,
    scratch: &mut [u8],
    mut emit: impl FnMut(usize, usize, &[u8]),
) -> Result<(), ()> {
    let end = total_rows.min(skip_rows.saturating_add(max_rows));
    if row_bytes == 0 || skip_rows >= end {
        return Ok(());
    }
    if scratch.is_empty() {
        return Err(());
    }
    let mut row = 0;
    let mut col = 0;
    let mut used = 0;
    let mut input = data;
    while row < end {
        let (&ctrl, rest) = input.split_first().ok_or(())?;
        let literal = ctrl < 128;
        let mut remaining = if literal {
            usize::from(ctrl) + 1
        } else {
            usize::from(ctrl - 128) + 2
        };
        let size = if literal { remaining } else { 1 };
        let (mut payload, rest) = rest.split_at_checked(size).ok_or(())?;
        input = rest;
        while remaining > 0 && row < end {
            let mut take = remaining.min(row_bytes - col);
            if row >= skip_rows {
                take = take.min(scratch.len() - used);
                let dest = &mut scratch[used..used + take];
                if literal {
                    dest.copy_from_slice(&payload[..take]);
                } else {
                    dest.fill(payload[0]);
                }
                used += take;
                if used == scratch.len() || col + take == row_bytes {
                    emit(row, col + take - used, &scratch[..used]);
                    used = 0;
                }
            }
            if literal {
                payload = &payload[take..];
            }
            col += take;
            remaining -= take;
            if col == row_bytes {
                col = 0;
                row += 1;
            }
        }
    }
    Ok(())
}

/// Decodes a full PackBits RLE payload directly into `out` (`out.len() == expected raw bytes`).
#[inline]
#[allow(clippy::result_unit_err)]
pub fn decode_rle_into(rle: &[u8], out: &mut [u8]) -> Result<(), ()> {
    let mut pos = 0usize;
    let mut dst = 0usize;
    while pos < rle.len() && dst < out.len() {
        let ctrl = rle[pos];
        pos += 1;
        if ctrl < 128 {
            let run = usize::from(ctrl) + 1;
            if pos + run > rle.len() || dst + run > out.len() {
                return Err(());
            }
            out[dst..dst + run].copy_from_slice(&rle[pos..pos + run]);
            pos += run;
            dst += run;
        } else {
            let run = usize::from(ctrl - 128) + 2;
            if pos >= rle.len() || dst + run > out.len() {
                return Err(());
            }
            out[dst..dst + run].fill(rle[pos]);
            pos += 1;
            dst += run;
        }
    }
    if dst == out.len() { Ok(()) } else { Err(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_u16_and_porter_duff_source_over() {
        let samples = [0u16, 0x001f, 0x07e0, 0xf800, 0x52aa, 0xa514, 0xffff];
        for &fg in &samples {
            let fg_c = Rgb565::from(RawU16::new(fg));
            for &bg in &samples {
                let bg_c = Rgb565::from(RawU16::new(bg));
                for a in 0..=15 {
                    assert_eq!(
                        blend_rgb565_a4_u16(fg, bg, a),
                        blend_rgb565_a4(fg_c, bg_c, a).into_storage(),
                        "fg={fg:#06x} bg={bg:#06x} a={a}"
                    );
                }
            }
        }

        // Verify all Porter-Duff branches: sa==0, sa==15, da==0, da==15, and partial/partial.
        assert_eq!(blend_over_rgb565_a4(0x1234, 7, 0xffff, 0), (0x1234, 7));
        assert_eq!(blend_over_rgb565_a4(0x1234, 7, 0xf800, 15), (0xf800, 15));
        assert_eq!(blend_over_rgb565_a4(0x1234, 0, 0x07e0, 6), (0x07e0, 6));
        assert_eq!(
            blend_over_rgb565_a4(0x52aa, 15, 0xf800, 8),
            (blend_rgb565_a4_u16(0xf800, 0x52aa, 8), 15)
        );
        let (mixed, out_a) = blend_over_rgb565_a4(0x001f, 5, 0xf800, 10);
        assert_eq!(out_a, 12); // 10 + (5 * 5 + 7) / 15 = 12
        assert_ne!(mixed, 0);
        assert_ne!(mixed, 0x001f);
        assert_ne!(mixed, 0xf800);
    }

    #[test]
    fn gamma_and_a8_lcd_and_rle_streaming_exact() {
        // Exhaustive zero-division blend_rgb565_a8_u16 check vs exact integer division.
        for &fg in &[0u16, 0x001f, 0x07e0, 0xf800, 0x52aa, 0xffff] {
            for &bg in &[0u16, 0x18c3, 0xa514, 0xffff] {
                for a in 0..=255u8 {
                    let a32 = u32::from(a);
                    let inv = 255 - a32;
                    let f = u32::from(fg);
                    let b = u32::from(bg);
                    let er = ((f >> 11) * a32 + (b >> 11) * inv + 127) / 255;
                    let eg = (((f >> 5) & 63) * a32 + ((b >> 5) & 63) * inv + 127) / 255;
                    let eb = ((f & 31) * a32 + (b & 31) * inv + 127) / 255;
                    let expected = ((er << 11) | (eg << 5) | eb) as u16;
                    assert_eq!(blend_rgb565_a8_u16(fg, bg, a), expected);
                }
            }
        }

        // LCD BGR swaps R and B channels cleanly.
        assert_eq!(
            blend_rgb565_lcd_a4_u16(0xffff, 0x0000, 15, 0, 0, false),
            0xf800
        );
        assert_eq!(
            blend_rgb565_lcd_a4_u16(0xffff, 0x0000, 15, 0, 0, true),
            0x001f
        );

        // Perceptual gamma LUTs preserve endpoints, monotonicity, and directional contrast.
        for a in 0..15u8 {
            assert!(gamma_remap_a4(a, 0xffff, 0x0000) <= gamma_remap_a4(a + 1, 0xffff, 0x0000));
            assert!(gamma_remap_a4(a, 0x0000, 0xffff) <= gamma_remap_a4(a + 1, 0x0000, 0xffff));
        }
        assert!(gamma_remap_a4(7, 0xffff, 0x0000) > gamma_remap_a4(7, 0x0000, 0xffff));
        for a in 0..255u8 {
            assert!(gamma_remap_a8(a, 0xffff, 0x0000) <= gamma_remap_a8(a + 1, 0xffff, 0x0000));
            assert!(gamma_remap_a8(a, 0x0000, 0xffff) <= gamma_remap_a8(a + 1, 0x0000, 0xffff));
        }
        assert_eq!(gamma_remap_a8(0, 0xffff, 0x0000), 0);
        assert_eq!(gamma_remap_a8(255, 0x0000, 0xffff), 255);

        // RLE validation and row streaming.
        // 3 rows of 4 bytes: [0x00,0x00,0x00,0x00], [0x12,0x34,0x56,0x78], [0xFF,0xFF,0xFF,0xFF]
        let rle: [u8; 10] = [130, 0x00, 3, 0x12, 0x34, 0x56, 0x78, 130, 0xFF, 0x00];
        assert!(validate_rle(&rle[..9], 12));
        assert!(!validate_rle(&rle[..9], 11));
        let mut row = [0u8; 4];
        let mut got = [(0usize, [0u8; 4]); 1];
        let mut n = 0;
        decode_rle_rows(&rle[..9], 4, 3, 1, 1, &mut row, |r, bytes| {
            got[n] = (r, bytes.try_into().unwrap());
            n += 1;
        })
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(got[0], (1, [0x12, 0x34, 0x56, 0x78]));
    }
}
