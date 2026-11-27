//! Bounded pixel storage reserved while the command list is still a candidate.
use crate::{
    capacity::BuildError,
    effects::Layer,
    geometry::{Rect, Rgb565},
    paint::{CommandList, PaintCmd, SceneResources},
    view::Key,
};
use alloc::vec::Vec;
use core::cell::RefCell;

pub const DEFAULT_LAYER_BYTES: usize = 512 * 1024;

#[derive(Default)]
pub struct GroupPixels {
    pub color: Vec<u16>,
    /// Packed 4-bit alpha (`0..=15`), 2 pixels per byte (low nibble even, high nibble odd).
    pub alpha: Vec<u8>,
    pub ready: bool,
    pub reused: bool,
}

pub use crate::composition::{get_a4, set_a4};

pub struct Backdrop {
    pub pixels: Vec<u16>,
    pub clear: Option<Rgb565>,
    pub tint: Option<(Rgb565, u8)>,
}

pub struct LayerCache {
    pub layout: Rect,
    pub composition: Option<crate::composition::SurfaceComposition>,
    pub shadow: Option<crate::effects::Shadow>,
    pub key: Key,
    pub style: Layer,
    pub radius: u8,
    pub source: Rect,
    pub output: Rect,
    pub capture: Rect,
    pub start: usize,
    pub backdrop_end: usize,
    bytes: usize,
    pub group: RefCell<GroupPixels>,
    pub backdrop: RefCell<Backdrop>,
}

fn words(len: usize) -> Result<Vec<u16>, BuildError> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| BuildError::LayerPixels)?;
    v.resize(len, 0);
    Ok(v)
}

fn bytes_buf(len: usize) -> Result<Vec<u8>, BuildError> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| BuildError::LayerPixels)?;
    v.resize(len, 0);
    Ok(v)
}

impl CommandList {
    pub(crate) fn begin_layer(
        &mut self,
        key: Key,
        rect: Rect,
        clip: Rect,
        source: Rect,
        style: Layer,
        radius: u8,
        backdrop_end: usize,
    ) -> Result<usize, BuildError> {
        let output = rect.intersect(clip);
        let capture = if style.backdrop_radius > 0 {
            output
                .expand(style.backdrop_radius.into())
                .intersect(self.screen)
        } else {
            Rect::ZERO
        };
        let bytes = source
            .area64()
            .saturating_mul(2)
            .saturating_add(source.area64().div_ceil(2))
            .saturating_add(capture.area64().saturating_mul(2));
        let layers = self.layers.get_or_insert_with(Vec::new);
        let slot = usize::from(self.used_layers);
        let used: usize = layers[..slot].iter().map(|l| l.bytes).sum();
        if bytes
            > self
                .layer_budget
                .unwrap_or(DEFAULT_LAYER_BYTES)
                .saturating_sub(used) as u64
        {
            return Err(BuildError::LayerPixels);
        }
        let start = self.count;
        let index = self.used_layers;
        if usize::from(index) >= crate::effects::MAX_EFFECTS {
            return Err(BuildError::Effects);
        }
        let count = source.area64() as usize;
        if let Some(existing) = layers.get_mut(slot)
            && existing.source.size() == source.size()
            && existing.capture.size() == capture.size()
        {
            existing.layout = rect;
            existing.output = output;
            existing.shadow = None;
            existing.composition = None;
            existing.key = key;
            existing.style = style;
            existing.radius = radius;
            existing.source = source;
            existing.capture = capture;
            existing.start = start;
            existing.backdrop_end = backdrop_end;
            existing.group.get_mut().ready = false;
            existing.group.get_mut().reused = false;
            existing.backdrop.get_mut().clear = None;
            existing.backdrop.get_mut().tint = None;
        } else {
            layers.truncate(slot);
            layers.try_reserve(1).map_err(|_| BuildError::LayerPixels)?;
            layers.push(LayerCache {
                layout: rect,
                output,
                shadow: None,
                composition: None,
                key,
                style,
                radius,
                source,
                capture,
                start,
                backdrop_end,
                bytes: bytes as usize,
                group: RefCell::new(GroupPixels {
                    color: words(count)?,
                    alpha: bytes_buf(count.div_ceil(2))?,
                    ready: false,
                    reused: false,
                }),
                backdrop: RefCell::new(Backdrop {
                    pixels: words(capture.area64() as usize)?,
                    clear: None,
                    tint: None,
                }),
            });
        }
        self.used_layers += 1;
        self.push(PaintCmd::Group {
            rect,
            clip,
            index,
            end: 0,
        })?;
        Ok(start)
    }
    pub fn layer_bytes(&self) -> usize {
        self.layers.iter().flatten().map(|l| l.bytes).sum()
    }
}

