use crate::capacity::{MAX_NODES, MAX_TEXT_BYTES};
use crate::geometry::{Rect, Size};
use crate::view::{
    FontId, Key, NodeKind, SlotLink, SparklinePool, StoredProps, TextAlign, TextRef,
};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeId {
    slot: u16,
    generation: u32,
}

impl NodeId {
    pub const ZERO: Self = Self::new(0, 0);

    #[inline]
    pub const fn new(slot: u16, generation: u32) -> Self {
        Self { slot, generation }
    }

    #[inline]
    pub const fn slot(self) -> u16 {
        self.slot
    }

    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TextCache {
    pub wrap_width: u32,
    pub size: Size,
    pub rel_ink: Rect,
    pub lines: u16,
    pub valid: bool,
    pub font: FontId,
    pub align: TextAlign,
    pub max_lines: u8,
    pub ellipsis: bool,
}

impl TextCache {
    pub const ZERO: Self = Self {
        wrap_width: 0,
        size: Size::new(0, 0),
        rel_ink: Rect::ZERO,
        lines: 0,
        valid: false,
        font: FontId::Body16,
        align: TextAlign::Start,
        max_lines: 0,
        ellipsis: false,
    };
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RetainedNode {
    pub occupied: bool,
    pub generation: u32,
    pub key: Key,
    pub kind: NodeKind,
    pub props: StoredProps,
    pub parent: SlotLink,
    pub first_child: SlotLink,
    pub last_child: SlotLink,
    pub next_sibling: SlotLink,
    pub measured_size: Size,
    pub layout_rect: Rect,
    pub ink_rect: Rect,
    pub clip_rect: Rect,
    pub visible_rect: Rect,
    pub text_cache: TextCache,
    pub dirty_measure: bool,
    pub dirty_paint: bool,
    pub scroll_y: i32,
    pub content_height: u32,
    pub pressed: bool,
    // Last successfully painted style target plus one. Zero marks a fresh binding.
    style_target: u16,
    pub(crate) motion: crate::view::MotionBinding,
    pub motion_ms: u16,
}

impl RetainedNode {
    pub const EMPTY: Self = Self {
        occupied: false,
        generation: 0,
        key: Key(0),
        kind: NodeKind::Label,
        props: StoredProps::ZERO,
        parent: SlotLink::NONE,
        first_child: SlotLink::NONE,
        last_child: SlotLink::NONE,
        next_sibling: SlotLink::NONE,
        measured_size: Size::new(0, 0),
        layout_rect: Rect::ZERO,
        ink_rect: Rect::ZERO,
        clip_rect: Rect::ZERO,
        visible_rect: Rect::ZERO,
        text_cache: TextCache::ZERO,
        dirty_measure: false,
        dirty_paint: false,
        scroll_y: 0,
        content_height: 0,
        pressed: false,
        style_target: 0,
        motion: crate::view::MotionBinding::EMPTY,
        motion_ms: 0,
    };

    pub fn clear_fresh(&mut self, generation: u32) {
        self.occupied = true;
        self.generation = generation;
        self.key = Key(0);
        self.kind = NodeKind::Label;
        self.props = StoredProps::ZERO;
        self.parent = SlotLink::NONE;
        self.first_child = SlotLink::NONE;
        self.last_child = SlotLink::NONE;
        self.next_sibling = SlotLink::NONE;
        self.measured_size = Size::new(0, 0);
        self.layout_rect = Rect::ZERO;
        self.ink_rect = Rect::ZERO;
        self.clip_rect = Rect::ZERO;
        self.visible_rect = Rect::ZERO;
        self.text_cache = TextCache::ZERO;
        self.dirty_measure = true;
        self.dirty_paint = true;
        self.scroll_y = 0;
        self.content_height = 0;
        self.pressed = false;
        self.style_target = 0;
    }

    pub fn motion_target(&self, focused: bool) -> i32 {
        use crate::view::MotionState::*;
        match self.motion.state() {
            None => 0,
            Checked => i32::from(self.props.checked()) * 1000,
            Pressed => i32::from(self.pressed) * 1000,
            Focused => i32::from(focused) * 1000,
            Value => {
                let lo = i32::from(self.props.min);
                let range = i32::from(self.props.max) - lo;
                if range <= 0 {
                    0
                } else {
                    ((i32::from(self.props.value) - lo) * 1000 / range).clamp(0, 1000)
                }
            }
        }
    }

