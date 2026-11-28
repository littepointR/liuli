//! Stateful selection decorations use measured content and never participate in hit testing.
use super::{
    BuiltinPaintCtx, CommandList, PaintCmd, control_content, draw_control_content, resolve_state_bg,
};
use crate::capacity::BuildError;
use crate::geometry::{Rect, Rgb565};
use crate::node::{RetainedNode, RetainedTree};
use crate::part_style::{IndicatorMotion, WidgetPartStyle};
use crate::time::{ANIM_SCALE, AnimPool, AnimProp};
use crate::view::{MotionState, NodeKind};

pub(super) fn selection_progress(node: &RetainedNode, anims: &AnimPool) -> i32 {
    let settled = if node.props.checked() { ANIM_SCALE } else { 0 };
    if node.motion.state() != MotionState::Checked {
        return settled;
    }
    anims
        .cached_value(node.key, AnimProp::Style)
        .unwrap_or(settled)
        .clamp(0, ANIM_SCALE)
}

fn emit_indicator(
    out: &mut CommandList,
    rect: Rect,
    clip: Rect,
    color: Rgb565,
    radius: u8,
    alpha: u8,
) -> Result<(), BuildError> {
    if alpha == 0 || !rect.intersects(clip) {
        return Ok(());
    }
    let cmd = match (radius, alpha) {
        (0, 15) => PaintCmd::FillRect { rect, clip, color },
        (_, 15) => PaintCmd::FillRoundedRect {
            rect,
            clip,
            color,
            radius,
            border_color: None,
            border_width: 0,
        },
        (0, _) => PaintCmd::Overlay {
            rect,
            clip,
            color,
            alpha,
        },
        _ => PaintCmd::RoundedOverlay {
            rect,
            clip,
            color,
            alpha,
            radius,
        },
    };
    out.push(cmd)
}