/// Return whether the backdrop preceding `src_backdrop_end` and `dst_backdrop_end` over `capture` is identical.
fn backdrop_matches(
    prev_cmds: &[PaintCmd],
    next_cmds: &[PaintCmd],
    capture: Rect,
    old: &impl SceneResources,
    new: &impl SceneResources,
) -> bool {
    let next_visible = |cmds: &[PaintCmd], idx: &mut usize| -> Option<PaintCmd> {
        while let Some(&cmd) = cmds.get(*idx) {
            let next_idx = match cmd {
                PaintCmd::Group { end, .. } => usize::from(end).max(*idx + 1),
                _ => *idx + 1,
            };
            let matches_rect = !matches!(cmd, PaintCmd::Empty) && cmd.bounds().intersects(capture);
            *idx = next_idx;
            if matches_rect {
                return Some(cmd);
            }
        }
        None
    };

    let (mut i, mut j) = (0usize, 0usize);
    loop {
        let a = next_visible(prev_cmds, &mut i);
        let b = next_visible(next_cmds, &mut j);
        match (a, b) {
            (None, None) => return true,
            (Some(l), Some(r)) => {
                if l != r {
                    return false;
                }
                match l {
                    PaintCmd::Text { text, .. } => {
                        if old.text(text) != new.text(text) {
                            return false;
                        }
                    }
                    PaintCmd::Sparkline { series, .. } => {
                        if old.sparkline(series) != new.sparkline(series) {
                            return false;
                        }
                    }
                    PaintCmd::Group { .. }
                    | PaintCmd::Surface { .. }
                    | PaintCmd::PathRun { .. }
                    | PaintCmd::MaskGroup { .. } => return false,
                    _ => {}
                }
            }
            _ => return false,
        }
    }
}

