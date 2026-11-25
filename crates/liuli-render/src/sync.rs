use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::capacity::{FrameMetrics, PresentError, PresentStatus};
use liuli_core::damage::DamageSet;
use liuli_core::geometry::{Rect, u16_to_rgb565};
use liuli_core::paint::{CommandList, SceneResources};
use liuli_text::{FontProvider, FontSet};

use crate::band::{BandIter, BandTarget};
use crate::display::DisplaySink;
use crate::raster::{blend_solid, render_band_with_pixels, render_band_with_provider};
use liuli_text::storage::{GlyphPixels, InlinePixels};
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError<D, F> {
    Display(PresentError<D>),
    Font(F),
    ShortGlyph { needed: usize, actual: usize },
}
impl<D, F> From<PresentError<D>> for FrameError<D, F> {
    fn from(e: PresentError<D>) -> Self {
        Self::Display(e)
    }
}
impl<D> FrameError<D, core::convert::Infallible> {
    pub fn into_display(self) -> PresentError<D> {
        match self {
            Self::Display(e) => e,
            Self::ShortGlyph { needed, actual } => PresentError::ShortGlyph { needed, actual },
            Self::Font(never) => match never {},
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
    finish: Fin,
) -> Result<(PresentStatus, FrameMetrics), PresentError<T::Error>>
where
    T: DrawTarget<Color = Rgb565>,
    R: SceneResources + ?Sized,
    Fin: FnMut(&mut T) -> Result<(), T::Error>,
{
    present_sync_with_blend(
        target,
        scratch,
        screen,
        configured_band_h,
        cmds,
        damage,
        resources,
        fonts,
        finish,
        blend_solid,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn present_sync_with_blend<T, R, Fin>(
    target: &mut T,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &FontSet<'_>,
    finish: Fin,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
) -> Result<(PresentStatus, FrameMetrics), PresentError<T::Error>>
where
    T: DrawTarget<Color = Rgb565>,
    R: SceneResources + ?Sized,
    Fin: FnMut(&mut T) -> Result<(), T::Error>,
{
    present_sync_with_pixels(
        target,
        scratch,
        screen,
        configured_band_h,
        cmds,
        damage,
        resources,
        fonts,
        finish,
        blend,
        &mut InlinePixels,
    )
    .map_err(FrameError::into_display)
}

#[allow(clippy::too_many_arguments)]
pub fn present_sync_with_pixels<T, R, Fin, G: GlyphPixels>(
    target: &mut T,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &FontSet<'_>,
    mut finish: Fin,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
    glyphs: &mut G,
) -> Result<(PresentStatus, FrameMetrics), FrameError<T::Error, G::Error>>
where
    T: DrawTarget<Color = Rgb565>,
    R: SceneResources + ?Sized,
    Fin: FnMut(&mut T) -> Result<(), T::Error>,
{
    if damage.is_empty() {
        return Ok((PresentStatus::Idle, FrameMetrics::default()));
    }
    if scratch.len() < screen.w as usize {
        return Err(PresentError::BufferTooSmall.into());
    }

    let target_bb = Rect::from_eg(target.bounding_box());
    let mut metrics = FrameMetrics::default();

    let bands = BandIter::new(damage.as_slice(), screen, configured_band_h, scratch.len())?;

    for band_rect in bands {
        let mut band_target = BandTarget::new(scratch, band_rect)?;
        metrics.commands_replayed = metrics.commands_replayed.saturating_add(
            render_band_with_pixels(&mut band_target, cmds, resources, fonts, blend, glyphs)
                .map_err(|err| match err {
                    crate::raster::GlyphReadError::Source(e) => FrameError::Font(e),
                    crate::raster::GlyphReadError::Short { needed, actual } => {
                        FrameError::ShortGlyph { needed, actual }
                    }
                })?,
        );

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
            return Err(PresentError::Target(err).into());
        }

        metrics.bands += 1;
        metrics.pixels_sent = metrics
            .pixels_sent
            .saturating_add((vis_w as u32).saturating_mul(vis_h as u32));
    }

    if let Err(err) = finish(target) {
        damage.mark_full_screen(screen);
        return Err(PresentError::Target(err).into());
    }

    damage.clear();
    Ok((PresentStatus::Presented, metrics))
}

#[allow(clippy::too_many_arguments)]
pub fn present_sync_with_provider<T, R, Fin, F: FontProvider>(
    target: &mut T,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &F,
    mut finish: Fin,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
) -> Result<(PresentStatus, FrameMetrics), FrameError<T::Error, F::Error>>
where
    T: DrawTarget<Color = Rgb565>,
    R: SceneResources + ?Sized,
    Fin: FnMut(&mut T) -> Result<(), T::Error>,
{
    if damage.is_empty() {
        return Ok((PresentStatus::Idle, FrameMetrics::default()));
    }
    if scratch.len() < screen.w as usize {
        return Err(PresentError::BufferTooSmall.into());
    }

    fonts.begin_frame();
    let target_bb = Rect::from_eg(target.bounding_box());
    let mut metrics = FrameMetrics::default();

    let bands = BandIter::new(damage.as_slice(), screen, configured_band_h, scratch.len())?;

    for band_rect in bands {
        let mut band_target = BandTarget::new(scratch, band_rect)?;
        metrics.commands_replayed = metrics.commands_replayed.saturating_add(
            render_band_with_provider(&mut band_target, cmds, resources, fonts, blend)
                .map_err(FrameError::Font)?,
        );

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
            return Err(PresentError::Target(err).into());
        }

        metrics.bands += 1;
        metrics.pixels_sent = metrics
            .pixels_sent
            .saturating_add((vis_w as u32).saturating_mul(vis_h as u32));
    }

    if let Err(err) = finish(target) {
        damage.mark_full_screen(screen);
        return Err(PresentError::Target(err).into());
    }

    damage.clear();
    Ok((PresentStatus::Presented, metrics))
}

/// Presents damaged bands into any bus-agnostic [`DisplaySink`] using `fonts`.
#[allow(clippy::too_many_arguments)]
pub fn present_sink<S, R, F: FontProvider>(
    sink: &mut S,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &F,
) -> Result<(PresentStatus, FrameMetrics), FrameError<S::Error, F::Error>>
where
    S: DisplaySink + ?Sized,
    R: SceneResources + ?Sized,
{
    present_sink_with_blend(
        sink,
        scratch,
        screen,
        configured_band_h,
        cmds,
        damage,
        resources,
        fonts,
        blend_solid,
    )
}

/// Presents damaged bands into any [`DisplaySink`] with a custom overlay blend kernel.
#[allow(clippy::too_many_arguments)]
pub fn present_sink_with_blend<S, R, F: FontProvider>(
    sink: &mut S,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &F,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
) -> Result<(PresentStatus, FrameMetrics), FrameError<S::Error, F::Error>>
where
    S: DisplaySink + ?Sized,
    R: SceneResources + ?Sized,
{
    if damage.is_empty() {
        return Ok((PresentStatus::Idle, FrameMetrics::default()));
    }
    if scratch.len() < screen.w as usize {
        return Err(PresentError::BufferTooSmall.into());
    }

    fonts.begin_frame();
    let mut metrics = FrameMetrics::default();
    let bands = BandIter::new(damage.as_slice(), screen, configured_band_h, scratch.len())?;

    for band_rect in bands {
        let mut band_target = BandTarget::new(scratch, band_rect)?;
        metrics.commands_replayed = metrics.commands_replayed.saturating_add(
            render_band_with_provider(&mut band_target, cmds, resources, fonts, blend)
                .map_err(FrameError::Font)?,
        );

        if let Err(err) = sink.flush_rect(band_rect, band_target.words()) {
            damage.mark_full_screen(screen);
            return Err(PresentError::Target(err).into());
        }

        metrics.bands += 1;
        metrics.pixels_sent = metrics
            .pixels_sent
            .saturating_add(band_rect.w.saturating_mul(band_rect.h));
    }

    if let Err(err) = sink.finish_frame() {
        damage.mark_full_screen(screen);
        return Err(PresentError::Target(err).into());
    }

    damage.clear();
    Ok((PresentStatus::Presented, metrics))
}

/// Presents damaged bands of a [`FontSet`] into a [`DisplaySink`] using a custom [`GlyphPixels`] source.
#[allow(clippy::too_many_arguments)]
pub fn present_sink_with_pixels<S, R, G: GlyphPixels>(
    sink: &mut S,
    scratch: &mut [u16],
    screen: Rect,
    configured_band_h: u32,
    cmds: &CommandList,
    damage: &mut DamageSet,
    resources: &R,
    fonts: &FontSet<'_>,
    blend: impl Fn(&mut [u16], Rgb565, u8) + Copy,
    glyphs: &mut G,
) -> Result<(PresentStatus, FrameMetrics), FrameError<S::Error, G::Error>>
where
    S: DisplaySink + ?Sized,
    R: SceneResources + ?Sized,
{
    if damage.is_empty() {
        return Ok((PresentStatus::Idle, FrameMetrics::default()));
    }
    if scratch.len() < screen.w as usize {
        return Err(PresentError::BufferTooSmall.into());
    }

    let mut metrics = FrameMetrics::default();
    let bands = BandIter::new(damage.as_slice(), screen, configured_band_h, scratch.len())?;

    for band_rect in bands {
        let mut band_target = BandTarget::new(scratch, band_rect)?;
        metrics.commands_replayed = metrics.commands_replayed.saturating_add(
            render_band_with_pixels(&mut band_target, cmds, resources, fonts, blend, glyphs)
                .map_err(|err| match err {
                    crate::raster::GlyphReadError::Source(e) => FrameError::Font(e),
                    crate::raster::GlyphReadError::Short { needed, actual } => {
                        FrameError::ShortGlyph { needed, actual }
                    }
                })?,
        );

        if let Err(err) = sink.flush_rect(band_rect, band_target.words()) {
            damage.mark_full_screen(screen);
            return Err(PresentError::Target(err).into());
        }

        metrics.bands += 1;
        metrics.pixels_sent = metrics
            .pixels_sent
            .saturating_add(band_rect.w.saturating_mul(band_rect.h));
    }

    if let Err(err) = sink.finish_frame() {
        damage.mark_full_screen(screen);
        return Err(PresentError::Target(err).into());
    }

    damage.clear();
    Ok((PresentStatus::Presented, metrics))
}
