use liuli_core::bitmap::{
    L565_FLAG_ALPHA4, L565_FLAG_CHROMA_KEY, L565_FLAG_INDEXED4, L565_FLAG_INDEXED8,
    L565_FLAG_MASK_A4, L565_FLAG_RLE,
};
use liuli_core::geometry::{Rect, Rgb565, rgb565_to_u16};
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

#[inline]
fn decode_nibble_in_chunk(buf: &[u8], chunk_start_px: usize, px_offset: usize) -> u8 {
    let cur_idx = chunk_start_px + px_offset;
    let b = buf[cur_idx / 2 - chunk_start_px / 2];
    if (cur_idx & 1) == 0 {
        b & 0x0F
    } else {
        (b >> 4) & 0x0F
    }
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
                let needed = usize::from(width)
                    .checked_mul(usize::from(height))
                    .and_then(|p| p.checked_mul(2))
                    .and_then(|b| b.checked_add(10))
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
                let fmt_bits = flags & (L565_FLAG_MASK_A4 | L565_FLAG_INDEXED4 | L565_FLAG_INDEXED8);
                if fmt_bits.count_ones() > 1 {
                    return Err(StoredBitmapError::InvalidFormat);
                }
                let is_mask = (flags & L565_FLAG_MASK_A4) != 0;
                let is_i4 = (flags & L565_FLAG_INDEXED4) != 0;
                let is_i8 = (flags & L565_FLAG_INDEXED8) != 0;
                let is_rle = (flags & L565_FLAG_RLE) != 0;
                let has_a4 = (flags & L565_FLAG_ALPHA4) != 0;
                if is_mask && (is_rle || has_a4) {
                    return Err(StoredBitmapError::InvalidFormat);
                }
                if (is_i4 || is_i8) && is_rle {
                    return Err(StoredBitmapError::InvalidFormat);
                }

                let chroma_key = if !is_mask && (flags & L565_FLAG_CHROMA_KEY) != 0 {
                    Some(u16::from_le_bytes([hdr[10], hdr[11]]))
                } else {
                    None
                };
                let data_offset = 12usize;
                let px_count = usize::from(width)
                    .checked_mul(usize::from(height))
                    .ok_or(StoredBitmapError::InvalidFormat)?;

                let payload_offset = if is_rle {
                    let tbl_bytes = usize::from(height) * 4;
                    let po = data_offset
                        .checked_add(tbl_bytes)
                        .ok_or(StoredBitmapError::InvalidFormat)?;
                    if total_len < po {
                        return Err(StoredBitmapError::InvalidFormat);
                    }
                    po
                } else if is_mask {
                    let m_bytes = px_count.div_ceil(2);
                    let needed = data_offset
                        .checked_add(m_bytes)
                        .ok_or(StoredBitmapError::InvalidFormat)?;
                    if total_len < needed {
                        return Err(StoredBitmapError::InvalidFormat);
                    }
                    data_offset
                } else if is_i4 || is_i8 {
                    let pal_bytes = if is_i4 { 16 * 2 } else { 256 * 2 };
                    let idx_bytes = if is_i4 { px_count.div_ceil(2) } else { px_count };
                    let a_bytes = if has_a4 { px_count.div_ceil(2) } else { 0 };
                    let needed = data_offset
                        .checked_add(pal_bytes)
                        .and_then(|v| v.checked_add(idx_bytes))
                        .and_then(|v| v.checked_add(a_bytes))
                        .ok_or(StoredBitmapError::InvalidFormat)?;
                    if total_len < needed {
                        return Err(StoredBitmapError::InvalidFormat);
                    }
                    data_offset + pal_bytes
                } else {
                    let px_bytes = px_count
                        .checked_mul(2)
                        .ok_or(StoredBitmapError::InvalidFormat)?;
                    let a_bytes = if has_a4 { px_count.div_ceil(2) } else { 0 };
                    let needed = data_offset
                        .checked_add(px_bytes)
                        .and_then(|v| v.checked_add(a_bytes))
                        .ok_or(StoredBitmapError::InvalidFormat)?;
                    if total_len < needed {
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
        self.chroma_key.is_some()
            || (self.flags & (L565_FLAG_ALPHA4 | L565_FLAG_MASK_A4)) != 0
    }

    #[inline]
    pub const fn is_rle(&self) -> bool {
        (self.flags & L565_FLAG_RLE) != 0
    }

    #[inline]
    pub const fn is_alpha_mask(&self) -> bool {
        (self.flags & L565_FLAG_MASK_A4) != 0
    }

    #[inline]
    pub const fn chroma_key(&self) -> Option<u16> {
        self.chroma_key
    }

    fn load_palette<R: ReadAt>(
        &self,
        reader: &mut R,
        palette: &mut [u16; 256],
    ) -> Result<(), StoredBitmapError<R::Error>> {
        let count = if (self.flags & L565_FLAG_INDEXED4) != 0 {
            16usize
        } else if (self.flags & L565_FLAG_INDEXED8) != 0 {
            256usize
        } else {
            return Ok(());
        };
        let mut buf = [0u8; 64];
        let mut done = 0usize;
        while done < count {
            let chunk = (count - done).min(32);
            read_exact_at(
                reader,
                self.data_offset + done * 2,
                &mut buf[..chunk * 2],
            )?;
            for i in 0..chunk {
                palette[done + i] = u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]);
            }
            done += chunk;
        }
        Ok(())
    }

    fn read_nibble_at<R: ReadAt>(
        &self,
        reader: &mut R,
        base: usize,
        idx: usize,
    ) -> Result<u8, StoredBitmapError<R::Error>> {
        let mut b = [0u8; 1];
        read_exact_at(reader, base + (idx / 2), &mut b)?;
        Ok(if (idx & 1) == 0 {
            b[0] & 0x0F
        } else {
            (b[0] >> 4) & 0x0F
        })
    }

    fn read_raw_pixel<R: ReadAt>(
        &self,
        reader: &mut R,
        palette: &[u16; 256],
        sx: usize,
        sy: usize,
    ) -> Result<(u16, u8), StoredBitmapError<R::Error>> {
        let idx = sy * usize::from(self.width) + sx;
        let px_count = usize::from(self.width) * usize::from(self.height);
        if self.is_alpha_mask() {
            let a = self.read_nibble_at(reader, self.payload_offset, idx)?;
            return Ok((0xFFFF, a));
        }

        let has_a4 = (self.flags & L565_FLAG_ALPHA4) != 0;
        let (rgb, alpha_base) = if (self.flags & L565_FLAG_INDEXED4) != 0 {
            let p_idx = self.read_nibble_at(reader, self.payload_offset, idx)?;
            (
                palette[usize::from(p_idx)],
                self.payload_offset + px_count.div_ceil(2),
            )
        } else if (self.flags & L565_FLAG_INDEXED8) != 0 {
            let mut b = [0u8; 1];
            read_exact_at(reader, self.payload_offset + idx, &mut b)?;
            (palette[usize::from(b[0])], self.payload_offset + px_count)
        } else {
            let mut rgb_buf = [0u8; 2];
            read_exact_at(reader, self.data_offset + idx * 2, &mut rgb_buf)?;
            (
                u16::from_le_bytes(rgb_buf),
                self.data_offset + px_count * 2,
            )
        };

        let mut alpha = if has_a4 {
            self.read_nibble_at(reader, alpha_base, idx)?
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
        self.blit_inner(reader, target, dst, clip, None, alpha)
    }

    /// Blits the bitmap scaled to `dst` clipped by `clip`, overriding texel color with `tint`
    /// while preserving alpha / mask coverage (ideal for streaming [`L565_FLAG_MASK_A4`] assets).
    pub fn blit_tinted_to_band<R: ReadAt>(
        &self,
        reader: &mut R,
        target: &mut BandTarget<'_>,
        dst: Rect,
        clip: Rect,
        tint: Rgb565,
        alpha: u8,
    ) -> Result<(), StoredBitmapError<R::Error>> {
        self.blit_inner(reader, target, dst, clip, Some(rgb565_to_u16(tint)), alpha)
    }

    fn blit_inner<R: ReadAt>(
        &self,
        reader: &mut R,
        target: &mut BandTarget<'_>,
        dst: Rect,
        clip: Rect,
        tint_rgb: Option<u16>,
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
                        target.blend_pixel_a4(idx, tint_rgb.unwrap_or(rgb), eff_a);
                    }
                }
            }
            return Ok(());
        }

        let mut palette = [0u16; 256];
        self.load_palette(reader, &mut palette)?;

        const ROW_CHUNK_PX: usize = 64;
        let mut raw_buf = [0u8; ROW_CHUNK_PX * 2];
        let mut a4_buf = [0u8; ROW_CHUNK_PX / 2 + 1];
        let is_mask = self.is_alpha_mask();
        let is_i4 = (self.flags & L565_FLAG_INDEXED4) != 0;
        let is_i8 = (self.flags & L565_FLAG_INDEXED8) != 0;
        let has_a4 = (self.flags & L565_FLAG_ALPHA4) != 0;
        let px_count = usize::from(self.width) * usize::from(self.height);
        let alpha_base = if is_i4 {
            self.payload_offset + px_count.div_ceil(2)
        } else if is_i8 {
            self.payload_offset + px_count
        } else {
            self.data_offset + px_count * 2
        };

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
                    let nib_start = px_idx / 2;
                    let nib_bytes = (px_idx + count - 1) / 2 - nib_start + 1;
                    if is_mask {
                        read_exact_at(reader, self.payload_offset + nib_start, &mut a4_buf[..nib_bytes])?;
                    } else if is_i4 {
                        read_exact_at(reader, self.payload_offset + nib_start, &mut raw_buf[..nib_bytes])?;
                    } else if is_i8 {
                        read_exact_at(reader, self.payload_offset + px_idx, &mut raw_buf[..count])?;
                    } else {
                        read_exact_at(
                            reader,
                            self.data_offset + px_idx * 2,
                            &mut raw_buf[..count * 2],
                        )?;
                    }
                    if !is_mask && has_a4 {
                        read_exact_at(reader, alpha_base + nib_start, &mut a4_buf[..nib_bytes])?;
                    }
                    let band_x0 = (area.x - band_rect.x) as usize + done;
                    for i in 0..count {
                        let (rgb, mut pa) = if is_mask {
                            (0xFFFF, decode_nibble_in_chunk(&a4_buf, px_idx, i))
                        } else {
                            let rgb = if is_i4 {
                                palette[usize::from(decode_nibble_in_chunk(&raw_buf, px_idx, i))]
                            } else if is_i8 {
                                palette[usize::from(raw_buf[i])]
                            } else {
                                u16::from_le_bytes([raw_buf[i * 2], raw_buf[i * 2 + 1]])
                            };
                            let pa = if has_a4 {
                                decode_nibble_in_chunk(&a4_buf, px_idx, i)
                            } else {
                                15
                            };
                            (rgb, pa)
                        };
                        if !is_mask && self.chroma_key == Some(rgb) {
                            pa = 0;
                        }
                        let eff_a = ((u16::from(pa) * u16::from(g_alpha) + 7) / 15) as u8;
                        if eff_a > 0 {
                            target.blend_pixel_a4(
                                row_dst_base + band_x0 + i,
                                tint_rgb.unwrap_or(rgb),
                                eff_a,
                            );
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
                    let (rgb, pa) = self.read_raw_pixel(reader, &palette, sx, sy)?;
                    let eff_a = ((u16::from(pa) * u16::from(g_alpha) + 7) / 15) as u8;
                    if eff_a > 0 {
                        let idx = row_dst_base + (x - i64::from(band_rect.x)) as usize;
                        target.blend_pixel_a4(idx, tint_rgb.unwrap_or(rgb), eff_a);
                    }
                }
            }
        }
        Ok(())
    }
}
