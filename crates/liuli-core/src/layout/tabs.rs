//! Intrinsic tab tracks and accessory slots. Measurement and arrangement share the
//! same line breaker. All scratch storage is independent of the number of tabs.
use super::{
    NodeKind, Rect, RetainedTree, child_outer_size, first_visible_child, next_visible_sibling,
};

#[derive(Default)]
struct Flow {
    width: u32,
    lines: u32,
    end: u32,
}

impl Flow {
    fn place(&mut self, width: u32, gap: u32, limit: u32) -> (u32, u32) {
        let mut x = if self.lines == 0 {
            self.lines = 1;
            0
        } else {
            self.end + gap
        };
        if x > 0 && x.saturating_add(width) > limit {
            self.lines += 1;
            x = 0;
        }
        self.end = x.saturating_add(width);
        self.width = self.width.max(self.end);
        (x, self.lines - 1)
    }
}

pub(super) struct BarLayout {
    pub width: u32,
    pub height: u32,
    leading: u32,
    viewport: u32,
    tab_width: u32,
    tab_height: u32,
    tools_height: u32,
    tools_rows: u32,
    tab_count: u32,
    rows: u32,
    columns: u32,
    gap: u32,
}

impl BarLayout {
    pub fn measure(tree: &RetainedTree, slot: u16, width: u32) -> Self {
        let bar = &tree.nodes[slot as usize];
        let children = || {
            core::iter::successors(first_visible_child(tree, bar.first_child), |&idx| {
                next_visible_sibling(tree, idx)
            })
        };
        let gap = u32::from(bar.props.gap);
        let mut tools = Flow::default();
        let mut leading = 0;
        let mut tools_height = 0;

        let mut tab_height = 0;
        let mut longest = 0;
        let mut tab_count = 0u32;
        let mut tools_count = 0;
        for i in children() {
            let n = &tree.nodes[usize::from(i)];
            let size = child_outer_size(tree, i);
            if n.kind == NodeKind::Tab {
                longest = longest.max(size.width);
                tab_height = tab_height.max(size.height);
                tab_count += 1;
            } else {
                tools_count += 1;
                tools.place(size.width, gap, width);
                tools_height = tools_height.max(size.height);
                if tab_count == 0 {
                    leading += size.width + gap;
                }
            }
        }
        let inline_width =
            width.saturating_sub(tools.width + if tools_count > 0 { gap } else { 0 });
        let stacked = tools_count > 0 && (tools.lines > 1 || inline_width < longest);
        let tools_rows = if stacked { tools.lines } else { 0 };
        let viewport = if stacked { width } else { inline_width };
        let mut flow = Flow::default();
        let mut tab_width = 0;
        for i in children().filter(|&i| tree.nodes[usize::from(i)].kind == NodeKind::Tab) {
            let w = child_outer_size(tree, i).width.min(viewport);
            flow.place(w, 0, viewport);
            tab_width += w;
        }
        let columns = u32::from(bar.props.grid_cols());
        let rows = if columns > 0 {
            tab_count.div_ceil(columns)
        } else if flow.lines <= bar.props.max.max(1) as u32 {
            flow.lines
        } else {
            1
        };
        let tabs_h = rows * tab_height;
        let height = if stacked {
            tools_rows * (tools_height + gap) + tabs_h
        } else {
            tabs_h.max(tools_height)
        };
        Self {
            width,
            height,
            leading: if stacked { 0 } else { leading },
            viewport,
            tab_width,
            tab_height,
            tools_height,
            tools_rows,
            tab_count,
            rows,
            columns,
            gap,
        }
    }

    /// Equal growth among intrinsic tracks, redistributing the share of capped tabs.
    fn track_growth(&self, tree: &RetainedTree, first: super::SlotLink) -> (u32, u32) {
        let spare = self.viewport.saturating_sub(self.tab_width);
        if spare == 0 || self.columns != 0 || self.tab_count == 0 {
            return (0, 0);
        }
        let rooms = || {
            core::iter::successors(first_visible_child(tree, first), |&i| {
                next_visible_sibling(tree, i)
            })
            .filter(|&i| tree.nodes[usize::from(i)].kind == NodeKind::Tab)
            .map(|i| {
                let n = &tree.nodes[usize::from(i)];
                let max = tree.layout_extras.get(n.key).max_w;
                if max == 0 {
                    u32::MAX
                } else {
                    u32::from(max).saturating_sub(n.measured_size.width)
                }
            })
        };
        let share = spare / self.tab_count;
        if rooms().all(|room| room >= spare.div_ceil(self.tab_count)) {
            return (share, spare % self.tab_count);
        }
        let allocated = |grow: u32| rooms().fold(0u64, |sum, room| sum + u64::from(room.min(grow)));
        let (mut low, mut high) = (share, spare);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if allocated(mid) <= u64::from(spare) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        (low, spare - allocated(low) as u32)
    }

