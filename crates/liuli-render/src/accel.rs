use liuli_core::color::{PixelFormat, rgb565_swap_rb, rgb888_to_rgb565_round};
use liuli_core::dither::{DitherMode, rgb888_to_rgb565_dithered};
use liuli_core::geometry::{Rect, Rgb565, rgb565_to_u16};
use liuli_text::blend_rgb565_a4_u16;

use crate::band::BandTarget;
use crate::wire::{BufferTooSmall, encode_rgb565_row};

/// Optional hardware 2D blit/fill/blend/PFC acceleration hook with pure-software fallbacks.
///
/// SoC BSPs (for example STM32 DMA2D / Chrom-ART) can override one or more methods while
/// automatically falling back to [`crate::raster`] and [`crate::wire`] for unaccelerated operations.
pub trait DrawAccel {
    #[inline]
    fn fill_rect(&mut self, target: &mut BandTarget<'_>, rect: Rect, color: Rgb565) {
        let band_rect = target.band_rect();
        let local = rect
            .intersect(band_rect)
            .translate(-band_rect.x, -band_rect.y);
        if !local.is_empty() {
            let _ = embedded_graphics::draw_target::DrawTarget::fill_solid(
                target,
                &local.to_eg(),
                color,
            );
        }
    }

    #[inline]
    fn blend_solid_row(&mut self, row: &mut [u16], color: Rgb565, alpha: u8) {
        crate::raster::blend_solid(row, color, alpha);
    }

    #[inline]
    fn blit_rgb565_row(&mut self, dst: &mut [u16], src: &[u16]) {
        let count = dst.len().min(src.len());
        dst[..count].copy_from_slice(&src[..count]);
    }

    /// Converts `count` pixels in one scanline (`y`) from `src_fmt` to `dst_fmt`.
    ///
    /// Returns total written bytes in `dst`, or [`BufferTooSmall`] when `src` or `dst` is short.
    #[allow(clippy::too_many_arguments)]
    fn convert_row(
        &mut self,
        src: &[u8],
        src_fmt: PixelFormat,
        dst: &mut [u8],
        dst_fmt: PixelFormat,
        count: usize,
        dither: DitherMode,
        y: u16,
    ) -> Result<usize, BufferTooSmall> {
        let count_u32 = u32::try_from(count).map_err(|_| BufferTooSmall)?;
        let src_needed = src_fmt.row_bytes(count_u32);
        let dst_needed = dst_fmt.row_bytes(count_u32);
        if src.len() < src_needed || dst.len() < dst_needed {
            return Err(BufferTooSmall);
        }
        if count == 0 {
            return Ok(0);
        }
        if src_fmt == dst_fmt {
            dst[..dst_needed].copy_from_slice(&src[..src_needed]);
            return Ok(dst_needed);
        }

        if src_fmt == PixelFormat::Rgb565Le {
            const CHUNK: usize = 64;
            let mut words = [0u16; CHUNK];
            let mut done = 0usize;
            while done < count {
                let n = (count - done).min(CHUNK);
                for (i, w) in words[..n].iter_mut().enumerate() {
                    let off = (done + i) * 2;
                    *w = u16::from_le_bytes([src[off], src[off + 1]]);
                }
                let dst_off = dst_fmt.row_bytes(done as u32);
                encode_rgb565_row(
                    &words[..n],
                    done as i32,
                    i32::from(y),
                    dst_fmt,
                    dither,
                    &mut dst[dst_off..],
                )?;
                done += n;
            }
            return Ok(dst_needed);
        }

        if dst_fmt == PixelFormat::Rgb565Le {
            match src_fmt {
                PixelFormat::Rgb888 => {
                    for (i, px) in src[..src_needed].as_chunks::<3>().0.iter().enumerate() {
                        let w =
                            quantize_rgb888(px[0], px[1], px[2], i as i32, i32::from(y), dither);
                        dst[i * 2..i * 2 + 2].copy_from_slice(&w.to_le_bytes());
                    }
                    return Ok(dst_needed);
                }
                PixelFormat::Argb8888 | PixelFormat::Xrgb8888 => {
                    for (i, px) in src[..src_needed].as_chunks::<4>().0.iter().enumerate() {
                        // ARGB8888 / XRGB8888 LE wire order: [B, G, R, A/X]
                        let w =
                            quantize_rgb888(px[2], px[1], px[0], i as i32, i32::from(y), dither);
                        dst[i * 2..i * 2 + 2].copy_from_slice(&w.to_le_bytes());
                    }
                    return Ok(dst_needed);
                }
                _ => {}
            }
        }

        // General chunked fallback through native RGB565 words
        const CHUNK: usize = 64;
        let mut words = [0u16; CHUNK];
        let mut done = 0usize;
        while done < count {
            let n = (count - done).min(CHUNK);
            for (i, w) in words[..n].iter_mut().enumerate() {
                let gx = (done + i) as i32;
                *w = read_pixel_as_rgb565(src, src_fmt, done + i, gx, i32::from(y), dither)
                    .ok_or(BufferTooSmall)?;
            }
            let dst_off = dst_fmt.row_bytes(done as u32);
            encode_rgb565_row(
                &words[..n],
                done as i32,
                i32::from(y),
                dst_fmt,
                dither,
                &mut dst[dst_off..],
            )?;
            done += n;
        }
        Ok(dst_needed)
    }