/// A moving marker belongs to the strip. Per-item checked transitions supply weights,
/// including the current positions when selection is retargeted between frames.
pub(super) fn paint_sliding_indicator(
    tree: &RetainedTree,
    ctx: BuiltinPaintCtx<'_>,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let (bar, anims, root_bg) = (ctx.node, ctx.anims, ctx.root_bg);
    let mut selected = None;
    let mut weighted = [0i64; 4];
    let mut total = 0;
    let mut child = bar.first_child;
    while let Some(i) = child.get() {
        let n = &tree.nodes[i];
        child = n.next_sibling;
        if n.kind != NodeKind::Tab || !n.props.visible() {
            continue;
        }
        let part = tree.part_styles.get(i);
        if part.indicator_transition != IndicatorMotion::Slide {
            continue;
        }
        tab_surface(n, tree.effects.get(n.key).composition, part, root_bg, out)?;
        let p = selection_progress(n, anims);
        if p == 0 && !n.props.checked() {
            continue;
        }
        if n.props.checked() {
            selected = Some((
                part.indicator_color().unwrap_or(n.props.accent),
                part.indicator_radius().unwrap_or(0),
                n.clip_rect,
                part,
                i,
                p,
            ));
        }
        let rect = part.indicator_layout().bounds(
            n.layout_rect,
            control_content(n, part, tree.text_cache(i)).bounds,
            part.indicator_inset().unwrap_or(0),
        );
        let weight = i64::from(p);
        for (v, coordinate) in weighted.iter_mut().zip([
            i64::from(rect.x),
            i64::from(rect.y),
            i64::from(rect.w),
            i64::from(rect.h),
        ]) {
            *v += weight * coordinate;
        }
        total += weight;
    }
    if let Some((color, radius, clip, selected_part, i, progress)) = selected
        && total > 0
    {
        let [x, y, w, h] = weighted.map(|v| v / total);
        let bounds = Rect::new(x as i32, y as i32, w as u32, h as u32);
        let owner = &tree.nodes[i];
        let focused = ctx.focus.focused_slot == Some(i as u16);
        let strip = BuiltinPaintCtx {
            node: owner,
            slot: i as u16,
            text: tree.text_cache(i),
            state: super::PaintState {
                pressed: owner.pressed,
                focused: focused && ctx.focus.ring_visible,
                editing: focused && ctx.focus.editing,
                checked: true,
                enabled: owner.props.enabled(),
                anim_progress: progress,
            },
            part: selected_part,
            ..ctx
        };
        if strip.paint_part_at(
            bar.layout_rect,
            crate::part_style::WidgetPart::Indicator,
            bounds,
            clip,
            color,
            15,
            out,
        )? == super::PaintOverride::Handled
        {
            return Ok(());
        }
        emit_indicator(out, bounds, clip, color, radius, 15)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_selection(
    out: &mut CommandList,
    ctx: BuiltinPaintCtx<'_>,
    rect: Rect,
    content: Rect,
    clip: Rect,
    accent: Rgb565,
    checked: bool,
    progress: i32,
) -> Result<(), BuildError> {
    let part = ctx.part;
    let shape = part
        .indicator_layout()
        .bounds(rect, content, part.indicator_inset().unwrap_or(0));
    let color = part.indicator_color().unwrap_or(accent);
    let (shape, alpha) = match part.indicator_transition {
        IndicatorMotion::Fade => (shape, (progress * 15 / ANIM_SCALE) as u8),
        IndicatorMotion::Instant | IndicatorMotion::Slide => (shape, if checked { 15 } else { 0 }),
        IndicatorMotion::Reveal => {
            let width = (u64::from(shape.w) * progress as u64 / ANIM_SCALE as u64) as u32;
            (
                Rect::new(
                    shape.x + ((shape.w - width) / 2) as i32,
                    shape.y,
                    width,
                    shape.h,
                ),
                15,
            )
        }
    };
    if ctx.paint_part(
        crate::part_style::WidgetPart::Indicator,
        shape,
        clip,
        color,
        alpha,
        out,
    )? == super::PaintOverride::Handled
    {
        return Ok(());
    }
    emit_indicator(
        out,
        shape,
        clip,
        color,
        part.indicator_radius().unwrap_or(0),
        alpha,
    )
}

fn tab_surface(
    node: &RetainedNode,
    composition: Option<&crate::composition::SurfaceComposition>,
    part: &WidgetPartStyle,
    root_bg: Rgb565,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let rect = node.layout_rect;
    let clip = rect.intersect(node.clip_rect);
    if clip.is_empty() {
        return Ok(());
    }
    super::emit_surface(
        node,
        composition,
        resolve_state_bg(node, part, root_bg),
        out,
    )?;
    let divider = u32::from(part.track_thickness().unwrap_or(1)).min(rect.h);
    if divider > 0 {
        out.push(PaintCmd::FillRect {
            rect: Rect::new(
                rect.x,
                rect.bottom64() as i32 - divider as i32,
                rect.w,
                divider,
            ),
            clip,
            color: part.track_color().unwrap_or(node.props.overlay_color),
        })?;
    }
    Ok(())
}

pub(super) fn paint_tab(ctx: BuiltinPaintCtx<'_>, out: &mut CommandList) -> Result<(), BuildError> {
    let node = ctx.node;
    let rect = node.layout_rect;
    let clip = rect.intersect(node.clip_rect);
    if clip.is_empty() {
        return Ok(());
    }
    let content = control_content(node, ctx.part, ctx.text);
    if !crate::capacity::TAB_INDICATOR
        || ctx.part.indicator_transition != IndicatorMotion::Slide
        || !ctx.in_tab_bar
    {
        tab_surface(node, ctx.composition, ctx.part, ctx.root_bg, out)?;
        paint_selection(
            out,
            ctx,
            rect,
            content.bounds,
            clip,
            node.props.accent,
            node.props.checked(),
            selection_progress(node, ctx.anims),
        )?;
    }
    draw_control_content(ctx, content, out)
}
