use crate::capacity::{MAX_NODES, MAX_TEXT_BYTES};
use crate::geometry::{Rect, Size};
use crate::view::{Key, NONE_IDX, NodeKind, NodeProps, TextRef};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeId {
    slot: u16,
    generation: u16,
}

impl NodeId {
    #[inline]
    pub(crate) const fn new(slot: u16, generation: u16) -> Self {
        Self { slot, generation }
    }

    #[inline]
    pub const fn slot(self) -> u16 {
        self.slot
    }

    #[inline]
    pub const fn generation(self) -> u16 {
        self.generation
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RetainedNode {
    pub occupied: bool,
    pub generation: u16,
    pub key: Key,
    pub kind: NodeKind,
    pub props: NodeProps,
    pub parent: u16,
    pub first_child: u16,
    pub last_child: u16,
    pub next_sibling: u16,
    pub measured_size: Size,
    pub layout_rect: Rect,
    pub ink_rect: Rect,
    pub clip_rect: Rect,
    pub visible_rect: Rect,
    pub old_visible_rect: Rect,
    pub text_cache: Option<(u32, Size, Rect, u16)>, // (wrap_width, layout_size, rel_ink, lines)
    pub dirty_measure: bool,
    pub dirty_paint: bool,
    pub scroll_y: i32,
    pub content_height: u32,
    pub pressed: bool,
}

impl RetainedNode {
    pub const EMPTY: Self = Self {
        occupied: false,
        generation: 0,
        key: Key(0),
        kind: NodeKind::Label,
        props: NodeProps::new(),
        parent: NONE_IDX,
        first_child: NONE_IDX,
        last_child: NONE_IDX,
        next_sibling: NONE_IDX,
        measured_size: Size::new(0, 0),
        layout_rect: Rect::ZERO,
        ink_rect: Rect::ZERO,
        clip_rect: Rect::ZERO,
        visible_rect: Rect::ZERO,
        old_visible_rect: Rect::ZERO,
        text_cache: None,
        dirty_measure: false,
        dirty_paint: false,
        scroll_y: 0,
        content_height: 0,
        pressed: false,
    };

    #[inline]
    pub fn id(&self, slot: u16) -> NodeId {
        NodeId::new(slot, self.generation)
    }

    pub fn is_effectively_visible(&self) -> bool {
        self.occupied && self.props.visible && !self.visible_rect.is_empty()
    }
}

pub struct RetainedTree {
    pub nodes: [RetainedNode; MAX_NODES],
    pub text_buf: [u8; MAX_TEXT_BYTES],
    pub text_len: u16,
    pub root: u16,
    pub modal: u16,
    pub active_count: u16,
    pub generation_hwm: [u16; MAX_NODES],
}

impl RetainedTree {
    pub const fn new() -> Self {
        Self {
            nodes: [RetainedNode::EMPTY; MAX_NODES],
            text_buf: [0; MAX_TEXT_BYTES],
            text_len: 0,
            root: NONE_IDX,
            modal: NONE_IDX,
            active_count: 0,
            generation_hwm: [0; MAX_NODES],
        }
    }

    pub fn reset(&mut self) {
        for slot in &mut self.nodes {
            slot.occupied = false;
            slot.parent = NONE_IDX;
            slot.first_child = NONE_IDX;
            slot.last_child = NONE_IDX;
            slot.next_sibling = NONE_IDX;
            slot.old_visible_rect = Rect::ZERO;
            slot.visible_rect = Rect::ZERO;
            slot.dirty_measure = false;
            slot.dirty_paint = false;
            slot.pressed = false;
        }
        self.text_len = 0;
        self.root = NONE_IDX;
        self.modal = NONE_IDX;
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

    pub fn is_descendant_of(&self, mut slot: u16, ancestor: u16) -> bool {
        while slot != NONE_IDX {
            if slot == ancestor {
                return true;
            }
            slot = self.nodes[slot as usize].parent;
        }
        false
    }

    pub fn is_effectively_enabled_and_visible(&self, mut slot: u16) -> bool {
        while slot != NONE_IDX {
            let n = &self.nodes[slot as usize];
            if !n.occupied || !n.props.visible || !n.props.enabled {
                return false;
            }
            slot = n.parent;
        }
        true
    }
}

impl Default for RetainedTree {
    fn default() -> Self {
        Self::new()
    }
}