    pub fn arrange(
        &self,
        tree: &mut RetainedTree,
        slot: u16,
        content: Rect,
        clip: Rect,
        resized: bool,
    ) {
        let bar = &tree.nodes[slot as usize];
        let tools_h = self.tools_rows * (self.tools_height + self.gap);
        let tabs_y = content.y
            + tools_h as i32
            + if self.tools_rows == 0 {
                (content.h.saturating_sub(self.rows * self.tab_height) / 2) as i32
            } else {
                0
            };
        let viewport = Rect::new(
            content.x + self.leading as i32,
            tabs_y,
            self.viewport,
            self.rows * self.tab_height,
        );
        let overflow = if self.rows == 1 && self.columns == 0 {
            self.tab_width.saturating_sub(self.viewport)
        } else {
            0
        };
        let reveal = resized || bar.dirty_measure || bar.scroll_y < 0;
        let mut offset = bar.scroll_y.clamp(0, overflow as i32);
        let mut flow = Flow::default();
        let mut tools = Flow::default();
        let (growth, mut bonus) = self.track_growth(tree, bar.first_child);
        let mut ordinal = 0;
        let mut trailing_x = viewport.right64() as i32 + self.gap as i32;
        let mut child = first_visible_child(tree, bar.first_child);
        while let Some(i) = child {
            child = next_visible_sibling(tree, i);
            let size = child_outer_size(tree, i);
            let n = &mut tree.nodes[i as usize];
            let extra = tree.layout_extras.get(n.key);
            if n.kind == NodeKind::Tab {
                let (x, row, w) = if self.columns > 0 {
                    let col = ordinal % self.columns;
                    let start = col * self.viewport / self.columns;
                    (
                        start,
                        ordinal / self.columns,
                        (col + 1) * self.viewport / self.columns - start,
                    )
                } else {
                    let base = size.width.min(self.viewport);
                    let room = if extra.max_w == 0 {
                        u32::MAX
                    } else {
                        u32::from(extra.max_w).saturating_sub(n.measured_size.width)
                    };
                    let add_one = u32::from(bonus > 0 && room > growth);
                    bonus -= add_one;
                    let w = base + room.min(growth) + add_one;
                    let (x, row) = flow.place(
                        w,
                        0,
                        if self.rows <= 1 {
                            u32::MAX
                        } else {
                            self.viewport
                        },
                    );
                    (x, row, w)
                };
                ordinal += 1;
                let rect = Rect::new(
                    viewport.x + x as i32,
                    tabs_y + (row * self.tab_height) as i32,
                    w,
                    self.tab_height,
                );
                let m = extra.margin;
                let rect = rect.inset(m.left.into(), m.top.into(), m.right.into(), m.bottom.into());
                let rect = extra.fit_slot(rect);
                if reveal && n.props.checked() {
                    offset = (offset + reveal_offset(viewport, rect.translate(-offset, 0)))
                        .clamp(0, overflow as i32);
                }
                n.ink_rect = rect;
                n.clip_rect = viewport.intersect(clip);
            } else {
                let (x, y) = if self.tools_rows > 0 {
                    let (x, row) = tools.place(size.width, self.gap, content.w);
                    (
                        content.x + x as i32,
                        content.y
                            + (row * (self.tools_height + self.gap)
                                + (self.tools_height - size.height) / 2)
                                as i32,
                    )
                } else {
                    let x = if ordinal == 0 {
                        content.x + tools.place(size.width, self.gap, content.w).0 as i32
                    } else {
                        let x = trailing_x;
                        trailing_x += (size.width + self.gap) as i32;
                        x
                    };
                    (
                        x,
                        content.y + (content.h.saturating_sub(size.height) / 2) as i32,
                    )
                };
                let m = extra.margin;
                n.ink_rect = Rect::new(x, y, size.width, size.height).inset(
                    m.left.into(),
                    m.top.into(),
                    m.right.into(),
                    m.bottom.into(),
                );
                n.clip_rect = clip.intersect(n.ink_rect);
            }
        }
        let bar = &mut tree.nodes[slot as usize];
        bar.content_height = bar.layout_rect.w.saturating_add(overflow);
        bar.scroll_y = offset;
        if offset > 0 {
            let mut child = first_visible_child(tree, tree.nodes[slot as usize].first_child);
            while let Some(i) = child {
                child = next_visible_sibling(tree, i);
                let n = &mut tree.nodes[i as usize];
                if n.kind == NodeKind::Tab {
                    n.ink_rect = n.ink_rect.translate(-offset, 0);
                }
            }
        }
    }
}

pub(super) fn reveal_offset(viewport: Rect, target: Rect) -> i32 {
    if target.x < viewport.x {
        target.x - viewport.x
    } else {
        (target.right64() - viewport.right64()).max(0) as i32
    }
}
