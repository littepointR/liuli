use crate::capacity::{BuildError, MAX_COMMANDS, MAX_DEPTH};
use crate::geometry::{Point, Rect, Rgb565};
use crate::node::RetainedTree;
use crate::time::{ANIM_SCALE, AnimPool, AnimProp};
use crate::view::{Align, IconKind, NONE_IDX, NodeKind, TextRef};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PaintCmd {
    FillRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
    },
    StrokeRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        width: u8,
    },
    FillRoundedRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        radius: u8,
        border_color: Option<Rgb565>,
        border_width: u8,
    },
    Text {
        pos: Point,
        clip: Rect,
        ink: Rect,
        text: TextRef,
        color: Rgb565,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
    },
    Overlay {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        alpha: u8,
    },
    Icon {
        rect: Rect,
        clip: Rect,
        kind: IconKind,
        color: Rgb565,
    },
}

impl PaintCmd {
    pub const EMPTY: Self = Self::FillRect {
        rect: Rect::ZERO,
        clip: Rect::ZERO,
        color: Rgb565::new(0, 0, 0),
    };

    pub fn bounds(&self) -> Rect {
        match *self {
            Self::FillRect { rect, clip, .. } => rect.intersect(clip),
            Self::StrokeRect { rect, clip, .. } => rect.intersect(clip),
            Self::FillRoundedRect { rect, clip, .. } => rect.intersect(clip),
            Self::Text { ink, clip, .. } => ink.intersect(clip),
            Self::Overlay { rect, clip, .. } => rect.intersect(clip),
            Self::Icon { rect, clip, .. } => rect.intersect(clip),
        }
    }
}

pub struct CommandList {
    pub cmds: [PaintCmd; MAX_COMMANDS],
    pub count: usize,
    pub root_bg: Rgb565,
    pub screen: Rect,
}

