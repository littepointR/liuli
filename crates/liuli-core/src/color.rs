use crate::geometry::{IntoStorage, RawU16, Rgb565, rgb565_to_u16};

/// 24-bit sRGB color (`R`, `G`, `B` in `0..=255`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Rgb888 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb888 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    #[inline]
    pub const fn to_bytes(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

/// 32-bit straight-alpha sRGBA color (`0..=255` per channel).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Rgba8888 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8888 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[inline]
    pub const fn rgb(self) -> Rgb888 {
        Rgb888 {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }
}

/// Cylindrical HSV color (`h` in `0..360` deg, `s`/`v` in `0..=255`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Hsv {
    pub h: u16,
    pub s: u8,
    pub v: u8,
}

impl Hsv {
    #[inline]
    pub const fn new(h: u16, s: u8, v: u8) -> Self {
        Self { h: h % 360, s, v }
    }
}

/// Cylindrical HSL color (`h` in `0..360` deg, `s`/`l` in `0..=255`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Hsl {
    pub h: u16,
    pub s: u8,
    pub l: u8,
}

impl Hsl {
    #[inline]
    pub const fn new(h: u16, s: u8, l: u8) -> Self {
        Self { h: h % 360, s, l }
    }
}

/// 8-bit grayscale luminance (`0..=255`).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Gray8(pub u8);

/// Target wire or framebuffer pixel format supported by row encoders and display sinks.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum PixelFormat {
    #[default]
    Rgb565Le = 0,
    Rgb565Be = 1,
    Bgr565Le = 2,
    Bgr565Be = 3,
    Rgb666 = 4,
    Rgb888 = 5,
    Bgr888 = 6,
    Xrgb8888 = 7,
    Argb8888 = 8,
    Abgr8888 = 9,
    Gray8 = 10,
    Gray4 = 11,
    Mono1Msb = 12,
    Mono1Lsb = 13,
}

impl PixelFormat {
    /// Nominal bits per pixel (18-bit `Rgb666` transfers as 3 bytes / 24 bits on 8-bit SPI/parallel buses).
    #[inline]
    pub const fn bits_per_pixel(self) -> u8 {
        match self {
            Self::Rgb565Le | Self::Rgb565Be | Self::Bgr565Le | Self::Bgr565Be => 16,
            Self::Rgb666 | Self::Rgb888 | Self::Bgr888 => 24,
            Self::Xrgb8888 | Self::Argb8888 | Self::Abgr8888 => 32,
            Self::Gray8 => 8,
            Self::Gray4 => 4,
            Self::Mono1Msb | Self::Mono1Lsb => 1,
        }
    }

    /// Packed byte count needed for one scanline of `width` pixels.
    #[inline]
    pub const fn row_bytes(self, width: u32) -> usize {
        let w = width as usize;
        match self {
            Self::Rgb565Le | Self::Rgb565Be | Self::Bgr565Le | Self::Bgr565Be => {
                w.saturating_mul(2)
            }
            Self::Rgb666 | Self::Rgb888 | Self::Bgr888 => w.saturating_mul(3),
            Self::Xrgb8888 | Self::Argb8888 | Self::Abgr8888 => w.saturating_mul(4),
            Self::Gray8 => w,
            Self::Gray4 => w.div_ceil(2),
            Self::Mono1Msb | Self::Mono1Lsb => w.div_ceil(8),
        }
    }

    /// Minimum staging buffer size in bytes required to encode at least one pixel/byte group.
    #[inline]
    pub const fn min_chunk_bytes(self) -> usize {
        match self {
            Self::Rgb565Le | Self::Rgb565Be | Self::Bgr565Le | Self::Bgr565Be => 2,
            Self::Rgb666 | Self::Rgb888 | Self::Bgr888 => 3,
            Self::Xrgb8888 | Self::Argb8888 | Self::Abgr8888 => 4,
            Self::Gray8 | Self::Gray4 | Self::Mono1Msb | Self::Mono1Lsb => 1,
        }
    }

    /// MIPI-DCS `0x3A (COLMOD)` interface pixel format code, if applicable.
    #[inline]
    pub const fn mipi_colmod(self) -> Option<u8> {
        match self {
            Self::Rgb565Le | Self::Rgb565Be | Self::Bgr565Le | Self::Bgr565Be => Some(0x55),
            Self::Rgb666 | Self::Rgb888 | Self::Bgr888 => Some(0x66),
            _ => None,
        }
    }

