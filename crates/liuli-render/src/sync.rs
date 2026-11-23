use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::capacity::{FrameMetrics, PresentError, PresentStatus};
use liuli_core::damage::DamageSet;
use liuli_core::geometry::{Rect, u16_to_rgb565};
use liuli_core::paint::{CommandList, SceneResources};
use liuli_text::FontSet;

use crate::band::{BandIter, BandTarget};
use crate::raster::render_band;

pub fn present_sync<T, R>(
    target: &mut T,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &FontSet<'_>,
) -> Result<(PresentStatus, FrameMetrics), PresentError<T::Error>>
where
    T: DrawTarget<Color = Rgb565>,
    R: SceneResources + ?Sized,
{
    present_sync_with(
        target,
        scratch,
        screen,
        configured_band_h,
        cmds,
        damage,
        resources,
        fonts,
        |_| Ok(()),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn present_sync_with<T, R, Fin>(
    target: &mut T,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &FontSet<'_>,
    mut finish: Fin,
) -> Result<(PresentStatus, FrameMetrics), PresentError<T::Error>>
where
    T: DrawTarget<Color = Rgb565>,
    R: SceneResources + ?Sized,
    Fin: FnMut(&mut T) -> Result<(), T::Error>,
{
    if damage.is_empty() {
        return Ok((PresentStatus::Idle, FrameMetrics::default()));
    }
    if scratch.len() < screen.w as usize {
        return Err(PresentError::BufferTooSmall);
    }

    let target_bb = Rect::from_eg(target.bounding_box());
    let mut metrics = FrameMetrics::default();

    let bands = BandIter::new(damage.as_slice(), screen, configured_band_h, scratch.len())?;

    for band_rect in bands {
        let mut band_target = BandTarget::new(scratch, band_rect)?;
        metrics.commands_replayed = metrics.commands_replayed.saturating_add(render_band(
            &mut band_target,
            cmds,
            resources,
            fonts,
        ));

        let phys_rect = band_rect.translate(target_bb.x, target_bb.y);
        let vis_phys = phys_rect.intersect(target_bb);
        if vis_phys.is_empty() {
            continue;
        }

        let skip_x = (vis_phys.x - phys_rect.x) as usize;
        let skip_y = (vis_phys.y - phys_rect.y) as usize;
        let stride = band_rect.w as usize;
        let vis_w = vis_phys.w as usize;
        let vis_h = vis_phys.h as usize;

        let color_iter = (0..vis_h).flat_map(|ry| {
            let row_start = (skip_y + ry) * stride + skip_x;
            band_target.words()[row_start..row_start + vis_w]
                .iter()
                .map(|&w| u16_to_rgb565(w))
        });

        if let Err(err) = target.fill_contiguous(&vis_phys.to_eg(), color_iter) {
            damage.mark_full_screen(screen);
            return Err(PresentError::Target(err));
        }

        metrics.bands += 1;
        metrics.pixels_sent = metrics
            .pixels_sent
            .saturating_add((vis_w as u32).saturating_mul(vis_h as u32));
    }

    if let Err(err) = finish(target) {
        damage.mark_full_screen(screen);
        return Err(PresentError::Target(err));
    }

    damage.clear();
    Ok((PresentStatus::Presented, metrics))
}