/// An unchanged subtree can reuse its rasterization when only its plane or opacity changes,
/// and reuse its blurred backdrop when preceding commands intersecting `capture` (excluding
/// own shadow) and tint settings are unchanged.
/// Resource IDs are scoped to a frame; equal indices alone cannot establish equality.
pub fn reuse_groups(
    lists: &mut [CommandList; 2],
    active: usize,
    old: &impl SceneResources,
    new: &impl SceneResources,
) {
    let (a, b) = lists.split_at_mut(1);
    let (prev, next) = if active == 0 {
        (&mut a[0], &mut b[0])
    } else {
        (&mut b[0], &mut a[0])
    };
    let (Some(before), Some(after)) = (&prev.layers, &next.layers) else {
        return;
    };
    for dst in after {
        let Some(src) = before
            .iter()
            .find(|s| s.key == dst.key && s.source == dst.source)
        else {
            continue;
        };
        let expected_tint =
            (dst.style.tint_alpha > 0).then_some((dst.style.tint_color, dst.style.tint_alpha));
        if dst.style.backdrop_radius > 0
            && src.style.backdrop_radius == dst.style.backdrop_radius
            && src.capture == dst.capture
            && prev.root_bg == next.root_bg
            && src.backdrop.borrow().clear == Some(prev.root_bg)
            && src.backdrop.borrow().tint == expected_tint
            && backdrop_matches(
                &prev.cmds[..src.backdrop_end],
                &next.cmds[..dst.backdrop_end],
                dst.capture,
                old,
                new,
            )
        {
            core::mem::swap(
                &mut *src.backdrop.borrow_mut(),
                &mut *dst.backdrop.borrow_mut(),
            );
        }
        if !src.group.borrow().ready {
            continue;
        }
        let (PaintCmd::Group { end: e1, .. }, PaintCmd::Group { end: e2, .. }) =
            (prev.cmds[src.start], next.cmds[dst.start])
        else {
            continue;
        };
        let left = &prev.cmds[src.start + 1..usize::from(e1)];
        let right = &next.cmds[dst.start + 1..usize::from(e2)];
        if left == right
            && left.iter().all(|c| match *c {
                PaintCmd::Text { text, .. } => old.text(text) == new.text(text),
                PaintCmd::Sparkline { series, .. } => {
                    old.sparkline(series) == new.sparkline(series)
                }
                PaintCmd::MaskGroup { slot, .. } => old.geometry(slot) == new.geometry(slot),
                PaintCmd::Group { .. } | PaintCmd::Surface { .. } => false,
                _ => true,
            })
        {
            core::mem::swap(&mut *src.group.borrow_mut(), &mut *dst.group.borrow_mut());
            dst.group.borrow_mut().reused = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{Layer, Projection, Shadow};
    use crate::paint::{SliceSceneResources, TextId};
    use crate::view::{FontId, TextAlign};

    #[test]
    fn packed_a4_roundtrips_even_and_odd_nibbles() {
        let mut buf = [0u8; 4];
        for i in 0..8 {
            let v = ((i * 3 + 1) & 15) as u8;
            set_a4(&mut buf, i, v);
            assert_eq!(get_a4(&buf, i), v);
        }
        for i in 0..8 {
            let v = ((i * 3 + 1) & 15) as u8;
            assert_eq!(get_a4(&buf, i), v);
        }
    }

    #[test]
    fn reuse_backdrop_survives_own_shadow_and_text_change_and_checks_tint() {
        let mut lists = [CommandList::new(), CommandList::new()];
        let rect = Rect::new(20, 20, 80, 60);
        let bg = Rgb565::new(2, 4, 2);
        let style = Layer {
            backdrop_radius: 4,
            opacity: 15,
            tint_color: bg,
            tint_alpha: 6,
            projection: Default::default(),
        };

        for (i, list) in lists.iter_mut().enumerate() {
            list.screen = Rect::new(0, 0, 200, 120);
            list.root_bg = bg;
            list.push(PaintCmd::FillRect {
                rect: Rect::new(0, 0, 120, 120),
                clip: list.screen,
                color: Rgb565::new(10, 20, 10),
            })
            .unwrap();
            // Non-overlapping command can differ across frames without invalidating backdrop.
            list.push(PaintCmd::FillRect {
                rect: Rect::new(150, 0, 20, 20),
                clip: list.screen,
                color: Rgb565::new(i as u8, 0, 0),
            })
            .unwrap();
            let backdrop_end = list.count;
            // Own rotating shadow before Group can change without invalidating backdrop.
            list.push(PaintCmd::Shadow {
                rect,
                clip: list.screen,
                shadow: Shadow {
                    x: 0,
                    y: 4,
                    blur: 8,
                    opacity: 9,
                    color: Rgb565::new(0, 0, 0),
                },
                projection: Projection((i * 12) as u8),
                radius: 8,
            })
            .unwrap();
            let start = list
                .begin_layer(
                    Key(303),
                    rect,
                    list.screen,
                    rect.intersect(list.screen),
                    style,
                    8,
                    backdrop_end,
                )
                .unwrap();
            list.push(PaintCmd::text(
                rect.top_left(),
                rect,
                TextId(0),
                Rgb565::new(31, 63, 31),
                80,
                1,
                false,
                FontId::Body16,
                TextAlign::Start,
            ))
            .unwrap();
            let end = list.count as u16;
            if let PaintCmd::Group { end: slot, .. } = &mut list.cmds[start] {
                *slot = end;
            }
        }
        {
            let layers = lists[0].layers.as_ref().unwrap();
            layers[0].group.borrow_mut().ready = true;
            let mut bd = layers[0].backdrop.borrow_mut();
            bd.clear = Some(bg);
            bd.tint = Some((bg, 6));
            bd.pixels[0] = 0x1234;
        }

        let old_res = SliceSceneResources {
            texts: &["Front"],
            sparklines: &[],
        };
        let new_res = SliceSceneResources {
            texts: &["Back"],
            sparklines: &[],
        };
        reuse_groups(&mut lists, 0, &old_res, &new_res);

        {
            let next_layers = lists[1].layers.as_ref().unwrap();
            assert!(!next_layers[0].group.borrow().ready);
            assert_eq!(next_layers[0].backdrop.borrow().clear, Some(bg));
            assert_eq!(next_layers[0].backdrop.borrow().tint, Some((bg, 6)));
            assert_eq!(next_layers[0].backdrop.borrow().pixels[0], 0x1234);
        }

        // Changing tint_alpha must invalidate the cached backdrop.
        lists[0].layers.as_mut().unwrap()[0].style.tint_alpha = 8;
        reuse_groups(&mut lists, 1, &new_res, &old_res);
        let invalidated = lists[0].layers.as_ref().unwrap();
        assert_eq!(invalidated[0].backdrop.borrow().clear, None);
    }
}
