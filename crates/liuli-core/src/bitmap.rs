//! Nearest-neighbour RGB565 sampling. Rotation is expressed as an unsigned binary angle (one turn = 256).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pixels<'a> {
    Native(&'a [u16]),
    LittleEndian(&'a [u8]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitmapView<'a> {
    width: u16,
    height: u16,
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
            pixels: Pixels::Native(pixels),
        })
    }
    /// Load an L565 v1 image produced by the SDK image converter.
    pub const fn from_asset(bytes: &'a [u8]) -> Option<Self> {
        let [b'L', b'5', b'6', b'5', 1, 0, wl, wh, hl, hh, pixels @ ..] = bytes else {
            return None;
        };
        let width = u16::from_le_bytes([*wl, *wh]);
        let height = u16::from_le_bytes([*hl, *hh]);
        if width == 0 || height == 0 || pixels.len() as u64 != width as u64 * height as u64 * 2 {
            return None;
        }
        Some(Self {
            width,
            height,
            pixels: Pixels::LittleEndian(pixels),
        })
    }
    /// Sample the scaled bitmap after a clockwise rotation about its centre.
    /// The destination rectangle is also the clipping window for the rotation.
    pub fn sample(&self, x: u32, y: u32, w: u32, h: u32, angle: u8) -> Option<u16> {
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
        let sx = (rx * i64::from(self.width) / (2 * i64::from(w))) as usize;
        let sy = (ry * i64::from(self.height) / (2 * i64::from(h))) as usize;
        let i = sy * usize::from(self.width) + sx;
        match self.pixels {
            Pixels::Native(words) => words.get(i).copied(),
            Pixels::LittleEndian(bytes) => {
                Some(u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]))
            }
        }
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
            pixels: Pixels::Native(&self.pixels),
        }
    }
}