impl CommandList {
    pub const fn new() -> Self {
        Self {
            cmds: [PaintCmd::EMPTY; MAX_COMMANDS],
            count: 0,
            root_bg: Rgb565::new(0, 0, 0),
            screen: Rect::ZERO,
        }
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub fn as_slice(&self) -> &[PaintCmd] {
        &self.cmds[..self.count]
    }

    pub fn push(&mut self, cmd: PaintCmd) -> Result<(), BuildError> {
        if self.count >= MAX_COMMANDS {
            return Err(BuildError::Commands);
        }
        self.cmds[self.count] = cmd;
        self.count += 1;
        Ok(())
    }
}

impl Default for CommandList {
    fn default() -> Self {
        Self::new()
    }
}

pub trait TextMetricsRef {
    fn line_height(&self) -> u32;
}

pub trait TextLookup {
    fn lookup(&self, text: TextRef) -> &str;
}

impl TextLookup for RetainedTree {
    #[inline]
    fn lookup(&self, text: TextRef) -> &str {
        self.text_str(text)
    }
}

pub struct StaticTextLookup;

impl TextLookup for StaticTextLookup {
    #[inline]
    fn lookup(&self, text: TextRef) -> &str {
        match text {
            TextRef::Static(s) => s,
            TextRef::Dynamic(_) => "",
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PaintFocusInfo {
    pub focused_slot: Option<u16>,
    pub ring_visible: bool,
    pub editing: bool,
}

pub fn emit_commands(
    tree: &RetainedTree,
    anims: &AnimPool,
    focus: PaintFocusInfo,
    screen: Rect,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    out.reset();
    out.screen = screen;

    if tree.root == NONE_IDX {
        return Ok(());
    }

    let root = &tree.nodes[tree.root as usize];
    out.root_bg = root.props.bg.unwrap_or(Rgb565::new(0, 0, 0));

    // Iterative pre-order traversal preserving declaration z-order.
    // On each frame, stack entries are `(node_slot, stage)`:
    // stage 0 => emit self pre-order, push children in reverse sibling order (or direct, see below)
    // stage 1 => emit post-children (scrollbar for ScrollY + focus ring)
    #[derive(Copy, Clone)]
    struct Frame {
        slot: u16,
        stage: u8,
    }

    let mut stack = [Frame { slot: 0, stage: 0 }; MAX_DEPTH * 8];
    let mut sp = 0usize;
    stack[sp] = Frame {
        slot: tree.root,
        stage: 0,
    };
    sp += 1;

    while sp > 0 {
        sp -= 1;
        let frame = stack[sp];
        let node = &tree.nodes[frame.slot as usize];
        if !node.occupied || !node.props.visible {
            continue;
        }

        if frame.stage == 0 {
            let clip = node.clip_rect;
            let rect = node.layout_rect;
            let visible = !rect.intersect(clip).is_empty() && !node.visible_rect.is_empty();

            if visible || node.kind == NodeKind::Modal {
                match node.kind {
                    NodeKind::Root => {
                        if let Some(bg) = node.props.bg {
                            out.push(PaintCmd::FillRect {
                                rect,
                                clip,
                                color: bg,
                            })?;
                        }
                    }
                    NodeKind::Row | NodeKind::Column | NodeKind::Stack | NodeKind::ScrollY => {
                        emit_box_bg_and_border(node, rect, clip, out)?;
                    }
                    NodeKind::Modal => {
                        if node.props.overlay_alpha > 0 {
                            out.push(PaintCmd::Overlay {
                                rect: screen,
                                clip: screen,
                                color: node.props.overlay_color,
                                alpha: node.props.overlay_alpha.min(15),
                            })?;
                        }
                        emit_box_bg_and_border(node, rect, clip, out)?;
                    }
                    NodeKind::Label => {
                        if let Some(bg) = node.props.bg {
                            out.push(PaintCmd::FillRect {
                                rect,
                                clip,
                                color: bg,
                            })?;
                        }
                        let inner = rect.inset(
                            node.props.padding.left as u32,
                            node.props.padding.top as u32,
                            node.props.padding.right as u32,
                            node.props.padding.bottom as u32,
                        );
                        let text_x = inner.x;
                        let mut text_y = inner.y;
                        if let Some((_, text_size, _, _)) = node.text_cache {
                            if node.props.align_cross == Align::Center && inner.h > text_size.height
                            {
                                text_y += ((inner.h - text_size.height) / 2) as i32;
                            } else if node.props.align_cross == Align::End
                                && inner.h > text_size.height
                            {
                                text_y += (inner.h - text_size.height) as i32;
                            }
                        }
                        let mut ink = node.ink_rect;
                        if let Some((_, _, rel_ink, _)) = node.text_cache {
                            ink = rel_ink.translate(text_x, text_y);
                        }
                        out.push(PaintCmd::Text {
                            pos: Point::new(text_x, text_y),
                            clip,
                            ink,
                            text: node.props.text,
                            color: node.props.fg,
                            max_width: inner.w,
                            max_lines: node.props.max_lines,
                            ellipsis: node.props.ellipsis,
                        })?;
                    }
                    NodeKind::Button => {
                        let bg = if node.pressed {
                            Some(darken_or_mix(node.props.bg.unwrap_or(node.props.accent), 3))
                        } else {
                            node.props.bg
                        };
                        if let Some(bg_col) = bg {
                            if node.props.radius > 0 || node.props.border_color.is_some() {
                                out.push(PaintCmd::FillRoundedRect {
                                    rect,
                                    clip,
                                    color: bg_col,
                                    radius: node.props.radius,
                                    border_color: node.props.border_color,
                                    border_width: node.props.border_width,
                                })?;
                            } else {
                                out.push(PaintCmd::FillRect {
                                    rect,
                                    clip,
                                    color: bg_col,
                                })?;
                            }
                        } else if let Some(bc) = node.props.border_color
                            && node.props.border_width > 0
                        {
                            out.push(PaintCmd::StrokeRect {
                                rect,
                                clip,
                                color: bc,
                                width: node.props.border_width,
                            })?;
                        }

                        let inner = rect.inset(
                            node.props.padding.left as u32,
                            node.props.padding.top as u32,
                            node.props.padding.right as u32,
                            node.props.padding.bottom as u32,
                        );
                        let text_x;
                        let mut text_y = inner.y;
                        let icon_w = if node.props.icon != IconKind::None {
                            16
                        } else {
                            0
                        };
                        let gap_w = if icon_w > 0 { 4 } else { 0 };
                        let text_w = node.text_cache.map(|c| c.1.width).unwrap_or(0);
                        let text_h = node.text_cache.map(|c| c.1.height).unwrap_or(16);
                        let total_w = icon_w + gap_w + text_w;
                        let start_x = if inner.w > total_w {
                            inner.x + ((inner.w - total_w) / 2) as i32
                        } else {
                            inner.x
                        };
                        if node.props.icon != IconKind::None {
                            let iy = inner.y + ((inner.h.saturating_sub(14)) / 2) as i32;
                            out.push(PaintCmd::Icon {
                                rect: Rect::new(start_x, iy, 14, 14),
                                clip,
                                kind: node.props.icon,
                                color: node.props.fg,
                            })?;
                            text_x = start_x + 18;
                        } else {
                            text_x = start_x;
                        }
                        if inner.h > text_h {
                            text_y = inner.y + ((inner.h - text_h) / 2) as i32;
                        }
                        let rel_ink = node
                            .text_cache
                            .map(|c| c.2)
                            .unwrap_or(Rect::new(0, 0, text_w, text_h));
                        out.push(PaintCmd::Text {
                            pos: Point::new(text_x, text_y),
                            clip,
                            ink: rel_ink.translate(text_x, text_y),
                            text: node.props.text,
                            color: node.props.fg,
                            max_width: inner.w.saturating_sub(icon_w + gap_w),
                            max_lines: node.props.max_lines.max(1),
                            ellipsis: node.props.ellipsis,
                        })?;
                    }
                    NodeKind::Toggle => {
                        let track_w = rect.w.clamp(28, 48);
                        let track_h = rect.h.clamp(18, 26);
                        let track_x = rect.x + ((rect.w - track_w) / 2) as i32;
                        let track_y = rect.y + ((rect.h - track_h) / 2) as i32;
                        let track_rect = Rect::new(track_x, track_y, track_w, track_h);

                        let default_progress = if node.props.checked { ANIM_SCALE } else { 0 };
                        let progress = anims
                            .cached_value(node.key, AnimProp::ToggleKnob)
                            .unwrap_or(default_progress)
                            .clamp(0, ANIM_SCALE);

                        let track_off = node.props.bg.unwrap_or(Rgb565::new(10, 20, 10));
                        let track_on = node.props.accent;
                        let track_color =
                            blend_rgb565_scale(track_off, track_on, progress, ANIM_SCALE);

                        let r = (track_h / 2) as u8;
                        out.push(PaintCmd::FillRoundedRect {
                            rect: track_rect,
                            clip,
                            color: track_color,
                            radius: r,
                            border_color: node.props.border_color,
                            border_width: node.props.border_width,
                        })?;

                        let knob_size = track_h.saturating_sub(4).max(8);
                        let travel = track_w.saturating_sub(knob_size + 4) as i32;
                        let knob_x =
                            track_x + 2 + ((travel * progress + ANIM_SCALE / 2) / ANIM_SCALE);
                        let knob_y = track_y + 2;
                        let knob_rect = Rect::new(knob_x, knob_y, knob_size, knob_size);
                        out.push(PaintCmd::FillRoundedRect {
                            rect: knob_rect,
                            clip,
                            color: node.props.fg,
                            radius: (knob_size / 2) as u8,
                            border_color: None,
                            border_width: 0,
                        })?;
                    }
                    NodeKind::Slider => {
                        let track_h = 6u32.min(rect.h.max(2));
                        let knob_w = 12u32;
                        let knob_h = rect.h.clamp(14, 20);
                        let pad_x = (knob_w / 2) as i32;
                        let usable_w = rect.w.saturating_sub(knob_w).max(1);
                        let track_x = rect.x + pad_x;
                        let track_y = rect.y + ((rect.h - track_h) / 2) as i32;
                        let track_rect = Rect::new(track_x, track_y, usable_w, track_h);
                        let track_bg = node.props.bg.unwrap_or(Rgb565::new(10, 20, 10));

                        out.push(PaintCmd::FillRoundedRect {
                            rect: track_rect,
                            clip,
                            color: track_bg,
                            radius: (track_h / 2) as u8,
                            border_color: None,
                            border_width: 0,
                        })?;

                        let span = (node.props.max as i32 - node.props.min as i32).max(1);
                        let clamped = (node.props.value as i32)
                            .clamp(node.props.min as i32, node.props.max as i32);
                        let rel = clamped - node.props.min as i32;
                        let fill_w = ((usable_w as i64 * rel as i64) / span as i64) as u32;
                        if fill_w > 0 {
                            out.push(PaintCmd::FillRoundedRect {
                                rect: Rect::new(track_x, track_y, fill_w, track_h),
                                clip,
                                color: node.props.accent,
                                radius: (track_h / 2) as u8,
                                border_color: None,
                                border_width: 0,
                            })?;
                        }

                        let knob_x = track_x + fill_w as i32 - (knob_w / 2) as i32;
                        let knob_y = rect.y + ((rect.h - knob_h) / 2) as i32;
                        let is_editing = focus.editing && focus.focused_slot == Some(frame.slot);
                        out.push(PaintCmd::FillRoundedRect {
                            rect: Rect::new(knob_x, knob_y, knob_w, knob_h),
                            clip,
                            color: node.props.fg,
                            radius: 4,
                            border_color: Some(if is_editing {
                                node.props.focus_color
                            } else {
                                node.props.accent
                            }),
                            border_width: if is_editing { 2 } else { 1 },
                        })?;
                    }
                }
            }

            // Push stage 1 for post-children (scrollbar + focus ring)
            stack[sp] = Frame {
                slot: frame.slot,
                stage: 1,
            };
            sp += 1;

            // Collect children onto temp buffer and push in reverse so first child pops first
            let mut children = [NONE_IDX; 64];
            let mut child_cnt = 0usize;
            let mut ch = node.first_child;
            while ch != NONE_IDX {
                children[child_cnt] = ch;
                child_cnt += 1;
                ch = tree.nodes[ch as usize].next_sibling;
            }
            while child_cnt > 0 {
                child_cnt -= 1;
                if sp >= stack.len() {
                    return Err(BuildError::Depth);
                }
                stack[sp] = Frame {
                    slot: children[child_cnt],
                    stage: 0,
                };
                sp += 1;
            }
        } else {
            // Stage 1: post-children overlays (ScrollY scrollbar, focus ring)
            let clip = node.clip_rect;
            let rect = node.layout_rect;

            if node.kind == NodeKind::ScrollY
                && node.content_height > rect.h
                && rect.h > 8
                && rect.w > 6
            {
                let track_h = rect.h.saturating_sub(4);
                let thumb_h = ((track_h as u64 * rect.h as u64) / node.content_height as u64)
                    .clamp(12, track_h as u64) as u32;
                let max_scroll = node.content_height.saturating_sub(rect.h).max(1);
                let thumb_travel = track_h.saturating_sub(thumb_h);
                let thumb_y = rect.y
                    + 2
                    + ((thumb_travel as u64 * (node.scroll_y.max(0) as u64)) / max_scroll as u64)
                        as i32;
                let thumb_rect = Rect::new(rect.x + rect.w as i32 - 4, thumb_y, 3, thumb_h);
                if !thumb_rect.intersect(clip).is_empty() {
                    out.push(PaintCmd::FillRoundedRect {
                        rect: thumb_rect,
                        clip,
                        color: node.props.accent,
                        radius: 1,
                        border_color: None,
                        border_width: 0,
                    })?;
                }
            }

            if focus.ring_visible
                && focus.focused_slot == Some(frame.slot)
                && !rect.intersect(clip).is_empty()
            {
                let ring_rect = rect.expand(2);
                let ring_clip = clip.intersect(screen);
                if !ring_rect.intersect(ring_clip).is_empty() {
                    out.push(PaintCmd::StrokeRect {
                        rect: ring_rect,
                        clip: ring_clip,
                        color: node.props.focus_color,
                        width: 2,
                    })?;
                    if focus.editing {
                        let inner_ring = rect.expand(1);
                        out.push(PaintCmd::StrokeRect {
                            rect: inner_ring,
                            clip: ring_clip,
                            color: node.props.accent,
                            width: 1,
                        })?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn emit_box_bg_and_border(
    node: &crate::node::RetainedNode,
    rect: Rect,
    clip: Rect,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    if rect.intersect(clip).is_empty() {
        return Ok(());
    }
    let bg = if node.pressed {
        node.props.bg.map(|c| darken_or_mix(c, 3))
    } else {
        node.props.bg
    };
    if let Some(color) = bg {
        if node.props.radius > 0
            || (node.props.border_color.is_some() && node.props.border_width > 0)
        {
            out.push(PaintCmd::FillRoundedRect {
                rect,
                clip,
                color,
                radius: node.props.radius,
                border_color: node.props.border_color,
                border_width: node.props.border_width,
            })?;
        } else {
            out.push(PaintCmd::FillRect { rect, clip, color })?;
        }
    } else if let Some(bc) = node.props.border_color
        && node.props.border_width > 0
    {
        out.push(PaintCmd::StrokeRect {
            rect,
            clip,
            color: bc,
            width: node.props.border_width,
        })?;
    }
    Ok(())
}

fn darken_or_mix(color: Rgb565, amount: u16) -> Rgb565 {
    use embedded_graphics_core::pixelcolor::{IntoStorage, raw::RawU16};
    let raw = color.into_storage();
    let r = ((raw >> 11) & 0x1F).saturating_sub(amount);
    let g = ((raw >> 5) & 0x3F).saturating_sub(amount * 2);
    let b = (raw & 0x1F).saturating_sub(amount);
    Rgb565::from(RawU16::new((r << 11) | (g << 5) | b))
}

pub fn blend_rgb565_scale(bg: Rgb565, fg: Rgb565, num: i32, den: i32) -> Rgb565 {
    use embedded_graphics_core::pixelcolor::{IntoStorage, raw::RawU16};
    if num <= 0 {
        return bg;
    }
    if num >= den {
        return fg;
    }
    let b = bg.into_storage() as i32;
    let f = fg.into_storage() as i32;
    let inv = den - num;

    let br = (b >> 11) & 0x1F;
    let bg_c = (b >> 5) & 0x3F;
    let bb = b & 0x1F;

    let fr = (f >> 11) & 0x1F;
    let fg_c = (f >> 5) & 0x3F;
    let fb = f & 0x1F;

    let rr = (fr * num + br * inv + den / 2) / den;
    let rg = (fg_c * num + bg_c * inv + den / 2) / den;
    let rb = (fb * num + bb * inv + den / 2) / den;

    Rgb565::from(RawU16::new(
        ((rr as u16) << 11) | ((rg as u16) << 5) | (rb as u16),
    ))
}
