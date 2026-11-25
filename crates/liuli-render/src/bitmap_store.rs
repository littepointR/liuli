use liuli_core::bitmap::{L565_FLAG_ALPHA4, L565_FLAG_CHROMA_KEY, L565_FLAG_RLE};
use liuli_core::geometry::Rect;
use liuli_text::ReadAt;

use crate::band::BandTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredBitmapError<E> {
    Io(E),
    ShortRead,
    InvalidFormat,
}

fn read_exact_at<R: ReadAt>(
    reader: &mut R,
    mut pos: usize,
    mut dst: &mut [u8],
) -> Result<(), StoredBitmapError<R::Error>> {
    while !dst.is_empty() {
        let n = reader.read_at(pos, dst).map_err(StoredBitmapError::Io)?;
        if n == 0 || n > dst.len() {
            return Err(StoredBitmapError::ShortRead);
        }
        pos = pos.checked_add(n).ok_or(StoredBitmapError::ShortRead)?;
        dst = &mut dst[n..];
    }
    Ok(())
}

/// Streaming L565 v1/v2 bitmap decoder backed by [`ReadAt`] (SPI/QSPI NOR flash or SD/flash file)
/// that renders directly into a [`BandTarget`] without heap allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredBitmap {
    width: u16,
    height: u16,
    version: u8,
    flags: u8,
    chroma_key: Option<u16>,
    data_offset: usize,
    payload_offset: usize,
}

