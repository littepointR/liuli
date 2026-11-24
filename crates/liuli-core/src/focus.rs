use crate::capacity::{MAX_DEPTH, MAX_NODES};
use crate::damage::DamageSet;
use crate::geometry::{Point, Rect};
use crate::input::{
    ActionKind, HoldKey, NavDir, PointerCaptureMode, PointerEvent, PointerId, PointerPhase,
    RawInput, UiAction,
};
use crate::layout::{ensure_visible, relayout_scroll_subtree};
use crate::node::{NodeId, RetainedTree};
use crate::time::{InstantMs, NextWakeup};
use crate::view::{Key, NodeKind, SlotLink};

const DRAG_THRESHOLD_PX: i32 = 6;
const _: () = assert!(MAX_NODES <= 64 && MAX_NODES < u8::MAX as usize);

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum CaptureKind {
    None = 0,
    Button(NodeId) = 1,
    Slider {
        id: NodeId,
    } = 2,
    ScrollCandidate {
        candidate: NodeId,
        scroll: NodeId,
        start_pos: Point,
        start_offset: i32,
        slider_candidate: bool,
    } = 3,
    Scrolling {
        scroll: NodeId,
        start_pos: Point,
        start_offset: i32,
    } = 4,
    CustomPointer {
        id: NodeId,
        start_pos: Point,
    } = 5,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PointerState {
    has_active: bool,
    active_id: PointerId,
    pub capture: CaptureKind,
}

impl PointerState {
    pub const ZERO: Self = Self {
        has_active: false,
        active_id: PointerId(0),
        capture: CaptureKind::None,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    #[inline]
    pub const fn active_id(&self) -> Option<PointerId> {
        if self.has_active {
            Some(self.active_id)
        } else {
            None
        }
    }

    #[inline]
    pub fn set_active_id(&mut self, id: Option<PointerId>) {
        match id {
            Some(p) => {
                self.has_active = true;
                self.active_id = p;
            }
            None => {
                self.has_active = false;
                self.active_id = PointerId(0);
            }
        }
    }

    pub fn clear(&mut self, tree: &mut RetainedTree, damage: &mut DamageSet, screen: Rect) {
        if let CaptureKind::Button(id)
        | CaptureKind::ScrollCandidate { candidate: id, .. }
        | CaptureKind::CustomPointer { id, .. } = self.capture
            && tree.is_id_valid(id)
            && tree.nodes[id.slot() as usize].pressed
        {
            let slot = id.slot() as usize;
            tree.nodes[slot].pressed = false;
            damage.add(tree.nodes[slot].visible_rect, screen);
        }
        self.set_active_id(None);
        self.capture = CaptureKind::None;
    }

    pub fn captured_id(&self) -> Option<NodeId> {
        match self.capture {
            CaptureKind::None => None,
            CaptureKind::Button(id)
            | CaptureKind::Slider { id }
            | CaptureKind::Scrolling { scroll: id, .. }
            | CaptureKind::CustomPointer { id, .. } => Some(id),
            CaptureKind::ScrollCandidate { candidate, .. } => Some(candidate),
        }
    }

    pub fn captured_slot(&self) -> Option<u16> {
        self.captured_id().map(NodeId::slot)
    }
}

impl Default for PointerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FocusState {
    has_focus: bool,
    focused_key: Key,
    pub ring_visible: bool,
    pub editing: bool,
    has_modal_trigger: bool,
    modal_saved_key: Key,
    modal_saved_ring: bool,
}

impl FocusState {
    pub const ZERO: Self = Self {
        has_focus: false,
        focused_key: Key(0),
        ring_visible: false,
        editing: false,
        has_modal_trigger: false,
        modal_saved_key: Key(0),
        modal_saved_ring: false,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    #[inline]
    pub const fn focused_key(&self) -> Option<Key> {
        if self.has_focus {
            Some(self.focused_key)
        } else {
            None
        }
    }

    #[inline]
    pub fn set_focused_key(&mut self, key: Option<Key>) {
        match key {
            Some(k) => {
                self.has_focus = true;
                self.focused_key = k;
            }
            None => {
                self.has_focus = false;
                self.focused_key = Key(0);
            }
        }
    }

    #[inline]
    pub const fn modal_saved_trigger(&self) -> Option<(Key, bool)> {
        if self.has_modal_trigger {
            Some((self.modal_saved_key, self.modal_saved_ring))
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        *self = Self::ZERO;
    }

    pub fn focused_slot(&self, tree: &RetainedTree) -> Option<u16> {
        self.focused_key().and_then(|k| tree.find_slot_by_key(k))
    }

    pub fn ensure_valid(&mut self, tree: &RetainedTree) {
        let Some(scope_root) = active_scope_root(tree) else {
            self.set_focused_key(None);
            self.editing = false;
            return;
        };
        if let Some(slot) = self.focused_slot(tree)
            && is_focus_eligible_in_scope(tree, slot, scope_root)
        {
            return;
        }
        self.editing = false;
        let first = first_focusable_in_scope(tree, scope_root).map(|s| tree.nodes[s as usize].key);
        self.set_focused_key(first);
    }

    pub fn on_modal_opened(&mut self, tree: &RetainedTree) {
        if !self.has_modal_trigger
            && let Some(prev) = self.focused_key()
        {
            self.has_modal_trigger = true;
            self.modal_saved_key = prev;
            self.modal_saved_ring = self.ring_visible;
        }
        self.editing = false;
        if let Some(modal_slot) = tree.modal.get_u16() {
            let first =
                first_focusable_in_scope(tree, modal_slot).map(|s| tree.nodes[s as usize].key);
            self.set_focused_key(first);
        }
    }

    pub fn on_modal_closed(
        &mut self,
        tree: &mut RetainedTree,
        damage: &mut DamageSet,
        screen: Rect,
    ) {
        self.on_modal_closed_with_filter(tree, damage, screen, |_| false);
    }

    pub fn on_modal_closed_with_filter(
        &mut self,
        tree: &mut RetainedTree,
        damage: &mut DamageSet,
        screen: Rect,
        is_tab: impl Fn(Key) -> bool,
    ) {
        self.editing = false;
        if self.has_modal_trigger {
            let trigger_key = self.modal_saved_key;
            let saved_ring = self.modal_saved_ring;
            self.has_modal_trigger = false;
            self.modal_saved_key = Key(0);
            self.modal_saved_ring = false;
            self.ring_visible = saved_ring;

            if let Some(root_slot) = tree.root.get_u16()
                && let Some(slot) = tree.find_slot_by_key(trigger_key)
                && is_focus_eligible_in_scope(tree, slot, root_slot)
            {
                self.set_focused_key(Some(trigger_key));
                if let Some(scroll_slot) = ensure_visible(tree, slot, screen) {
                    damage.add(tree.nodes[scroll_slot as usize].visible_rect, screen);
                }
                return;
            }
        }
        let first = self
            .first_content_focusable(tree, is_tab)
            .map(|s| tree.nodes[s as usize].key);
        self.set_focused_key(first);
    }

    #[inline]
    pub fn clear_modal_trigger(&mut self) {
        self.has_modal_trigger = false;
        self.modal_saved_key = Key(0);
        self.modal_saved_ring = false;
    }

    pub fn is_slot_eligible_in_scope(&self, tree: &RetainedTree, slot: u16) -> bool {
        let Some(scope_root) = active_scope_root(tree) else {
            return false;
        };
        is_focus_eligible_in_scope(tree, slot, scope_root)
    }

    pub fn first_content_focusable(
        &self,
        tree: &RetainedTree,
        is_tab: impl Fn(Key) -> bool,
    ) -> Option<u16> {
        let scope_root = active_scope_root(tree)?;
        let mut list = [0u16; MAX_NODES];
        let cnt = collect_focusables_in_scope(tree, scope_root, &mut list);
        if cnt == 0 {
            return None;
        }
        for &slot in &list[..cnt] {
            if !is_tab(tree.nodes[slot as usize].key) {
                return Some(slot);
            }
        }
        Some(list[0])
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum RepeatSource {
    #[default]
    None = 0,
    Pointer = 1,
    Key = 2,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RepeatState {
    pub source: RepeatSource,
    pub pointer_id: PointerId,
    pub hold_key: HoldKey,
    pub fired_any: bool,
    pub target_id: NodeId,
    pub key: Key,
    pub adjust_delta: i16,
    pub next_fire_ms: InstantMs,
}

impl RepeatState {
    pub const ZERO: Self = Self {
        source: RepeatSource::None,
        pointer_id: PointerId(0),
        hold_key: HoldKey::Activate,
        fired_any: false,
        target_id: NodeId::ZERO,
        key: Key(0),
        adjust_delta: 0,
        next_fire_ms: InstantMs(0),
    };

    #[inline]
    pub const fn is_active(&self) -> bool {
        !matches!(self.source, RepeatSource::None)
    }

    #[inline]
    pub fn clear(&mut self) {
        *self = Self::ZERO;
    }

    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        if !self.is_active() {
            NextWakeup::OnEvent
        } else if now >= self.next_fire_ms {
            NextWakeup::Now
        } else {
            NextWakeup::At(self.next_fire_ms)
        }
    }
}

impl Default for RepeatState {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Default for FocusState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn active_scope_root(tree: &RetainedTree) -> Option<u16> {
    if let Some(m) = tree.modal.get_u16()
        && tree.nodes[m as usize].occupied
    {
        return Some(m);
    }
    tree.root.get_u16()
}

fn is_focus_eligible_in_scope(tree: &RetainedTree, slot: u16, scope_root: u16) -> bool {
    let n = &tree.nodes[slot as usize];
    n.occupied
        && n.props.focusable()
        && tree.is_effectively_enabled_and_visible(slot)
        && tree.is_descendant_of(slot, scope_root)
}

#[derive(Copy, Clone)]
struct WalkCursor {
    slot: u8,
    entered: bool,
    next_child: SlotLink,
}

impl WalkCursor {
    const EMPTY: Self = Self {
        slot: 0,
        entered: false,
        next_child: SlotLink::NONE,
    };
}

pub fn collect_focusables_in_scope(
    tree: &RetainedTree,
    scope_root: u16,
    out: &mut [u16; MAX_NODES],
) -> usize {
    if (scope_root as usize) >= MAX_NODES || !tree.nodes[scope_root as usize].occupied {
        return 0;
    }
    let modal_skip = tree.root.get_u16() == Some(scope_root);
    let mut stack = [WalkCursor::EMPTY; MAX_DEPTH + 2];
    let mut sp = 0usize;
    let mut count = 0usize;

    stack[sp] = WalkCursor {
        slot: scope_root as u8,
        entered: false,
        next_child: SlotLink::NONE,
    };
    sp += 1;

    while sp > 0 {
        let top = sp - 1;
        let slot = stack[top].slot as usize;
        let n = &tree.nodes[slot];

        if !stack[top].entered {
            if !n.occupied
                || !n.props.visible()
                || !n.props.enabled()
                || (modal_skip && n.kind == NodeKind::Modal)
            {
                sp -= 1;
                continue;
            }
            if n.props.focusable() {
                out[count] = slot as u16;
                count += 1;
            }
            stack[top].entered = true;
            stack[top].next_child = n.first_child;
        } else {
            let next_link = stack[top].next_child;
            let Some(ch) = next_link.get() else {
                sp -= 1;
                continue;
            };
            stack[top].next_child = tree.nodes[ch].next_sibling;
            if sp < stack.len() {
                stack[sp] = WalkCursor {
                    slot: ch as u8,
                    entered: false,
                    next_child: SlotLink::NONE,
                };
                sp += 1;
            }
        }
    }
    count
}

fn first_focusable_in_scope(tree: &RetainedTree, scope_root: u16) -> Option<u16> {
    if (scope_root as usize) >= MAX_NODES || !tree.nodes[scope_root as usize].occupied {
        return None;
    }
    let modal_skip = tree.root.get_u16() == Some(scope_root);
    let mut stack = [WalkCursor::EMPTY; MAX_DEPTH + 2];
    let mut sp = 0usize;

    stack[sp] = WalkCursor {
        slot: scope_root as u8,
        entered: false,
        next_child: SlotLink::NONE,
    };
    sp += 1;

    while sp > 0 {
        let top = sp - 1;
        let slot = stack[top].slot as usize;
        let n = &tree.nodes[slot];

        if !stack[top].entered {
            if !n.occupied
                || !n.props.visible()
                || !n.props.enabled()
                || (modal_skip && n.kind == NodeKind::Modal)
            {
                sp -= 1;
                continue;
            }
            if n.props.focusable() {
                return Some(slot as u16);
            }
            stack[top].entered = true;
            stack[top].next_child = n.first_child;
        } else {
            let Some(ch) = stack[top].next_child.get() else {
                sp -= 1;
                continue;
            };
            stack[top].next_child = tree.nodes[ch].next_sibling;
            if sp < stack.len() {
                stack[sp] = WalkCursor {
                    slot: ch as u8,
                    entered: false,
                    next_child: SlotLink::NONE,
                };
                sp += 1;
            }
        }
    }
    None
}

#[inline]
fn build_pointer_event(
    tree: &RetainedTree,
    slot: u16,
    phase: PointerPhase,
    pos: Point,
    start_pos: Point,
) -> (Key, PointerEvent) {
    let n = &tree.nodes[slot as usize];
    let projected = tree.local_point(slot, pos).unwrap_or(pos);
    let local = Point::new(projected.x - n.layout_rect.x, projected.y - n.layout_rect.y);
    let delta = Point::new(pos.x - start_pos.x, pos.y - start_pos.y);
    (
        n.key,
        PointerEvent {
            phase,
            local,
            delta,
            size: n.layout_rect.size(),
        },
    )
}

pub fn dispatch_input(
    tree: &mut RetainedTree,
    focus: &mut FocusState,
    pointer: &mut PointerState,
    input: RawInput,
    screen: Rect,
    damage: &mut DamageSet,
) -> (bool, Option<UiAction>) {
    let (consumed, action, _) = dispatch_input_ext(tree, focus, pointer, input, screen, damage);
    (consumed, action)
}

pub fn dispatch_input_ext(
    tree: &mut RetainedTree,
    focus: &mut FocusState,
    pointer: &mut PointerState,
    input: RawInput,
    screen: Rect,
    damage: &mut DamageSet,
) -> (bool, Option<UiAction>, Option<(Key, PointerEvent)>) {
    focus.ensure_valid(tree);
    let Some(scope_root) = active_scope_root(tree) else {
        return (false, None, None);
    };

    match input {
        RawInput::Nav(dir) => {
            if focus.editing
                && let Some(slot) = focus.focused_slot(tree)
            {
                let node = &tree.nodes[slot as usize];
                if node.kind == NodeKind::Slider {
                    let delta = if dir == NavDir::Next { 1 } else { -1 };
                    let (c, a) = adjust_slider_by_steps(tree, focus, slot, delta, damage, screen);
                    return (c, a, None);
                } else if node.props.editable() {
                    let delta = if dir == NavDir::Next { 1 } else { -1 };
                    damage.add(node.visible_rect, screen);
                    return (
                        true,
                        Some(UiAction {
                            key: node.key,
                            kind: ActionKind::Adjust(delta),
                        }),
                        None,
                    );
                }
            }

            let mut list = [0u16; MAX_NODES];
            let cnt = collect_focusables_in_scope(tree, scope_root, &mut list);
            if cnt == 0 {
                return (false, None, None);
            }

            let old_slot = focus.focused_slot(tree);
            let old_ring = focus.ring_visible;

            let next_slot = match old_slot.and_then(|s| list[..cnt].iter().position(|&x| x == s)) {
                Some(idx) => match dir {
                    NavDir::Next => list[(idx + 1) % cnt],
                    NavDir::Prev => list[(idx + cnt - 1) % cnt],
                },
                None => list[0],
            };

            focus.set_focused_key(Some(tree.nodes[next_slot as usize].key));
            focus.ring_visible = true;

            if let Some(os) = old_slot
                && (os != next_slot || !old_ring)
            {
                damage.add(tree.nodes[os as usize].visible_rect, screen);
            }
            if let Some(scroll_slot) = ensure_visible(tree, next_slot, screen) {
                damage.add(tree.nodes[scroll_slot as usize].visible_rect, screen);
            }
            damage.add(tree.nodes[next_slot as usize].visible_rect, screen);
            (true, None, None)
        }
        RawInput::Adjust(steps) => {
            let Some(slot) = focus.focused_slot(tree) else {
                return (false, None, None);
            };
            if focus.editing {
                let node = &tree.nodes[slot as usize];
                if node.kind == NodeKind::Slider {
                    let (c, a) = adjust_slider_by_steps(tree, focus, slot, steps, damage, screen);
                    return (c, a, None);
                } else if node.props.editable() {
                    if steps == 0 {
                        return (true, None, None);
                    }
                    damage.add(node.visible_rect, screen);
                    return (
                        true,
                        Some(UiAction {
                            key: node.key,
                            kind: ActionKind::Adjust(steps),
                        }),
                        None,
                    );
                }
            }
            if steps == 0 {
                return (true, None, None);
            }
            let dir = if steps > 0 {
                NavDir::Next
            } else {
                NavDir::Prev
            };
            let count = steps.unsigned_abs();
            let mut consumed = false;
            for _ in 0..count {
                let (c, _, _) =
                    dispatch_input_ext(tree, focus, pointer, RawInput::Nav(dir), screen, damage);
                consumed |= c;
            }
            (consumed, None, None)
        }
        RawInput::Activate => {
            let Some(slot) = focus.focused_slot(tree) else {
                return (false, None, None);
            };
            if !focus.ring_visible {
                focus.ring_visible = true;
                damage.add(tree.nodes[slot as usize].visible_rect, screen);
            }
            let node = &tree.nodes[slot as usize];
            if node.kind == NodeKind::Slider || node.props.editable() {
                focus.editing = !focus.editing;
                damage.add(node.visible_rect, screen);
                return (true, None, None);
            }
            match node.kind {
                NodeKind::Toggle => (
                    true,
                    Some(UiAction {
                        key: node.key,
                        kind: ActionKind::Toggle,
                    }),
                    None,
                ),
                NodeKind::Button
                | NodeKind::Row
                | NodeKind::Column
                | NodeKind::Stack
                | NodeKind::Wrap
                | NodeKind::CustomContainer
                | NodeKind::Custom => (
                    true,
                    Some(UiAction {
                        key: node.key,
                        kind: ActionKind::Activate,
                    }),
                    None,
                ),
                _ => (false, None, None),
            }
        }
        RawInput::Back => {
            if focus.editing {
                focus.editing = false;
                if let Some(slot) = focus.focused_slot(tree) {
                    damage.add(tree.nodes[slot as usize].visible_rect, screen);
                }
                return (true, None, None);
            }
            if tree.modal.get_u16() == Some(scope_root) {
                return (
                    true,
                    Some(UiAction {
                        key: tree.nodes[scope_root as usize].key,
                        kind: ActionKind::Dismiss,
                    }),
                    None,
                );
            }
            if let Some(root_slot) = tree.root.get_u16() {
                return (
                    false,
                    Some(UiAction {
                        key: tree.nodes[root_slot as usize].key,
                        kind: ActionKind::Dismiss,
                    }),
                    None,
                );
            }
            (false, None, None)
        }
        RawInput::KeyDown(hold_key) => match hold_key {
            HoldKey::AdjustPrev => {
                dispatch_input_ext(tree, focus, pointer, RawInput::Adjust(-1), screen, damage)
            }
            HoldKey::AdjustNext => {
                dispatch_input_ext(tree, focus, pointer, RawInput::Adjust(1), screen, damage)
            }
            HoldKey::Activate => {
                dispatch_input_ext(tree, focus, pointer, RawInput::Activate, screen, damage)
            }
        },
        RawInput::KeyUp(_) => (true, None, None),
        RawInput::PointerDown { id, pos } => {
            if pointer.active_id().is_some() {
                return (false, None, None);
            }

            let is_modal_scope = tree.modal.get_u16() == Some(scope_root);
            let hit = hit_test_in_scope(tree, scope_root, pos);
            let Some(hit_slot) = hit else {
                if is_modal_scope {
                    pointer.set_active_id(Some(id));
                    pointer.capture = CaptureKind::None;
                    return (true, None, None);
                }
                return (false, None, None);
            };

            pointer.set_active_id(Some(id));

            if focus.editing && focus.focused_slot(tree) != Some(hit_slot) {
                if let Some(old_slot) = focus.focused_slot(tree) {
                    damage.add(tree.nodes[old_slot as usize].visible_rect, screen);
                }
                focus.editing = false;
            }

            if focus.ring_visible {
                if let Some(old_slot) = focus.focused_slot(tree) {
                    damage.add(tree.nodes[old_slot as usize].visible_rect, screen);
                }
                focus.ring_visible = false;
            }

            let hit_kind = tree.nodes[hit_slot as usize].kind;
            let hit_cap = tree.nodes[hit_slot as usize].props.pointer_capture();
            let scroll_anc = if hit_kind == NodeKind::ScrollY {
                Some(hit_slot)
            } else {
                find_scroll_ancestor(tree, hit_slot, scope_root)
            };
            let hit_id = tree.nodes[hit_slot as usize].id(hit_slot);

            if hit_kind == NodeKind::Modal {
                pointer.capture = CaptureKind::None;
                return (true, None, None);
            }

            if let Some(scroll_slot) = scroll_anc {
                if hit_cap == PointerCaptureMode::Free {
                    if tree.nodes[hit_slot as usize].props.focusable() {
                        focus.set_focused_key(Some(tree.nodes[hit_slot as usize].key));
                    }
                    tree.nodes[hit_slot as usize].pressed = true;
                    damage.add(tree.nodes[hit_slot as usize].visible_rect, screen);
                    pointer.capture = CaptureKind::CustomPointer {
                        id: hit_id,
                        start_pos: pos,
                    };
                    let pev = build_pointer_event(tree, hit_slot, PointerPhase::Down, pos, pos);
                    return (true, None, Some(pev));
                }
                let scroll_id = tree.nodes[scroll_slot as usize].id(scroll_slot);
                let is_slider = hit_kind == NodeKind::Slider;
                if is_interactive_leaf_or_row(&tree.nodes[hit_slot as usize]) {
                    tree.nodes[hit_slot as usize].pressed = true;
                    damage.add(tree.nodes[hit_slot as usize].visible_rect, screen);
                }
                if tree.nodes[hit_slot as usize].props.focusable() {
                    focus.set_focused_key(Some(tree.nodes[hit_slot as usize].key));
                }
                pointer.capture = CaptureKind::ScrollCandidate {
                    candidate: hit_id,
                    scroll: scroll_id,
                    start_pos: pos,
                    start_offset: tree.nodes[scroll_slot as usize].scroll_y,
                    slider_candidate: is_slider,
                };
                (true, None, None)
            } else if hit_cap != PointerCaptureMode::None {
                if tree.nodes[hit_slot as usize].props.focusable() {
                    focus.set_focused_key(Some(tree.nodes[hit_slot as usize].key));
                }
                tree.nodes[hit_slot as usize].pressed = true;
                damage.add(tree.nodes[hit_slot as usize].visible_rect, screen);
                pointer.capture = CaptureKind::CustomPointer {
                    id: hit_id,
                    start_pos: pos,
                };
                let pev = build_pointer_event(tree, hit_slot, PointerPhase::Down, pos, pos);
                (true, None, Some(pev))
            } else if hit_kind == NodeKind::Slider {
                if tree.nodes[hit_slot as usize].props.focusable() {
                    focus.set_focused_key(Some(tree.nodes[hit_slot as usize].key));
                }
                pointer.capture = CaptureKind::Slider { id: hit_id };
                let (_, action) = update_slider_from_pos(tree, hit_slot, pos, damage, screen);
                (true, action, None)
            } else if is_interactive_leaf_or_row(&tree.nodes[hit_slot as usize]) {
                if tree.nodes[hit_slot as usize].props.focusable() {
                    focus.set_focused_key(Some(tree.nodes[hit_slot as usize].key));
                }
                tree.nodes[hit_slot as usize].pressed = true;
                damage.add(tree.nodes[hit_slot as usize].visible_rect, screen);
                pointer.capture = CaptureKind::Button(hit_id);
                (true, None, None)
            } else {
                pointer.capture = CaptureKind::None;
                (is_modal_scope, None, None)
            }
        }
        RawInput::PointerMove { id, pos } => {
            if pointer.active_id() != Some(id) {
                return (false, None, None);
            }

            match pointer.capture {
                CaptureKind::None => (false, None, None),
                CaptureKind::Button(target_id) => {
                    let Some(slot) = capture_id_in_scope(tree, target_id, scope_root) else {
                        pointer.clear(tree, damage, screen);
                        return (false, None, None);
                    };
                    let inside = pointer_hits_slot(tree, slot, pos);
                    if tree.nodes[slot as usize].pressed != inside {
                        tree.nodes[slot as usize].pressed = inside;
                        damage.add(tree.nodes[slot as usize].visible_rect, screen);
                    }
                    (true, None, None)
                }
                CaptureKind::CustomPointer {
                    id: target_id,
                    start_pos,
                } => {
                    let Some(slot) = capture_id_in_scope(tree, target_id, scope_root) else {
                        pointer.clear(tree, damage, screen);
                        return (false, None, None);
                    };
                    let pev = build_pointer_event(tree, slot, PointerPhase::Move, pos, start_pos);
                    (true, None, Some(pev))
                }
                CaptureKind::Slider { id: target_id } => {
                    let Some(slot) = capture_id_in_scope(tree, target_id, scope_root) else {
                        pointer.clear(tree, damage, screen);
                        return (false, None, None);
                    };
                    let (_, action) = update_slider_from_pos(tree, slot, pos, damage, screen);
                    (true, action, None)
                }
                CaptureKind::ScrollCandidate {
                    candidate,
                    scroll,
                    start_pos,
                    start_offset,
                    slider_candidate,
                } => {
                    let Some(candidate_slot) = capture_id_in_scope(tree, candidate, scope_root)
                    else {
                        pointer.clear(tree, damage, screen);
                        return (false, None, None);
                    };
                    let Some(scroll_slot) = capture_id_in_scope(tree, scroll, scope_root) else {
                        pointer.clear(tree, damage, screen);
                        return (false, None, None);
                    };

                    let dx = pos.x.saturating_sub(start_pos.x);
                    let dy = pos.y.saturating_sub(start_pos.y);
                    let adx = dx.saturating_abs();
                    let ady = dy.saturating_abs();

                    if tree.nodes[candidate_slot as usize].props.swipeable()
                        && adx >= DRAG_THRESHOLD_PX
                        && adx > ady
                    {
                        release_pressed_if_valid(tree, candidate, damage, screen);
                        pointer.capture = CaptureKind::None;
                        return (
                            true,
                            Some(UiAction {
                                key: tree.nodes[candidate_slot as usize].key,
                                kind: if dx < 0 {
                                    ActionKind::SwipeLeft
                                } else {
                                    ActionKind::SwipeRight
                                },
                            }),
                            None,
                        );
                    }
                    let cap_mode = tree.nodes[candidate_slot as usize].props.pointer_capture();
                    let want_custom_lock = match cap_mode {
                        PointerCaptureMode::LockX => adx >= DRAG_THRESHOLD_PX && adx > ady,
                        PointerCaptureMode::LockY => ady >= DRAG_THRESHOLD_PX && ady >= adx,
                        _ => false,
                    };
                    if want_custom_lock {
                        pointer.capture = CaptureKind::CustomPointer {
                            id: candidate,
                            start_pos,
                        };
                        let pev = build_pointer_event(
                            tree,
                            candidate_slot,
                            PointerPhase::Down,
                            pos,
                            start_pos,
                        );
                        return (true, None, Some(pev));
                    }
                    if slider_candidate && adx >= DRAG_THRESHOLD_PX && adx > ady {
                        if tree.nodes[candidate_slot as usize].pressed {
                            tree.nodes[candidate_slot as usize].pressed = false;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                        pointer.capture = CaptureKind::Slider { id: candidate };
                        let (_, action) =
                            update_slider_from_pos(tree, candidate_slot, pos, damage, screen);
                        return (true, action, None);
                    }

                    if ady >= DRAG_THRESHOLD_PX && ady >= adx {
                        if tree.nodes[candidate_slot as usize].pressed {
                            tree.nodes[candidate_slot as usize].pressed = false;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                        pointer.capture = CaptureKind::Scrolling {
                            scroll,
                            start_pos,
                            start_offset,
                        };
                        apply_scroll_drag(
                            tree,
                            scroll_slot,
                            start_offset.saturating_sub(dy),
                            screen,
                            damage,
                        );
                        return (true, None, None);
                    }

                    if is_interactive_leaf_or_row(&tree.nodes[candidate_slot as usize]) {
                        let inside = pointer_hits_slot(tree, candidate_slot, pos);
                        if tree.nodes[candidate_slot as usize].pressed != inside {
                            tree.nodes[candidate_slot as usize].pressed = inside;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                    }
                    (true, None, None)
                }
                CaptureKind::Scrolling {
                    scroll,
                    start_pos,
                    start_offset,
                } => {
                    let Some(scroll_slot) = capture_id_in_scope(tree, scroll, scope_root) else {
                        pointer.clear(tree, damage, screen);
                        return (false, None, None);
                    };
                    let dy = pos.y.saturating_sub(start_pos.y);
                    apply_scroll_drag(
                        tree,
                        scroll_slot,
                        start_offset.saturating_sub(dy),
                        screen,
                        damage,
                    );
                    (true, None, None)
                }
            }
        }
        RawInput::PointerUp { id, pos } => {
            if pointer.active_id() != Some(id) {
                return (false, None, None);
            }
            let cap = pointer.capture;
            pointer.set_active_id(None);
            pointer.capture = CaptureKind::None;

            match cap {
                CaptureKind::None => (false, None, None),
                CaptureKind::Button(target_id) => {
                    if let Some(slot) = capture_id_in_scope(tree, target_id, scope_root) {
                        let (c, a) = activate_if_hit_on_up(tree, slot, pos, damage, screen);
                        (c, a, None)
                    } else {
                        release_pressed_if_valid(tree, target_id, damage, screen);
                        (false, None, None)
                    }
                }
                CaptureKind::CustomPointer {
                    id: target_id,
                    start_pos,
                } => {
                    if let Some(slot) = capture_id_in_scope(tree, target_id, scope_root) {
                        tree.nodes[slot as usize].pressed = false;
                        damage.add(tree.nodes[slot as usize].visible_rect, screen);
                        let pev = build_pointer_event(tree, slot, PointerPhase::Up, pos, start_pos);
                        (true, None, Some(pev))
                    } else {
                        release_pressed_if_valid(tree, target_id, damage, screen);
                        (false, None, None)
                    }
                }
                CaptureKind::Slider { id: target_id } => {
                    let Some(slot) = capture_id_in_scope(tree, target_id, scope_root) else {
                        return (false, None, None);
                    };
                    let (_, action) = update_slider_from_pos(tree, slot, pos, damage, screen);
                    (true, action, None)
                }
                CaptureKind::ScrollCandidate { candidate, .. } => {
                    if let Some(candidate_slot) = capture_id_in_scope(tree, candidate, scope_root) {
                        if tree.nodes[candidate_slot as usize].kind == NodeKind::Slider {
                            let (_, action) =
                                update_slider_from_pos(tree, candidate_slot, pos, damage, screen);
                            (true, action, None)
                        } else if !tree.nodes[candidate_slot as usize].props.clickable()
                            && tree.nodes[candidate_slot as usize].props.pointer_capture()
                                != PointerCaptureMode::None
                        {
                            tree.nodes[candidate_slot as usize].pressed = false;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                            let pev = build_pointer_event(
                                tree,
                                candidate_slot,
                                PointerPhase::Up,
                                pos,
                                pos,
                            );
                            (true, None, Some(pev))
                        } else {
                            let (c, a) =
                                activate_if_hit_on_up(tree, candidate_slot, pos, damage, screen);
                            (c, a, None)
                        }
                    } else {
                        release_pressed_if_valid(tree, candidate, damage, screen);
                        (false, None, None)
                    }
                }
                CaptureKind::Scrolling { scroll, .. } => {
                    if capture_id_in_scope(tree, scroll, scope_root).is_some() {
                        (true, None, None)
                    } else {
                        (false, None, None)
                    }
                }
            }
        }
        RawInput::PointerCancel { id } => {
            if pointer.active_id() != Some(id) {
                return (false, None, None);
            }
            let cancel_ev = if let CaptureKind::CustomPointer {
                id: target_id,
                start_pos,
            } = pointer.capture
                && let Some(slot) = capture_id_in_scope(tree, target_id, scope_root)
            {
                Some(build_pointer_event(
                    tree,
                    slot,
                    PointerPhase::Cancel,
                    start_pos,
                    start_pos,
                ))
            } else {
                None
            };
            pointer.clear(tree, damage, screen);
            (true, None, cancel_ev)
        }
    }
}

#[inline]
fn capture_id_in_scope(tree: &RetainedTree, id: NodeId, scope_root: u16) -> Option<u16> {
    if tree.is_id_valid(id) && tree.is_descendant_of(id.slot(), scope_root) {
        Some(id.slot())
    } else {
        None
    }
}

#[inline]
fn release_pressed_if_valid(
    tree: &mut RetainedTree,
    id: NodeId,
    damage: &mut DamageSet,
    screen: Rect,
) {
    if tree.is_id_valid(id) {
        let slot = id.slot() as usize;
        if tree.nodes[slot].pressed {
            tree.nodes[slot].pressed = false;
            damage.add(tree.nodes[slot].visible_rect, screen);
        }
    }
}

pub fn adjust_slider_by_steps_pub(
    tree: &mut RetainedTree,
    slot: u16,
    steps: i16,
    damage: &mut DamageSet,
    screen: Rect,
) -> (bool, Option<UiAction>) {
    let dummy = FocusState::ZERO;
    adjust_slider_by_steps(tree, &dummy, slot, steps, damage, screen)
}

pub fn pointer_hits_id_in_scope(tree: &RetainedTree, id: NodeId, pos: Point) -> bool {
    let Some(scope_root) = active_scope_root(tree) else {
        return false;
    };
    let Some(slot) = capture_id_in_scope(tree, id, scope_root) else {
        return false;
    };
    pointer_hits_slot(tree, slot, pos)
}

pub fn is_valid_repeat_target(
    tree: &RetainedTree,
    focus: &FocusState,
    pointer: &PointerState,
    repeat: &RepeatState,
) -> bool {
    let Some(scope_root) = active_scope_root(tree) else {
        return false;
    };
    match repeat.source {
        RepeatSource::None => false,
        RepeatSource::Pointer => {
            if pointer.active_id() != Some(repeat.pointer_id)
                || pointer.captured_id() != Some(repeat.target_id)
            {
                return false;
            }
            let Some(slot) = capture_id_in_scope(tree, repeat.target_id, scope_root) else {
                return false;
            };
            let n = &tree.nodes[slot as usize];
            n.occupied
                && n.pressed
                && n.key == repeat.key
                && is_interactive_leaf_or_row(n)
                && (n.props.repeatable() || n.props.long_press())
                && tree.is_effectively_enabled_and_visible(slot)
        }
        RepeatSource::Key => {
            if focus.focused_key() != Some(repeat.key) {
                return false;
            }
            let Some(slot) = focus.focused_slot(tree) else {
                return false;
            };
            if !is_focus_eligible_in_scope(tree, slot, scope_root) {
                return false;
            }
            let n = &tree.nodes[slot as usize];
            if repeat.adjust_delta != 0 {
                focus.editing && (n.kind == NodeKind::Slider || n.props.editable())
            } else {
                is_interactive_leaf_or_row(n) && n.props.repeatable()
            }
        }
    }
}

fn adjust_slider_by_steps(
    tree: &mut RetainedTree,
    _focus: &FocusState,
    slot: u16,
    steps: i16,
    damage: &mut DamageSet,
    screen: Rect,
) -> (bool, Option<UiAction>) {
    let n = &tree.nodes[slot as usize];
    if n.props.min >= n.props.max || n.props.step <= 0 {
        return (true, None);
    }
    let min = n.props.min as i64;
    let max = n.props.max as i64;
    let step = n.props.step as i64;
    let new_val = (n.props.value as i64 + (steps as i64) * step).clamp(min, max) as i16;
    if new_val == n.props.value {
        return (true, None);
    }
    let key = n.key;
    damage.add(n.visible_rect, screen);
    (
        true,
        Some(UiAction {
            key,
            kind: ActionKind::SetValue(new_val),
        }),
    )
}

fn update_slider_from_pos(
    tree: &mut RetainedTree,
    slot: u16,
    pos: Point,
    damage: &mut DamageSet,
    screen: Rect,
) -> (bool, Option<UiAction>) {
    let n = &tree.nodes[slot as usize];
    if !n.occupied || !tree.is_effectively_enabled_and_visible(slot) {
        return (false, None);
    }
    if n.props.min >= n.props.max || n.props.step <= 0 {
        return (true, None);
    }
    let rect = n.layout_rect;
    let Some(pos) = tree.local_point(slot, pos) else {
        return (true, None);
    };
    let knob_w = (rect.w as i32).min(12);
    let usable = (rect.w as i32 - knob_w).max(1);
    let rel_x = (pos.x - (rect.x + knob_w / 2)).clamp(0, usable);
    let min = n.props.min as i64;
    let max = n.props.max as i64;
    let span = (max - min).max(1);
    let raw_val = min + ((rel_x as i64 * span + (usable as i64 / 2)) / usable as i64);
    let step = n.props.step as i64;
    let offset_from_min = raw_val - min;
    let snapped_steps = (offset_from_min + step / 2) / step;
    let snapped = (min + snapped_steps * step).clamp(min, max) as i16;

    if snapped == n.props.value {
        return (true, None);
    }
    damage.add(n.visible_rect, screen);
    (
        true,
        Some(UiAction {
            key: n.key,
            kind: ActionKind::SetValue(snapped),
        }),
    )
}

fn activate_if_hit_on_up(
    tree: &mut RetainedTree,
    slot: u16,
    pos: Point,
    damage: &mut DamageSet,
    screen: Rect,
) -> (bool, Option<UiAction>) {
    if (slot as usize) >= MAX_NODES || !tree.nodes[slot as usize].occupied {
        return (false, None);
    }
    let was_pressed = tree.nodes[slot as usize].pressed;
    tree.nodes[slot as usize].pressed = false;
    damage.add(tree.nodes[slot as usize].visible_rect, screen);

    if was_pressed
        && tree.is_effectively_enabled_and_visible(slot)
        && pointer_hits_slot(tree, slot, pos)
    {
        let n = &tree.nodes[slot as usize];
        let kind = match n.kind {
            NodeKind::Toggle => ActionKind::Toggle,
            _ => ActionKind::Activate,
        };
        return (true, Some(UiAction { key: n.key, kind }));
    }
    (true, None)
}

fn apply_scroll_drag(
    tree: &mut RetainedTree,
    scroll_slot: u16,
    desired_offset: i32,
    screen: Rect,
    damage: &mut DamageSet,
) {
    let sv = &tree.nodes[scroll_slot as usize];
    let max_scroll = sv.content_height.saturating_sub(sv.layout_rect.h) as i32;
    let new_offset = desired_offset.clamp(0, max_scroll);
    if new_offset != sv.scroll_y {
        tree.nodes[scroll_slot as usize].scroll_y = new_offset;
        relayout_scroll_subtree(tree, scroll_slot, screen);
        damage.add(tree.nodes[scroll_slot as usize].visible_rect, screen);
    }
}

pub fn is_interactive_leaf_or_row(n: &crate::node::RetainedNode) -> bool {
    matches!(
        n.kind,
        NodeKind::Button | NodeKind::Toggle | NodeKind::Slider
    ) || n.props.clickable()
        || n.props.pointer_capture() != PointerCaptureMode::None
        || n.props.editable()
}

fn pointer_hits_slot(tree: &RetainedTree, slot: u16, pos: Point) -> bool {
    let n = &tree.nodes[slot as usize];
    n.occupied
        && tree.is_effectively_enabled_and_visible(slot)
        && tree
            .local_point(slot, pos)
            .is_some_and(|p| n.layout_rect.intersect(n.clip_rect).contains_point(p))
}

fn find_scroll_ancestor(tree: &RetainedTree, slot: u16, scope_root: u16) -> Option<u16> {
    let mut cur = tree.nodes[slot as usize].parent;
    while let Some(idx) = cur.get_u16() {
        if tree.nodes[idx as usize].kind == NodeKind::ScrollY {
            return Some(idx);
        }
        if idx == scope_root {
            break;
        }
        cur = tree.nodes[idx as usize].parent;
    }
    None
}

pub fn hit_test_in_scope(tree: &RetainedTree, scope_root: u16, pos: Point) -> Option<u16> {
    if (scope_root as usize) >= MAX_NODES || !tree.nodes[scope_root as usize].occupied {
        return None;
    }
    let modal_skip = tree.root.get_u16() == Some(scope_root);
    let mut order = [0u8; MAX_NODES];
    let mut order_len = 0usize;

    let mut stack = [WalkCursor::EMPTY; MAX_DEPTH + 2];
    let mut sp = 0usize;
    stack[sp] = WalkCursor {
        slot: scope_root as u8,
        entered: false,
        next_child: SlotLink::NONE,
    };
    sp += 1;

    while sp > 0 {
        let top = sp - 1;
        let slot = stack[top].slot as usize;
        let n = &tree.nodes[slot];

        if !stack[top].entered {
            if !n.occupied || !n.props.visible() || (modal_skip && n.kind == NodeKind::Modal) {
                sp -= 1;
                continue;
            }
            order[order_len] = slot as u8;
            order_len += 1;
            stack[top].entered = true;
            stack[top].next_child = n.first_child;
        } else {
            let Some(ch) = stack[top].next_child.get() else {
                sp -= 1;
                continue;
            };
            stack[top].next_child = tree.nodes[ch].next_sibling;
            if sp < stack.len() {
                stack[sp] = WalkCursor {
                    slot: ch as u8,
                    entered: false,
                    next_child: SlotLink::NONE,
                };
                sp += 1;
            }
        }
    }

    for &slot_u8 in order[..order_len].iter().rev() {
        let slot = slot_u8 as usize;
        let n = &tree.nodes[slot];
        if !n.props.enabled() {
            continue;
        }
        let hit_box = n.layout_rect.intersect(n.clip_rect);
        if !tree
            .local_point(slot as u16, pos)
            .is_some_and(|p| hit_box.contains_point(p))
        {
            continue;
        }
        if is_interactive_leaf_or_row(n) || n.kind == NodeKind::ScrollY || n.kind == NodeKind::Modal
        {
            return Some(slot as u16);
        }
    }
    None
}
