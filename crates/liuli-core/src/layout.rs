mod tabs;

use crate::capacity::MAX_DEPTH;
use crate::geometry::{Point, Rect, Size};
use crate::node::{RetainedNode, RetainedTree, TextCache};
use crate::part_style::IconPosition;
use crate::view::{
    Align, BUTTON_ICON_GAP, BUTTON_ICON_SIZE, FontId, IconKind, Justify, Length, NodeKind,
    SlotLink, StackAlign, TextAlign,
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

    fn custom_size_styled(
        &self,
        key: crate::view::Key,
        font: FontId,
        available: Size,
        _metrics: crate::part_style::PartMetrics,
    ) -> Size {
        self.custom_size(key, font, available)
    }

    fn layout_custom(
        &self,
        _key: crate::view::Key,
        _child_sizes: &[Size],
        _available: Size,
        _out_rects: &mut [Rect],
    ) -> Size {
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
        arrange_iterative(tree, root, screen, screen, screen, Some(measurer));
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
    arrange_iterative(tree, scroll_slot, rect, clip, screen, None);
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
    let min_width = n.props.min.max(0) as u32;

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
            tree.text_geometry.set(
                slot as usize,
                TextCache {
                    wrap_width: wrap_w,
                    size: tm.size,
                    rel_ink: tm.ink,
                    lines: tm.lines,
                    valid: true,
                    align: measure_align,
                },
            );

            let w = resolve_axis_size(width_prop, tm.size.width.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(height_prop, tm.size.height.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Button | NodeKind::Tab => {
            let part = tree.part_styles.get(usize::from(slot));
            let avail_w = resolve_axis_max(width_prop, c.max_w);
            let (icon_w, icon_gap, icon_pos) = if icon != IconKind::None {
                let sz = u32::from(part.icon_size().unwrap_or(BUTTON_ICON_SIZE as u8));
                let gp = u32::from(part.icon_gap().unwrap_or(BUTTON_ICON_GAP as u8));
                (sz, gp, part.icon_pos().unwrap_or(IconPosition::Left))
            } else {
                (0, 0, IconPosition::Left)
            };
            let horiz_icon = icon != IconKind::None
                && matches!(icon_pos, IconPosition::Left | IconPosition::Right);
            let vert_icon = icon != IconKind::None
                && matches!(icon_pos, IconPosition::Top | IconPosition::Bottom);
            let icon_space = if horiz_icon { icon_w + icon_gap } else { 0 };
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
            tree.text_geometry.set(
                slot as usize,
                TextCache {
                    wrap_width: wrap_w,
                    size: tm.size,
                    rel_ink: tm.ink,
                    lines: tm.lines,
                    valid: true,
                    align: measure_align,
                },
            );

            let mut content_w = if vert_icon {
                tm.size.width.max(icon_w).saturating_add(pad_w)
            } else {
                tm.size.width.saturating_add(pad_w + icon_space)
            };
            if kind == NodeKind::Tab {
                content_w = content_w.max(min_width);
            }
            let content_h = if vert_icon {
                tm.size.height.saturating_add(icon_w + icon_gap + pad_h)
            } else {
                tm.size.height.max(icon_w.max(14)).saturating_add(pad_h)
            };
            let w = resolve_axis_size(width_prop, content_w, c.max_w);
            let h = resolve_axis_size(
                height_prop,
                if kind == NodeKind::Tab {
                    content_h.max(min_width)
                } else {
                    content_h
                },
                c.max_h,
            );
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
            let content = measurer.custom_size_styled(
                n.key,
                font,
                Size::new(c.max_w.saturating_sub(pad_w), c.max_h.saturating_sub(pad_h)),
                tree.part_styles.get(usize::from(slot)).metrics(),
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
        let n_kind = tree.nodes[slot as usize].kind.layout_kind();
        let n_occupied = tree.nodes[slot as usize].occupied;
        let n_visible = tree.nodes[slot as usize].props.visible();

        if !n_occupied || !n_visible {
            tree.nodes[slot as usize].measured_size = Size::new(0, 0);
            sp -= 1;
            continue;
        }

        if stack[top].phase == 0 {
            measured_count += u32::from(crate::capacity::METRICS);
            let n_key = tree.nodes[slot as usize].key;
            let extra = tree.layout_extras.get(n_key);
            let c = if extra.is_zero() {
                c
            } else {
                let adj = apply_extra_to_constraints(c, extra);
                stack[top].constraints = adj;
                adj
            };
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
                } else if n_kind == NodeKind::ScrollX {
                    tree.nodes[slot as usize].content_height = pad_w;
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
                NodeKind::ScrollY | NodeKind::ScrollX => {
                    if let Some(first) = first_visible_child(tree, first_child) {
                        stack[top].phase = 2;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(first).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_scroll_axis(
                            n_kind == NodeKind::ScrollX,
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
                NodeKind::Grid => {
                    if let Some(first) = first_visible_child(tree, first_child) {
                        let cols = u32::from(tree.nodes[slot as usize].props.grid_cols().max(1));
                        let cell_w = grid_cell_width(inner_max_w, u32::from(gap), cols);
                        stack[top].phase = 2;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(first).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_grid(
                            cell_w,
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
                NodeKind::Root
                | NodeKind::Stack
                | NodeKind::Modal
                | NodeKind::Wrap
                | NodeKind::TabBar
                | NodeKind::CustomContainer => {
                    if let Some(first) = first_visible_child(tree, first_child) {
                        stack[top].phase = 2;
                        stack[top].active_child =
                            SlotLink::from_slot_u16(first).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_stack(
                            stack[top].inner_max_w,
                            stack[top].inner_max_h,
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
            let prev_sz = child_outer_size(tree, prev);
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
            let prev_sz = child_outer_size(tree, prev);

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
                NodeKind::ScrollY | NodeKind::ScrollX => {
                    let is_x = n_kind == NodeKind::ScrollX;
                    if is_x {
                        stack[top].non_fill_used =
                            stack[top].non_fill_used.saturating_add(prev_sz.width);
                        stack[top].cross_max = stack[top].cross_max.max(prev_sz.height);
                    } else {
                        stack[top].non_fill_used =
                            stack[top].non_fill_used.saturating_add(prev_sz.height);
                        stack[top].cross_max = stack[top].cross_max.max(prev_sz.width);
                    }
                    let next = next_visible_sibling(tree, prev);
                    if let Some(nx) = next {
                        stack[top].active_child =
                            SlotLink::from_slot_u16(nx).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_scroll_axis(
                            is_x,
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
                        let gap = tree.nodes[slot as usize].props.gap;
                        let width_prop = tree.nodes[slot as usize].props.width;
                        let height_prop = tree.nodes[slot as usize].props.height;
                        let gap_total =
                            (stack[top].vis_count.saturating_sub(1) as u32) * (gap as u32);
                        let content_main = stack[top]
                            .non_fill_used
                            .saturating_add(gap_total)
                            .saturating_add(if is_x { pad_w } else { pad_h });
                        let (w, h) = if is_x {
                            (
                                resolve_axis_size(width_prop, content_main, c.max_w),
                                resolve_axis_size(
                                    height_prop,
                                    stack[top].cross_max.saturating_add(pad_h),
                                    c.max_h,
                                ),
                            )
                        } else {
                            (
                                resolve_axis_size(
                                    width_prop,
                                    stack[top].cross_max.saturating_add(pad_w),
                                    c.max_w,
                                ),
                                resolve_axis_size(height_prop, content_main, c.max_h),
                            )
                        };
                        let clamped = c.clamp_size(Size::new(w, h));
                        tree.nodes[slot as usize].content_height = content_main;
                        tree.nodes[slot as usize].measured_size = clamped;
                        // Arrangement clamps against the final allocated viewport.
                        sp -= 1;
                    }
                }
                NodeKind::Grid => {
                    let next = next_visible_sibling(tree, prev);
                    if let Some(nx) = next {
                        let gap = u32::from(tree.nodes[slot as usize].props.gap);
                        let cols = u32::from(tree.nodes[slot as usize].props.grid_cols().max(1));
                        let cell_w = grid_cell_width(stack[top].inner_max_w, gap, cols);
                        stack[top].active_child =
                            SlotLink::from_slot_u16(nx).unwrap_or(SlotLink::NONE);
                        let ch_c = child_constraints_grid(
                            cell_w,
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
                        finalize_grid_measure(tree, slot, c, stack[top].inner_max_w);
                        sp -= 1;
                    }
                }
                NodeKind::Root
                | NodeKind::Stack
                | NodeKind::Modal
                | NodeKind::Wrap
                | NodeKind::TabBar
                | NodeKind::CustomContainer => {
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
                        match n_kind {
                            NodeKind::TabBar => {
                                let plan =
                                    tabs::BarLayout::measure(tree, slot, stack[top].inner_max_w);
                                let node = &mut tree.nodes[slot as usize];
                                node.measured_size = c.clamp_size(Size::new(
                                    resolve_axis_size(
                                        node.props.width,
                                        plan.width + node.props.padding.horizontal(),
                                        c.max_w,
                                    ),
                                    resolve_axis_size(
                                        node.props.height,
                                        plan.height + node.props.padding.vertical(),
                                        c.max_h,
                                    ),
                                ));
                            }
                            NodeKind::Wrap => {
                                finalize_wrap_measure(tree, slot, c, stack[top].inner_max_w);
                            }
                            NodeKind::CustomContainer => {
                                finalize_custom_measure(
                                    tree,
                                    slot,
                                    c,
                                    Size::new(stack[top].inner_max_w, stack[top].inner_max_h),
                                    measurer,
                                );
                            }
                            _ => {
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
                                tree.nodes[slot as usize].measured_size =
                                    c.clamp_size(Size::new(w, h));
                            }
                        }
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

fn apply_extra_to_constraints(
    c: Constraints,
    extra: crate::part_style::LayoutExtra,
) -> Constraints {
    let mh = extra.margin.horizontal();
    let mv = extra.margin.vertical();
    let mut min_w = c.min_w.saturating_sub(mh);
    let mut max_w = c.max_w.saturating_sub(mh);
    let mut min_h = c.min_h.saturating_sub(mv);
    let mut max_h = c.max_h.saturating_sub(mv);
    if extra.max_w > 0 {
        max_w = max_w.min(u32::from(extra.max_w));
    }
    if extra.max_h > 0 {
        max_h = max_h.min(u32::from(extra.max_h));
    }
    if extra.min_w > 0 {
        min_w = min_w.max(u32::from(extra.min_w)).min(max_w);
    } else {
        min_w = min_w.min(max_w);
    }
    if extra.min_h > 0 {
        min_h = min_h.max(u32::from(extra.min_h)).min(max_h);
    } else {
        min_h = min_h.min(max_h);
    }
    Constraints {
        min_w,
        max_w,
        min_h,
        max_h,
    }
}

#[inline]
fn child_outer_size(tree: &RetainedTree, slot: u16) -> Size {
    let n = &tree.nodes[slot as usize];
    let m = tree.layout_extras.get(n.key).margin;
    Size::new(
        n.measured_size.width.saturating_add(m.horizontal()),
        n.measured_size.height.saturating_add(m.vertical()),
    )
}

#[inline]
fn grid_cell_width(inner_max_w: u32, gap: u32, cols: u32) -> u32 {
    let c = cols.max(1);
    let gap_total = gap.saturating_mul(c.saturating_sub(1));
    inner_max_w.saturating_sub(gap_total) / c
}

fn child_constraints_scroll_axis(
    horizontal: bool,
    inner_max_w: u32,
    inner_max_h: u32,
    child: &crate::node::RetainedNode,
) -> Constraints {
    if horizontal {
        let stretch = matches!(child.props.height, Length::Fill(_));
        Constraints {
            min_w: 0,
            max_w: u32::MAX / 4,
            min_h: if stretch { inner_max_h } else { 0 },
            max_h: inner_max_h,
        }
    } else {
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
}

fn child_constraints_grid(
    cell_w: u32,
    inner_max_h: u32,
    child: &crate::node::RetainedNode,
) -> Constraints {
    let min_w = if matches!(child.props.width, Length::Fill(_)) {
        cell_w
    } else {
        0
    };
    Constraints {
        min_w,
        max_w: cell_w,
        min_h: 0,
        max_h: inner_max_h,
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

use crate::capacity::MAX_CUSTOM_CHILDREN;

#[inline(never)]
fn finalize_wrap_measure(tree: &mut RetainedTree, slot: u16, c: Constraints, inner_max_w: u32) {
    let n = &tree.nodes[slot as usize];
    let gap_h = u32::from(n.props.gap);
    let row_gap = if n.props.line_spacing > 0 {
        n.props.line_spacing as u32
    } else {
        gap_h
    };
    let pad_w = n.props.padding.horizontal();
    let pad_h = n.props.padding.vertical();
    let width_prop = n.props.width;
    let height_prop = n.props.height;
    let mut row_w = 0u32;
    let mut row_h = 0u32;
    let mut max_row_w = 0u32;
    let mut total_h = 0u32;
    let mut ch = first_visible_child(tree, n.first_child);
    while let Some(idx) = ch {
        let cs = child_outer_size(tree, idx);
        if row_w > 0 && row_w.saturating_add(gap_h).saturating_add(cs.width) > inner_max_w {
            max_row_w = max_row_w.max(row_w);
            total_h = total_h.saturating_add(row_h).saturating_add(row_gap);
            row_w = cs.width;
            row_h = cs.height;
        } else {
            row_w = if row_w > 0 {
                row_w.saturating_add(gap_h).saturating_add(cs.width)
            } else {
                cs.width
            };
            row_h = row_h.max(cs.height);
        }
        ch = next_visible_sibling(tree, idx);
    }
    max_row_w = max_row_w.max(row_w);
    total_h = total_h.saturating_add(row_h);
    let w = resolve_axis_size(width_prop, max_row_w.saturating_add(pad_w), c.max_w);
    let h = resolve_axis_size(height_prop, total_h.saturating_add(pad_h), c.max_h);
    tree.nodes[slot as usize].measured_size = c.clamp_size(Size::new(w, h));
}

#[inline(never)]
fn finalize_grid_measure(tree: &mut RetainedTree, slot: u16, c: Constraints, inner_max_w: u32) {
    let n = &tree.nodes[slot as usize];
    let cols = usize::from(n.props.grid_cols().max(1));
    let col_gap = u32::from(n.props.gap);
    let row_gap = if n.props.line_spacing > 0 {
        n.props.line_spacing as u32
    } else {
        col_gap
    };
    let pad_w = n.props.padding.horizontal();
    let pad_h = n.props.padding.vertical();
    let width_prop = n.props.width;
    let height_prop = n.props.height;
    let cell_w = grid_cell_width(inner_max_w, col_gap, cols as u32);

    let mut col = 0usize;
    let mut vis_count = 0usize;
    let mut row_h = 0u32;
    let mut total_h = 0u32;
    let mut rows = 0u32;
    let mut ch = first_visible_child(tree, n.first_child);
    while let Some(idx) = ch {
        let cs = child_outer_size(tree, idx);
        if col == 0 {
            if rows > 0 {
                total_h = total_h.saturating_add(row_gap);
            }
            rows += 1;
            row_h = 0;
        }
        row_h = row_h.max(cs.height);
        col += 1;
        vis_count += 1;
        if col == cols {
            total_h = total_h.saturating_add(row_h);
            row_h = 0;
            col = 0;
        }
        ch = next_visible_sibling(tree, idx);
    }
    if col > 0 {
        total_h = total_h.saturating_add(row_h);
    }
    let used_cols = vis_count.min(cols) as u32;
    let content_w = if used_cols == 0 {
        0
    } else {
        used_cols
            .saturating_mul(cell_w)
            .saturating_add(col_gap.saturating_mul(used_cols.saturating_sub(1)))
    };
    let w = resolve_axis_size(width_prop, content_w.saturating_add(pad_w), c.max_w);
    let h = resolve_axis_size(height_prop, total_h.saturating_add(pad_h), c.max_h);
    tree.nodes[slot as usize].measured_size = c.clamp_size(Size::new(w, h));
}

#[inline(never)]
fn finalize_custom_measure(
    tree: &mut RetainedTree,
    slot: u16,
    c: Constraints,
    avail: Size,
    measurer: &dyn TextMeasurer,
) {
    let mut child_sizes = [Size::zero(); MAX_CUSTOM_CHILDREN];
    let mut out_rects = [Rect::ZERO; MAX_CUSTOM_CHILDREN];
    let mut count = 0usize;
    let first_child = tree.nodes[slot as usize].first_child;
    let key = tree.nodes[slot as usize].key;
    let pad_w = tree.nodes[slot as usize].props.padding.horizontal();
    let pad_h = tree.nodes[slot as usize].props.padding.vertical();
    let width_prop = tree.nodes[slot as usize].props.width;
    let height_prop = tree.nodes[slot as usize].props.height;
    let mut ch = first_visible_child(tree, first_child);
    while let Some(idx) = ch {
        if count < MAX_CUSTOM_CHILDREN {
            child_sizes[count] = tree.nodes[idx as usize].measured_size;
            count += 1;
        }
        ch = next_visible_sibling(tree, idx);
    }
    let content_sz =
        measurer.layout_custom(key, &child_sizes[..count], avail, &mut out_rects[..count]);
    let w = resolve_axis_size(width_prop, content_sz.width.saturating_add(pad_w), c.max_w);
    let h = resolve_axis_size(
        height_prop,
        content_sz.height.saturating_add(pad_h),
        c.max_h,
    );
    tree.nodes[slot as usize].measured_size = c.clamp_size(Size::new(w, h));
}

#[inline(never)]
fn arrange_custom_children(
    tree: &mut RetainedTree,
    slot: u16,
    prev_rect: Rect,
    content: Rect,
    measurer: Option<&dyn TextMeasurer>,
) {
    let first_child = tree.nodes[slot as usize].first_child;
    let Some(m) = measurer else {
        let padding = tree.nodes[slot as usize].props.padding;
        let dx = content.x - prev_rect.x - padding.left as i32;
        let dy = content.y - prev_rect.y - padding.top as i32;
        let mut ch = first_visible_child(tree, first_child);
        while let Some(idx) = ch {
            tree.nodes[idx as usize].ink_rect =
                tree.nodes[idx as usize].layout_rect.translate(dx, dy);
            ch = next_visible_sibling(tree, idx);
        }
        return;
    };
    let mut child_sizes = [Size::zero(); MAX_CUSTOM_CHILDREN];
    let mut out_rects = [Rect::ZERO; MAX_CUSTOM_CHILDREN];
    let mut child_slots = [0u16; MAX_CUSTOM_CHILDREN];
    let mut count = 0usize;
    let key = tree.nodes[slot as usize].key;
    let mut ch = first_visible_child(tree, first_child);
    while let Some(idx) = ch {
        if count < MAX_CUSTOM_CHILDREN {
            child_sizes[count] = tree.nodes[idx as usize].measured_size;
            child_slots[count] = idx;
            count += 1;
        }
        ch = next_visible_sibling(tree, idx);
    }
    let _ = m.layout_custom(
        key,
        &child_sizes[..count],
        content.size(),
        &mut out_rects[..count],
    );
    for i in 0..count {
        tree.nodes[child_slots[i] as usize].ink_rect = out_rects[i].translate(content.x, content.y);
    }
}

fn stack_child_origin(
    content: Rect,
    cw: u32,
    ch: u32,
    is_modal: bool,
    parent_align: Align,
    child_align: StackAlign,
) -> (i32, i32) {
    if is_modal {
        return (
            content.x + ((content.w.saturating_sub(cw)) / 2) as i32,
            content.y + ((content.h.saturating_sub(ch)) / 2) as i32,
        );
    }
    if child_align == StackAlign::TopStart {
        let ax = match parent_align {
            Align::Center => content.x + ((content.w.saturating_sub(cw)) / 2) as i32,
            Align::End => content.x + content.w.saturating_sub(cw) as i32,
            _ => content.x,
        };
        (ax, content.y)
    } else {
        let x = match child_align {
            StackAlign::TopStart | StackAlign::CenterStart | StackAlign::BottomStart => content.x,
            StackAlign::TopCenter | StackAlign::Center | StackAlign::BottomCenter => {
                content.x + ((content.w as i32 - cw as i32) / 2)
            }
            StackAlign::TopEnd | StackAlign::CenterEnd | StackAlign::BottomEnd => {
                content.x + (content.w as i32 - cw as i32)
            }
        };
        let y = match child_align {
            StackAlign::TopStart | StackAlign::TopCenter | StackAlign::TopEnd => content.y,
            StackAlign::CenterStart | StackAlign::Center | StackAlign::CenterEnd => {
                content.y + ((content.h as i32 - ch as i32) / 2)
            }
            StackAlign::BottomStart | StackAlign::BottomCenter | StackAlign::BottomEnd => {
                content.y + (content.h as i32 - ch as i32)
            }
        };
        (x, y)
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
    extra_gap: u16,
    extra_gap_rem: u16,
    grid_cols: u8,
    grid_col: u8,
    grid_row_h: u16,
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
        extra_gap: 0,
        extra_gap_rem: 0,
        grid_cols: 1,
        grid_col: 0,
        grid_row_h: 0,
    };
}

#[inline(never)]
fn arrange_iterative(
    tree: &mut RetainedTree,
    start_slot: u16,
    start_rect: Rect,
    start_clip: Rect,
    screen: Rect,
    measurer: Option<&dyn TextMeasurer>,
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

            let kind = tree.nodes[slot as usize].kind.layout_kind();
            let prev_rect = tree.nodes[slot as usize].layout_rect;
            let rect = frame.rect_or_content;
            let own_clip = match kind {
                NodeKind::Root | NodeKind::Modal => screen,
                NodeKind::ScrollY | NodeKind::ScrollX => frame.clip.intersect(rect),
                _ => frame.clip,
            };

            let padding = tree.nodes[slot as usize].props.padding;
            let border_width = tree.nodes[slot as usize].props.border_width as u32;
            let first_child = tree.nodes[slot as usize].first_child;
            let gap = tree.nodes[slot as usize].props.gap;

            {
                let style = tree.effects.get(tree.nodes[slot as usize].key);
                let part = tree.part_styles.get(slot as usize);
                let n = &mut tree.nodes[slot as usize];
                n.layout_rect = rect;
                n.clip_rect = own_clip;
                refresh_node_ink_rect_with(n, tree.text_geometry.get(slot as usize), part, &style);
            }

            if !kind.is_container() {
                sp -= 1;
                continue;
            }

            let child_clip = match kind {
                NodeKind::Root | NodeKind::Modal => screen,
                NodeKind::ScrollY | NodeKind::ScrollX => own_clip,
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
                            let outer = child_outer_size(tree, ch_idx);
                            non_fill_sum = non_fill_sum.saturating_add(if is_row {
                                outer.width
                            } else {
                                outer.height
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
                    let (justify_cursor, extra_gap, extra_gap_rem) =
                        if total_weight == 0 && vis_count > 0 && remaining > 0 {
                            let cnt = vis_count as u32;
                            match tree.nodes[slot as usize].props.justify() {
                                Justify::Start => (0i64, 0u16, 0u16),
                                Justify::Center => ((remaining / 2) as i64, 0, 0),
                                Justify::End => (remaining as i64, 0, 0),
                                Justify::SpaceBetween => {
                                    if cnt > 1 {
                                        let gaps = cnt - 1;
                                        (
                                            0,
                                            (remaining / gaps).min(u16::MAX as u32) as u16,
                                            (remaining % gaps).min(u16::MAX as u32) as u16,
                                        )
                                    } else {
                                        (0, 0, 0)
                                    }
                                }
                                Justify::SpaceAround => {
                                    let unit = remaining / cnt;
                                    let rem = remaining % cnt;
                                    (
                                        (unit / 2) as i64,
                                        unit.min(u16::MAX as u32) as u16,
                                        rem.min(u16::MAX as u32) as u16,
                                    )
                                }
                                Justify::SpaceEvenly => {
                                    let gaps = cnt + 1;
                                    let unit = remaining / gaps;
                                    let rem = remaining % gaps;
                                    let lead = u32::from(rem > 0);
                                    (
                                        (unit + lead) as i64,
                                        unit.min(u16::MAX as u32) as u16,
                                        rem.saturating_sub(lead).min(u16::MAX as u32) as u16,
                                    )
                                }
                            }
                        } else {
                            (0i64, 0u16, 0u16)
                        };

                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                    stack[top].cursor = justify_cursor;
                    stack[top].remaining = remaining;
                    stack[top].w_total = w_total;
                    stack[top].rem_left = rem_left;
                    stack[top].extra_gap = extra_gap;
                    stack[top].extra_gap_rem = extra_gap_rem;
                }
                NodeKind::ScrollY | NodeKind::ScrollX => {
                    let is_x = kind == NodeKind::ScrollX;
                    let mut total_main = if is_x {
                        padding.horizontal()
                    } else {
                        padding.vertical()
                    };
                    let mut vis_count = 0u16;
                    let mut ch = first_visible_child(tree, first_child);
                    while let Some(ch_idx) = ch {
                        vis_count += 1;
                        let outer = child_outer_size(tree, ch_idx);
                        total_main = total_main.saturating_add(if is_x {
                            outer.width
                        } else {
                            outer.height
                        });
                        ch = next_visible_sibling(tree, ch_idx);
                    }
                    total_main = total_main
                        .saturating_add((vis_count.saturating_sub(1) as u32) * (gap as u32));
                    tree.nodes[slot as usize].content_height = total_main;
                    let max_scroll = tree.nodes[slot as usize].scroll_limit().unwrap_or(0);
                    let scroll_off = tree.nodes[slot as usize].scroll_y.clamp(0, max_scroll);
                    tree.nodes[slot as usize].scroll_y = scroll_off;

                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                    let base_origin = if is_x { content.x } else { content.y };
                    stack[top].cursor = (base_origin as i64) - (scroll_off as i64);
                }
                NodeKind::Grid => {
                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                    stack[top].cursor = 0;
                    stack[top].grid_cols = tree.nodes[slot as usize].props.grid_cols().max(1);
                    stack[top].grid_col = 0;
                    stack[top].grid_row_h = 0;
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
                NodeKind::Wrap => {
                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                    stack[top].cursor = 0;
                    stack[top].remaining = 0;
                    stack[top].w_total = 0;
                }
                NodeKind::TabBar => {
                    let plan = tabs::BarLayout::measure(tree, slot, content.w);
                    plan.arrange(tree, slot, content, child_clip, prev_rect.w != rect.w);
                    stack[top].phase = 1;
                    stack[top].next_child = first_visible_child(tree, first_child)
                        .and_then(SlotLink::from_slot_u16)
                        .unwrap_or(SlotLink::NONE);
                    stack[top].rect_or_content = content;
                    stack[top].clip = child_clip;
                }
                NodeKind::CustomContainer => {
                    arrange_custom_children(tree, slot, prev_rect, content, measurer);
                    stack[top].phase = 1;
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

            let kind = tree.nodes[slot as usize].kind.layout_kind();
            let align_cross = tree.nodes[slot as usize].props.align_cross;
            let gap = tree.nodes[slot as usize].props.gap as i64;
            let content = frame.rect_or_content;
            let child_clip = frame.clip;

            let child_rect = match kind {
                NodeKind::Row | NodeKind::Column => {
                    let is_row = kind == NodeKind::Row;
                    let cn = &tree.nodes[ch as usize];
                    let m = tree.layout_extras.get(cn.key).margin;
                    let (m_main, m_cross) = if is_row {
                        (m.horizontal(), m.vertical())
                    } else {
                        (m.vertical(), m.horizontal())
                    };
                    let (box_main, outer_main) =
                        if let Some(w) = is_main_fill(kind, cn.props.width, cn.props.height) {
                            let extra = if stack[top].rem_left > 0 {
                                stack[top].rem_left -= 1;
                                1
                            } else {
                                0
                            };
                            let total_main =
                                ((frame.remaining as u64 * w as u64) / frame.w_total as u64) as u32
                                    + extra;
                            (total_main.saturating_sub(m_main), total_main)
                        } else {
                            let bm = if is_row {
                                cn.measured_size.width
                            } else {
                                cn.measured_size.height
                            };
                            (bm, bm.saturating_add(m_main))
                        };

                    let avail_cross = if is_row { content.h } else { content.w };
                    let child_cross_len = if is_row {
                        cn.props.height
                    } else {
                        cn.props.width
                    };
                    let (box_cross, outer_cross) = if align_cross == Align::Stretch
                        || matches!(child_cross_len, Length::Fill(_))
                    {
                        (avail_cross.saturating_sub(m_cross), avail_cross)
                    } else {
                        let bc = if is_row {
                            cn.measured_size.height
                        } else {
                            cn.measured_size.width
                        };
                        (bc, bc.saturating_add(m_cross))
                    };
                    let cross_off: i64 = match align_cross {
                        Align::Start | Align::Stretch => 0,
                        Align::Center => ((avail_cross as i64) - (outer_cross as i64)) / 2,
                        Align::End => (avail_cross as i64) - (outer_cross as i64),
                    }
                    .max(0);

                    let rect = if is_row {
                        Rect::new(
                            (content.x as i64 + frame.cursor + i64::from(m.left))
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            (content.y as i64 + cross_off + i64::from(m.top))
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            box_main,
                            box_cross,
                        )
                    } else {
                        Rect::new(
                            (content.x as i64 + cross_off + i64::from(m.left))
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            (content.y as i64 + frame.cursor + i64::from(m.top))
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            box_cross,
                            box_main,
                        )
                    };
                    let mut step_extra = u32::from(frame.extra_gap);
                    if stack[top].extra_gap_rem > 0 {
                        stack[top].extra_gap_rem -= 1;
                        step_extra += 1;
                    }
                    stack[top].cursor += outer_main as i64 + gap + step_extra as i64;
                    rect
                }
                NodeKind::ScrollY => {
                    let cn = &tree.nodes[ch as usize];
                    let m = tree.layout_extras.get(cn.key).margin;
                    let outer_w = if matches!(cn.props.width, Length::Fill(_)) {
                        content.w
                    } else {
                        cn.measured_size
                            .width
                            .saturating_add(m.horizontal())
                            .min(content.w)
                    };
                    let cw = outer_w.saturating_sub(m.horizontal());
                    let ch_h = cn.measured_size.height;
                    let cx = match align_cross {
                        Align::Center => {
                            content.x + ((content.w.saturating_sub(outer_w)) / 2) as i32
                        }
                        Align::End => content.x + content.w.saturating_sub(outer_w) as i32,
                        _ => content.x,
                    };
                    let rect = Rect::new(
                        cx.saturating_add(i32::from(m.left)),
                        (frame.cursor + i64::from(m.top)).clamp(i32::MIN as i64, i32::MAX as i64)
                            as i32,
                        cw,
                        ch_h,
                    );
                    stack[top].cursor += ch_h as i64 + m.vertical() as i64 + gap;
                    rect
                }
                NodeKind::ScrollX => {
                    let cn = &tree.nodes[ch as usize];
                    let m = tree.layout_extras.get(cn.key).margin;
                    let outer_h = if align_cross == Align::Stretch
                        || matches!(cn.props.height, Length::Fill(_))
                    {
                        content.h
                    } else {
                        cn.measured_size
                            .height
                            .saturating_add(m.vertical())
                            .min(content.h)
                    };
                    let ch_h = outer_h.saturating_sub(m.vertical());
                    let cw = cn.measured_size.width;
                    let cy = match align_cross {
                        Align::Center => {
                            content.y + ((content.h.saturating_sub(outer_h)) / 2) as i32
                        }
                        Align::End => content.y + content.h.saturating_sub(outer_h) as i32,
                        _ => content.y,
                    };
                    let rect = Rect::new(
                        (frame.cursor + i64::from(m.left)).clamp(i32::MIN as i64, i32::MAX as i64)
                            as i32,
                        cy.saturating_add(i32::from(m.top)),
                        cw,
                        ch_h,
                    );
                    stack[top].cursor += cw as i64 + m.horizontal() as i64 + gap;
                    rect
                }
                NodeKind::Grid => {
                    let cn = &tree.nodes[ch as usize];
                    let m = tree.layout_extras.get(cn.key).margin;
                    let cols = u32::from(frame.grid_cols.max(1));
                    let col = u32::from(frame.grid_col);
                    let col_gap = gap.max(0) as u32;
                    let row_gap = if tree.nodes[slot as usize].props.line_spacing > 0 {
                        tree.nodes[slot as usize].props.line_spacing as i64
                    } else {
                        col_gap as i64
                    };
                    let avail = content.w.saturating_sub(col_gap.saturating_mul(cols - 1));
                    let base_w = avail / cols;
                    let rem = avail % cols;
                    let track_w = base_w + u32::from(col < rem);
                    let col_x = content
                        .x
                        .saturating_add((col * (base_w + col_gap) + col.min(rem)) as i32);

                    let avail_box_w = track_w.saturating_sub(m.horizontal());
                    let cw = if align_cross == Align::Stretch
                        || matches!(cn.props.width, Length::Fill(_))
                    {
                        avail_box_w
                    } else {
                        cn.measured_size.width.min(avail_box_w)
                    };
                    let align_x = match align_cross {
                        Align::Center => (avail_box_w.saturating_sub(cw) / 2) as i32,
                        Align::End => avail_box_w.saturating_sub(cw) as i32,
                        _ => 0,
                    };
                    let ch_h = cn.measured_size.height;
                    let rect = Rect::new(
                        col_x
                            .saturating_add(i32::from(m.left))
                            .saturating_add(align_x),
                        (content.y as i64 + frame.cursor + i64::from(m.top))
                            .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                        cw,
                        ch_h,
                    );
                    let outer_h = ch_h.saturating_add(m.vertical());
                    let next_row_h = u32::from(frame.grid_row_h).max(outer_h);
                    if col + 1 >= cols {
                        stack[top].cursor += next_row_h as i64 + row_gap;
                        stack[top].grid_col = 0;
                        stack[top].grid_row_h = 0;
                    } else {
                        stack[top].grid_col += 1;
                        stack[top].grid_row_h = next_row_h.min(u32::from(u16::MAX)) as u16;
                    }
                    rect
                }
                NodeKind::Wrap => {
                    let cn = &tree.nodes[ch as usize];
                    let m = tree.layout_extras.get(cn.key).margin;
                    let cw = cn.measured_size.width;
                    let ch_h = cn.measured_size.height;
                    let outer_w = cw.saturating_add(m.horizontal());
                    let outer_h = ch_h.saturating_add(m.vertical());
                    let gap_h = gap.max(0) as u32;
                    let row_gap = if tree.nodes[slot as usize].props.line_spacing > 0 {
                        tree.nodes[slot as usize].props.line_spacing as i64
                    } else {
                        gap.max(0)
                    };
                    let mut rx = frame.remaining;
                    let mut ry = frame.cursor;
                    let mut rh = frame.w_total;
                    if rx > 0 && rx.saturating_add(gap_h).saturating_add(outer_w) > content.w {
                        ry += rh as i64 + row_gap;
                        rx = 0;
                        rh = 0;
                    }
                    let x_off = if rx > 0 { rx.saturating_add(gap_h) } else { 0 };
                    let rect = Rect::new(
                        content
                            .x
                            .saturating_add(x_off as i32)
                            .saturating_add(i32::from(m.left)),
                        (content.y as i64 + ry + i64::from(m.top))
                            .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                        cw,
                        ch_h,
                    );
                    stack[top].remaining = x_off.saturating_add(outer_w);
                    stack[top].cursor = ry;
                    stack[top].w_total = rh.max(outer_h);
                    rect
                }
                NodeKind::CustomContainer | NodeKind::TabBar => tree.nodes[ch as usize].ink_rect,
                _ => {
                    let cn = &tree.nodes[ch as usize];
                    let m = tree.layout_extras.get(cn.key).margin;
                    let outer_w = if matches!(cn.props.width, Length::Fill(_)) {
                        content.w
                    } else {
                        cn.measured_size.width.saturating_add(m.horizontal())
                    };
                    let outer_h = if matches!(cn.props.height, Length::Fill(_)) {
                        content.h
                    } else {
                        cn.measured_size.height.saturating_add(m.vertical())
                    };
                    let cw = outer_w.saturating_sub(m.horizontal());
                    let ch_h = outer_h.saturating_sub(m.vertical());
                    let (cx, cy) = stack_child_origin(
                        content,
                        outer_w,
                        outer_h,
                        frame.is_modal,
                        align_cross,
                        cn.props.stack_align(),
                    );
                    Rect::new(
                        cx.saturating_add(i32::from(m.left)),
                        cy.saturating_add(i32::from(m.top)),
                        cw,
                        ch_h,
                    )
                }
            };

            let child_extra = tree.layout_extras.get(tree.nodes[ch as usize].key);
            let child_rect = child_extra
                .motion
                .arrange(child_extra.fit_slot(child_rect), content);
            let child_rect = if !child_rect.is_empty()
                && (child_extra.offset_x != 0 || child_extra.offset_y != 0)
            {
                child_rect.translate(
                    i32::from(child_extra.offset_x),
                    i32::from(child_extra.offset_y),
                )
            } else {
                child_rect
            };

            if sp < stack.len() {
                stack[sp] = ArrangeDepthFrame {
                    slot: ch,
                    phase: 0,
                    rect_or_content: child_rect,
                    clip: if kind == NodeKind::TabBar {
                        tree.nodes[ch as usize].clip_rect
                    } else {
                        child_clip
                    },
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

    let mut ancestor = tree.nodes[target_slot as usize].parent;
    let n = &tree.nodes[usize::from(target_slot)];
    let mut target = n.ink_rect;
    if n.props.focusable() && n.props.focus_ring().enabled {
        // Reveal the space the ring will occupy after scrolling. Its current clipped
        // footprint can be smaller while the control is entering the viewport.
        let available = n.layout_rect.expand(u32::from(n.props.focus_ring().offset));
        target = target.union(
            n.focus_outline(
                tree.part_styles.get(usize::from(target_slot)),
                &tree.effects.get(n.key),
                available,
            )
            .bounds,
        );
    }
    let mut outermost_changed = None;
    while let Some(slot) = ancestor.get_u16() {
        let sv = &tree.nodes[slot as usize];
        ancestor = sv.parent;
        let Some(limit) = sv.scroll_limit() else {
            continue;
        };
        let viewport = sv.layout_rect;
        let is_x = sv.kind.scrolls_x();
        let offset = sv.scroll_y;
        let next = offset
            .saturating_add(if sv.kind == NodeKind::TabBar {
                tabs::reveal_offset(tree.nodes[target_slot as usize].clip_rect, target)
            } else {
                reveal_delta(sv, target)
            })
            .clamp(0, limit);
        if next != offset {
            tree.nodes[slot as usize].scroll_y = next;
            relayout_scroll_subtree(tree, slot, screen);
            let movement = offset - next;
            target = if is_x {
                target.translate(movement, 0)
            } else {
                target.translate(0, movement)
            };
            outermost_changed = Some(slot);
        }
        target = target.intersect(viewport);
        if target.is_empty() {
            break;
        }
    }
    outermost_changed
}

fn reveal_delta(sv: &crate::node::RetainedNode, target_rect: Rect) -> i32 {
    let is_x = sv.kind.scrolls_x();
    let vp = sv.layout_rect.inset(
        sv.props.padding.left as u32,
        sv.props.padding.top as u32,
        sv.props.padding.right as u32,
        sv.props.padding.bottom as u32,
    );
    let mut delta = 0i32;

    if is_x {
        if target_rect.w >= vp.w || i64::from(target_rect.x) < i64::from(vp.x) {
            delta = target_rect.x - vp.x;
        } else if target_rect.right64() > vp.right64() {
            delta = (target_rect.right64() - vp.right64()) as i32;
        }
    } else if target_rect.h >= vp.h {
        delta = target_rect.y - vp.y;
    } else if target_rect.top64() < vp.top64() {
        delta = (target_rect.top64() - vp.top64()) as i32;
    } else if target_rect.bottom64() > vp.bottom64() {
        delta = (target_rect.bottom64() - vp.bottom64()) as i32;
    }

    delta
}

pub fn compute_text_origin(node: &RetainedNode, cache: &TextCache) -> Point {
    let rect = node.layout_rect;
    let inner = rect.inset(
        node.props.padding.left as u32,
        node.props.padding.top as u32,
        node.props.padding.right as u32,
        node.props.padding.bottom as u32,
    );
    let mut text_y = inner.y;
    if cache.valid {
        let text_size = cache.size;
        if node.props.align_cross == Align::Center && inner.h > text_size.height {
            text_y += ((inner.h - text_size.height) / 2) as i32;
        } else if node.props.align_cross == Align::End && inner.h > text_size.height {
            text_y += (inner.h - text_size.height) as i32;
        }
    }
    Point::new(inner.x, text_y)
}

pub fn refresh_node_ink_rect(node: &mut RetainedNode, cache: &TextCache) {
    refresh_node_ink_rect_with(
        node,
        cache,
        &crate::part_style::WidgetPartStyle::ZERO,
        &crate::effects::SurfaceStyle::FLAT,
    );
}
pub(crate) fn refresh_node_ink_rect_with(
    node: &mut RetainedNode,
    cache: &TextCache,
    part: &crate::part_style::WidgetPartStyle,
    style: &crate::effects::SurfaceStyle,
) {
    let mut ink = node.layout_rect;
    if node.kind == NodeKind::Label && cache.valid {
        let origin = compute_text_origin(node, cache);
        ink = ink.union(cache.rel_ink.translate(origin.x, origin.y));
    }
    if node.props.focusable() {
        let ring = node.props.focus_ring();
        if ring.enabled {
            ink = ink.union(node.focus_outline(part, style, node.clip_rect).bounds);
        }
    }
    node.ink_rect = ink;
    node.visible_rect = ink.intersect(node.clip_rect);
}