    /// Blends one scanline of packed 4-bit alpha mask (`mask`, 2 px/byte low-even/high-odd)
    /// starting at pixel index `mask_offset_px` over `dst` with foreground `color`.
    #[inline]
    fn blend_a4_mask_row(
        &mut self,
        dst: &mut [u16],
        mask: &[u8],
        mask_offset_px: usize,
        color: Rgb565,
    ) {
        let fg = rgb565_to_u16(color);
        for (i, pixel) in dst.iter_mut().enumerate() {
            let idx = mask_offset_px + i;
            let Some(&packed) = mask.get(idx / 2) else {
                break;
            };
            let cov = if (idx & 1) == 0 {
                packed & 0x0F
            } else {
                (packed >> 4) & 0x0F
            };
            if cov > 0 {
                *pixel = blend_rgb565_a4_u16(fg, *pixel, cov);
            }
        }
    }
}

#[inline]
fn quantize_rgb888(r: u8, g: u8, b: u8, x: i32, y: i32, dither: DitherMode) -> u16 {
    match dither {
        DitherMode::Bayer4x4 => rgb888_to_rgb565_dithered(r, g, b, x, y),
        DitherMode::None => rgb565_to_u16(rgb888_to_rgb565_round(r, g, b)),
    }
}

fn read_pixel_as_rgb565(
    src: &[u8],
    fmt: PixelFormat,
    index: usize,
    x: i32,
    y: i32,
    dither: DitherMode,
) -> Option<u16> {
    Some(match fmt {
        PixelFormat::Rgb565Le => {
            let off = index * 2;
            u16::from_le_bytes([*src.get(off)?, *src.get(off + 1)?])
        }
        PixelFormat::Rgb565Be => {
            let off = index * 2;
            u16::from_be_bytes([*src.get(off)?, *src.get(off + 1)?])
        }
        PixelFormat::Bgr565Le => {
            let off = index * 2;
            rgb565_swap_rb(u16::from_le_bytes([*src.get(off)?, *src.get(off + 1)?]))
        }
        PixelFormat::Bgr565Be => {
            let off = index * 2;
            rgb565_swap_rb(u16::from_be_bytes([*src.get(off)?, *src.get(off + 1)?]))
        }
        PixelFormat::Rgb666 | PixelFormat::Rgb888 => {
            let off = index * 3;
            quantize_rgb888(
                *src.get(off)?,
                *src.get(off + 1)?,
                *src.get(off + 2)?,
                x,
                y,
                dither,
            )
        }
        PixelFormat::Bgr888 => {
            let off = index * 3;
            quantize_rgb888(
                *src.get(off + 2)?,
                *src.get(off + 1)?,
                *src.get(off)?,
                x,
                y,
                dither,
            )
        }
        PixelFormat::Xrgb8888 | PixelFormat::Argb8888 => {
            let off = index * 4;
            quantize_rgb888(
                *src.get(off + 2)?,
                *src.get(off + 1)?,
                *src.get(off)?,
                x,
                y,
                dither,
            )
        }
        PixelFormat::Abgr8888 => {
            let off = index * 4;
            quantize_rgb888(
                *src.get(off)?,
                *src.get(off + 1)?,
                *src.get(off + 2)?,
                x,
                y,
                dither,
            )
        }
        PixelFormat::Gray8 => {
            let g = *src.get(index)?;
            quantize_rgb888(g, g, g, x, y, dither)
        }
        PixelFormat::Gray4 => {
            let b = *src.get(index / 2)?;
            let nib = if (index & 1) == 0 {
                (b >> 4) & 0x0F
            } else {
                b & 0x0F
            };
            let g = (nib << 4) | nib;
            quantize_rgb888(g, g, g, x, y, dither)
        }
        PixelFormat::Mono1Msb | PixelFormat::Mono1Lsb => {
            let b = *src.get(index / 8)?;
            let bit = if matches!(fmt, PixelFormat::Mono1Msb) {
                7 - (index & 7)
            } else {
                index & 7
            };
            if ((b >> bit) & 1) != 0 {
                0xFFFF
            } else {
                0x0000
            }
        }
    })
}

/// Pure-software [`DrawAccel`] implementation used when no DMA2D/GPU engine is attached.
#[derive(Copy, Clone, Debug, Default)]
pub struct SoftAccel;

impl DrawAccel for SoftAccel {}