    /// Returns `true` for Blue-first pixel formats (`BGR*` or `ABGR*`).
    #[inline]
    pub const fn is_bgr(self) -> bool {
        matches!(
            self,
            Self::Bgr565Le | Self::Bgr565Be | Self::Bgr888 | Self::Abgr8888
        )
    }
}

/// Fast bit-truncating conversion from 8-bit sRGB channels to `Rgb565`.
#[inline]
pub const fn rgb888_to_rgb565_trunc(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

/// Nearest-level rounded conversion from 8-bit sRGB channels to `Rgb565`.
#[inline]
pub const fn rgb888_to_rgb565_round(r: u8, g: u8, b: u8) -> Rgb565 {
    let r5 = ((r as u16 * 31 + 127) / 255) as u8;
    let g6 = ((g as u16 * 63 + 127) / 255) as u8;
    let b5 = ((b as u16 * 31 + 127) / 255) as u8;
    Rgb565::new(r5, g6, b5)
}

/// Expands raw 16-bit RGB565 into `Rgb888` via standard bit replication.
#[inline]
pub const fn rgb565_u16_to_rgb888(raw: u16) -> Rgb888 {
    let r5 = ((raw >> 11) & 0x1F) as u8;
    let g6 = ((raw >> 5) & 0x3F) as u8;
    let b5 = (raw & 0x1F) as u8;
    Rgb888 {
        r: (r5 << 3) | (r5 >> 2),
        g: (g6 << 2) | (g6 >> 4),
        b: (b5 << 3) | (b5 >> 2),
    }
}

/// Expands `Rgb565` into `Rgb888` via standard bit replication.
#[inline]
pub fn rgb565_to_rgb888(color: Rgb565) -> Rgb888 {
    rgb565_u16_to_rgb888(rgb565_to_u16(color))
}

/// Swaps the red and blue 5-bit fields of a packed RGB565 word (`RGB565 <-> BGR565`).
#[inline]
pub const fn rgb565_swap_rb(raw: u16) -> u16 {
    ((raw & 0x001F) << 11) | (raw & 0x07E0) | (raw >> 11)
}

/// Converts packed 24-bit `0x00RRGGBB` integer to `Rgb565`.
#[inline]
pub const fn rgb565_from_hex(rgb24: u32) -> Rgb565 {
    rgb888_to_rgb565_trunc((rgb24 >> 16) as u8, (rgb24 >> 8) as u8, rgb24 as u8)
}

/// Converts packed 24-bit `0x00RRGGBB` integer to `Rgb888`.
#[inline]
pub const fn rgb888_from_hex(rgb24: u32) -> Rgb888 {
    Rgb888::new((rgb24 >> 16) as u8, (rgb24 >> 8) as u8, rgb24 as u8)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

const fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    let (Some(h), Some(l)) = (hex_nibble(hi), hex_nibble(lo)) else {
        return None;
    };
    Some((h << 4) | l)
}

/// Parses `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA` (leading `#` and whitespace optional).
pub fn parse_hex_str(s: &str) -> Option<Rgba8888> {
    let b = s.trim().as_bytes();
    let hex = match b.first() {
        Some(b'#') => &b[1..],
        _ => b,
    };
    match *hex {
        [r, g, b] => Some(Rgba8888::new(
            hex_byte(r, r)?,
            hex_byte(g, g)?,
            hex_byte(b, b)?,
            255,
        )),
        [r, g, b, a] => Some(Rgba8888::new(
            hex_byte(r, r)?,
            hex_byte(g, g)?,
            hex_byte(b, b)?,
            hex_byte(a, a)?,
        )),
        [r1, r2, g1, g2, b1, b2] => Some(Rgba8888::new(
            hex_byte(r1, r2)?,
            hex_byte(g1, g2)?,
            hex_byte(b1, b2)?,
            255,
        )),
        [r1, r2, g1, g2, b1, b2, a1, a2] => Some(Rgba8888::new(
            hex_byte(r1, r2)?,
            hex_byte(g1, g2)?,
            hex_byte(b1, b2)?,
            hex_byte(a1, a2)?,
        )),
        _ => None,
    }
}

/// Converts `Rgb888` to BT.709 grayscale (`(54*R + 183*G + 19*B + 128) >> 8`).
#[inline]
pub const fn rgb888_to_gray8(c: Rgb888) -> Gray8 {
    Gray8(((54 * c.r as u32 + 183 * c.g as u32 + 19 * c.b as u32 + 128) >> 8) as u8)
}

/// Converts `Rgb565` to BT.709 grayscale (`Gray8`).
#[inline]
pub fn rgb565_to_gray8(c: Rgb565) -> Gray8 {
    rgb888_to_gray8(rgb565_to_rgb888(c))
}

/// Converts grayscale luminance (`Gray8`) to neutral `Rgb565`.
#[inline]
pub const fn gray8_to_rgb565(g: Gray8) -> Rgb565 {
    rgb888_to_rgb565_trunc(g.0, g.0, g.0)
}

/// Converts raw RGB565 word into 18-bit MIPI-DCS RGB666 wire bytes (`6` MSB per byte, 2 LSB zero).
#[inline]
pub const fn rgb565_u16_to_rgb666_bytes(raw: u16) -> [u8; 3] {
    let c = rgb565_u16_to_rgb888(raw);
    [c.r & 0xFC, c.g & 0xFC, c.b & 0xFC]
}

/// Packs raw RGB565 word with 8-bit alpha into little-endian `ARGB8888` (`0xAARRGGBB`).
#[inline]
pub const fn rgb565_u16_to_argb8888(raw: u16, alpha: u8) -> u32 {
    let c = rgb565_u16_to_rgb888(raw);
    ((alpha as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)
}

/// Packs raw RGB565 word with 8-bit alpha into little-endian `ABGR8888` (`0xAABBGGRR`).
#[inline]
pub const fn rgb565_u16_to_abgr8888(raw: u16, alpha: u8) -> u32 {
    let c = rgb565_u16_to_rgb888(raw);
    ((alpha as u32) << 24) | ((c.b as u32) << 16) | ((c.g as u32) << 8) | (c.r as u32)
}

/// Pure-integer conversion from `Hsv` to `Rgb888`.
pub fn hsv_to_rgb888(hsv: Hsv) -> Rgb888 {
    if hsv.s == 0 {
        return Rgb888::new(hsv.v, hsv.v, hsv.v);
    }
    let h = hsv.h % 360;
    let region = h / 60;
    let rem = u32::from(h % 60);
    let s = u32::from(hsv.s);
    let v = u32::from(hsv.v);

    const DENOM: u32 = 255 * 60;
    let p = ((v * (255 - s) + 127) / 255) as u8;
    let q = ((v * (DENOM - s * rem) + DENOM / 2) / DENOM) as u8;
    let t = ((v * (DENOM - s * (60 - rem)) + DENOM / 2) / DENOM) as u8;
    let v8 = hsv.v;

    match region {
        0 => Rgb888::new(v8, t, p),
        1 => Rgb888::new(q, v8, p),
        2 => Rgb888::new(p, v8, t),
        3 => Rgb888::new(p, q, v8),
        4 => Rgb888::new(t, p, v8),
        _ => Rgb888::new(v8, p, q),
    }
}

#[inline]
fn signed_hue_term(diff: i32, denom: i32) -> i32 {
    if diff >= 0 {
        (60 * diff + denom / 2) / denom
    } else {
        -((60 * (-diff) + denom / 2) / denom)
    }
}

/// Pure-integer conversion from `Rgb888` to `Hsv`.
pub fn rgb888_to_hsv(c: Rgb888) -> Hsv {
    let r = u32::from(c.r);
    let g = u32::from(c.g);
    let b = u32::from(c.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max as u8;
    if max == 0 || delta == 0 {
        return Hsv { h: 0, s: 0, v };
    }
    let s = ((delta * 255 + max / 2) / max).min(255) as u8;
    let d = delta as i32;
    let raw_h = if max == r {
        signed_hue_term(g as i32 - b as i32, d)
    } else if max == g {
        120 + signed_hue_term(b as i32 - r as i32, d)
    } else {
        240 + signed_hue_term(r as i32 - g as i32, d)
    };
    Hsv {
        h: raw_h.rem_euclid(360) as u16,
        s,
        v,
    }
}

/// Converts `Hsv` directly to rounded `Rgb565`.
#[inline]
pub fn hsv_to_rgb565(hsv: Hsv) -> Rgb565 {
    let c = hsv_to_rgb888(hsv);
    rgb888_to_rgb565_round(c.r, c.g, c.b)
}

/// Pure-integer conversion from `Hsl` to `Rgb888`.
pub fn hsl_to_rgb888(hsl: Hsl) -> Rgb888 {
    if hsl.s == 0 {
        return Rgb888::new(hsl.l, hsl.l, hsl.l);
    }
    let h = hsl.h % 360;
    let sect = h / 60;
    let rem = u32::from(h % 60);
    let f = if (sect & 1) == 0 { rem } else { 60 - rem };

    let s = u32::from(hsl.s);
    let l = u32::from(hsl.l);
    let two_l_diff = (2 * l as i32 - 255).unsigned_abs();
    let c_num = (255u32.saturating_sub(two_l_diff)) * s;
    let m_num = (l * 255).saturating_sub(c_num / 2);
    let x_num = (c_num * f + 30) / 60;

    let hi = ((c_num + m_num + 127) / 255).min(255) as u8;
    let mid = ((x_num + m_num + 127) / 255).min(255) as u8;
    let lo = ((m_num + 127) / 255).min(255) as u8;

    match sect {
        0 => Rgb888::new(hi, mid, lo),
        1 => Rgb888::new(mid, hi, lo),
        2 => Rgb888::new(lo, hi, mid),
        3 => Rgb888::new(lo, mid, hi),
        4 => Rgb888::new(mid, lo, hi),
        _ => Rgb888::new(hi, lo, mid),
    }
}

/// Pure-integer conversion from `Rgb888` to `Hsl`.
pub fn rgb888_to_hsl(c: Rgb888) -> Hsl {
    let r = u32::from(c.r);
    let g = u32::from(c.g);
    let b = u32::from(c.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min).div_ceil(2) as u8;
    if delta == 0 {
        return Hsl { h: 0, s: 0, l };
    }
    let denom = 255u32.saturating_sub((max + min).abs_diff(255)).max(1);
    let s = ((delta * 255 + denom / 2) / denom).min(255) as u8;
    let d = delta as i32;
    let raw_h = if max == r {
        signed_hue_term(g as i32 - b as i32, d)
    } else if max == g {
        120 + signed_hue_term(b as i32 - r as i32, d)
    } else {
        240 + signed_hue_term(r as i32 - g as i32, d)
    };
    Hsl {
        h: raw_h.rem_euclid(360) as u16,
        s,
        l,
    }
}

/// Converts `Hsl` directly to rounded `Rgb565`.
#[inline]
pub fn hsl_to_rgb565(hsl: Hsl) -> Rgb565 {
    let c = hsl_to_rgb888(hsl);
    rgb888_to_rgb565_round(c.r, c.g, c.b)
}

/// Linearly blends two `Rgb565` colors (`t = 0` returns `a`, `t = 255` returns `b`).
#[inline]
pub fn mix_rgb565(a: Rgb565, b: Rgb565, t: u8) -> Rgb565 {
    if t == 0 {
        return a;
    }
    if t == 255 {
        return b;
    }
    let fa = u32::from(a.into_storage());
    let fb = u32::from(b.into_storage());
    let t32 = u32::from(t);
    let inv = 255 - t32;
    let rr = (((fa >> 11) * inv + (fb >> 11) * t32 + 127) / 255) as u16;
    let rg = ((((fa >> 5) & 0x3F) * inv + ((fb >> 5) & 0x3F) * t32 + 127) / 255) as u16;
    let rb = (((fa & 0x1F) * inv + (fb & 0x1F) * t32 + 127) / 255) as u16;
    Rgb565::from(RawU16::new((rr << 11) | (rg << 5) | rb))
}

/// Lightens an `Rgb565` color toward white by `amount` (`0..=255`).
#[inline]
pub fn lighten_rgb565(c: Rgb565, amount: u8) -> Rgb565 {
    mix_rgb565(c, Rgb565::new(31, 63, 31), amount)
}

/// Darkens an `Rgb565` color toward black by `amount` (`0..=255`).
#[inline]
pub fn darken_rgb565(c: Rgb565, amount: u8) -> Rgb565 {
    mix_rgb565(c, Rgb565::new(0, 0, 0), amount)
}

/// Bitwise channel complement of an `Rgb565` color.
#[inline]
pub fn invert_rgb565(c: Rgb565) -> Rgb565 {
    Rgb565::from(RawU16::new(!c.into_storage()))
}

/// Exact integer sRGB->linearized 33-entry lookup table scaled to `0..=10_000`.
const SRGB_LIN_LUT: [u16; 33] = [
    0, 24, 52, 87, 134, 195, 272, 366, 478, 608, 759, 930, 1122, 1337, 1574, 1835, 2120, 2430,
    2766, 3128, 3516, 3932, 4376, 4848, 5349, 5880, 6440, 7031, 7653, 8306, 8991, 9708, 10000,
];

#[inline]
const fn channel_lin_5bit(v5: u8) -> u32 {
    let idx = if v5 > 31 { 31 } else { v5 } as usize;
    let pos = (idx * 32) / 31;
    let rem = (idx * 32) % 31;
    let a = SRGB_LIN_LUT[pos] as u32;
    if rem == 0 {
        a
    } else {
        let b = SRGB_LIN_LUT[pos + 1] as u32;
        a + ((b - a) * rem as u32) / 31
    }
}

#[inline]
const fn channel_lin_6bit(v6: u8) -> u32 {
    let idx = if v6 > 63 { 63 } else { v6 } as usize;
    let pos = (idx * 32) / 63;
    let rem = (idx * 32) % 63;
    let a = SRGB_LIN_LUT[pos] as u32;
    if rem == 0 {
        a
    } else {
        let b = SRGB_LIN_LUT[pos + 1] as u32;
        a + ((b - a) * rem as u32) / 63
    }
}

/// Relative luminance scaled to `0..=100_000_000`.
#[inline]
pub fn relative_luminance_scaled(color: Rgb565) -> u32 {
    let raw = rgb565_to_u16(color);
    let r = channel_lin_5bit((raw >> 11) as u8);
    let g = channel_lin_6bit(((raw >> 5) & 0x3F) as u8);
    let b = channel_lin_5bit((raw & 0x1F) as u8);
    2126 * r + 7152 * g + 722 * b
}

/// WCAG relative contrast ratio between two `Rgb565` colors in tenths (e.g. `45` = 4.5:1, `210` = 21.0:1).
pub fn contrast_ratio_x10(a: Rgb565, b: Rgb565) -> u16 {
    let la = relative_luminance_scaled(a);
    let lb = relative_luminance_scaled(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    let num = (hi + 5_000_000) as u64 * 10;
    let den = (lo + 5_000_000) as u64;
    ((num + den / 2) / den) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_rgb888_and_bgr_roundtrip() {
        let red565 = rgb888_to_rgb565_trunc(255, 0, 0);
        assert_eq!(rgb565_to_u16(red565), 0xF800);
        assert_eq!(rgb565_to_rgb888(red565), Rgb888::new(255, 0, 0));

        // 195/255 = 0.7647 -> *31 = 23.7 -> round gives 24, trunc gives 24; test 131: 131>>3=16, round(131*31/255)=16
        // 135: 135>>3=16 (132), 135*31/255 = 16.41 -> 16; 140: 140>>3=17, 140*31/255=17.02 -> 17
        // 7: 7>>3=0, (7*31+127)/255 = 344/255 = 1
        assert_eq!(rgb565_to_u16(rgb888_to_rgb565_trunc(7, 3, 7)), 0);
        assert_eq!(
            rgb565_to_u16(rgb888_to_rgb565_round(7, 3, 7)),
            (1 << 11) | (1 << 5) | 1
        );

        assert_eq!(rgb565_swap_rb(0xF800), 0x001F);
        assert_eq!(rgb565_swap_rb(0x07E0), 0x07E0);
        assert_eq!(rgb565_swap_rb(0x001F), 0xF800);

        assert_eq!(rgb565_u16_to_rgb666_bytes(0xF800), [0xFC, 0x00, 0x00]);
        assert_eq!(rgb565_u16_to_argb8888(0xF800, 0x80), 0x80FF_0000);
        assert_eq!(rgb565_u16_to_abgr8888(0xF800, 0x80), 0x8000_00FF);
    }

    #[test]
    fn hex_parsing_handles_short_full_and_invalid_strings() {
        assert_eq!(parse_hex_str("#f80"), Some(Rgba8888::new(255, 136, 0, 255)));
        assert_eq!(parse_hex_str("08f4"), Some(Rgba8888::new(0, 136, 255, 68)));
        assert_eq!(
            parse_hex_str("  #1490ec "),
            Some(Rgba8888::new(20, 144, 236, 255))
        );
        assert_eq!(
            parse_hex_str("#1490ec80"),
            Some(Rgba8888::new(20, 144, 236, 128))
        );
        assert_eq!(parse_hex_str("#12"), None);
        assert_eq!(parse_hex_str("#zzzzzz"), None);
        assert_eq!(
            rgb565_to_u16(rgb565_from_hex(0x00FF00)),
            rgb565_to_u16(Rgb565::new(0, 63, 0))
        );
        assert_eq!(rgb888_from_hex(0x123456), Rgb888::new(0x12, 0x34, 0x56));
    }

    #[test]
    fn hsv_and_hsl_conversions_cover_primaries_grays_and_roundtrips() {
        let cases = [
            (Rgb888::new(255, 0, 0), 0u16),
            (Rgb888::new(255, 255, 0), 60),
            (Rgb888::new(0, 255, 0), 120),
            (Rgb888::new(0, 255, 255), 180),
            (Rgb888::new(0, 0, 255), 240),
            (Rgb888::new(255, 0, 255), 300),
        ];
        for (rgb, hue) in cases {
            let hsv = rgb888_to_hsv(rgb);
            assert_eq!((hsv.h, hsv.s, hsv.v), (hue, 255, 255));
            assert_eq!(hsv_to_rgb888(hsv), rgb);

            let hsl = rgb888_to_hsl(rgb);
            assert_eq!((hsl.h, hsl.s), (hue, 255));
            let back = hsl_to_rgb888(hsl);
            assert!(back.r.abs_diff(rgb.r) <= 1);
            assert!(back.g.abs_diff(rgb.g) <= 1);
            assert!(back.b.abs_diff(rgb.b) <= 1);
            assert_eq!(
                hsl_to_rgb565(hsl),
                rgb888_to_rgb565_round(rgb.r, rgb.g, rgb.b)
            );
            assert_eq!(
                hsv_to_rgb565(hsv),
                rgb888_to_rgb565_round(rgb.r, rgb.g, rgb.b)
            );
        }

        for gray in [0u8, 64, 128, 255] {
            let c = Rgb888::new(gray, gray, gray);
            let hsv = rgb888_to_hsv(c);
            assert_eq!((hsv.s, hsv.v), (0, gray));
            assert_eq!(hsv_to_rgb888(hsv), c);
            let hsl = rgb888_to_hsl(c);
            assert_eq!((hsl.s, hsl.l), (0, gray));
            assert_eq!(hsl_to_rgb888(hsl), c);
        }

        // Arbitrary saturated/unsaturated colors roundtrip within +/-2 in 8-bit space.
        for r in [12u8, 84, 196] {
            for g in [40u8, 162, 220] {
                for b in [28u8, 130, 244] {
                    let c = Rgb888::new(r, g, b);
                    let hsv_back = hsv_to_rgb888(rgb888_to_hsv(c));
                    assert!(hsv_back.r.abs_diff(r) <= 2);
                    assert!(hsv_back.g.abs_diff(g) <= 2);
                    assert!(hsv_back.b.abs_diff(b) <= 2);
                    let hsl_back = hsl_to_rgb888(rgb888_to_hsl(c));
                    assert!(hsl_back.r.abs_diff(r) <= 2);
                    assert!(hsl_back.g.abs_diff(g) <= 2);
                    assert!(hsl_back.b.abs_diff(b) <= 2);
                }
            }
        }
    }

    #[test]
    fn pixel_format_tone_and_contrast_helpers_match_specs() {
        assert_eq!(PixelFormat::Rgb565Le.bits_per_pixel(), 16);
        assert_eq!(PixelFormat::Rgb666.row_bytes(10), 30);
        assert_eq!(PixelFormat::Argb8888.row_bytes(10), 40);
        assert_eq!(PixelFormat::Gray4.row_bytes(5), 3);
        assert_eq!(PixelFormat::Mono1Msb.row_bytes(9), 2);
        assert_eq!(PixelFormat::Rgb666.mipi_colmod(), Some(0x66));
        assert_eq!(PixelFormat::Bgr565Be.mipi_colmod(), Some(0x55));
        assert!(PixelFormat::Bgr888.is_bgr());
        assert!(!PixelFormat::Rgb888.is_bgr());

        let black = Rgb565::new(0, 0, 0);
        let white = Rgb565::new(31, 63, 31);
        assert_eq!(contrast_ratio_x10(black, white), 210);
        assert_eq!(rgb565_to_gray8(white), Gray8(255));
        assert_eq!(gray8_to_rgb565(Gray8(255)), white);
        assert_eq!(lighten_rgb565(black, 255), white);
        assert_eq!(darken_rgb565(white, 255), black);
        assert_eq!(invert_rgb565(black), white);
    }
}
