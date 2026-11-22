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
