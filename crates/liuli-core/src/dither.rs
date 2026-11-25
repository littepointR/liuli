/// Ordered dithering strategy for color-depth reduction.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum DitherMode {
    #[default]
    None = 0,
    Bayer4x4 = 1,
}

/// Standard normalized 4x4 Bayer threshold matrix (`0..=15`).
pub const BAYER_4X4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// Returns the 4-bit Bayer matrix threshold (`0..=15`) at global coordinates `(x, y)`.
#[inline(always)]
pub const fn bayer_4x4(x: i32, y: i32) -> u8 {
    BAYER_4X4[(((y & 3) << 2) | (x & 3)) as usize]
}

#[inline(always)]
const fn clamp_max(v: u32, max: u32) -> u16 {
    (if v > max { max } else { v }) as u16
}

/// Converts 8-bit sRGB channels to raw RGB565 using stateless Bayer 4x4 ordered dithering.
#[inline(always)]
pub const fn rgb888_to_rgb565_dithered(r: u8, g: u8, b: u8, x: i32, y: i32) -> u16 {
    let t = bayer_4x4(x, y) as u32;
    let r5 = clamp_max(((r as u32 * 31 * 16) / 255 + t) >> 4, 31);
    let g6 = clamp_max(((g as u32 * 63 * 16) / 255 + t) >> 4, 63);
    let b5 = clamp_max(((b as u32 * 31 * 16) / 255 + t) >> 4, 31);
    (r5 << 11) | (g6 << 5) | b5
}

/// Interpolates two raw RGB565 colors with `t_q8` (`0..=256`) and stateless Bayer 4x4 dithering.
///
/// Endpoints (`t_q8 == 0`, `t_q8 >= 256`) and constant fills (`start_raw == end_raw`) remain exact.
#[inline(always)]
pub const fn rgb565_lerp_dithered(start_raw: u16, end_raw: u16, t_q8: u32, x: i32, y: i32) -> u16 {
    let t = if t_q8 > 256 { 256 } else { t_q8 };
    if t == 0 {
        return start_raw;
    }
    if t == 256 {
        return end_raw;
    }
    let inv = 256 - t;
    let b = bayer_4x4(x, y) as u32;

    let sr = ((start_raw >> 11) & 0x1F) as u32;
    let sg = ((start_raw >> 5) & 0x3F) as u32;
    let sb = (start_raw & 0x1F) as u32;

    let er = ((end_raw >> 11) & 0x1F) as u32;
    let eg = ((end_raw >> 5) & 0x3F) as u32;
    let eb = (end_raw & 0x1F) as u32;

    let rr = clamp_max((((sr * inv + er * t) >> 4) + b) >> 4, 31);
    let rg = clamp_max((((sg * inv + eg * t) >> 4) + b) >> 4, 63);
    let rb = clamp_max((((sb * inv + eb * t) >> 4) + b) >> 4, 31);
    (rr << 11) | (rg << 5) | rb
}

/// Quantizes 8-bit grayscale to 4-bit grayscale (`0..=15`) using stateless Bayer 4x4 dithering.
#[inline(always)]
pub const fn gray8_to_gray4_dithered(g: u8, x: i32, y: i32) -> u8 {
    clamp_max(
        ((g as u32 * 15 * 16) / 255 + bayer_4x4(x, y) as u32) >> 4,
        15,
    ) as u8
}

/// Thresholds 8-bit grayscale to 1-bit monochrome using stateless Bayer 4x4 ordered halftone.
#[inline(always)]
pub const fn gray8_to_mono1_dithered(g: u8, x: i32, y: i32) -> bool {
    (g as u16) > (((bayer_4x4(x, y) as u16) << 4) + 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bayer_lookup_handles_negative_coords_and_exact_endpoints() {
        for y in -8..8 {
            for x in -8..8 {
                let v = bayer_4x4(x, y);
                assert!(v < 16);
                assert_eq!(rgb888_to_rgb565_dithered(0, 0, 0, x, y), 0);
                assert_eq!(rgb888_to_rgb565_dithered(255, 255, 255, x, y), 0xFFFF);
                assert_eq!(rgb565_lerp_dithered(0x1234, 0xABCD, 0, x, y), 0x1234);
                assert_eq!(rgb565_lerp_dithered(0x1234, 0xABCD, 256, x, y), 0xABCD);
                assert_eq!(rgb565_lerp_dithered(0x55AA, 0x55AA, 128, x, y), 0x55AA);
                assert_eq!(gray8_to_gray4_dithered(0, x, y), 0);
                assert_eq!(gray8_to_gray4_dithered(255, x, y), 15);
                assert!(!gray8_to_mono1_dithered(0, x, y));
                assert!(gray8_to_mono1_dithered(255, x, y));
            }
        }
    }

    #[test]
    fn bayer_halftone_and_rgb565_midpoint_distribute_evenly() {
        let mut lit = 0usize;
        let mut high = 0usize;
        for y in 0..4 {
            for x in 0..4 {
                if gray8_to_mono1_dithered(128, x, y) {
                    lit += 1;
                }
                let mid = rgb565_lerp_dithered(10 << 11, 11 << 11, 128, x, y);
                let r5 = mid >> 11;
                assert!(r5 == 10 || r5 == 11);
                if r5 == 11 {
                    high += 1;
                }
            }
        }
        assert_eq!(lit, 8);
        assert_eq!(high, 8);
    }
}
