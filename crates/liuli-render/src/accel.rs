use liuli_core::geometry::{Rect, Rgb565};

use crate::band::BandTarget;

/// Optional hardware 2D blit/fill/blend acceleration hook with pure-software fallbacks.
///
/// SoC BSPs (for example STM32 DMA2D or Chrom-ART) can override one or more methods while
/// automatically falling back to [`crate::raster`] for unaccelerated operations.
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
}

/// Pure-software [`DrawAccel`] implementation used when no DMA2D/GPU engine is attached.
#[derive(Copy, Clone, Debug, Default)]
pub struct SoftAccel;

impl DrawAccel for SoftAccel {}
