use core::fmt;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ByteOrder {
    BigEndian,
    LittleEndian,
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
