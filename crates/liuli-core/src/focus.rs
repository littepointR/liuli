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
const _: () = assert!(MAX_NODES < u8::MAX as usize);

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
    // The most recent eight panels retain a stable child key without keeping their trees alive.
    panel_history: [(Key, Key); 8],
    history_len: u8,
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
        panel_history: [(Key(0), Key(0)); 8],
        history_len: 0,
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

    pub fn remember_panel_focus(&mut self, tree: &RetainedTree) {
        let Some(slot) = self.focused_slot(tree) else {
            return;
        };
        let Some((_, tab)) = panel_for(tree, slot) else {
            return;
        };
        let key = tree.nodes[slot as usize].key;
        let len = self.history_len as usize;
        let pos = self.panel_history[..len]
            .iter()
            .position(|&(id, _)| id == tab);
        let index = pos.unwrap_or(len.min(self.panel_history.len() - 1));
        self.panel_history[..=index].rotate_right(1);
        self.panel_history[0] = (tab, key);
        if pos.is_none() && len < self.panel_history.len() {
            self.history_len += 1;
        }
    }

    fn panel_entry(&self, tree: &RetainedTree, old: Option<u16>, target: u16) -> u16 {
        let Some((panel, tab)) = panel_for(tree, target) else {
            return target;
        };
        if old.is_some_and(|s| tree.is_descendant_of(s, panel)) {
            return target;
        }
        self.panel_history[..self.history_len as usize]
            .iter()
            .find(|(id, _)| *id == tab)
            .and_then(|(_, key)| tree.find_slot_by_key(*key))
            .filter(|&s| is_focus_eligible_in_scope(tree, s, panel))
            .unwrap_or(target)
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
            // Restore the target. Input inside the modal may have changed the indication.
            self.clear_modal_trigger();

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
            if is_group_entry(tree, slot) && !is_tab(tree.nodes[slot as usize].key) {
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

fn panel_for(tree: &RetainedTree, slot: u16) -> Option<(u16, Key)> {
    let mut parent = tree.nodes[slot as usize].parent;
    while let Some(i) = parent.get_u16() {
        let n = &tree.nodes[i as usize];
        if let Some(tab) = n.active_tab() {
            return Some((i, tab));
        }
        parent = n.parent;
    }
    None
}

fn sibling_tabs(tree: &RetainedTree, slot: u16) -> impl Iterator<Item = u16> + '_ {
    let node = &tree.nodes[slot as usize];
    let first = node
        .parent
        .get()
        .and_then(|p| tree.nodes[p].first_child.get_u16());
    core::iter::successors(first, |&i| tree.nodes[i as usize].next_sibling.get_u16()).filter(|&i| {
        let n = &tree.nodes[i as usize];
        n.kind == NodeKind::Tab && n.props.focusable() && n.props.visible() && n.props.enabled()
    })
}

fn is_group_entry(tree: &RetainedTree, slot: u16) -> bool {
    let n = &tree.nodes[slot as usize];
    n.kind != NodeKind::Tab || n.props.checked() || {
        let mut tabs = sibling_tabs(tree, slot);
        tabs.next() == Some(slot) && !tabs.any(|i| tree.nodes[i as usize].props.checked())
    }
}

fn navigate_tabs(
    tree: &mut RetainedTree,
    focus: &mut FocusState,
    slot: u16,
    steps: i16,
    screen: Rect,
    damage: &mut DamageSet,
) -> (bool, Option<UiAction>, Option<(Key, PointerEvent)>) {
    let count = sibling_tabs(tree, slot).count() as i32;
    let Some(index) = sibling_tabs(tree, slot).position(|i| i == slot) else {
        return (false, None, None);
    };
    let next = (index as i32 + i32::from(steps)).rem_euclid(count) as usize;
    let dest = sibling_tabs(tree, slot).nth(next).unwrap_or(slot);
    damage.add(tree.nodes[slot as usize].visible_rect, screen);
    let key = tree.nodes[dest as usize].key;
    focus.set_focused_key(Some(key));
    if let Some(s) = ensure_visible(tree, dest, screen) {
        damage.add(tree.nodes[s as usize].visible_rect, screen);
    }
    damage.add(tree.nodes[dest as usize].visible_rect, screen);
    (
        true,
        (!focus.editing && dest != slot).then_some(UiAction {
            key,
            kind: ActionKind::Activate,
        }),
        None,
    )
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
            if n.props.focusable() && is_group_entry(tree, slot as u16) {
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
    dispatch_input_with_scroll(
        tree,
        focus,
        pointer,
        input,
        screen,
        damage,
        |_, _, event| event.offset,
    )
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ScrollPhase {
    Begin,
    Move,
    Release,
    Cancel,
}

#[derive(Copy, Clone, Debug)]
pub struct ScrollGesture {
    pub phase: ScrollPhase,
    pub offset: i32,
}

/// Route a drag to its winning viewport. The host supplies the displayed offset
/// so input, layout and subsequent animation share one position.
pub fn dispatch_input_with_scroll(
    tree: &mut RetainedTree,
    focus: &mut FocusState,
    pointer: &mut PointerState,
    input: RawInput,
    screen: Rect,
    damage: &mut DamageSet,
    mut scroll_input: impl FnMut(NodeId, &crate::node::RetainedNode, ScrollGesture) -> i32,
) -> (bool, Option<UiAction>, Option<(Key, PointerEvent)>) {
    focus.ensure_valid(tree);
    focus.remember_panel_focus(tree);
    let Some(scope_root) = active_scope_root(tree) else {
        return (false, None, None);
    };

    // A navigation action can edit a value without moving the target. A touch
    // can land on empty space. Both still identify the active input method.
    let show_ring = match input {
        RawInput::Nav(_) | RawInput::Activate => true,
        RawInput::Adjust(steps) if steps != 0 => true,
        RawInput::PointerDown { pos, .. }
            if pointer.active_id().is_none() && screen.contains_point(pos) =>
        {
            false
        }
        _ => focus.ring_visible,
    };
    if show_ring != focus.ring_visible {
        focus.ring_visible = show_ring;
        if let Some(slot) = focus.focused_slot(tree) {
            damage.add(tree.nodes[slot as usize].visible_rect, screen);
        }
    }

    match input {
        RawInput::Nav(dir) => {
            if let Some(slot) = focus.focused_slot(tree)
                && tree.nodes[slot as usize].kind == NodeKind::Tab
                && (focus.editing || matches!(dir, NavDir::Left | NavDir::Right))
            {
                let forward = matches!(dir, NavDir::Next | NavDir::Right | NavDir::Down);
                return navigate_tabs(
                    tree,
                    focus,
                    slot,
                    if forward { 1 } else { -1 },
                    screen,
                    damage,
                );
            }
            if focus.editing
                && let Some(slot) = focus.focused_slot(tree)
            {
                let delta = match dir {
                    NavDir::Next | NavDir::Right | NavDir::Up => 1,
                    NavDir::Prev | NavDir::Left | NavDir::Down => -1,
                };
                let node = &tree.nodes[slot as usize];
                if node.kind == NodeKind::Slider {
                    let (c, a) = adjust_slider_by_steps(tree, focus, slot, delta, damage, screen);
                    return (c, a, None);
                } else if node.props.editable() {
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
            let mut cnt = collect_focusables_in_scope(tree, scope_root, &mut list);
            if matches!(dir, NavDir::Next | NavDir::Prev) {
                let mut written = 0;
                for i in 0..cnt {
                    let slot = list[i];
                    if is_group_entry(tree, slot) {
                        list[written] = slot;
                        written += 1;
                    }
                }
                cnt = written;
            }
            if cnt == 0 {
                return (false, None, None);
            }

            let old_slot = focus.focused_slot(tree);

            let next_slot = match dir {
                NavDir::Next | NavDir::Prev => {
                    match old_slot.and_then(|s| list[..cnt].iter().position(|&x| x == s)) {
                        Some(idx) => {
                            if dir == NavDir::Next {
                                list[(idx + 1) % cnt]
                            } else {
                                list[(idx + cnt - 1) % cnt]
                            }
                        }
                        None => list[0],
                    }
                }
                NavDir::Up | NavDir::Down | NavDir::Left | NavDir::Right => {
                    find_spatial_next(tree, &list[..cnt], old_slot, dir)
                }
            };

            let next_slot = if matches!(dir, NavDir::Next | NavDir::Prev) {
                focus.panel_entry(tree, old_slot, next_slot)
            } else {
                next_slot
            };
            focus.set_focused_key(Some(tree.nodes[next_slot as usize].key));

            if let Some(os) = old_slot
                && os != next_slot
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
            if focus.editing && tree.nodes[slot as usize].kind == NodeKind::Tab {
                return navigate_tabs(tree, focus, slot, steps, screen, damage);
            }
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
            let node = &tree.nodes[slot as usize];
            if node.kind == NodeKind::Tab {
                focus.editing = !focus.editing;
                damage.add(node.visible_rect, screen);
                return (
                    true,
                    (!focus.editing).then_some(UiAction {
                        key: node.key,
                        kind: ActionKind::Activate,
                    }),
                    None,
                );
            }
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
                | NodeKind::Tab
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
                    let selected = if tree.nodes[slot as usize].kind == NodeKind::Tab {
                        sibling_tabs(tree, slot).find(|&i| tree.nodes[i as usize].props.checked())
                    } else {
                        None
                    };
                    if let Some(i) = selected {
                        focus.set_focused_key(Some(tree.nodes[i as usize].key));
                        if let Some(s) = ensure_visible(tree, i, screen) {
                            damage.add(tree.nodes[s as usize].visible_rect, screen);
                        }
                        damage.add(tree.nodes[i as usize].visible_rect, screen);
                    }
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

            let hit_kind = tree.nodes[hit_slot as usize].kind;
            let hit_cap = tree.nodes[hit_slot as usize].props.pointer_capture();
            let scroll_anc = if hit_kind.is_scroll_view() {
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

                    let horizontal = adx > ady;
                    let axis_delta = if horizontal { dx } else { dy };
                    if adx.max(ady) >= DRAG_THRESHOLD_PX
                        && let Some(owner) =
                            scroll_for_drag(tree, scroll_slot, scope_root, horizontal, axis_delta)
                    {
                        if tree.nodes[candidate_slot as usize].pressed {
                            tree.nodes[candidate_slot as usize].pressed = false;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                        let node = &tree.nodes[owner as usize];
                        let start_offset = if owner == scroll_slot {
                            start_offset
                        } else {
                            node.scroll_y
                        };
                        pointer.capture = CaptureKind::Scrolling {
                            scroll: node.id(owner),
                            start_pos,
                            start_offset,
                        };
                        let offset = scroll_input(
                            node.id(owner),
                            node,
                            ScrollGesture {
                                phase: ScrollPhase::Begin,
                                offset: start_offset.saturating_sub(axis_delta),
                            },
                        );
                        apply_scroll_drag(tree, owner, offset, screen, damage);
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
                    let is_scroll_x = tree.nodes[scroll_slot as usize].kind.scrolls_x();
                    let axis_delta = if is_scroll_x {
                        pos.x.saturating_sub(start_pos.x)
                    } else {
                        pos.y.saturating_sub(start_pos.y)
                    };
                    let node = &tree.nodes[scroll_slot as usize];
                    let offset = scroll_input(
                        scroll,
                        node,
                        ScrollGesture {
                            phase: ScrollPhase::Move,
                            offset: start_offset.saturating_sub(axis_delta),
                        },
                    );
                    apply_scroll_drag(tree, scroll_slot, offset, screen, damage);
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
                CaptureKind::Scrolling {
                    scroll,
                    start_pos,
                    start_offset,
                } => {
                    if let Some(slot) = capture_id_in_scope(tree, scroll, scope_root) {
                        let node = &tree.nodes[slot as usize];
                        let delta = if node.kind.scrolls_x() {
                            pos.x.saturating_sub(start_pos.x)
                        } else {
                            pos.y.saturating_sub(start_pos.y)
                        };
                        let offset = scroll_input(
                            scroll,
                            node,
                            ScrollGesture {
                                phase: ScrollPhase::Release,
                                offset: start_offset.saturating_sub(delta),
                            },
                        );
                        apply_scroll_drag(tree, slot, offset, screen, damage);
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
            if let CaptureKind::Scrolling { scroll, .. } = pointer.capture
                && let Some(node) = tree.node_by_id(scroll)
            {
                scroll_input(
                    scroll,
                    node,
                    ScrollGesture {
                        phase: ScrollPhase::Cancel,
                        offset: node.scroll_y,
                    },
                );
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
    if n.props.min >= n.props.max || n.props.step <= 0 || steps == 0 {
        return (true, None);
    }
    let min = i32::from(n.props.min);
    let max = i32::from(n.props.max);
    let step = i32::from(n.props.step);
    let rel = i32::from(n.props.value.clamp(n.props.min, n.props.max)) - min;
    let index = if steps > 0 {
        rel / step
    } else {
        (rel + step - 1) / step
    };
    let new_val = (min + (index + i32::from(steps)) * step).clamp(min, max) as i16;
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
    let (knob_w, _) = tree.part_style(n.key).slider_knob_size(rect.size());
    let usable = i64::from(rect.w - knob_w);
    if usable == 0 {
        return (true, None);
    }
    let rel_x = (i64::from(pos.x) - i64::from(rect.x) - i64::from(knob_w / 2)).clamp(0, usable);
    let min = n.props.min as i64;
    let max = n.props.max as i64;
    let span = (max - min).max(1);
    let raw_val = min + (rel_x * span + usable / 2) / usable;
    let snapped =
        crate::input::snap_stepped_value(n.props.min, n.props.max, n.props.step, raw_val as i16);

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
    let max_scroll = sv.scroll_limit().unwrap_or(0);
    let new_offset = desired_offset.clamp(0, max_scroll);
    if new_offset != sv.scroll_y {
        tree.nodes[scroll_slot as usize].scroll_y = new_offset;
        relayout_scroll_subtree(tree, scroll_slot, screen);
        damage.add(tree.nodes[scroll_slot as usize].visible_rect, screen);
    }
}

fn find_spatial_next(
    tree: &RetainedTree,
    candidates: &[u16],
    cur_slot: Option<u16>,
    dir: NavDir,
) -> u16 {
    let Some(cur) = cur_slot else {
        return candidates[0];
    };
    let rect_of = |slot: u16| -> Rect {
        let r = tree.nodes[slot as usize].layout_rect;
        if r.w == 0 && r.h == 0 {
            tree.nodes[slot as usize].ink_rect
        } else {
            r
        }
    };
    let center_of = |r: Rect| -> (i32, i32) {
        (
            r.x.saturating_add((r.w / 2) as i32),
            r.y.saturating_add((r.h / 2) as i32),
        )
    };
    let (cx, cy) = center_of(rect_of(cur));
    let mut best_forward: Option<(i64, u32, u16)> = None;
    let mut best_wrap: Option<(i64, u32, u16)> = None;

    for &cand in candidates {
        if cand == cur {
            continue;
        }
        let (tx, ty) = center_of(rect_of(cand));
        let (primary, cross) = match dir {
            NavDir::Right => (tx.saturating_sub(cx), ty.saturating_sub(cy).unsigned_abs()),
            NavDir::Left => (cx.saturating_sub(tx), ty.saturating_sub(cy).unsigned_abs()),
            NavDir::Down => (ty.saturating_sub(cy), tx.saturating_sub(cx).unsigned_abs()),
            NavDir::Up => (cy.saturating_sub(ty), tx.saturating_sub(cx).unsigned_abs()),
            NavDir::Next | NavDir::Prev => return candidates[0],
        };
        let score = i64::from(primary) + 3 * i64::from(cross);
        if primary > 0 {
            let replace = match best_forward {
                None => true,
                Some((bs, bc, _)) => score < bs || (score == bs && cross < bc),
            };
            if replace {
                best_forward = Some((score, cross, cand));
            }
        } else {
            let replace = match best_wrap {
                None => true,
                Some((bs, bc, _)) => score < bs || (score == bs && cross < bc),
            };
            if replace {
                best_wrap = Some((score, cross, cand));
            }
        }
    }

    best_forward
        .or(best_wrap)
        .map(|(_, _, slot)| slot)
        .unwrap_or(cur)
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
            .hit_point(slot, pos)
            .is_some_and(|p| n.layout_rect.intersect(n.clip_rect).contains_point(p))
}

fn find_scroll_ancestor(tree: &RetainedTree, slot: u16, scope_root: u16) -> Option<u16> {
    let mut child = slot;
    let mut cur = tree.nodes[slot as usize].parent;
    while let Some(idx) = cur.get_u16() {
        let kind = tree.nodes[idx as usize].kind;
        if kind.is_scroll_view()
            && (kind != NodeKind::TabBar || tree.nodes[child as usize].kind == NodeKind::Tab)
        {
            return Some(idx);
        }
        if idx == scope_root {
            break;
        }
        child = idx;
        cur = tree.nodes[idx as usize].parent;
    }
    None
}

// Lock once, to the nearest ancestor that can move along the dominant drag axis.
// At the end of the whole chain, still consume the drag instead of activating a row.
fn scroll_for_drag(
    tree: &RetainedTree,
    start: u16,
    scope: u16,
    horizontal: bool,
    delta: i32,
) -> Option<u16> {
    let kind = if horizontal {
        NodeKind::ScrollX
    } else {
        NodeKind::ScrollY
    };
    let mut slot = Some(start);
    let mut nearest = None;
    while let Some(index) = slot {
        let node = &tree.nodes[index as usize];
        if node.kind == kind || (horizontal && node.kind == NodeKind::TabBar) {
            nearest.get_or_insert(index);
            if (delta > 0 && node.scroll_y > 0)
                || (delta < 0 && node.scroll_y < node.scroll_limit()?)
            {
                return Some(index);
            }
        }
        if index == scope {
            break;
        }
        slot = node.parent.get_u16();
    }
    nearest
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
            .hit_point(slot as u16, pos)
            .is_some_and(|p| hit_box.contains_point(p))
        {
            continue;
        }
        if is_interactive_leaf_or_row(n)
            || matches!(
                n.kind,
                NodeKind::ScrollY | NodeKind::ScrollX | NodeKind::Modal
            )
        {
            return Some(slot as u16);
        }
    }
    None
}
