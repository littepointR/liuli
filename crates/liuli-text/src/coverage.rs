use liuli_core::geometry::{IntoStorage, RawU16, Rgb565};

/// Reads the 4-bit A4 coverage (`0..=15`) at `(x, y)` from a tightly packed row-major nibble bitmap.
///
/// High nibble stores the even column pixel, low nibble stores the odd column pixel.
#[inline]
pub fn a4_pixel(bitmap: &[u8], width: u16, x: u16, y: u16) -> u8 {
    let w = width as usize;
    let row_bytes = w.div_ceil(2);
    let idx = (y as usize) * row_bytes + (x as usize) / 2;
    let byte = bitmap[idx];
    if x % 2 == 0 {
        (byte >> 4) & 0x0F
    } else {
        byte & 0x0F
    }
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
}
