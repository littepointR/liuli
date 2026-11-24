use crate::capacity::MAX_DEPTH;
use crate::geometry::{Point, Rect, Size};
use crate::node::{RetainedNode, RetainedTree, TextCache};
use crate::view::{
    Align, BUTTON_ICON_SPAN, FontId, IconKind, Length, NodeKind, SlotLink, TextAlign,
};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Constraints {
    pub min_w: u32,
    pub max_w: u32,
    pub min_h: u32,
    pub max_h: u32,
}

impl Constraints {
    pub const ZERO: Self = Self {
        min_w: 0,
        max_w: 0,
        min_h: 0,
        max_h: 0,
    };

    pub const fn loose(max_w: u32, max_h: u32) -> Self {
        Self {
            min_w: 0,
            max_w,
            min_h: 0,
            max_h,
        }
    }

    pub const fn tight(w: u32, h: u32) -> Self {
        Self {
            min_w: w,
            max_w: w,
            min_h: h,
            max_h: h,
        }
    }

    pub fn clamp_size(self, s: Size) -> Size {
        Size::new(
            s.width.clamp(self.min_w, self.max_w.max(self.min_w)),
            s.height.clamp(self.min_h, self.max_h.max(self.min_h)),
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TextMeasureOut {
    pub size: Size,
    pub ink: Rect,
    pub lines: u16,
}

pub trait TextMeasurer {
    fn custom_size(&self, _key: crate::view::Key, _font: FontId, _available: Size) -> Size {
        Size::zero()
    }

    fn line_height(&self, font: FontId) -> u32;
    fn measure_text(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
    ) -> TextMeasureOut;

    #[allow(clippy::too_many_arguments)]
    fn measure_text_styled(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
        _wrap: crate::view::WrapMode,
        _letter_spacing: i8,
        _line_spacing: i8,
    ) -> TextMeasureOut {
        self.measure_text(font, text, max_width, max_lines, ellipsis, align)
    }
}

pub fn layout_tree(
    tree: &mut RetainedTree,
    screen: Rect,
    measurer: &dyn TextMeasurer,
) -> Result<u32, crate::capacity::LayoutError> {
    let Some(root) = tree.root.get_u16() else {
        return Err(crate::capacity::LayoutError::EmptyTree);
    };

    let mut measured_nodes = 0u32;

    if tree.nodes[root as usize].dirty_measure {
        let root_c = Constraints::tight(screen.w, screen.h);
        measured_nodes = measure_iterative(tree, root, root_c, measurer);
        arrange_iterative(tree, root, screen, screen, screen);
    }

    for node in &mut tree.nodes {
        if node.occupied {
            node.dirty_measure = false;
        }
    }

    Ok(measured_nodes)
}

pub fn relayout_scroll_subtree(tree: &mut RetainedTree, scroll_slot: u16, screen: Rect) {
    if !tree.nodes[scroll_slot as usize].occupied {
        return;
    }
    let rect = tree.nodes[scroll_slot as usize].layout_rect;
    let clip = tree.nodes[scroll_slot as usize].clip_rect;
    arrange_iterative(tree, scroll_slot, rect, clip, screen);
}

#[inline]
fn resolve_axis_max(len: Length, parent_max: u32) -> u32 {
    match len {
        Length::Px(px) => px.min(parent_max),
        Length::Fill(_) | Length::Content => parent_max,
    }
}

#[inline]
fn resolve_axis_size(len: Length, content_with_pad: u32, max_bound: u32) -> u32 {
    match len {
        Length::Px(px) => px,
        Length::Fill(_) => max_bound,
        Length::Content => content_with_pad,
    }
}

#[inline]
fn is_main_fill(kind: NodeKind, child_w: Length, child_h: Length) -> Option<u16> {
    match kind {
        NodeKind::Row => match child_w {
            Length::Fill(w) => Some(w.max(1)),
            _ => None,
        },
        NodeKind::Column => match child_h {
            Length::Fill(w) => Some(w.max(1)),
            _ => None,
        },
        _ => None,
    }
}

fn first_visible_child(tree: &RetainedTree, mut ch: SlotLink) -> Option<u16> {
    while let Some(idx) = ch.get_u16() {
        let n = &tree.nodes[idx as usize];
        if n.occupied && n.props.visible() {
            return Some(idx);
        }
        ch = n.next_sibling;
    }
    None
}

fn next_visible_sibling(tree: &RetainedTree, ch: u16) -> Option<u16> {
    first_visible_child(tree, tree.nodes[ch as usize].next_sibling)
}

fn measure_leaf(
    tree: &mut RetainedTree,
    slot: u16,
    c: Constraints,
    measurer: &dyn TextMeasurer,
) -> Size {
    let n = &tree.nodes[slot as usize];
    let pad_w = n.props.padding.horizontal();
    let pad_h = n.props.padding.vertical();
    let kind = n.kind;
    let width_prop = n.props.width;
    let height_prop = n.props.height;
    let text_ref = n.props.text();
    let font = n.props.font();
    let prop_align = n.props.text_align();
    let measure_align = if matches!(width_prop, Length::Content) {
        TextAlign::Start
    } else {
        prop_align
    };
    let max_lines = n.props.max_lines;
    let ellipsis = n.props.ellipsis();
    let wrap = n.props.wrap();
    let letter_spacing = n.props.letter_spacing;
    let line_spacing = n.props.line_spacing;
    let icon = n.props.icon;

    match kind {
        NodeKind::Label => {
            let avail_w = resolve_axis_max(width_prop, c.max_w);
            let wrap_w = avail_w.saturating_sub(pad_w);
            let text_str = tree.text_str(text_ref);
            let tm = measurer.measure_text_styled(
                font,
                text_str,
                wrap_w,
                max_lines,
                ellipsis,
                measure_align,
                wrap,
                letter_spacing,
                line_spacing,
            );
            tree.nodes[slot as usize].text_cache = TextCache {
                wrap_width: wrap_w,
                size: tm.size,
                rel_ink: tm.ink,
                lines: tm.lines,
                valid: true,
                font,
                align: measure_align,
                max_lines,
                ellipsis,
            };

            let w = resolve_axis_size(width_prop, tm.size.width.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, tm.size.height.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Button => {
            let avail_w = resolve_axis_max(width_prop, c.max_w);
            let icon_space = if icon != IconKind::None {
                BUTTON_ICON_SPAN
            } else {
                0
            };
            let wrap_w = avail_w.saturating_sub(pad_w + icon_space);
            let eff_max_lines = max_lines.max(1);
            let text_str = tree.text_str(text_ref);
            let tm = measurer.measure_text_styled(
                font,
                text_str,
                wrap_w,
                eff_max_lines,
                ellipsis,
                measure_align,
                wrap,
                letter_spacing,
                line_spacing,
            );
            tree.nodes[slot as usize].text_cache = TextCache {
                wrap_width: wrap_w,
                size: tm.size,
                rel_ink: tm.ink,
                lines: tm.lines,
                valid: true,
                font,
                align: measure_align,
                max_lines: eff_max_lines,
                ellipsis,
            };

            let content_w = tm.size.width.saturating_add(pad_w + icon_space);
            let content_h = tm.size.height.max(14).saturating_add(pad_h);
            let w = resolve_axis_size(width_prop, content_w, c.max_w);
            let h = resolve_axis_size(height_prop, content_h, c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Toggle => {
            let w = resolve_axis_size(width_prop, 40u32.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, 24u32.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Slider => {
            let w = resolve_axis_size(width_prop, 120u32.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, 24u32.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Custom => {
            let content = measurer.custom_size(
                n.key,
                font,
                Size::new(c.max_w.saturating_sub(pad_w), c.max_h.saturating_sub(pad_h)),
            );
            let w = resolve_axis_size(width_prop, content.width.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, content.height.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::ProgressBar => {
            let w = resolve_axis_size(width_prop, 120u32.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, 12u32.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Sparkline => {
            let w = resolve_axis_size(width_prop, 120u32.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, 40u32.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        _ => c.clamp_size(Size::new(pad_w, pad_h)),
    }
}

#[derive(Copy, Clone)]
struct MeasureFrame {
    slot: u16,
    constraints: Constraints,
    phase: u8,
    active_child: SlotLink,
    vis_count: u16,
    non_fill_used: u32,
    fill_used: u32,
    cross_max: u32,
    inner_max_w: u32,
    inner_max_h: u32,
    total_fill_weight: u32,
    remaining_fill: u32,
    fill_rem_pixels: u32,
}

impl MeasureFrame {
    const EMPTY: Self = Self {
        slot: 0,
        constraints: Constraints::ZERO,
        phase: 0,
        active_child: SlotLink::NONE,
        vis_count: 0,
        non_fill_used: 0,
        fill_used: 0,
        cross_max: 0,
        inner_max_w: 0,
        inner_max_h: 0,
        total_fill_weight: 0,
        remaining_fill: 0,
        fill_rem_pixels: 0,
    };
}

#[inline(never)]
fn measure_iterative(
    tree: &mut RetainedTree,
    root: u16,
    root_constraints: Constraints,
    measurer: &dyn TextMeasurer,
) -> u32 {
    let mut stack = [MeasureFrame::EMPTY; MAX_DEPTH + 2];
    let mut sp = 0usize;
    let mut measured_count = 0u32;

    stack[sp] = MeasureFrame {
        slot: root,
        constraints: root_constraints,
        ..MeasureFrame::EMPTY
    };
    sp += 1;

    while sp > 0 {
        let top = sp - 1;
        let slot = stack[top].slot;
        let c = stack[top].constraints;
        let n_kind = tree.nodes[slot as usize].kind;
        let n_occupied = tree.nodes[slot as usize].occupied;
        let n_visible = tree.nodes[slot as usize].props.visible();

        if !n_occupied || !n_visible {
            tree.nodes[slot as usize].measured_size = Size::new(0, 0);
            sp -= 1;
            continue;
        }

        if stack[top].phase == 0 {
            measured_count += 1;
            if !n_kind.is_container() {
                let sz = measure_leaf(tree, slot, c, measurer);
                tree.nodes[slot as usize].measured_size = sz;
                sp -= 1;
                continue;
            }

            let pad_w = tree.nodes[slot as usize].props.padding.horizontal();
            let pad_h = tree.nodes[slot as usize].props.padding.vertical();
            let width_prop = tree.nodes[slot as usize].props.width;
            let height_prop = tree.nodes[slot as usize].props.height;
            let align_cross = tree.nodes[slot as usize].props.align_cross;
            let gap = tree.nodes[slot as usize].props.gap;
            let first_child = tree.nodes[slot as usize].first_child;

            let inner_max_w = resolve_axis_max(width_prop, c.max_w).saturating_sub(pad_w);
            let inner_max_h = resolve_axis_max(height_prop, c.max_h).saturating_sub(pad_h);

            let mut vis_count = 0u16;
            let mut total_fill_weight = 0u32;
            let mut first_non_fill: Option<u16> = None;
            let mut first_fill: Option<u16> = None;

            let mut ch = first_visible_child(tree, first_child);
            while let Some(ch_idx) = ch {
                vis_count += 1;
                let cn = &tree.nodes[ch_idx as usize];
                if let Some(w) = is_main_fill(n_kind, cn.props.width, cn.props.height) {
                    total_fill_weight += w as u32;
                    if first_fill.is_none() {
                        first_fill = Some(ch_idx);
                    }
                } else if first_non_fill.is_none() {
                    first_non_fill = Some(ch_idx);
                }
                ch = next_visible_sibling(tree, ch_idx);
            }

            if vis_count == 0 {
                let w = resolve_axis_size(width_prop, pad_w, c.max_w);
                let h = resolve_axis_size(height_prop, pad_h, c.max_h);
                if n_kind == NodeKind::ScrollY {
                    tree.nodes[slot as usize].content_height = pad_h;
                }
                tree.nodes[slot as usize].measured_size = c.clamp_size(Size::new(w, h));
                sp -= 1;
                continue;
            }

            stack[top].vis_count = vis_count;
            stack[top].inner_max_w = inner_max_w;
            stack[top].inner_max_h = inner_max_h;
            stack[top].total_fill_weight = total_fill_weight;

            match n_kind {
                NodeKind::Row | NodeKind::Column => {
                    if let Some(fnf) = first_non_fill {
                        stack[top].phase = 1;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(fnf).unwrap_or(SlotLink::NONE);
                        let child_c = child_constraints_flex(
                            n_kind,
                            align_cross,
                            width_prop,
                            height_prop,
                            inner_max_w,
                            inner_max_h,
                            &tree.nodes[fnf as usize],
                            None,
                        );
                        stack[sp] = MeasureFrame {
                            slot: fnf,
                            constraints: child_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else if let Some(ff) = first_fill {
                        let gap_total = (vis_count.saturating_sub(1) as u32) * (gap as u32);
                        let avail_main = if n_kind == NodeKind::Row {
                            inner_max_w
                        } else {
                            inner_max_h
                        };
                        let remaining = avail_main.saturating_sub(gap_total);
                        let w_total = total_fill_weight.max(1);
                        let mut rem_left = remaining
                            - distribute_base_sum(tree, n_kind, first_child, remaining, w_total);
                        let cn = &tree.nodes[ff as usize];
                        let weight = is_main_fill(n_kind, cn.props.width, cn.props.height)
                            .unwrap_or(1) as u32;
                        let extra = if rem_left > 0 {
                            rem_left -= 1;
                            1
                        } else {
                            0
                        };
                        let first_share =
                            ((remaining as u64 * weight as u64) / w_total as u64) as u32 + extra;

                        stack[top].phase = 2;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(ff).unwrap_or(SlotLink::NONE);
                        stack[top].remaining_fill = remaining;
                        stack[top].fill_rem_pixels = rem_left;

                        let child_c = child_constraints_flex(
                            n_kind,
                            align_cross,
                            width_prop,
                            height_prop,
                            inner_max_w,
                            inner_max_h,
                            cn,
                            Some(first_share),
                        );
                        stack[sp] = MeasureFrame {
                            slot: ff,
                            constraints: child_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    }
                }
                NodeKind::ScrollY => {
                    if let Some(first) = first_visible_child(tree, first_child) {
                        stack[top].phase = 2;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(first).unwrap_or(SlotLink::NONE);
                        let ch_c =
                            child_constraints_scroll(inner_max_w, &tree.nodes[first as usize]);
                        stack[sp] = MeasureFrame {
                            slot: first,
                            constraints: ch_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    }
                }
                NodeKind::Root | NodeKind::Stack | NodeKind::Modal => {
                    if let Some(first) = first_visible_child(tree, first_child) {
                        stack[top].phase = 2;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(first).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_stack(
                            inner_max_w,
                            inner_max_h,
                            &tree.nodes[first as usize],
                        );
                        stack[sp] = MeasureFrame {
                            slot: first,
                            constraints: ch_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    }
                }
                _ => unreachable!(),
            }
        } else if stack[top].phase == 1 {
            let Some(prev) = stack[top].active_child.get_u16() else {
                sp -= 1;
                continue;
            };
            let prev_sz = tree.nodes[prev as usize].measured_size;
            if n_kind == NodeKind::Row {
                stack[top].non_fill_used = stack[top].non_fill_used.saturating_add(prev_sz.width);
                stack[top].cross_max = stack[top].cross_max.max(prev_sz.height);
            } else {
                stack[top].non_fill_used = stack[top].non_fill_used.saturating_add(prev_sz.height);
                stack[top].cross_max = stack[top].cross_max.max(prev_sz.width);
            }

            let mut next = next_visible_sibling(tree, prev);
            while let Some(nx) = next {
                let cn = &tree.nodes[nx as usize];
                if is_main_fill(n_kind, cn.props.width, cn.props.height).is_none() {
                    break;
                }
                next = next_visible_sibling(tree, nx);
            }

            let width_prop = tree.nodes[slot as usize].props.width;
            let height_prop = tree.nodes[slot as usize].props.height;
            let align_cross = tree.nodes[slot as usize].props.align_cross;
            let gap = tree.nodes[slot as usize].props.gap;
            let first_child = tree.nodes[slot as usize].first_child;

            if let Some(nx) = next {
                stack[top].active_child = SlotLink::from_slot_u16(nx).unwrap_or(SlotLink::NONE);
                let child_c = child_constraints_flex(
                    n_kind,
                    align_cross,
                    width_prop,
                    height_prop,
                    stack[top].inner_max_w,
                    stack[top].inner_max_h,
                    &tree.nodes[nx as usize],
                    None,
                );
                stack[sp] = MeasureFrame {
                    slot: nx,
                    constraints: child_c,
                    ..MeasureFrame::EMPTY
                };
                sp += 1;
            } else {
                let first_fill = find_next_fill_child(tree, n_kind, first_child);
                if let Some(ff) = first_fill {
                    let gap_total = (stack[top].vis_count.saturating_sub(1) as u32) * (gap as u32);
                    let avail_main = if n_kind == NodeKind::Row {
                        stack[top].inner_max_w
                    } else {
                        stack[top].inner_max_h
                    };
                    let remaining = avail_main
                        .saturating_sub(stack[top].non_fill_used.saturating_add(gap_total));
                    let w_total = stack[top].total_fill_weight.max(1);
                    let mut rem_left = remaining
                        - distribute_base_sum(tree, n_kind, first_child, remaining, w_total);
                    let cn = &tree.nodes[ff as usize];
                    let weight =
                        is_main_fill(n_kind, cn.props.width, cn.props.height).unwrap_or(1) as u32;
                    let extra = if rem_left > 0 {
                        rem_left -= 1;
                        1
                    } else {
                        0
                    };
                    let share =
                        ((remaining as u64 * weight as u64) / w_total as u64) as u32 + extra;

                    stack[top].phase = 2;
                    stack[top].active_child = SlotLink::from_slot_u16(ff).unwrap_or(SlotLink::NONE);
                    stack[top].remaining_fill = remaining;
                    stack[top].fill_rem_pixels = rem_left;

                    let child_c = child_constraints_flex(
                        n_kind,
                        align_cross,
                        width_prop,
                        height_prop,
                        stack[top].inner_max_w,
                        stack[top].inner_max_h,
                        cn,
                        Some(share),
                    );
                    stack[sp] = MeasureFrame {
                        slot: ff,
                        constraints: child_c,
                        ..MeasureFrame::EMPTY
                    };
                    sp += 1;
                } else {
                    finalize_flex_measure(tree, slot, &stack[top], c);
                    sp -= 1;
                }
            }
        } else {
            let Some(prev) = stack[top].active_child.get_u16() else {
                sp -= 1;
                continue;
            };
            let prev_sz = tree.nodes[prev as usize].measured_size;

            match n_kind {
                NodeKind::Row | NodeKind::Column => {
                    if n_kind == NodeKind::Row {
                        stack[top].fill_used = stack[top].fill_used.saturating_add(prev_sz.width);
                        stack[top].cross_max = stack[top].cross_max.max(prev_sz.height);
                    } else {
                        stack[top].fill_used = stack[top].fill_used.saturating_add(prev_sz.height);
                        stack[top].cross_max = stack[top].cross_max.max(prev_sz.width);
                    }

                    let next_sib = tree.nodes[prev as usize].next_sibling;
                    let next_fill = find_next_fill_child(tree, n_kind, next_sib);
                    if let Some(nf) = next_fill {
                        let w_total = stack[top].total_fill_weight.max(1);
                        let remaining = stack[top].remaining_fill;
                        let width_prop = tree.nodes[slot as usize].props.width;
                        let height_prop = tree.nodes[slot as usize].props.height;
                        let align_cross = tree.nodes[slot as usize].props.align_cross;
                        let cn = &tree.nodes[nf as usize];
                        let weight = is_main_fill(n_kind, cn.props.width, cn.props.height)
                            .unwrap_or(1) as u32;
                        let extra = if stack[top].fill_rem_pixels > 0 {
                            stack[top].fill_rem_pixels -= 1;
                            1
                        } else {
                            0
                        };
                        let share =
                            ((remaining as u64 * weight as u64) / w_total as u64) as u32 + extra;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(nf).unwrap_or(SlotLink::NONE);
                        let child_c = child_constraints_flex(
                            n_kind,
                            align_cross,
                            width_prop,
                            height_prop,
                            stack[top].inner_max_w,
                            stack[top].inner_max_h,
                            cn,
                            Some(share),
                        );
                        stack[sp] = MeasureFrame {
                            slot: nf,
                            constraints: child_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else {
                        finalize_flex_measure(tree, slot, &stack[top], c);
                        sp -= 1;
                    }
                }
                NodeKind::ScrollY => {
                    stack[top].non_fill_used =
                        stack[top].non_fill_used.saturating_add(prev_sz.height);
                    stack[top].cross_max = stack[top].cross_max.max(prev_sz.width);
                    let next = next_visible_sibling(tree, prev);
                    if let Some(nx) = next {
                        stack[top].active_child =
                            SlotLink::from_slot_u16(nx).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_scroll(
                            stack[top].inner_max_w,
                            &tree.nodes[nx as usize],
                        );
                        stack[sp] = MeasureFrame {
                            slot: nx,
                            constraints: ch_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else {
                        let pad_w = tree.nodes[slot as usize].props.padding.horizontal();
                        let pad_h = tree.nodes[slot as usize].props.padding.vertical();
                        let gap = tree.nodes[slot as usize].props.gap;
                        let width_prop = tree.nodes[slot as usize].props.width;
                        let height_prop = tree.nodes[slot as usize].props.height;
                        let gap_total =
                            (stack[top].vis_count.saturating_sub(1) as u32) * (gap as u32);
                        let content_h = stack[top]
                            .non_fill_used
                            .saturating_add(gap_total)
                            .saturating_add(pad_h);
                        let w = resolve_axis_size(
                            width_prop,
                            stack[top].cross_max.saturating_add(pad_w),
                            c.max_w,
                        );
                        let h = resolve_axis_size(height_prop, content_h, c.max_h);
                        let clamped = c.clamp_size(Size::new(w, h));
                        tree.nodes[slot as usize].content_height = content_h;
                        tree.nodes[slot as usize].measured_size = clamped;
                        let max_scroll = content_h.saturating_sub(clamped.height) as i32;
                        tree.nodes[slot as usize].scroll_y =
                            tree.nodes[slot as usize].scroll_y.clamp(0, max_scroll);
                        sp -= 1;
                    }
                }
                NodeKind::Root | NodeKind::Stack | NodeKind::Modal => {
                    stack[top].non_fill_used = stack[top].non_fill_used.max(prev_sz.width);
                    stack[top].cross_max = stack[top].cross_max.max(prev_sz.height);
                    let next = next_visible_sibling(tree, prev);
                    if let Some(nx) = next {
                        stack[top].active_child =
                            SlotLink::from_slot_u16(nx).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_stack(
                            stack[top].inner_max_w,
                            stack[top].inner_max_h,
                            &tree.nodes[nx as usize],
                        );
                        stack[sp] = MeasureFrame {
                            slot: nx,
                            constraints: ch_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else {
                        let pad_w = tree.nodes[slot as usize].props.padding.horizontal();
                        let pad_h = tree.nodes[slot as usize].props.padding.vertical();
                        let width_prop = tree.nodes[slot as usize].props.width;
                        let height_prop = tree.nodes[slot as usize].props.height;
                        let w = resolve_axis_size(
                            width_prop,
                            stack[top].non_fill_used.saturating_add(pad_w),
                            c.max_w,
                        );
                        let h = resolve_axis_size(
                            height_prop,
                            stack[top].cross_max.saturating_add(pad_h),
                            c.max_h,
                        );
                        tree.nodes[slot as usize].measured_size = c.clamp_size(Size::new(w, h));
                        sp -= 1;
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    measured_count
}

fn find_next_fill_child(
    tree: &RetainedTree,
    parent_kind: NodeKind,
    first: SlotLink,
) -> Option<u16> {
    let mut ch = first_visible_child(tree, first);
    while let Some(idx) = ch {
        let cn = &tree.nodes[idx as usize];
        if is_main_fill(parent_kind, cn.props.width, cn.props.height).is_some() {
            return Some(idx);
        }
        ch = next_visible_sibling(tree, idx);
    }
    None
}

fn distribute_base_sum(
    tree: &RetainedTree,
    parent_kind: NodeKind,
    first_child: SlotLink,
    remaining: u32,
    w_total: u32,
) -> u32 {
    let mut sum = 0u32;
    let mut ch = first_visible_child(tree, first_child);
    while let Some(idx) = ch {
        let cn = &tree.nodes[idx as usize];
        if let Some(w) = is_main_fill(parent_kind, cn.props.width, cn.props.height) {
            sum += ((remaining as u64 * w as u64) / w_total as u64) as u32;
        }
        ch = next_visible_sibling(tree, idx);
    }
    sum
}

#[allow(clippy::too_many_arguments)]
fn child_constraints_flex(
    parent_kind: NodeKind,
    parent_align: Align,
    parent_w: Length,
    parent_h: Length,
    inner_max_w: u32,
    inner_max_h: u32,
    child: &crate::node::RetainedNode,
    fill_share: Option<u32>,
) -> Constraints {
    if parent_kind == NodeKind::Row {
        let (min_w, max_w) = match fill_share {
            Some(s) => (s, s),
            None => (0, inner_max_w),
        };
        let parent_cross_fixed = !matches!(parent_h, Length::Content);
        let stretch_cross =
            parent_align == Align::Stretch || matches!(child.props.height, Length::Fill(_));
        let (min_h, max_h) = if stretch_cross && parent_cross_fixed {
            (inner_max_h, inner_max_h)
        } else {
            (0, inner_max_h)
        };
        Constraints {
            min_w,
            max_w,
            min_h,
            max_h,
        }
    } else {
        let (min_h, max_h) = match fill_share {
            Some(s) => (s, s),
            None => (0, inner_max_h),
        };
        let parent_cross_fixed = !matches!(parent_w, Length::Content);
        let stretch_cross =
            parent_align == Align::Stretch || matches!(child.props.width, Length::Fill(_));
        let (min_w, max_w) = if stretch_cross && parent_cross_fixed {
            (inner_max_w, inner_max_w)
        } else {
            (0, inner_max_w)
        };
        Constraints {
            min_w,
            max_w,
            min_h,
            max_h,
        }
    }
}

fn child_constraints_scroll(inner_max_w: u32, child: &crate::node::RetainedNode) -> Constraints {
    let min_w = if matches!(child.props.width, Length::Fill(_)) {
        inner_max_w
    } else {
        0
    };
    Constraints {
        min_w,
        max_w: inner_max_w,
        min_h: 0,
        max_h: u32::MAX / 4,
    }
}

fn child_constraints_stack(
    inner_max_w: u32,
    inner_max_h: u32,
    child: &crate::node::RetainedNode,
) -> Constraints {
    Constraints {
        min_w: if matches!(child.props.width, Length::Fill(_)) {
            inner_max_w
        } else {
            0
        },
        max_w: inner_max_w,
        min_h: if matches!(child.props.height, Length::Fill(_)) {
            inner_max_h
        } else {
            0
        },
        max_h: inner_max_h,
    }
}

fn finalize_flex_measure(tree: &mut RetainedTree, slot: u16, frame: &MeasureFrame, c: Constraints) {
    let n = &tree.nodes[slot as usize];
    let pad_w = n.props.padding.horizontal();
    let pad_h = n.props.padding.vertical();
    let gap_total = (frame.vis_count.saturating_sub(1) as u32) * (n.props.gap as u32);
    let main_content = frame
        .non_fill_used
        .saturating_add(frame.fill_used)
        .saturating_add(gap_total);

    let (w, h) = if n.kind == NodeKind::Row {
        (
            resolve_axis_size(n.props.width, main_content.saturating_add(pad_w), c.max_w),
            resolve_axis_size(
                n.props.height,
                frame.cross_max.saturating_add(pad_h),
                c.max_h,
            ),
        )
    } else {
        (
            resolve_axis_size(
                n.props.width,
                frame.cross_max.saturating_add(pad_w),
                c.max_w,
            ),
            resolve_axis_size(n.props.height, main_content.saturating_add(pad_h), c.max_h),
        )
    };
    tree.nodes[slot as usize].measured_size = c.clamp_size(Size::new(w, h));
}

#[derive(Copy, Clone)]
struct ArrangeDepthFrame {
    slot: u16,
    phase: u8,
    is_modal: bool,
    next_child: SlotLink,
    rect_or_content: Rect,
    clip: Rect,
    cursor: i64,
    remaining: u32,
    w_total: u32,
    rem_left: u32,
}

impl ArrangeDepthFrame {
    const EMPTY: Self = Self {
        slot: 0,
        phase: 0,
        is_modal: false,
        next_child: SlotLink::NONE,
        rect_or_content: Rect::ZERO,
        clip: Rect::ZERO,
        cursor: 0,
        remaining: 0,
        w_total: 1,
        rem_left: 0,
    };
}

#[inline(never)]
fn arrange_iterative(
    tree: &mut RetainedTree,
    start_slot: u16,
    start_rect: Rect,
    start_clip: Rect,
    screen: Rect,
) {
    let mut stack = [ArrangeDepthFrame::EMPTY; MAX_DEPTH + 2];
    let mut sp = 0usize;

    stack[sp] = ArrangeDepthFrame {
        slot: start_slot,
        phase: 0,
        rect_or_content: start_rect,
        clip: start_clip,
        ..ArrangeDepthFrame::EMPTY
    };
    sp += 1;

    while sp > 0 {
        let top = sp - 1;
        let frame = stack[top];
        let slot = frame.slot;

        if frame.phase == 0 {
            let occupied = tree.nodes[slot as usize].occupied;
            let visible = tree.nodes[slot as usize].props.visible();
            if !occupied || !visible {
                let n = &mut tree.nodes[slot as usize];
                n.layout_rect = Rect::ZERO;
                n.ink_rect = Rect::ZERO;
                n.clip_rect = Rect::ZERO;
                n.visible_rect = Rect::ZERO;
                sp -= 1;
                continue;
            }

            let kind = tree.nodes[slot as usize].kind;
            let rect = frame.rect_or_content;
            let own_clip = match kind {
                NodeKind::Root | NodeKind::Modal => screen,
                NodeKind::ScrollY => frame.clip.intersect(rect),
                _ => frame.clip,
            };

            let mut ink = rect;
            let padding = tree.nodes[slot as usize].props.padding;
            let align_cross = tree.nodes[slot as usize].props.align_cross;
            let focusable = tree.nodes[slot as usize].props.focusable();
            let border_width = tree.nodes[slot as usize].props.border_width as u32;
            let text_cache = tree.nodes[slot as usize].text_cache;
            let first_child = tree.nodes[slot as usize].first_child;
            let gap = tree.nodes[slot as usize].props.gap;

            if kind == NodeKind::Label && text_cache.valid {
                let inner = rect.inset(
                    padding.left as u32,
                    padding.top as u32,
                    padding.right as u32,
                    padding.bottom as u32,
                );
                let mut text_y = inner.y;
                if align_cross == Align::Center && inner.h > text_cache.size.height {
                    text_y += ((inner.h - text_cache.size.height) / 2) as i32;
                } else if align_cross == Align::End && inner.h > text_cache.size.height {
                    text_y += (inner.h - text_cache.size.height) as i32;
                }
                ink = ink.union(text_cache.rel_ink.translate(inner.x, text_y));
            }
            if focusable {
                ink = ink.expand(2);
            }

            {
                let n = &mut tree.nodes[slot as usize];
                n.layout_rect = rect;
                n.ink_rect = ink;
                n.clip_rect = own_clip;
                n.visible_rect = ink.intersect(own_clip);
            }

            if !kind.is_container() {
                sp -= 1;
                continue;
            }

            let child_clip = match kind {
                NodeKind::Root | NodeKind::Modal => screen,
                NodeKind::ScrollY => own_clip,
                _ => own_clip.intersect(rect.inset(
                    border_width,
                    border_width,
                    border_width,
                    border_width,
                )),
            };
            let content = rect.inset(
                padding.left as u32,
                padding.top as u32,
                padding.right as u32,
                padding.bottom as u32,
            );

            match kind {
                NodeKind::Row | NodeKind::Column => {
                    let is_row = kind == NodeKind::Row;
                    let mut vis_count = 0u16;
                    let mut non_fill_sum = 0u32;
                    let mut total_weight = 0u32;

                    let mut ch = first_visible_child(tree, first_child);
                    while let Some(ch_idx) = ch {
                        vis_count += 1;
                        let cn = &tree.nodes[ch_idx as usize];
                        if let Some(w) = is_main_fill(kind, cn.props.width, cn.props.height) {
                            total_weight += w as u32;
                        } else {
                            non_fill_sum = non_fill_sum.saturating_add(if is_row {
                                cn.measured_size.width
                            } else {
                                cn.measured_size.height
                            });
                        }
                        ch = next_visible_sibling(tree, ch_idx);
                    }

                    let gap_total = (vis_count.saturating_sub(1) as u32) * (gap as u32);
                    let avail_main = if is_row { content.w } else { content.h };
                    let remaining =
                        avail_main.saturating_sub(non_fill_sum.saturating_add(gap_total));
                    let w_total = total_weight.max(1);
                    let rem_left = if total_weight > 0 {
                        remaining - distribute_base_sum(tree, kind, first_child, remaining, w_total)
                    } else {
                        0
                    };

                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                    stack[top].cursor = 0;
                    stack[top].remaining = remaining;
                    stack[top].w_total = w_total;
                    stack[top].rem_left = rem_left;
                }
                NodeKind::ScrollY => {
                    let mut total_h = padding.vertical();
                    let mut vis_count = 0u16;
                    let mut ch = first_visible_child(tree, first_child);
                    while let Some(ch_idx) = ch {
                        vis_count += 1;
                        total_h = total_h
                            .saturating_add(tree.nodes[ch_idx as usize].measured_size.height);
                        ch = next_visible_sibling(tree, ch_idx);
                    }
                    total_h =
                        total_h.saturating_add((vis_count.saturating_sub(1) as u32) * (gap as u32));
                    tree.nodes[slot as usize].content_height = total_h;
                    let max_scroll = total_h.saturating_sub(rect.h) as i32;
                    let scroll_y = tree.nodes[slot as usize].scroll_y.clamp(0, max_scroll);
                    tree.nodes[slot as usize].scroll_y = scroll_y;

                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                    stack[top].cursor = (content.y as i64) - (scroll_y as i64);
                }
                NodeKind::Root | NodeKind::Stack | NodeKind::Modal => {
                    stack[top].phase = 1;
                    stack[top].is_modal = kind == NodeKind::Modal;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                }
                _ => {
                    sp -= 1;
                }
            }
        } else {
            let Some(ch) = frame.next_child.get_u16() else {
                sp -= 1;
                continue;
            };
            stack[top].next_child = next_visible_sibling(tree, ch)
                .and_then(SlotLink::from_slot_u16)
                .unwrap_or(SlotLink::NONE);

            let kind = tree.nodes[slot as usize].kind;
            let align_cross = tree.nodes[slot as usize].props.align_cross;
            let gap = tree.nodes[slot as usize].props.gap as i64;
            let content = frame.rect_or_content;
            let child_clip = frame.clip;

            let child_rect = match kind {
                NodeKind::Row | NodeKind::Column => {
                    let is_row = kind == NodeKind::Row;
                    let cn = &tree.nodes[ch as usize];
                    let main_sz = if let Some(w) =
                        is_main_fill(kind, cn.props.width, cn.props.height)
                    {
                        let extra = if stack[top].rem_left > 0 {
                            stack[top].rem_left -= 1;
                            1
                        } else {
                            0
                        };
                        ((frame.remaining as u64 * w as u64) / frame.w_total as u64) as u32 + extra
                    } else if is_row {
                        cn.measured_size.width
                    } else {
                        cn.measured_size.height
                    };

                    let avail_cross = if is_row { content.h } else { content.w };
                    let child_cross_len = if is_row {
                        cn.props.height
                    } else {
                        cn.props.width
                    };
                    let mut cross_sz = if is_row {
                        cn.measured_size.height
                    } else {
                        cn.measured_size.width
                    };
                    if align_cross == Align::Stretch || matches!(child_cross_len, Length::Fill(_)) {
                        cross_sz = avail_cross;
                    }
                    let cross_off: i64 = match align_cross {
                        Align::Start | Align::Stretch => 0,
                        Align::Center => ((avail_cross as i64) - (cross_sz as i64)) / 2,
                        Align::End => (avail_cross as i64) - (cross_sz as i64),
                    }
                    .max(0);

                    let rect = if is_row {
                        Rect::new(
                            (content.x as i64 + frame.cursor)
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            (content.y as i64 + cross_off).clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            main_sz,
                            cross_sz,
                        )
                    } else {
                        Rect::new(
                            (content.x as i64 + cross_off).clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            (content.y as i64 + frame.cursor)
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            cross_sz,
                            main_sz,
                        )
                    };
                    stack[top].cursor += main_sz as i64 + gap;
                    rect
                }
                NodeKind::ScrollY => {
                    let cn = &tree.nodes[ch as usize];
                    let cw = if matches!(cn.props.width, Length::Fill(_)) {
                        content.w
                    } else {
                        cn.measured_size.width.min(content.w)
                    };
                    let ch_h = cn.measured_size.height;
                    let cx = match align_cross {
                        Align::Center => content.x + ((content.w.saturating_sub(cw)) / 2) as i32,
                        Align::End => content.x + content.w.saturating_sub(cw) as i32,
                        _ => content.x,
                    };
                    let rect = Rect::new(
                        cx,
                        frame.cursor.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                        cw,
                        ch_h,
                    );
                    stack[top].cursor += ch_h as i64 + gap;
                    rect
                }
                _ => {
                    let cn = &tree.nodes[ch as usize];
                    let cw = if matches!(cn.props.width, Length::Fill(_)) {
                        content.w
                    } else {
                        cn.measured_size.width
                    };
                    let ch_h = if matches!(cn.props.height, Length::Fill(_)) {
                        content.h
                    } else {
                        cn.measured_size.height
                    };
                    let (cx, cy) = if frame.is_modal {
                        (
                            content.x + ((content.w.saturating_sub(cw)) / 2) as i32,
                            content.y + ((content.h.saturating_sub(ch_h)) / 2) as i32,
                        )
                    } else {
                        let ax = match align_cross {
                            Align::Center => {
                                content.x + ((content.w.saturating_sub(cw)) / 2) as i32
                            }
                            Align::End => content.x + content.w.saturating_sub(cw) as i32,
                            _ => content.x,
                        };
                        (ax, content.y)
                    };
                    Rect::new(cx, cy, cw, ch_h)
                }
            };

            if sp < stack.len() {
                stack[sp] = ArrangeDepthFrame {
                    slot: ch,
                    phase: 0,
                    rect_or_content: child_rect,
                    clip: child_clip,
                    ..ArrangeDepthFrame::EMPTY
                };
                sp += 1;
            }
        }
    }
}

pub fn ensure_visible(tree: &mut RetainedTree, target_slot: u16, screen: Rect) -> Option<u16> {
    if !tree.nodes[target_slot as usize].occupied {
        return None;
    }

    let mut anc = tree.nodes[target_slot as usize].parent;
    let mut scroll_slot: Option<u16> = None;
    while let Some(anc_idx) = anc.get_u16() {
        if tree.nodes[anc_idx as usize].kind == NodeKind::ScrollY {
            scroll_slot = Some(anc_idx);
            break;
        }
        anc = tree.nodes[anc_idx as usize].parent;
    }
    let scroll_idx = scroll_slot?;

    let sv = &tree.nodes[scroll_idx as usize];
    let vp = sv.layout_rect.inset(
        sv.props.padding.left as u32,
        sv.props.padding.top as u32,
        sv.props.padding.right as u32,
        sv.props.padding.bottom as u32,
    );
    let target_rect = tree.nodes[target_slot as usize].ink_rect;
    let mut delta = 0i32;

    if target_rect.h >= vp.h {
        delta = target_rect.y - vp.y;
    } else if target_rect.top64() < vp.top64() {
        delta = (target_rect.top64() - vp.top64()) as i32;
    } else if target_rect.bottom64() > vp.bottom64() {
        delta = (target_rect.bottom64() - vp.bottom64()) as i32;
    }

    if delta == 0 {
        return None;
    }

    let max_scroll = sv.content_height.saturating_sub(sv.layout_rect.h) as i32;
    let old_scroll = sv.scroll_y;
    let new_scroll = old_scroll.saturating_add(delta).clamp(0, max_scroll);
    if new_scroll == old_scroll {
        return None;
    }

    tree.nodes[scroll_idx as usize].scroll_y = new_scroll;
    relayout_scroll_subtree(tree, scroll_idx, screen);
    Some(scroll_idx)
}

pub fn compute_text_origin(node: &RetainedNode) -> Point {
    let rect = node.layout_rect;
    let inner = rect.inset(
        node.props.padding.left as u32,
        node.props.padding.top as u32,
        node.props.padding.right as u32,
        node.props.padding.bottom as u32,
    );
    let mut text_y = inner.y;
    if node.text_cache.valid {
        let text_size = node.text_cache.size;
        if node.props.align_cross == Align::Center && inner.h > text_size.height {
            text_y += ((inner.h - text_size.height) / 2) as i32;
        } else if node.props.align_cross == Align::End && inner.h > text_size.height {
            text_y += (inner.h - text_size.height) as i32;
        }
    }
    Point::new(inner.x, text_y)
}

pub fn refresh_node_ink_rect(node: &mut RetainedNode) {
    let mut ink = node.layout_rect;
    if node.kind == NodeKind::Label && node.text_cache.valid {
        let origin = compute_text_origin(node);
        ink = ink.union(node.text_cache.rel_ink.translate(origin.x, origin.y));
    }
    if node.props.focusable() {
        ink = ink.expand(2);
    }
    node.ink_rect = ink;
    node.visible_rect = ink.intersect(node.clip_rect);
}