impl StoredBitmap {
    pub fn open<R: ReadAt>(reader: &mut R) -> Result<Self, StoredBitmapError<R::Error>> {
        let mut hdr = [0u8; 12];
        read_exact_at(reader, 0, &mut hdr)?;
        if &hdr[..4] != b"L565" {
            return Err(StoredBitmapError::InvalidFormat);
        }
        let version = hdr[4];
        let flags = hdr[5];
        let width = u16::from_le_bytes([hdr[6], hdr[7]]);
        let height = u16::from_le_bytes([hdr[8], hdr[9]]);
        if width == 0 || height == 0 {
            return Err(StoredBitmapError::InvalidFormat);
        }

        let total_len = reader.byte_len();
        match version {
            1 if flags == 0 => {
                let needed = 10usize
                    .checked_add(usize::from(width) * usize::from(height) * 2)
                    .ok_or(StoredBitmapError::InvalidFormat)?;
                if total_len < needed {
                    return Err(StoredBitmapError::InvalidFormat);
                }
                Ok(Self {
                    width,
                    height,
                    version,
                    flags,
                    chroma_key: None,
                    data_offset: 10,
                    payload_offset: 10,
                })
            }
            2 => {
                let chroma_key = if (flags & L565_FLAG_CHROMA_KEY) != 0 {
                    Some(u16::from_le_bytes([hdr[10], hdr[11]]))
                } else {
                    None
                };
                let data_offset = 12usize;
                let is_rle = (flags & L565_FLAG_RLE) != 0;
                let has_a4 = (flags & L565_FLAG_ALPHA4) != 0;
                let payload_offset = if is_rle {
                    let tbl_bytes = usize::from(height) * 4;
                    let po = data_offset
                        .checked_add(tbl_bytes)
                        .ok_or(StoredBitmapError::InvalidFormat)?;
                    if total_len < po {
                        return Err(StoredBitmapError::InvalidFormat);
                    }
                    po
                } else {
                    let px_bytes = usize::from(width) * usize::from(height) * 2;
                    let a_bytes = if has_a4 {
                        (usize::from(width) * usize::from(height)).div_ceil(2)
                    } else {
                        0
                    };
                    if total_len < data_offset + px_bytes + a_bytes {
                        return Err(StoredBitmapError::InvalidFormat);
                    }
                    data_offset
                };
                Ok(Self {
                    width,
                    height,
                    version,
                    flags,
                    chroma_key,
                    data_offset,
                    payload_offset,
                })
            }
            _ => Err(StoredBitmapError::InvalidFormat),
        }
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
    pub const fn version(&self) -> u8 {
        self.version
    }

    #[inline]
    pub const fn has_alpha(&self) -> bool {
        self.chroma_key.is_some() || (self.flags & L565_FLAG_ALPHA4) != 0
    }

    #[inline]
    pub const fn is_rle(&self) -> bool {
        (self.flags & L565_FLAG_RLE) != 0
    }

    #[inline]
    pub const fn chroma_key(&self) -> Option<u16> {
        self.chroma_key
    }

    fn read_raw_pixel<R: ReadAt>(
        &self,
        reader: &mut R,
        sx: usize,
        sy: usize,
    ) -> Result<(u16, u8), StoredBitmapError<R::Error>> {
        let idx = sy * usize::from(self.width) + sx;
        let mut rgb_buf = [0u8; 2];
        read_exact_at(reader, self.data_offset + idx * 2, &mut rgb_buf)?;
        let rgb = u16::from_le_bytes(rgb_buf);
        let mut alpha = if (self.flags & L565_FLAG_ALPHA4) != 0 {
            let alpha_base =
                self.data_offset + usize::from(self.width) * usize::from(self.height) * 2;
            let mut a_byte = [0u8; 1];
            read_exact_at(reader, alpha_base + (idx / 2), &mut a_byte)?;
            if (idx & 1) == 0 {
                a_byte[0] & 0x0F
            } else {
                (a_byte[0] >> 4) & 0x0F
            }
        } else {
            15
        };
        if self.chroma_key == Some(rgb) {
            alpha = 0;
        }
        Ok((rgb, alpha))
    }

    /// Blits the bitmap scaled to `dst` clipped by `clip` directly into `target` row-by-row.
    pub fn blit_to_band<R: ReadAt>(
        &self,
        reader: &mut R,
        target: &mut BandTarget<'_>,
        dst: Rect,
        clip: Rect,
        alpha: u8,
    ) -> Result<(), StoredBitmapError<R::Error>> {
        let band_rect = target.band_rect();
        let area = dst.intersect(clip).intersect(band_rect);
        if area.is_empty() || alpha == 0 || dst.w == 0 || dst.h == 0 {
            return Ok(());
        }
        let g_alpha = alpha.min(15);
        let stride = target.width() as usize;

        if self.is_rle() {
            let has_a4 = (self.flags & L565_FLAG_ALPHA4) != 0;
            let px_stride: usize = if has_a4 { 3 } else { 2 };
            let mut tbl_buf = [0u8; 4];
            let mut pkt_buf = [0u8; 64 * 3];

            for py in 0..area.h {
                let y = area.y as i64 + i64::from(py);
                let ly = (y - i64::from(dst.y)) as usize;
                let sy = ((ly * usize::from(self.height)) / dst.h as usize)
                    .min(usize::from(self.height - 1));
                read_exact_at(reader, self.data_offset + sy * 4, &mut tbl_buf)?;
                let mut stream_pos = self.payload_offset + u32::from_le_bytes(tbl_buf) as usize;

                let row_dst_base = (y - i64::from(band_rect.y)) as usize * stride;
                let mut rle_col = 0usize;
                let mut run_rem = 0usize;
                let mut run_is_repeat = false;
                let mut repeat_rgb = 0u16;
                let mut repeat_a = 15u8;
                let mut lit_len = 0usize;
                let mut lit_idx = 0usize;

                let mut last_sx: Option<usize> = None;
                let mut cached_px = (0u16, 15u8);

                for px in 0..area.w {
                    let x = area.x as i64 + i64::from(px);
                    let lx = (x - i64::from(dst.x)) as usize;
                    let sx = ((lx * usize::from(self.width)) / dst.w as usize)
                        .min(usize::from(self.width - 1));

                    let (rgb, px_alpha) = if last_sx == Some(sx) {
                        cached_px
                    } else {
                        while rle_col <= sx {
                            if run_rem == 0 {
                                let mut ctrl_b = [0u8; 1];
                                read_exact_at(reader, stream_pos, &mut ctrl_b)?;
                                stream_pos += 1;
                                run_is_repeat = (ctrl_b[0] & 0x80) != 0;
                                run_rem = usize::from(ctrl_b[0] & 0x7F) + 1;
                                if run_is_repeat {
                                    read_exact_at(reader, stream_pos, &mut pkt_buf[..px_stride])?;
                                    stream_pos += px_stride;
                                    repeat_rgb = u16::from_le_bytes([pkt_buf[0], pkt_buf[1]]);
                                    repeat_a = if has_a4 { pkt_buf[2] & 0x0F } else { 15 };
                                } else {
                                    lit_len = run_rem;
                                    lit_idx = 0;
                                    let chunk = (lit_len.min(64)) * px_stride;
                                    read_exact_at(reader, stream_pos, &mut pkt_buf[..chunk])?;
                                }
                            }
                            let step = (sx - rle_col + 1).min(run_rem);
                            if !run_is_repeat {
                                let mut consumed = 0usize;
                                while consumed < step {
                                    if lit_idx > 0 && lit_idx.is_multiple_of(64) {
                                        let next_rem = (lit_len - lit_idx).min(64) * px_stride;
                                        read_exact_at(
                                            reader,
                                            stream_pos + lit_idx * px_stride,
                                            &mut pkt_buf[..next_rem],
                                        )?;
                                    }
                                    let off = (lit_idx % 64) * px_stride;
                                    repeat_rgb =
                                        u16::from_le_bytes([pkt_buf[off], pkt_buf[off + 1]]);
                                    repeat_a = if has_a4 { pkt_buf[off + 2] & 0x0F } else { 15 };
                                    lit_idx += 1;
                                    consumed += 1;
                                }
                            }
                            rle_col += step;
                            run_rem -= step;
                            if !run_is_repeat && run_rem == 0 {
                                stream_pos += lit_len * px_stride;
                            }
                        }
                        let mut a = repeat_a;
                        if self.chroma_key == Some(repeat_rgb) {
                            a = 0;
                        }
                        cached_px = (repeat_rgb, a);
                        last_sx = Some(sx);
                        cached_px
                    };
                    let eff_a = ((u16::from(px_alpha) * u16::from(g_alpha) + 7) / 15) as u8;
                    if eff_a > 0 {
                        let idx = row_dst_base + (x - i64::from(band_rect.x)) as usize;
                        target.blend_pixel_a4(idx, rgb, eff_a);
                    }
                }
            }
            return Ok(());
        }

        const ROW_CHUNK_PX: usize = 64;
        let mut raw_buf = [0u8; ROW_CHUNK_PX * 2];
        let mut a4_buf = [0u8; ROW_CHUNK_PX / 2 + 1];
        let has_a4 = (self.flags & L565_FLAG_ALPHA4) != 0;
        let alpha_base = self.data_offset + usize::from(self.width) * usize::from(self.height) * 2;

        for py in 0..area.h {
            let y = area.y as i64 + i64::from(py);
            let ly = (y - i64::from(dst.y)) as usize;
            let sy = ((ly * usize::from(self.height)) / dst.h as usize)
                .min(usize::from(self.height - 1));
            let row_dst_base = (y - i64::from(band_rect.y)) as usize * stride;

            if dst.w == u32::from(self.width) && area.x >= dst.x {
                let sx0 = (area.x - dst.x) as usize;
                let mut done = 0usize;
                let total = area.w as usize;
                while done < total {
                    let count = (total - done).min(ROW_CHUNK_PX);
                    let px_idx = sy * usize::from(self.width) + sx0 + done;
                    read_exact_at(
                        reader,
                        self.data_offset + px_idx * 2,
                        &mut raw_buf[..count * 2],
                    )?;
                    if has_a4 {
                        let a_start = px_idx / 2;
                        let a_end = (px_idx + count - 1) / 2;
                        let a_bytes = a_end - a_start + 1;
                        read_exact_at(reader, alpha_base + a_start, &mut a4_buf[..a_bytes])?;
                    }
                    let band_x0 = (area.x - band_rect.x) as usize + done;
                    for i in 0..count {
                        let rgb = u16::from_le_bytes([raw_buf[i * 2], raw_buf[i * 2 + 1]]);
                        let mut pa = if has_a4 {
                            let cur_idx = px_idx + i;
                            let byte_i = cur_idx / 2 - px_idx / 2;
                            let b = a4_buf[byte_i];
                            if (cur_idx & 1) == 0 {
                                b & 0x0F
                            } else {
                                (b >> 4) & 0x0F
                            }
                        } else {
                            15
                        };
                        if self.chroma_key == Some(rgb) {
                            pa = 0;
                        }
                        let eff_a = ((u16::from(pa) * u16::from(g_alpha) + 7) / 15) as u8;
                        if eff_a > 0 {
                            target.blend_pixel_a4(row_dst_base + band_x0 + i, rgb, eff_a);
                        }
                    }
                    done += count;
                }
            } else {
                for px in 0..area.w {
                    let x = area.x as i64 + i64::from(px);
                    let lx = (x - i64::from(dst.x)) as usize;
                    let sx = ((lx * usize::from(self.width)) / dst.w as usize)
                        .min(usize::from(self.width - 1));
                    let (rgb, pa) = self.read_raw_pixel(reader, sx, sy)?;
                    let eff_a = ((u16::from(pa) * u16::from(g_alpha) + 7) / 15) as u8;
                    if eff_a > 0 {
                        let idx = row_dst_base + (x - i64::from(band_rect.x)) as usize;
                        target.blend_pixel_a4(idx, rgb, eff_a);
                    }
                }
            }
        }
        Ok(())
    }
}