    pub(crate) fn reset_style_target(&mut self) {
        self.style_target = 0;
    }

    pub fn copy_from(&mut self, src: &Self) {
        *self = *src;
    }

    /// The stable key of the page currently mounted in this tab panel.
    pub fn active_tab(&self) -> Option<Key> {
        if self.kind != NodeKind::TabPanel {
            return None;
        }
        Some(self.props.key_payload())
    }

    /// Maximum legal offset along the scrolling axis, including the content padding.
    pub fn scroll_limit(&self) -> Option<i32> {
        let viewport = match self.kind {
            NodeKind::ScrollX | NodeKind::TabBar => self.layout_rect.w,
            NodeKind::ScrollY => self.layout_rect.h,
            _ => return None,
        };
        let overflow = self.content_height.saturating_sub(viewport);
        Some(overflow.min(i32::MAX as u32) as i32)
    }

    #[inline]
    pub fn id(&self, slot: u16) -> NodeId {
        NodeId::new(slot, self.generation)
    }

    pub fn is_effectively_visible(&self) -> bool {
        self.occupied && self.props.visible() && !self.visible_rect.is_empty()
    }
}

pub struct RetainedTree {
    pub(crate) curves: crate::view::CurvePalette,
    pub effects: crate::effects::Effects,
    pub part_styles: crate::part_style::PartStyles,
    pub layout_extras: crate::part_style::LayoutExtras,
    pub nodes: [RetainedNode; MAX_NODES],
    pub text_buf: [u8; MAX_TEXT_BYTES],
    pub text_len: u16,
    pub sparklines: SparklinePool,
    pub root: SlotLink,
    pub modal: SlotLink,
    pub active_count: u16,
    pub generation_hwm: [u32; MAX_NODES],
}

impl RetainedTree {
    pub fn part_style(&self, key: Key) -> &crate::part_style::WidgetPartStyle {
        self.find_slot_by_key(key)
            .map_or(&crate::part_style::WidgetPartStyle::ZERO, |i| {
                self.part_styles.get(usize::from(i))
            })
    }

    /// Geometry that can contain pixels from a node's selection animation.
    pub(crate) fn style_bounds(&self, i: usize) -> Rect {
        let n = &self.nodes[i];
        if n.kind == NodeKind::Tab
            && self.part_styles.get(i).indicator_transition
                == crate::part_style::IndicatorMotion::Slide
        {
            n.parent
                .get()
                .map_or(n.visible_rect, |p| self.nodes[p].visible_rect)
        } else {
            n.visible_rect
        }
    }

    pub fn sync_style_motion(
        &self,
        anims: &mut crate::time::AnimPool,
        now: crate::time::InstantMs,
        focused: Option<u16>,
        capture: crate::focus::CaptureKind,
    ) {
        use crate::focus::CaptureKind;
        use crate::time::AnimProp;
        use crate::view::MotionState;
        for (i, n) in self.nodes.iter().enumerate() {
            if !n.occupied || n.motion.state() == crate::view::MotionState::None {
                continue;
            }
            let target = n.motion_target(focused == Some(i as u16));
            // Once a drag wins, the slider must track the finger and losing buttons must release immediately.
            let direct = match (capture, n.motion.state()) {
                (CaptureKind::Slider { id }, MotionState::Value) => id == n.id(i as u16),
                (CaptureKind::Scrolling { scroll, .. }, MotionState::Pressed) => {
                    self.is_descendant_of(i as u16, scroll.slot())
                }
                _ => false,
            };
            let damage = self.style_bounds(i);
            let instant = n.kind == NodeKind::Tab
                && self.part_styles.get(i).indicator_transition
                    == crate::part_style::IndicatorMotion::Instant;
            if n.motion_ms == 0 || n.style_target == 0 || direct || instant {
                anims.clear_property(n.key, AnimProp::Style);
            } else if i32::from(n.style_target) - 1 != target {
                anims.start_with_curve(
                    n.key,
                    AnimProp::Style,
                    i32::from(n.style_target) - 1,
                    target,
                    now,
                    n.motion_ms.into(),
                    damage,
                    n.props.anim_easing(),
                    n.motion.curve(&self.curves),
                );
            }
            anims.update_rect(n.key, AnimProp::Style, damage);
        }
    }

