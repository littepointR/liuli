use core::fmt;

use liuli_core::color::{
    PixelFormat, rgb565_swap_rb, rgb565_u16_to_abgr8888, rgb565_u16_to_argb8888,
    rgb565_u16_to_rgb666_bytes, rgb565_u16_to_rgb888, rgb888_to_gray8,
};
use liuli_core::dither::{DitherMode, gray8_to_gray4_dithered, gray8_to_mono1_dithered};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ByteOrder {
    BigEndian,
    LittleEndian,
}

impl ByteOrder {
    /// Encodes a native RGB565 word into host `u16` storage matching wire byte order `self`.
    #[inline]
    pub const fn encode_word(self, word: u16) -> u16 {
        match self {
            Self::BigEndian => u16::from_ne_bytes(word.to_be_bytes()),
            Self::LittleEndian => u16::from_ne_bytes(word.to_le_bytes()),
        }
    }
}

/// Converts a legacy RGB565 [`ByteOrder`] to the equivalent [`PixelFormat`].
#[inline]
pub const fn byte_order_to_pixel_format(order: ByteOrder) -> PixelFormat {
    match order {
        ByteOrder::BigEndian => PixelFormat::Rgb565Be,
        ByteOrder::LittleEndian => PixelFormat::Rgb565Le,
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BufferTooSmall;

impl fmt::Display for BufferTooSmall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("output byte buffer too small for RGB565 slice")
    }
}

/// Encodes logical RGB565 `words` into byte slice `out` in the requested wire `order`.
///
/// Checks capacity up front before writing so short buffers remain untouched.
pub fn encode_rgb565(
    words: &[u16],
    out: &mut [u8],
    order: ByteOrder,
) -> Result<usize, BufferTooSmall> {
    let needed = words.len().checked_mul(2).ok_or(BufferTooSmall)?;
    if out.len() < needed {
        return Err(BufferTooSmall);
    }
    for (i, &w) in words.iter().enumerate() {
        let bytes = match order {
            ByteOrder::BigEndian => w.to_be_bytes(),
            ByteOrder::LittleEndian => w.to_le_bytes(),
        };
        let off = i * 2;
        out[off] = bytes[0];
        out[off + 1] = bytes[1];
    }
    Ok(needed)
}

/// Encodes logical RGB565 `words` into 16-bit wire words in `out` in the requested `order`.
///
/// Useful for 16-bit DMA/SPI/QSPI TX FIFOs expecting pre-swapped `u16` buffers.
pub fn encode_rgb565_words(
    words: &[u16],
    out: &mut [u16],
    order: ByteOrder,
) -> Result<usize, BufferTooSmall> {
    if out.len() < words.len() {
        return Err(BufferTooSmall);
    }
    for (dst, &w) in out.iter_mut().zip(words.iter()) {
        *dst = order.encode_word(w);
    }
    Ok(words.len())
}

/// Converts logical RGB565 `words` in place to wire `order` (no-op if host order already matches).
pub fn encode_rgb565_in_place(words: &mut [u16], order: ByteOrder) {
    let needs_swap = match order {
        ByteOrder::BigEndian => cfg!(target_endian = "little"),
        ByteOrder::LittleEndian => cfg!(target_endian = "big"),
    };
    if !needs_swap {
        return;
    }
    for w in words {
        *w = w.swap_bytes();
    }
}

/// Encodes one row of native RGB565 `words` starting at global `(gx0, gy)` into `out`
/// using the requested target [`PixelFormat`] and stateless [`DitherMode`].
pub fn encode_rgb565_row(
    words: &[u16],
    gx0: i32,
    gy: i32,
    format: PixelFormat,
    dither: DitherMode,
    out: &mut [u8],
) -> Result<usize, BufferTooSmall> {
    let width = u32::try_from(words.len()).map_err(|_| BufferTooSmall)?;
    let needed = format.row_bytes(width);
    if out.len() < needed {
        return Err(BufferTooSmall);
    }
    if needed == 0 {
        return Ok(0);
    }

    match format {
        PixelFormat::Rgb565Le => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 2..i * 2 + 2].copy_from_slice(&w.to_le_bytes());
            }
        }
        PixelFormat::Rgb565Be => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 2..i * 2 + 2].copy_from_slice(&w.to_be_bytes());
            }
        }
        PixelFormat::Bgr565Le => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 2..i * 2 + 2].copy_from_slice(&rgb565_swap_rb(w).to_le_bytes());
            }
        }
        PixelFormat::Bgr565Be => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 2..i * 2 + 2].copy_from_slice(&rgb565_swap_rb(w).to_be_bytes());
            }
        }
        PixelFormat::Rgb666 => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 3..i * 3 + 3].copy_from_slice(&rgb565_u16_to_rgb666_bytes(w));
            }
        }
        PixelFormat::Rgb888 => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 3..i * 3 + 3].copy_from_slice(&rgb565_u16_to_rgb888(w).to_bytes());
            }
        }
        PixelFormat::Bgr888 => {
            for (i, &w) in words.iter().enumerate() {
                let c = rgb565_u16_to_rgb888(w);
                out[i * 3..i * 3 + 3].copy_from_slice(&[c.b, c.g, c.r]);
            }
        }
        PixelFormat::Xrgb8888 | PixelFormat::Argb8888 => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 4..i * 4 + 4].copy_from_slice(&rgb565_u16_to_argb8888(w, 0xFF).to_le_bytes());
            }
        }
        PixelFormat::Abgr8888 => {
            for (i, &w) in words.iter().enumerate() {
                out[i * 4..i * 4 + 4].copy_from_slice(&rgb565_u16_to_abgr8888(w, 0xFF).to_le_bytes());
            }
        }
        PixelFormat::Gray8 => {
            for (i, &w) in words.iter().enumerate() {
                out[i] = rgb888_to_gray8(rgb565_u16_to_rgb888(w)).0;
            }
        }
        PixelFormat::Gray4 => {
            out[..needed].fill(0);
            for (i, &w) in words.iter().enumerate() {
                let g = rgb888_to_gray8(rgb565_u16_to_rgb888(w)).0;
                let q4 = match dither {
                    DitherMode::Bayer4x4 => gray8_to_gray4_dithered(g, gx0.saturating_add(i as i32), gy),
                    DitherMode::None => g >> 4,
                };
                if (i & 1) == 0 {
                    out[i >> 1] |= q4 << 4;
                } else {
                    out[i >> 1] |= q4 & 0x0F;
                }
            }
        }
        PixelFormat::Mono1Msb | PixelFormat::Mono1Lsb => {
            out[..needed].fill(0);
            let msb = matches!(format, PixelFormat::Mono1Msb);
            for (i, &w) in words.iter().enumerate() {
                let g = rgb888_to_gray8(rgb565_u16_to_rgb888(w)).0;
                let on = match dither {
                    DitherMode::Bayer4x4 => gray8_to_mono1_dithered(g, gx0.saturating_add(i as i32), gy),
                    DitherMode::None => g >= 128,
                };
                if on {
                    let bit = if msb { 7 - (i & 7) } else { i & 7 };
                    out[i >> 3] |= 1u8 << bit;
                }
            }
        }
    }
    Ok(needed)
}
