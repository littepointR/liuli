use crate::capacity::{MAX_DEPTH, MAX_NODES};
use crate::geometry::{Point, Rect, Size};
use crate::node::RetainedTree;
use crate::view::{Align, IconKind, Length, NONE_IDX, NodeKind};

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
    fn line_height(&self) -> u32;
    fn measure_text(
        &self,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
    ) -> TextMeasureOut;
}

pub fn layout_tree(
    tree: &mut RetainedTree,
    screen: Rect,
    measurer: &dyn TextMeasurer,
) -> Result<u32, crate::capacity::LayoutError> {
    if tree.root == NONE_IDX {
        return Err(crate::capacity::LayoutError::EmptyTree);
    }

    let root = tree.root;
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
    if scroll_slot == NONE_IDX || !tree.nodes[scroll_slot as usize].occupied {
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

fn first_visible_child(tree: &RetainedTree, mut ch: u16) -> u16 {
    while ch != NONE_IDX {
        let n = &tree.nodes[ch as usize];
        if n.occupied && n.props.visible {
            return ch;
        }
        ch = n.next_sibling;
    }
    NONE_IDX
}

fn next_visible_sibling(tree: &RetainedTree, mut ch: u16) -> u16 {
    if ch == NONE_IDX {
        return NONE_IDX;
    }
    ch = tree.nodes[ch as usize].next_sibling;
    first_visible_child(tree, ch)
}

fn measure_leaf(
    tree: &mut RetainedTree,
    slot: u16,
    c: Constraints,
    measurer: &dyn TextMeasurer,
) -> Size {
    let n = tree.nodes[slot as usize];
    let pad_w = n.props.padding.horizontal();
    let pad_h = n.props.padding.vertical();

    match n.kind {
        NodeKind::Label => {
            let avail_w = resolve_axis_max(n.props.width, c.max_w);
            let wrap_w = avail_w.saturating_sub(pad_w);
            let text_str = tree.text_str(n.props.text);
            let tm = measurer.measure_text(text_str, wrap_w, n.props.max_lines, n.props.ellipsis);
            tree.nodes[slot as usize].text_cache = Some((wrap_w, tm.size, tm.ink, tm.lines));

            let w = resolve_axis_size(n.props.width, tm.size.width.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(
                n.props.height,
                tm.size.height.saturating_add(pad_h),
                c.max_h,
            );
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Button => {
            let avail_w = resolve_axis_max(n.props.width, c.max_w);
            let icon_space = if n.props.icon != IconKind::None {
                18
            } else {
                0
            };
            let wrap_w = avail_w.saturating_sub(pad_w + icon_space);
            let text_str = tree.text_str(n.props.text);
            let tm =
                measurer.measure_text(text_str, wrap_w, n.props.max_lines.max(1), n.props.ellipsis);
            tree.nodes[slot as usize].text_cache = Some((wrap_w, tm.size, tm.ink, tm.lines));

            let content_w = tm.size.width.saturating_add(pad_w + icon_space);
            let content_h = tm.size.height.max(14).saturating_add(pad_h);
            let w = resolve_axis_size(n.props.width, content_w, c.max_w);
            let h = resolve_axis_size(n.props.height, content_h, c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Toggle => {
            let w = resolve_axis_size(n.props.width, 40u32.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(n.props.height, 24u32.saturating_add(pad_h), c.max_h);
            c.clamp_size(Size::new(w, h))
        }
        NodeKind::Slider => {
            let w = resolve_axis_size(n.props.width, 120u32.saturating_add(pad_w), c.max_w);
            let h = resolve_axis_size(n.props.height, 24u32.saturating_add(pad_h), c.max_h);
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
    active_child: u16,
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
        slot: NONE_IDX,
        constraints: Constraints::ZERO,
        phase: 0,
        active_child: NONE_IDX,
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
        let n = tree.nodes[slot as usize];

        if !n.occupied || !n.props.visible {
            tree.nodes[slot as usize].measured_size = Size::new(0, 0);
            sp -= 1;
            continue;
        }

        if stack[top].phase == 0 {
            measured_count += 1;
            if !n.kind.is_container() {
                let sz = measure_leaf(tree, slot, c, measurer);
                tree.nodes[slot as usize].measured_size = sz;
                sp -= 1;
                continue;
            }

            let pad_w = n.props.padding.horizontal();
            let pad_h = n.props.padding.vertical();
            let inner_max_w = resolve_axis_max(n.props.width, c.max_w).saturating_sub(pad_w);
            let inner_max_h = resolve_axis_max(n.props.height, c.max_h).saturating_sub(pad_h);

            let mut vis_count = 0u16;
            let mut total_fill_weight = 0u32;
            let mut first_non_fill = NONE_IDX;
            let mut first_fill = NONE_IDX;

            let mut ch = first_visible_child(tree, n.first_child);
            while ch != NONE_IDX {
                vis_count += 1;
                let cn = &tree.nodes[ch as usize];
                if let Some(w) = is_main_fill(n.kind, cn.props.width, cn.props.height) {
                    total_fill_weight += w as u32;
                    if first_fill == NONE_IDX {
                        first_fill = ch;
                    }
                } else if first_non_fill == NONE_IDX {
                    first_non_fill = ch;
                }
                ch = next_visible_sibling(tree, ch);
            }

            if vis_count == 0 {
                let w = resolve_axis_size(n.props.width, pad_w, c.max_w);
                let h = resolve_axis_size(n.props.height, pad_h, c.max_h);
                if n.kind == NodeKind::ScrollY {
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

            match n.kind {
                NodeKind::Row | NodeKind::Column => {
                    if first_non_fill != NONE_IDX {
                        stack[top].phase = 1;
                        stack[top].active_child = first_non_fill;
                        let child_c = child_constraints_flex(
                            n.kind,
                            n.props.align_cross,
                            n.props.width,
                            n.props.height,
                            inner_max_w,
                            inner_max_h,
                            &tree.nodes[first_non_fill as usize],
                            None,
                        );
                        stack[sp] = MeasureFrame {
                            slot: first_non_fill,
                            constraints: child_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else {
                        let gap_total = (vis_count.saturating_sub(1) as u32) * (n.props.gap as u32);
                        let avail_main = if n.kind == NodeKind::Row {
                            inner_max_w
                        } else {
                            inner_max_h
                        };
                        let remaining = avail_main.saturating_sub(gap_total);
                        let w_total = total_fill_weight.max(1);
                        let mut rem_left = remaining
                            - distribute_base_sum(tree, n.kind, n.first_child, remaining, w_total);
                        let cn = &tree.nodes[first_fill as usize];
                        let weight = is_main_fill(n.kind, cn.props.width, cn.props.height)
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
                        stack[top].active_child = first_fill;
                        stack[top].remaining_fill = remaining;
                        stack[top].fill_rem_pixels = rem_left;

                        let child_c = child_constraints_flex(
                            n.kind,
                            n.props.align_cross,
                            n.props.width,
                            n.props.height,
                            inner_max_w,
                            inner_max_h,
                            cn,
                            Some(first_share),
                        );
                        stack[sp] = MeasureFrame {
                            slot: first_fill,
                            constraints: child_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    }
                }
                NodeKind::ScrollY => {
                    let first = first_visible_child(tree, n.first_child);
                    stack[top].phase = 2;
                    stack[top].active_child = first;
                    let ch_c = child_constraints_scroll(inner_max_w, &tree.nodes[first as usize]);
                    stack[sp] = MeasureFrame {
                        slot: first,
                        constraints: ch_c,
                        ..MeasureFrame::EMPTY
                    };
                    sp += 1;
                }
                NodeKind::Root | NodeKind::Stack | NodeKind::Modal => {
                    let first = first_visible_child(tree, n.first_child);
                    stack[top].phase = 2;
                    stack[top].active_child = first;
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
                _ => unreachable!(),
            }
        } else if stack[top].phase == 1 {
            let prev = stack[top].active_child;
            let prev_sz = tree.nodes[prev as usize].measured_size;
            if n.kind == NodeKind::Row {
                stack[top].non_fill_used = stack[top].non_fill_used.saturating_add(prev_sz.width);
                stack[top].cross_max = stack[top].cross_max.max(prev_sz.height);
            } else {
                stack[top].non_fill_used = stack[top].non_fill_used.saturating_add(prev_sz.height);
                stack[top].cross_max = stack[top].cross_max.max(prev_sz.width);
            }

            let mut next = next_visible_sibling(tree, prev);
            while next != NONE_IDX {
                let cn = &tree.nodes[next as usize];
                if is_main_fill(n.kind, cn.props.width, cn.props.height).is_none() {
                    break;
                }
                next = next_visible_sibling(tree, next);
            }

            if next != NONE_IDX {
                stack[top].active_child = next;
                let child_c = child_constraints_flex(
                    n.kind,
                    n.props.align_cross,
                    n.props.width,
                    n.props.height,
                    stack[top].inner_max_w,
                    stack[top].inner_max_h,
                    &tree.nodes[next as usize],
                    None,
                );
                stack[sp] = MeasureFrame {
                    slot: next,
                    constraints: child_c,
                    ..MeasureFrame::EMPTY
                };
                sp += 1;
            } else {
                let first_fill = find_next_fill_child(tree, n.kind, n.first_child);
                if first_fill == NONE_IDX {
                    finalize_flex_measure(tree, slot, &stack[top], c);
                    sp -= 1;
                } else {
                    let gap_total =
                        (stack[top].vis_count.saturating_sub(1) as u32) * (n.props.gap as u32);
                    let avail_main = if n.kind == NodeKind::Row {
                        stack[top].inner_max_w
                    } else {
                        stack[top].inner_max_h
                    };
                    let remaining = avail_main
                        .saturating_sub(stack[top].non_fill_used.saturating_add(gap_total));
                    let w_total = stack[top].total_fill_weight.max(1);
                    let mut rem_left = remaining
                        - distribute_base_sum(tree, n.kind, n.first_child, remaining, w_total);
                    let cn = &tree.nodes[first_fill as usize];
                    let weight =
                        is_main_fill(n.kind, cn.props.width, cn.props.height).unwrap_or(1) as u32;
                    let extra = if rem_left > 0 {
                        rem_left -= 1;
                        1
                    } else {
                        0
                    };
                    let share =
                        ((remaining as u64 * weight as u64) / w_total as u64) as u32 + extra;

                    stack[top].phase = 2;
                    stack[top].active_child = first_fill;
                    stack[top].remaining_fill = remaining;
                    stack[top].fill_rem_pixels = rem_left;

                    let child_c = child_constraints_flex(
                        n.kind,
                        n.props.align_cross,
                        n.props.width,
                        n.props.height,
                        stack[top].inner_max_w,
                        stack[top].inner_max_h,
                        cn,
                        Some(share),
                    );
                    stack[sp] = MeasureFrame {
                        slot: first_fill,
                        constraints: child_c,
                        ..MeasureFrame::EMPTY
                    };
                    sp += 1;
                }
            }
        } else {
            let prev = stack[top].active_child;
            let prev_sz = tree.nodes[prev as usize].measured_size;

            match n.kind {
                NodeKind::Row | NodeKind::Column => {
                    if n.kind == NodeKind::Row {
                        stack[top].fill_used = stack[top].fill_used.saturating_add(prev_sz.width);
                        stack[top].cross_max = stack[top].cross_max.max(prev_sz.height);
                    } else {
                        stack[top].fill_used = stack[top].fill_used.saturating_add(prev_sz.height);
                        stack[top].cross_max = stack[top].cross_max.max(prev_sz.width);
                    }

                    let next_fill =
                        find_next_fill_child(tree, n.kind, next_visible_sibling(tree, prev));
                    if next_fill != NONE_IDX {
                        let w_total = stack[top].total_fill_weight.max(1);
                        let remaining = stack[top].remaining_fill;
                        let cn = &tree.nodes[next_fill as usize];
                        let weight = is_main_fill(n.kind, cn.props.width, cn.props.height)
                            .unwrap_or(1) as u32;
                        let extra = if stack[top].fill_rem_pixels > 0 {
                            stack[top].fill_rem_pixels -= 1;
                            1
                        } else {
                            0
                        };
                        let share =
                            ((remaining as u64 * weight as u64) / w_total as u64) as u32 + extra;
                        stack[top].active_child = next_fill;
                        let child_c = child_constraints_flex(
                            n.kind,
                            n.props.align_cross,
                            n.props.width,
                            n.props.height,
                            stack[top].inner_max_w,
                            stack[top].inner_max_h,
                            cn,
                            Some(share),
                        );
                        stack[sp] = MeasureFrame {
                            slot: next_fill,
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
                    if next != NONE_IDX {
                        stack[top].active_child = next;
                        let ch_c = child_constraints_scroll(
                            stack[top].inner_max_w,
                            &tree.nodes[next as usize],
                        );
                        stack[sp] = MeasureFrame {
                            slot: next,
                            constraints: ch_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else {
                        let pad_w = n.props.padding.horizontal();
                        let pad_h = n.props.padding.vertical();
                        let gap_total =
                            (stack[top].vis_count.saturating_sub(1) as u32) * (n.props.gap as u32);
                        let content_h = stack[top]
                            .non_fill_used
                            .saturating_add(gap_total)
                            .saturating_add(pad_h);
                        let w = resolve_axis_size(
                            n.props.width,
                            stack[top].cross_max.saturating_add(pad_w),
                            c.max_w,
                        );
                        let h = resolve_axis_size(n.props.height, content_h, c.max_h);
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
                    if next != NONE_IDX {
                        stack[top].active_child = next;
                        let ch_c = child_constraints_stack(
                            stack[top].inner_max_w,
                            stack[top].inner_max_h,
                            &tree.nodes[next as usize],
                        );
                        stack[sp] = MeasureFrame {
                            slot: next,
                            constraints: ch_c,
                            ..MeasureFrame::EMPTY
                        };
                        sp += 1;
                    } else {
                        let pad_w = n.props.padding.horizontal();
                        let pad_h = n.props.padding.vertical();
                        let w = resolve_axis_size(
                            n.props.width,
                            stack[top].non_fill_used.saturating_add(pad_w),
                            c.max_w,
                        );
                        let h = resolve_axis_size(
                            n.props.height,
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

fn find_next_fill_child(tree: &RetainedTree, parent_kind: NodeKind, mut ch: u16) -> u16 {
    ch = first_visible_child(tree, ch);
    while ch != NONE_IDX {
        let cn = &tree.nodes[ch as usize];
        if is_main_fill(parent_kind, cn.props.width, cn.props.height).is_some() {
            return ch;
        }
        ch = next_visible_sibling(tree, ch);
    }
    NONE_IDX
}

fn distribute_base_sum(
    tree: &RetainedTree,
    parent_kind: NodeKind,
    first_child: u16,
    remaining: u32,
    w_total: u32,
) -> u32 {
    let mut sum = 0u32;
    let mut ch = first_visible_child(tree, first_child);
    while ch != NONE_IDX {
        let cn = &tree.nodes[ch as usize];
        if let Some(w) = is_main_fill(parent_kind, cn.props.width, cn.props.height) {
            sum += ((remaining as u64 * w as u64) / w_total as u64) as u32;
        }
        ch = next_visible_sibling(tree, ch);
    }
    sum
}

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
    let n = tree.nodes[slot as usize];
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

fn arrange_iterative(
    tree: &mut RetainedTree,
    start_slot: u16,
    start_rect: Rect,
    start_clip: Rect,
    screen: Rect,
) {
    #[derive(Copy, Clone)]
    struct ArrangeFrame {
        slot: u16,
        rect: Rect,
        clip: Rect,
    }

    let mut stack = [ArrangeFrame {
        slot: NONE_IDX,
        rect: Rect::ZERO,
        clip: Rect::ZERO,
    }; MAX_NODES];
    let mut sp = 0usize;

    stack[sp] = ArrangeFrame {
        slot: start_slot,
        rect: start_rect,
        clip: start_clip,
    };
    sp += 1;

    while sp > 0 {
        sp -= 1;
        let frame = stack[sp];
        let slot = frame.slot;
        let n = tree.nodes[slot as usize];
        if !n.occupied || !n.props.visible {
            tree.nodes[slot as usize].layout_rect = Rect::ZERO;
            tree.nodes[slot as usize].ink_rect = Rect::ZERO;
            tree.nodes[slot as usize].clip_rect = Rect::ZERO;
            tree.nodes[slot as usize].visible_rect = Rect::ZERO;
            continue;
        }

        let rect = frame.rect;
        let own_clip = match n.kind {
            NodeKind::Root | NodeKind::Modal => screen,
            NodeKind::ScrollY => frame.clip.intersect(rect),
            _ => frame.clip,
        };

        let mut ink = rect;
        if n.kind == NodeKind::Label
            && let Some((_, text_size, rel_ink, _)) = n.text_cache
        {
            let inner = rect.inset(
                n.props.padding.left as u32,
                n.props.padding.top as u32,
                n.props.padding.right as u32,
                n.props.padding.bottom as u32,
            );
            let mut text_y = inner.y;
            if n.props.align_cross == Align::Center && inner.h > text_size.height {
                text_y += ((inner.h - text_size.height) / 2) as i32;
            } else if n.props.align_cross == Align::End && inner.h > text_size.height {
                text_y += (inner.h - text_size.height) as i32;
            }
            ink = ink.union(rel_ink.translate(inner.x, text_y));
        }
        if n.props.focusable {
            ink = ink.expand(2);
        }

        tree.nodes[slot as usize].layout_rect = rect;
        tree.nodes[slot as usize].ink_rect = ink;
        tree.nodes[slot as usize].clip_rect = own_clip;
        tree.nodes[slot as usize].visible_rect = ink.intersect(own_clip);

        if !n.kind.is_container() {
            continue;
        }

        let bw = n.props.border_width as u32;
        let child_clip = match n.kind {
            NodeKind::Root | NodeKind::Modal => screen,
            NodeKind::ScrollY => own_clip,
            _ => own_clip.intersect(rect.inset(bw, bw, bw, bw)),
        };

        let content = rect.inset(
            n.props.padding.left as u32,
            n.props.padding.top as u32,
            n.props.padding.right as u32,
            n.props.padding.bottom as u32,
        );

        match n.kind {
            NodeKind::Row | NodeKind::Column => {
                let is_row = n.kind == NodeKind::Row;
                let mut vis_count = 0u16;
                let mut non_fill_sum = 0u32;
                let mut total_weight = 0u32;

                let mut ch = first_visible_child(tree, n.first_child);
                while ch != NONE_IDX {
                    vis_count += 1;
                    let cn = &tree.nodes[ch as usize];
                    if let Some(w) = is_main_fill(n.kind, cn.props.width, cn.props.height) {
                        total_weight += w as u32;
                    } else {
                        non_fill_sum = non_fill_sum.saturating_add(if is_row {
                            cn.measured_size.width
                        } else {
                            cn.measured_size.height
                        });
                    }
                    ch = next_visible_sibling(tree, ch);
                }

                let gap_total = (vis_count.saturating_sub(1) as u32) * (n.props.gap as u32);
                let avail_main = if is_row { content.w } else { content.h };
                let remaining = avail_main.saturating_sub(non_fill_sum.saturating_add(gap_total));
                let w_total = total_weight.max(1);
                let mut rem_left = if total_weight > 0 {
                    remaining - distribute_base_sum(tree, n.kind, n.first_child, remaining, w_total)
                } else {
                    0
                };

                let mut cursor = 0i64;
                let mut children_buf = [ArrangeFrame {
                    slot: NONE_IDX,
                    rect: Rect::ZERO,
                    clip: Rect::ZERO,
                }; MAX_NODES];
                let mut c_cnt = 0usize;

                let mut ch = first_visible_child(tree, n.first_child);
                while ch != NONE_IDX {
                    let cn = &tree.nodes[ch as usize];
                    let main_sz =
                        if let Some(w) = is_main_fill(n.kind, cn.props.width, cn.props.height) {
                            let extra = if rem_left > 0 {
                                rem_left -= 1;
                                1
                            } else {
                                0
                            };
                            ((remaining as u64 * w as u64) / w_total as u64) as u32 + extra
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
                    if n.props.align_cross == Align::Stretch
                        || matches!(child_cross_len, Length::Fill(_))
                    {
                        cross_sz = avail_cross;
                    }
                    let cross_off: i64 = match n.props.align_cross {
                        Align::Start | Align::Stretch => 0,
                        Align::Center => ((avail_cross as i64) - (cross_sz as i64)) / 2,
                        Align::End => (avail_cross as i64) - (cross_sz as i64),
                    }
                    .max(0);

                    let child_rect = if is_row {
                        Rect::new(
                            (content.x as i64 + cursor).clamp(i32::MIN as i64, i32::MAX as i64)
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
                            (content.y as i64 + cursor).clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            cross_sz,
                            main_sz,
                        )
                    };

                    children_buf[c_cnt] = ArrangeFrame {
                        slot: ch,
                        rect: child_rect,
                        clip: child_clip,
                    };
                    c_cnt += 1;

                    cursor += main_sz as i64 + n.props.gap as i64;
                    ch = next_visible_sibling(tree, ch);
                }

                while c_cnt > 0 {
                    c_cnt -= 1;
                    stack[sp] = children_buf[c_cnt];
                    sp += 1;
                }
            }
            NodeKind::ScrollY => {
                let mut total_h = n.props.padding.vertical();
                let mut vis_count = 0u16;
                let mut ch = first_visible_child(tree, n.first_child);
                while ch != NONE_IDX {
                    vis_count += 1;
                    total_h = total_h.saturating_add(tree.nodes[ch as usize].measured_size.height);
                    ch = next_visible_sibling(tree, ch);
                }
                total_h = total_h
                    .saturating_add((vis_count.saturating_sub(1) as u32) * (n.props.gap as u32));
                tree.nodes[slot as usize].content_height = total_h;
                let max_scroll = total_h.saturating_sub(rect.h) as i32;
                let scroll_y = tree.nodes[slot as usize].scroll_y.clamp(0, max_scroll);
                tree.nodes[slot as usize].scroll_y = scroll_y;

                let mut cursor_y = (content.y as i64) - (scroll_y as i64);
                let mut children_buf = [ArrangeFrame {
                    slot: NONE_IDX,
                    rect: Rect::ZERO,
                    clip: Rect::ZERO,
                }; MAX_NODES];
                let mut c_cnt = 0usize;

                let mut ch = first_visible_child(tree, n.first_child);
                while ch != NONE_IDX {
                    let cn = &tree.nodes[ch as usize];
                    let cw = if matches!(cn.props.width, Length::Fill(_)) {
                        content.w
                    } else {
                        cn.measured_size.width.min(content.w)
                    };
                    let ch_h = cn.measured_size.height;
                    let cx = match n.props.align_cross {
                        Align::Center => content.x + ((content.w.saturating_sub(cw)) / 2) as i32,
                        Align::End => content.x + content.w.saturating_sub(cw) as i32,
                        _ => content.x,
                    };
                    let child_rect = Rect::new(
                        cx,
                        cursor_y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                        cw,
                        ch_h,
                    );
                    children_buf[c_cnt] = ArrangeFrame {
                        slot: ch,
                        rect: child_rect,
                        clip: child_clip,
                    };
                    c_cnt += 1;
                    cursor_y += ch_h as i64 + n.props.gap as i64;
                    ch = next_visible_sibling(tree, ch);
                }

                while c_cnt > 0 {
                    c_cnt -= 1;
                    stack[sp] = children_buf[c_cnt];
                    sp += 1;
                }
            }
            NodeKind::Root | NodeKind::Stack | NodeKind::Modal => {
                let mut children_buf = [ArrangeFrame {
                    slot: NONE_IDX,
                    rect: Rect::ZERO,
                    clip: Rect::ZERO,
                }; MAX_NODES];
                let mut c_cnt = 0usize;
                let is_modal = n.kind == NodeKind::Modal;

                let mut ch = first_visible_child(tree, n.first_child);
                while ch != NONE_IDX {
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
                    let (cx, cy) = if is_modal {
                        (
                            content.x + ((content.w.saturating_sub(cw)) / 2) as i32,
                            content.y + ((content.h.saturating_sub(ch_h)) / 2) as i32,
                        )
                    } else {
                        let ax = match n.props.align_cross {
                            Align::Center => {
                                content.x + ((content.w.saturating_sub(cw)) / 2) as i32
                            }
                            Align::End => content.x + content.w.saturating_sub(cw) as i32,
                            _ => content.x,
                        };
                        (ax, content.y)
                    };

                    children_buf[c_cnt] = ArrangeFrame {
                        slot: ch,
                        rect: Rect::new(cx, cy, cw, ch_h),
                        clip: child_clip,
                    };
                    c_cnt += 1;
                    ch = next_visible_sibling(tree, ch);
                }

                while c_cnt > 0 {
                    c_cnt -= 1;
                    stack[sp] = children_buf[c_cnt];
                    sp += 1;
                }
            }
            _ => {}
        }
    }
}

pub fn ensure_visible(tree: &mut RetainedTree, target_slot: u16, screen: Rect) -> Option<u16> {
    if target_slot == NONE_IDX || !tree.nodes[target_slot as usize].occupied {
        return None;
    }

    // Find nearest ScrollY ancestor
    let mut anc = tree.nodes[target_slot as usize].parent;
    let mut scroll_slot = NONE_IDX;
    while anc != NONE_IDX {
        if tree.nodes[anc as usize].kind == NodeKind::ScrollY {
            scroll_slot = anc;
            break;
        }
        anc = tree.nodes[anc as usize].parent;
    }
    if scroll_slot == NONE_IDX {
        return None;
    }

    let sv = &tree.nodes[scroll_slot as usize];
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

    tree.nodes[scroll_slot as usize].scroll_y = new_scroll;
    relayout_scroll_subtree(tree, scroll_slot, screen);
    Some(scroll_slot)
}

pub fn compute_text_origin(node: &crate::node::RetainedNode) -> Point {
    let rect = node.layout_rect;
    let inner = rect.inset(
        node.props.padding.left as u32,
        node.props.padding.top as u32,
        node.props.padding.right as u32,
        node.props.padding.bottom as u32,
    );
    let mut text_y = inner.y;
    if let Some((_, text_size, _, _)) = node.text_cache {
        if node.props.align_cross == Align::Center && inner.h > text_size.height {
            text_y += ((inner.h - text_size.height) / 2) as i32;
        } else if node.props.align_cross == Align::End && inner.h > text_size.height {
            text_y += (inner.h - text_size.height) as i32;
        }
    }
    Point::new(inner.x, text_y)
}