    pub fn accept_style_targets(&mut self, focused: Option<u16>) {
        for (i, n) in self.nodes.iter_mut().enumerate() {
            if n.occupied && n.motion.state() != crate::view::MotionState::None {
                n.style_target = 1 + n.motion_target(focused == Some(i as u16)) as u16;
            }
        }
    }

    pub const ZERO: Self = Self {
        effects: crate::effects::Effects::EMPTY,
        curves: [None; crate::capacity::MAX_ANIMATIONS],
        part_styles: crate::part_style::PartStyles::EMPTY,
        layout_extras: crate::part_style::LayoutExtras::EMPTY,
        nodes: [RetainedNode::EMPTY; MAX_NODES],
        text_buf: [0; MAX_TEXT_BYTES],
        text_len: 0,
        sparklines: SparklinePool::ZERO,
        root: SlotLink::NONE,
        modal: SlotLink::NONE,
        active_count: 0,
        generation_hwm: [0; MAX_NODES],
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn reset(&mut self) {
        for slot in &mut self.nodes {
            slot.occupied = false;
            slot.parent = SlotLink::NONE;
            slot.first_child = SlotLink::NONE;
            slot.last_child = SlotLink::NONE;
            slot.next_sibling = SlotLink::NONE;
            slot.visible_rect = Rect::ZERO;
            slot.dirty_measure = false;
            slot.dirty_paint = false;
            slot.pressed = false;
        }
        self.text_len = 0;
        self.sparklines.reset();
        self.effects = crate::effects::Effects::EMPTY;
        self.curves = [None; crate::capacity::MAX_ANIMATIONS];
        self.part_styles = crate::part_style::PartStyles::EMPTY;
        self.layout_extras = crate::part_style::LayoutExtras::EMPTY;
        self.root = SlotLink::NONE;
        self.modal = SlotLink::NONE;
        self.active_count = 0;
    }

    pub fn text_str(&self, text: TextRef) -> &str {
        match text {
            TextRef::Static(s) => s,
            TextRef::Dynamic(r) => {
                let start = r.start as usize;
                let end = start.saturating_add(r.len as usize);
                if end <= self.text_len as usize {
                    core::str::from_utf8(&self.text_buf[start..end]).unwrap_or("")
                } else {
                    ""
                }
            }
        }
    }

    pub fn find_slot_by_key(&self, key: Key) -> Option<u16> {
        for (idx, node) in self.nodes.iter().enumerate() {
            if node.occupied && node.key == key {
                return Some(idx as u16);
            }
        }
        None
    }

    pub fn get_id(&self, key: Key) -> Option<NodeId> {
        self.find_slot_by_key(key)
            .map(|slot| self.nodes[slot as usize].id(slot))
    }

    pub fn is_id_valid(&self, id: NodeId) -> bool {
        let idx = id.slot() as usize;
        if idx >= MAX_NODES {
            return false;
        }
        let n = &self.nodes[idx];
        n.occupied && n.generation == id.generation()
    }

    pub fn node_by_id(&self, id: NodeId) -> Option<&RetainedNode> {
        if self.is_id_valid(id) {
            Some(&self.nodes[id.slot() as usize])
        } else {
            None
        }
    }

    pub fn node_by_key(&self, key: Key) -> Option<&RetainedNode> {
        self.find_slot_by_key(key).map(|s| &self.nodes[s as usize])
    }

    pub fn is_descendant_of(&self, slot: u16, ancestor: u16) -> bool {
        let mut cur = SlotLink::from_slot_u16(slot);
        while let Some(link) = cur
            && let Some(idx) = link.get_u16()
        {
            if idx == ancestor {
                return true;
            }
            cur = Some(self.nodes[idx as usize].parent);
        }
        false
    }

    pub fn is_effectively_enabled_and_visible(&self, slot: u16) -> bool {
        let mut cur = SlotLink::from_slot_u16(slot);
        while let Some(link) = cur
            && let Some(idx) = link.get()
        {
            let n = &self.nodes[idx];
            if !n.occupied || !n.props.visible() || !n.props.enabled() {
                return false;
            }
            cur = Some(n.parent);
        }
        true
    }
}

impl Default for RetainedTree {
    fn default() -> Self {
        Self::new()
    }
}
