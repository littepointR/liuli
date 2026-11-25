//! RGB565 + optional A4/chroma-key/RLE, A4 mask, and I4/I8 indexed bitmap sampling. Rotation uses 256 steps per turn.

pub const L565_FLAG_CHROMA_KEY: u8 = 0x01;
pub const L565_FLAG_ALPHA4: u8 = 0x02;
pub const L565_FLAG_RLE: u8 = 0x04;
pub const L565_FLAG_MASK_A4: u8 = 0x08;
pub const L565_FLAG_INDEXED4: u8 = 0x10;
pub const L565_FLAG_INDEXED8: u8 = 0x20;

#[inline]
fn read_nibble(bytes: &[u8], idx: usize) -> Option<u8> {
    let packed = *bytes.get(idx / 2)?;
    Some(if (idx & 1) == 0 {
        packed & 0x0F
    } else {
        (packed >> 4) & 0x0F
    })
}

#[inline]
fn read_palette_rgb565(palette: &[u8], index: u8) -> Option<u16> {
    let off = usize::from(index) * 2;
    Some(u16::from_le_bytes([
        *palette.get(off)?,
        *palette.get(off + 1)?,
    ]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pixels<'a> {
    Native(&'a [u16]),
    LittleEndian(&'a [u8]),
    AlphaPlanar {
        pixels: &'a [u8],
        alpha4: &'a [u8],
    },
    Rle {
        offsets: &'a [u8],
        payload: &'a [u8],
        has_alpha4: bool,
    },
    MaskA4(&'a [u8]),
    Indexed4 {
        palette: &'a [u8],
        indices: &'a [u8],
        alpha4: Option<&'a [u8]>,
    },
    Indexed8 {
        palette: &'a [u8],
        indices: &'a [u8],
        alpha4: Option<&'a [u8]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitmapView<'a> {
    width: u16,
    height: u16,
    chroma_key: Option<u16>,
    pixels: Pixels<'a>,
}

pub type Bitmap = BitmapView<'static>;

impl<'a> BitmapView<'a> {
    pub const fn new(width: u16, height: u16, pixels: &'a [u16]) -> Option<Self> {
        if width == 0 || height == 0 || pixels.len() != width as usize * height as usize {
            return None;
        }
        Some(Self {
            width,
            height,
            chroma_key: None,
            pixels: Pixels::Native(pixels),
        })
    }

    #[inline]
    pub const fn width(&self) -> u16 {
        self.width
    }

    #[inline]
    pub const fn height(&self) -> u16 {
        self.height
    }

    #[inline]
    pub const fn has_alpha(&self) -> bool {
        self.chroma_key.is_some()
            || matches!(
                self.pixels,
                Pixels::AlphaPlanar { .. }
                    | Pixels::MaskA4(_)
                    | Pixels::Indexed4 {
                        alpha4: Some(_),
                        ..
                    }
                    | Pixels::Indexed8 {
                        alpha4: Some(_),
                        ..
                    }
                    | Pixels::Rle {
                        has_alpha4: true,
                        ..
                    }
            )
    }

    #[inline]
    pub const fn is_rle(&self) -> bool {
        matches!(self.pixels, Pixels::Rle { .. })
    }

    #[inline]
    pub const fn is_alpha_mask(&self) -> bool {
        matches!(self.pixels, Pixels::MaskA4(_))
    }

    #[inline]
    pub const fn mask_a4_bytes(&self) -> Option<&'a [u8]> {
        match self.pixels {
            Pixels::MaskA4(bytes) => Some(bytes),
            _ => None,
        }
    }

    #[inline]
    pub const fn chroma_key(&self) -> Option<u16> {
        self.chroma_key
    }

    /// Load an L565 v1 or v2 image produced by `liuli-image`.
    pub const fn from_asset(bytes: &'a [u8]) -> Option<Self> {
        match bytes {
            [b'L', b'5', b'6', b'5', 1, 0, wl, wh, hl, hh, pixels @ ..] => {
                let width = u16::from_le_bytes([*wl, *wh]);
                let height = u16::from_le_bytes([*hl, *hh]);
                if width == 0
                    || height == 0
                    || pixels.len() as u64 != width as u64 * height as u64 * 2
                {
                    return None;
                }
                Some(Self {
                    width,
                    height,
                    chroma_key: None,
                    pixels: Pixels::LittleEndian(pixels),
                })
            }
            [
                b'L',
                b'5',
                b'6',
                b'5',
                2,
                flags,
                wl,
                wh,
                hl,
                hh,
                ck_lo,
                ck_hi,
                rest @ ..,
            ] => {
                let width = u16::from_le_bytes([*wl, *wh]);
                let height = u16::from_le_bytes([*hl, *hh]);
                if width == 0 || height == 0 {
                    return None;
                }
                let Some(px_count) = (width as usize).checked_mul(height as usize) else {
                    return None;
                };
                let f = *flags;
                let fmt_bits = f & (L565_FLAG_MASK_A4 | L565_FLAG_INDEXED4 | L565_FLAG_INDEXED8);
                if fmt_bits.count_ones() > 1 {
                    return None;
                }
                let is_mask_a4 = (f & L565_FLAG_MASK_A4) != 0;
                let is_i4 = (f & L565_FLAG_INDEXED4) != 0;
                let is_i8 = (f & L565_FLAG_INDEXED8) != 0;
                let is_rle = (f & L565_FLAG_RLE) != 0;
                let has_alpha4 = (f & L565_FLAG_ALPHA4) != 0;
                if is_mask_a4 && (is_rle || has_alpha4) {
                    return None;
                }
                if (is_i4 || is_i8) && is_rle {
                    return None;
                }
                let chroma_key = if !is_mask_a4 && (f & L565_FLAG_CHROMA_KEY) != 0 {
                    Some(u16::from_le_bytes([*ck_lo, *ck_hi]))
                } else {
                    None
                };

                if is_rle {
                    let row_tbl = height as usize * 4;
                    if rest.len() < row_tbl {
                        return None;
                    }
                    let (offsets, payload) = rest.split_at(row_tbl);
                    Some(Self {
                        width,
                        height,
                        chroma_key,
                        pixels: Pixels::Rle {
                            offsets,
                            payload,
                            has_alpha4,
                        },
                    })
                } else if is_mask_a4 {
                    if rest.len() != px_count.div_ceil(2) {
                        return None;
                    }
                    Some(Self {
                        width,
                        height,
                        chroma_key: None,
                        pixels: Pixels::MaskA4(rest),
                    })
                } else if is_i4 || is_i8 {
                    let pal_bytes = if is_i4 { 16usize * 2 } else { 256usize * 2 };
                    let idx_bytes = if is_i4 {
                        px_count.div_ceil(2)
                    } else {
                        px_count
                    };
                    let a_bytes = if has_alpha4 {
                        px_count.div_ceil(2)
                    } else {
                        0
                    };
                    let Some(pal_plus_idx) = pal_bytes.checked_add(idx_bytes) else {
                        return None;
                    };
                    let Some(needed) = pal_plus_idx.checked_add(a_bytes) else {
                        return None;
                    };
                    if rest.len() != needed {
                        return None;
                    }
                    let (palette, tail) = rest.split_at(pal_bytes);
                    let (indices, a_tail) = tail.split_at(idx_bytes);
                    let alpha4 = if has_alpha4 { Some(a_tail) } else { None };
                    Some(Self {
                        width,
                        height,
                        chroma_key,
                        pixels: if is_i4 {
                            Pixels::Indexed4 {
                                palette,
                                indices,
                                alpha4,
                            }
                        } else {
                            Pixels::Indexed8 {
                                palette,
                                indices,
                                alpha4,
                            }
                        },
                    })
                } else {
                    let Some(px_bytes) = px_count.checked_mul(2) else {
                        return None;
                    };
                    if has_alpha4 {
                        let a_bytes = px_count.div_ceil(2);
                        let Some(needed) = px_bytes.checked_add(a_bytes) else {
                            return None;
                        };
                        if rest.len() != needed {
                            return None;
                        }
                        let (pixels, alpha4) = rest.split_at(px_bytes);
                        Some(Self {
                            width,
                            height,
                            chroma_key,
                            pixels: Pixels::AlphaPlanar { pixels, alpha4 },
                        })
                    } else {
                        if rest.len() != px_bytes {
                            return None;
                        }
                        Some(Self {
                            width,
                            height,
                            chroma_key,
                            pixels: Pixels::LittleEndian(rest),
                        })
                    }
                }
            }
            _ => None,
        }
    }

    /// Reads an uncompressed or RLE source pixel at `(sx, sy)` with its 4-bit alpha (`0..=15`).
    pub fn pixel_rgba(&self, sx: usize, sy: usize) -> Option<(u16, u8)> {
        if sx >= usize::from(self.width) || sy >= usize::from(self.height) {
            return None;
        }
        let idx = sy * usize::from(self.width) + sx;
        let (raw, mut alpha) = match self.pixels {
            Pixels::Native(words) => (*words.get(idx)?, 15),
            Pixels::LittleEndian(bytes) => {
                let p = idx * 2;
                (u16::from_le_bytes([*bytes.get(p)?, *bytes.get(p + 1)?]), 15)
            }
            Pixels::AlphaPlanar { pixels, alpha4 } => {
                let p = idx * 2;
                let rgb = u16::from_le_bytes([*pixels.get(p)?, *pixels.get(p + 1)?]);
                let a = read_nibble(alpha4, idx)?;
                (rgb, a)
            }
            Pixels::MaskA4(mask) => (0xFFFF, read_nibble(mask, idx)?),
            Pixels::Indexed4 {
                palette,
                indices,
                alpha4,
            } => {
                let pal_idx = read_nibble(indices, idx)?;
                let rgb = read_palette_rgb565(palette, pal_idx)?;
                let a = match alpha4 {
                    Some(a4) => read_nibble(a4, idx)?,
                    None => 15,
                };
                (rgb, a)
            }
            Pixels::Indexed8 {
                palette,
                indices,
                alpha4,
            } => {
                let pal_idx = *indices.get(idx)?;
                let rgb = read_palette_rgb565(palette, pal_idx)?;
                let a = match alpha4 {
                    Some(a4) => read_nibble(a4, idx)?,
                    None => 15,
                };
                (rgb, a)
            }
            Pixels::Rle {
                offsets,
                payload,
                has_alpha4,
            } => {
                let tbl_off = sy * 4;
                let row_start = u32::from_le_bytes([
                    *offsets.get(tbl_off)?,
                    *offsets.get(tbl_off + 1)?,
                    *offsets.get(tbl_off + 2)?,
                    *offsets.get(tbl_off + 3)?,
                ]) as usize;
                let row_end = if sy + 1 < usize::from(self.height) {
                    let next_off = (sy + 1) * 4;
                    u32::from_le_bytes([
                        *offsets.get(next_off)?,
                        *offsets.get(next_off + 1)?,
                        *offsets.get(next_off + 2)?,
                        *offsets.get(next_off + 3)?,
                    ]) as usize
                } else {
                    payload.len()
                };
                if row_start > row_end || row_end > payload.len() {
                    return None;
                }
                let mut pos = row_start;
                let mut col = 0usize;
                let px_stride = if has_alpha4 { 3 } else { 2 };
                loop {
                    let ctrl = *payload.get(pos)?;
                    pos += 1;
                    let is_repeat = (ctrl & 0x80) != 0;
                    let run = usize::from(ctrl & 0x7F) + 1;
                    if sx < col + run {
                        let px_pos = if is_repeat {
                            pos
                        } else {
                            pos + (sx - col) * px_stride
                        };
                        let rgb =
                            u16::from_le_bytes([*payload.get(px_pos)?, *payload.get(px_pos + 1)?]);
                        let a = if has_alpha4 {
                            (*payload.get(px_pos + 2)?) & 0x0F
                        } else {
                            15
                        };
                        break (rgb, a);
                    }
                    col += run;
                    pos += if is_repeat {
                        px_stride
                    } else {
                        run * px_stride
                    };
                    if pos > row_end {
                        return None;
                    }
                }
            }
        };
        if !matches!(self.pixels, Pixels::MaskA4(_)) && self.chroma_key == Some(raw) {
            alpha = 0;
        }
        Some((raw, alpha))
    }

    /// Reads the 4-bit alpha coverage (`0..=15`) at source texel `(sx, sy)`.
    #[inline]
    pub fn sample_alpha(&self, sx: usize, sy: usize) -> Option<u8> {
        self.pixel_rgba(sx, sy).map(|(_, alpha)| alpha)
    }

    fn rotated_source_q8(&self, x: u32, y: u32, w: u32, h: u32, angle: u8) -> Option<(i64, i64)> {
        if x >= w || y >= h {
            return None;
        }
        let cx = 2 * i64::from(x) + 1 - i64::from(w);
        let cy = 2 * i64::from(y) + 1 - i64::from(h);
        let sin = i64::from(sine(angle));
        let cos = i64::from(sine(angle.wrapping_add(64)));
        let rx = (cx * cos + cy * sin).div_euclid(16384) + i64::from(w);
        let ry = (cy * cos - cx * sin).div_euclid(16384) + i64::from(h);
        if rx < 0 || ry < 0 || rx >= 2 * i64::from(w) || ry >= 2 * i64::from(h) {
            return None;
        }
        let u_q8 = (rx * i64::from(self.width) * 256) / (2 * i64::from(w));
        let v_q8 = (ry * i64::from(self.height) * 256) / (2 * i64::from(h));
        Some((u_q8, v_q8))
    }

    /// Sample nearest-neighbor `(rgb565, a4)` after a clockwise rotation about the centre.
    pub fn sample_rgba(&self, x: u32, y: u32, w: u32, h: u32, angle: u8) -> Option<(u16, u8)> {
        let (u_q8, v_q8) = self.rotated_source_q8(x, y, w, h, angle)?;
        let sx = (u_q8 >> 8) as usize;
        let sy = (v_q8 >> 8) as usize;
        self.pixel_rgba(sx, sy)
    }

    /// Sample the scaled bitmap after a clockwise rotation about its centre.
    /// Returns `None` outside rotation bounds or at transparent (`alpha == 0`) texels.
    pub fn sample(&self, x: u32, y: u32, w: u32, h: u32, angle: u8) -> Option<u16> {
        let (rgb, alpha) = self.sample_rgba(x, y, w, h, angle)?;
        (alpha > 0).then_some(rgb)
    }

    /// Bilinearly sample `(rgb565, a4)` using fixed-point texel interpolation.
    pub fn sample_bilinear(&self, x: u32, y: u32, w: u32, h: u32, angle: u8) -> Option<(u16, u8)> {
        let (u_q8, v_q8) = self.rotated_source_q8(x, y, w, h, angle)?;
        // Align pixel center offset by -0.5 texel (128 in Q8) for centered bilinear taps.
        let ux = (u_q8 - 128).max(0);
        let vy = (v_q8 - 128).max(0);
        let max_sx = usize::from(self.width - 1);
        let max_sy = usize::from(self.height - 1);
        let x0 = ((ux >> 8) as usize).min(max_sx);
        let y0 = ((vy >> 8) as usize).min(max_sy);
        let x1 = (x0 + 1).min(max_sx);
        let y1 = (y0 + 1).min(max_sy);
        let fx = (ux & 0xFF) as u32;
        let fy = (vy & 0xFF) as u32;
        let ix = 256 - fx;
        let iy = 256 - fy;

        let p00 = self.pixel_rgba(x0, y0)?;
        let p10 = self.pixel_rgba(x1, y0)?;
        let p01 = self.pixel_rgba(x0, y1)?;
        let p11 = self.pixel_rgba(x1, y1)?;

        let w00 = ix * iy;
        let w10 = fx * iy;
        let w01 = ix * fy;
        let w11 = fx * fy;

        let blend_ch = |shift: u32, mask: u16| -> u16 {
            let acc = u32::from((p00.0 >> shift) & mask) * w00
                + u32::from((p10.0 >> shift) & mask) * w10
                + u32::from((p01.0 >> shift) & mask) * w01
                + u32::from((p11.0 >> shift) & mask) * w11;
            ((acc + 32_768) >> 16) as u16
        };
        let r = blend_ch(11, 0x1F);
        let g = blend_ch(5, 0x3F);
        let b = blend_ch(0, 0x1F);
        let a_acc = u32::from(p00.1) * w00
            + u32::from(p10.1) * w10
            + u32::from(p01.1) * w01
            + u32::from(p11.1) * w11;
        let a = ((a_acc + 32_768) >> 16).min(15) as u8;
        Some(((r << 11) | (g << 5) | b, a))
    }
}
pub(crate) fn sine(angle: u8) -> i16 {
    // round(2^14 * sin(i*pi/128)) for one quadrant.
    const QUARTER: [i16; 65] = [
        0, 402, 804, 1205, 1606, 2006, 2404, 2801, 3196, 3590, 3981, 4370, 4756, 5139, 5520, 5897,
        6270, 6639, 7005, 7366, 7723, 8076, 8423, 8765, 9102, 9434, 9760, 10080, 10394, 10702,
        11003, 11297, 11585, 11866, 12140, 12406, 12665, 12916, 13160, 13395, 13623, 13842, 14053,
        14256, 14449, 14635, 14811, 14978, 15137, 15286, 15426, 15557, 15679, 15791, 15893, 15986,
        16069, 16143, 16207, 16261, 16305, 16340, 16364, 16379, 16384,
    ];
    let i = usize::from(angle & 63);
    match angle >> 6 {
        0 => QUARTER[i],
        1 => QUARTER[64 - i],
        2 => -QUARTER[i],
        _ => -QUARTER[64 - i],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampling_uses_pixel_centres_and_clips_after_rotation() {
        let image = Bitmap::new(2, 2, &[1, 2, 3, 4]).unwrap();
        for (angle, expected) in [
            (0, [1, 2, 3, 4]),
            (64, [3, 1, 4, 2]),
            (128, [4, 3, 2, 1]),
            (192, [2, 4, 1, 3]),
        ] {
            for i in 0..4 {
                assert_eq!(
                    image.sample(i % 2, i / 2, 2, 2, angle),
                    Some(expected[i as usize])
                );
            }
        }
        assert_eq!(image.sample(3, 3, 4, 4, 0), Some(4));
        assert_eq!(image.sample(0, 0, 8, 8, 32), None);
        assert_eq!(image.sample(0, 0, 0, 0, 0), None);
    }

    #[test]
    fn v2_chroma_key_planar_a4_rle_and_bilinear_work() {
        // 2x2 chroma-key + planar A4 (1 byte packing 4 nibbles? 2 bytes = 4 nibbles across 4 px)
        // pixels: [0xF800, 0xF81F (ck), 0x07E0, 0x001F], alphas: [15, 15, 7, 15]
        let mut raw_a4 = b"L565\x02\x03\x02\x00\x02\x00\x1f\xf8".to_vec();
        for w in [0xF800u16, 0xF81F, 0x07E0, 0x001F] {
            raw_a4.extend_from_slice(&w.to_le_bytes());
        }
        raw_a4.extend_from_slice(&[0xFF, 0xF7]); // px0=15, px1=15(ck->0), px2=7, px3=15
        let bmp = BitmapView::from_asset(&raw_a4).unwrap();
        assert_eq!(bmp.width(), 2);
        assert_eq!(bmp.height(), 2);
        assert!(bmp.has_alpha());
        assert!(!bmp.is_rle());
        assert_eq!(bmp.sample_rgba(0, 0, 2, 2, 0), Some((0xF800, 15)));
        assert_eq!(bmp.sample_rgba(1, 0, 2, 2, 0), Some((0xF81F, 0)));
        assert_eq!(bmp.sample(1, 0, 2, 2, 0), None);
        assert_eq!(bmp.sample_rgba(0, 1, 2, 2, 0), Some((0x07E0, 7)));

        // 2x2 RLE + A4: row 0 = repeat 2x (0xF800, a=12), row 1 = literal (0x07E0, a=5), (0x001F, a=15)
        let mut rle_a4 = b"L565\x02\x06\x02\x00\x02\x00\x00\x00".to_vec();
        rle_a4.extend_from_slice(&0u32.to_le_bytes()); // row 0 offset
        rle_a4.extend_from_slice(&4u32.to_le_bytes()); // row 1 offset
        rle_a4.push(0x81); // repeat 2
        rle_a4.extend_from_slice(&0xF800u16.to_le_bytes());
        rle_a4.push(12);
        rle_a4.push(0x01); // literal 2
        rle_a4.extend_from_slice(&0x07E0u16.to_le_bytes());
        rle_a4.push(5);
        rle_a4.extend_from_slice(&0x001Fu16.to_le_bytes());
        rle_a4.push(15);

        let rb = BitmapView::from_asset(&rle_a4).unwrap();
        assert!(rb.is_rle());
        assert!(rb.has_alpha());
        assert_eq!(rb.sample_rgba(1, 0, 2, 2, 0), Some((0xF800, 12)));
        assert_eq!(rb.sample_rgba(0, 1, 2, 2, 0), Some((0x07E0, 5)));
        assert_eq!(rb.sample_rgba(1, 1, 2, 2, 0), Some((0x001F, 15)));

        // Bilinear 2x upscale across a black/white gradient interpolates midtones
        let bw = Bitmap::new(2, 2, &[0x0000, 0xFFFF, 0x0000, 0xFFFF]).unwrap();
        let mid = bw.sample_bilinear(2, 1, 4, 4, 0).unwrap();
        assert!(mid.0 != 0x0000 && mid.0 != 0xFFFF);
        assert_eq!(mid.1, 15);
    }

    #[test]
    fn v2_mask_a4_and_indexed_formats_decode_correctly() {
        // 3x1 MaskA4 -> 2 packed bytes: px0=0x2, px1=0xB (0xB2), px2=0xF (0x0F)
        let mut mask = b"L565\x02\x08\x03\x00\x01\x00\x00\x00".to_vec();
        mask.extend_from_slice(&[0xB2, 0x0F]);
        let mb = BitmapView::from_asset(&mask).unwrap();
        assert!(mb.is_alpha_mask());
        assert!(mb.has_alpha());
        assert!(!mb.is_rle());
        assert_eq!(mb.sample_alpha(0, 0), Some(2));
        assert_eq!(mb.sample_alpha(1, 0), Some(11));
        assert_eq!(mb.sample_alpha(2, 0), Some(15));
        assert_eq!(mb.pixel_rgba(1, 0), Some((0xFFFF, 11)));

        // Reject invalid flag combinations (MASK_A4 + RLE or MASK_A4 + ALPHA4 or I4 + I8)
        let mut bad = mask.clone();
        bad[5] = L565_FLAG_MASK_A4 | L565_FLAG_RLE;
        assert!(BitmapView::from_asset(&bad).is_none());
        bad[5] = L565_FLAG_MASK_A4 | L565_FLAG_ALPHA4;
        assert!(BitmapView::from_asset(&bad).is_none());
        bad[5] = L565_FLAG_INDEXED4 | L565_FLAG_INDEXED8;
        assert!(BitmapView::from_asset(&bad).is_none());

        // 2x2 Indexed4 with 16-entry palette + chroma key 0x001F + 2 packed index bytes
        let mut i4 = b"L565\x02\x11\x02\x00\x02\x00\x1f\x00".to_vec();
        let mut pal16 = [0u16; 16];
        pal16[0] = 0xF800;
        pal16[1] = 0x07E0;
        pal16[2] = 0x001F; // chroma key
        pal16[15] = 0xFFE0;
        for w in pal16 {
            i4.extend_from_slice(&w.to_le_bytes());
        }
        // indices: px0=0, px1=1 -> 0x10; px2=2(ck), px3=15 -> 0xF2
        i4.extend_from_slice(&[0x10, 0xF2]);
        let b4 = BitmapView::from_asset(&i4).unwrap();
        assert!(b4.has_alpha());
        assert!(!b4.is_alpha_mask());
        assert_eq!(b4.pixel_rgba(0, 0), Some((0xF800, 15)));
        assert_eq!(b4.pixel_rgba(1, 0), Some((0x07E0, 15)));
        assert_eq!(b4.pixel_rgba(0, 1), Some((0x001F, 0)));
        assert_eq!(b4.pixel_rgba(1, 1), Some((0xFFE0, 15)));

        // 2x1 Indexed8 with ALPHA4 (256-entry palette)
        let mut i8 = b"L565\x02\x22\x02\x00\x01\x00\x00\x00".to_vec();
        let mut pal256 = [0u16; 256];
        pal256[42] = 0x1234;
        pal256[200] = 0xABCD;
        for w in pal256 {
            i8.extend_from_slice(&w.to_le_bytes());
        }
        i8.extend_from_slice(&[42, 200]); // indices
        i8.push(0x93); // a0=3, a1=9
        let b8 = BitmapView::from_asset(&i8).unwrap();
        assert!(b8.has_alpha());
        assert_eq!(b8.pixel_rgba(0, 0), Some((0x1234, 3)));
        assert_eq!(b8.pixel_rgba(1, 0), Some((0xABCD, 9)));
    }
}

/// Owned immutable RGB565 pixels; a committed frame retains a reference until it is replaced.
#[cfg(feature = "owned-images")]
#[derive(Debug)]
pub struct OwnedBitmap {
    width: u16,
    height: u16,
    pixels: alloc::boxed::Box<[u16]>,
}
#[cfg(feature = "owned-images")]
impl OwnedBitmap {
    pub fn new(width: u16, height: u16, pixels: alloc::boxed::Box<[u16]>) -> Option<Self> {
        BitmapView::new(width, height, &pixels)?;
        Some(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn view(&self) -> BitmapView<'_> {
        BitmapView {
            width: self.width,
            height: self.height,
            chroma_key: None,
            pixels: Pixels::Native(&self.pixels),
        }
    }
}
