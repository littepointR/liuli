use core::fmt;

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
