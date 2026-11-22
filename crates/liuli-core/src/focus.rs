use crate::capacity::MAX_NODES;
use crate::damage::DamageSet;
use crate::geometry::{Point, Rect};
use crate::input::{ActionKind, NavDir, PointerId, RawInput, UiAction};
use crate::layout::{ensure_visible, relayout_scroll_subtree};
use crate::node::RetainedTree;
use crate::view::{Key, NONE_IDX, NodeKind};

const DRAG_THRESHOLD_PX: i32 = 6;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CaptureKind {
    None,
    Button(u16),
    Slider {
        slot: u16,
    },
    ScrollCandidate {
        candidate_slot: u16,
        scroll_slot: u16,
        start_pos: Point,
        start_offset: i32,
        slider_candidate: bool,
    },
    Scrolling {
        scroll_slot: u16,
        start_pos: Point,
        start_offset: i32,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PointerState {
    pub active_id: Option<PointerId>,
    pub capture: CaptureKind,
}

impl PointerState {
    pub const fn new() -> Self {
        Self {
            active_id: None,
            capture: CaptureKind::None,
        }
    }

    pub fn clear(&mut self, tree: &mut RetainedTree, damage: &mut DamageSet, screen: Rect) {
        if let CaptureKind::Button(slot)
        | CaptureKind::ScrollCandidate {
            candidate_slot: slot,
            ..
        } = self.capture
            && slot != NONE_IDX
            && tree.nodes[slot as usize].occupied
            && tree.nodes[slot as usize].pressed
        {
            tree.nodes[slot as usize].pressed = false;
            damage.add(tree.nodes[slot as usize].visible_rect, screen);
        }
        self.active_id = None;
        self.capture = CaptureKind::None;
    }

    pub fn captured_slot(&self) -> Option<u16> {
        match self.capture {
            CaptureKind::None => None,
            CaptureKind::Button(s)
            | CaptureKind::Slider { slot: s }
            | CaptureKind::Scrolling { scroll_slot: s, .. } => Some(s),
            CaptureKind::ScrollCandidate { candidate_slot, .. } => Some(candidate_slot),
        }
    }
}

impl Default for PointerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FocusState {
    pub focused_key: Option<Key>,
    pub ring_visible: bool,
    pub editing: bool,
    pub modal_saved_trigger: Option<(Key, bool)>,
}

impl FocusState {
    pub const fn new() -> Self {
        Self {
            focused_key: None,
            ring_visible: false,
            editing: false,
            modal_saved_trigger: None,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn focused_slot(&self, tree: &RetainedTree) -> Option<u16> {
        self.focused_key.and_then(|k| tree.find_slot_by_key(k))
    }

    pub fn ensure_valid(&mut self, tree: &RetainedTree) {
        let scope_root = active_scope_root(tree);
        if scope_root == NONE_IDX {
            self.focused_key = None;
            self.editing = false;
            return;
        }
        if let Some(slot) = self.focused_slot(tree)
            && is_focus_eligible_in_scope(tree, slot, scope_root)
        {
            return;
        }
        self.editing = false;
        self.focused_key =
            first_focusable_in_scope(tree, scope_root).map(|s| tree.nodes[s as usize].key);
    }

    pub fn on_modal_opened(&mut self, tree: &RetainedTree) {
        if self.modal_saved_trigger.is_none()
            && let Some(prev) = self.focused_key
        {
            self.modal_saved_trigger = Some((prev, self.ring_visible));
        }
        self.editing = false;
        if tree.modal != NONE_IDX {
            self.focused_key =
                first_focusable_in_scope(tree, tree.modal).map(|s| tree.nodes[s as usize].key);
        }
    }

    pub fn on_modal_closed(
        &mut self,
        tree: &mut RetainedTree,
        damage: &mut DamageSet,
        screen: Rect,
    ) {
        self.editing = false;
        if let Some((trigger_key, saved_ring)) = self.modal_saved_trigger.take() {
            self.ring_visible = saved_ring;
            if let Some(slot) = tree.find_slot_by_key(trigger_key)
                && is_focus_eligible_in_scope(tree, slot, tree.root)
            {
                self.focused_key = Some(trigger_key);
                if let Some(scroll_slot) = ensure_visible(tree, slot, screen) {
                    damage.add(tree.nodes[scroll_slot as usize].visible_rect, screen);
                }
                return;
            }
        }
        self.focused_key =
            first_focusable_in_scope(tree, tree.root).map(|s| tree.nodes[s as usize].key);
    }
}

impl Default for FocusState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn active_scope_root(tree: &RetainedTree) -> u16 {
    if tree.modal != NONE_IDX && tree.nodes[tree.modal as usize].occupied {
        tree.modal
    } else {
        tree.root
    }
}

fn is_focus_eligible_in_scope(tree: &RetainedTree, slot: u16, scope_root: u16) -> bool {
    let n = &tree.nodes[slot as usize];
    n.occupied
        && n.props.focusable
        && tree.is_effectively_enabled_and_visible(slot)
        && tree.is_descendant_of(slot, scope_root)
}

pub fn collect_focusables_in_scope(
    tree: &RetainedTree,
    scope_root: u16,
    out: &mut [u16; MAX_NODES],
) -> usize {
    if scope_root == NONE_IDX || !tree.nodes[scope_root as usize].occupied {
        return 0;
    }
    let mut stack = [NONE_IDX; MAX_NODES];
    let mut sp = 0usize;
    let mut count = 0usize;

    stack[sp] = scope_root;
    sp += 1;

    while sp > 0 {
        sp -= 1;
        let slot = stack[sp];
        let n = &tree.nodes[slot as usize];
        if !n.occupied || !n.props.visible || !n.props.enabled {
            continue;
        }
        // When not inside modal, skip modal subtree
        if scope_root == tree.root && n.kind == NodeKind::Modal {
            continue;
        }
        if n.props.focusable {
            out[count] = slot;
            count += 1;
        }
        let mut children = [NONE_IDX; MAX_NODES];
        let mut c_cnt = 0usize;
        let mut ch = n.first_child;
        while ch != NONE_IDX {
            children[c_cnt] = ch;
            c_cnt += 1;
            ch = tree.nodes[ch as usize].next_sibling;
        }
        while c_cnt > 0 {
            c_cnt -= 1;
            stack[sp] = children[c_cnt];
            sp += 1;
        }
    }
    count
}

fn first_focusable_in_scope(tree: &RetainedTree, scope_root: u16) -> Option<u16> {
    let mut buf = [NONE_IDX; MAX_NODES];
    let cnt = collect_focusables_in_scope(tree, scope_root, &mut buf);
    (cnt > 0).then(|| buf[0])
}

pub fn dispatch_input(
    tree: &mut RetainedTree,
    focus: &mut FocusState,
    pointer: &mut PointerState,
    input: RawInput,
    screen: Rect,
    damage: &mut DamageSet,
) -> (bool, Option<UiAction>) {
    focus.ensure_valid(tree);
    let scope_root = active_scope_root(tree);
    if scope_root == NONE_IDX {
        return (false, None);
    }

    match input {
        RawInput::Nav(dir) => {
            if focus.editing {
                // In edit mode, Nav acts as +/- step for sliders, or navigates out? Spec §7.1:
                // "浏览态映射为导航；亮度进入编辑态后映射为数值调整。"
                if let Some(slot) = focus.focused_slot(tree)
                    && tree.nodes[slot as usize].kind == NodeKind::Slider
                {
                    let delta = if dir == NavDir::Next { 1 } else { -1 };
                    return adjust_slider_by_steps(tree, focus, slot, delta, damage, screen);
                }
            }

            let mut list = [NONE_IDX; MAX_NODES];
            let cnt = collect_focusables_in_scope(tree, scope_root, &mut list);
            if cnt == 0 {
                return (false, None);
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

            focus.focused_key = Some(tree.nodes[next_slot as usize].key);
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
            (true, None)
        }
        RawInput::Adjust(steps) => {
            let Some(slot) = focus.focused_slot(tree) else {
                return (false, None);
            };
            if focus.editing && tree.nodes[slot as usize].kind == NodeKind::Slider {
                return adjust_slider_by_steps(tree, focus, slot, steps, damage, screen);
            }
            if steps == 0 {
                return (true, None);
            }
            let dir = if steps > 0 {
                NavDir::Next
            } else {
                NavDir::Prev
            };
            let count = steps.unsigned_abs();
            let mut consumed = false;
            for _ in 0..count {
                let (c, _) =
                    dispatch_input(tree, focus, pointer, RawInput::Nav(dir), screen, damage);
                consumed |= c;
            }
            (consumed, None)
        }
        RawInput::Activate => {
            let Some(slot) = focus.focused_slot(tree) else {
                return (false, None);
            };
            if !focus.ring_visible {
                focus.ring_visible = true;
                damage.add(tree.nodes[slot as usize].visible_rect, screen);
            }
            let node = &tree.nodes[slot as usize];
            match node.kind {
                NodeKind::Slider => {
                    focus.editing = !focus.editing;
                    damage.add(node.visible_rect, screen);
                    (true, None)
                }
                NodeKind::Toggle => (
                    true,
                    Some(UiAction {
                        key: node.key,
                        kind: ActionKind::Toggle,
                    }),
                ),
                NodeKind::Button | NodeKind::Row | NodeKind::Column | NodeKind::Stack => (
                    true,
                    Some(UiAction {
                        key: node.key,
                        kind: ActionKind::Activate,
                    }),
                ),
                _ => (false, None),
            }
        }
        RawInput::Back => {
            if focus.editing {
                focus.editing = false;
                if let Some(slot) = focus.focused_slot(tree) {
                    damage.add(tree.nodes[slot as usize].visible_rect, screen);
                }
                return (true, None);
            }
            if scope_root == tree.modal {
                return (
                    true,
                    Some(UiAction {
                        key: tree.nodes[scope_root as usize].key,
                        kind: ActionKind::Dismiss,
                    }),
                );
            }
            (false, None)
        }
        RawInput::PointerDown { id, pos } => {
            if pointer.active_id.is_some() {
                // Ignore extra pointers until active pointer is released/cancelled.
                return (false, None);
            }

            let hit = hit_test_in_scope(tree, scope_root, pos);
            let Some(hit_slot) = hit else {
                if scope_root == tree.modal {
                    // Backdrop consumes pointer events inside modal
                    pointer.active_id = Some(id);
                    pointer.capture = CaptureKind::None;
                    return (true, None);
                }
                return (false, None);
            };

            pointer.active_id = Some(id);

            // Exit edit mode if touching a different control
            if focus.editing && focus.focused_slot(tree) != Some(hit_slot) {
                if let Some(old_slot) = focus.focused_slot(tree) {
                    damage.add(tree.nodes[old_slot as usize].visible_rect, screen);
                }
                focus.editing = false;
            }

            // Touch hides focus ring unless previously editing or slider
            if focus.ring_visible {
                if let Some(old_slot) = focus.focused_slot(tree) {
                    damage.add(tree.nodes[old_slot as usize].visible_rect, screen);
                }
                focus.ring_visible = false;
            }

            let scroll_anc = find_scroll_ancestor(tree, hit_slot, scope_root);
            let hit_kind = tree.nodes[hit_slot as usize].kind;

            if hit_kind == NodeKind::Modal {
                pointer.capture = CaptureKind::None;
                return (true, None);
            }

            if let Some(scroll_slot) = scroll_anc {
                let is_slider = hit_kind == NodeKind::Slider;
                if is_interactive_leaf_or_row(&tree.nodes[hit_slot as usize]) {
                    tree.nodes[hit_slot as usize].pressed = true;
                    damage.add(tree.nodes[hit_slot as usize].visible_rect, screen);
                }
                if tree.nodes[hit_slot as usize].props.focusable {
                    focus.focused_key = Some(tree.nodes[hit_slot as usize].key);
                }
                pointer.capture = CaptureKind::ScrollCandidate {
                    candidate_slot: hit_slot,
                    scroll_slot,
                    start_pos: pos,
                    start_offset: tree.nodes[scroll_slot as usize].scroll_y,
                    slider_candidate: is_slider,
                };
                (true, None)
            } else if hit_kind == NodeKind::Slider {
                if tree.nodes[hit_slot as usize].props.focusable {
                    focus.focused_key = Some(tree.nodes[hit_slot as usize].key);
                }
                pointer.capture = CaptureKind::Slider { slot: hit_slot };
                let (_, action) = update_slider_from_pos(tree, hit_slot, pos, damage, screen);
                (true, action)
            } else if is_interactive_leaf_or_row(&tree.nodes[hit_slot as usize]) {
                if tree.nodes[hit_slot as usize].props.focusable {
                    focus.focused_key = Some(tree.nodes[hit_slot as usize].key);
                }
                tree.nodes[hit_slot as usize].pressed = true;
                damage.add(tree.nodes[hit_slot as usize].visible_rect, screen);
                pointer.capture = CaptureKind::Button(hit_slot);
                (true, None)
            } else {
                pointer.capture = CaptureKind::None;
                (scope_root == tree.modal, None)
            }
        }
        RawInput::PointerMove { id, pos } => {
            if pointer.active_id != Some(id) {
                return (false, None);
            }

            match pointer.capture {
                CaptureKind::None => (false, None),
                CaptureKind::Button(slot) => {
                    if !tree.nodes[slot as usize].occupied {
                        pointer.clear(tree, damage, screen);
                        return (false, None);
                    }
                    let inside = pointer_hits_slot(tree, slot, pos);
                    if tree.nodes[slot as usize].pressed != inside {
                        tree.nodes[slot as usize].pressed = inside;
                        damage.add(tree.nodes[slot as usize].visible_rect, screen);
                    }
                    (true, None)
                }
                CaptureKind::Slider { slot } => {
                    if !tree.nodes[slot as usize].occupied {
                        pointer.clear(tree, damage, screen);
                        return (false, None);
                    }
                    let (_, action) = update_slider_from_pos(tree, slot, pos, damage, screen);
                    (true, action)
                }
                CaptureKind::ScrollCandidate {
                    candidate_slot,
                    scroll_slot,
                    start_pos,
                    start_offset,
                    slider_candidate,
                } => {
                    let dx = pos.x.saturating_sub(start_pos.x);
                    let dy = pos.y.saturating_sub(start_pos.y);
                    let adx = dx.abs();
                    let ady = dy.abs();

                    if slider_candidate && adx >= DRAG_THRESHOLD_PX && adx > ady {
                        if tree.nodes[candidate_slot as usize].pressed {
                            tree.nodes[candidate_slot as usize].pressed = false;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                        pointer.capture = CaptureKind::Slider {
                            slot: candidate_slot,
                        };
                        let (_, action) =
                            update_slider_from_pos(tree, candidate_slot, pos, damage, screen);
                        return (true, action);
                    }

                    if ady >= DRAG_THRESHOLD_PX && ady >= adx {
                        // Scroll wins: cancel candidate pressed state and capture Scrolling
                        if candidate_slot != NONE_IDX
                            && tree.nodes[candidate_slot as usize].occupied
                            && tree.nodes[candidate_slot as usize].pressed
                        {
                            tree.nodes[candidate_slot as usize].pressed = false;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                        pointer.capture = CaptureKind::Scrolling {
                            scroll_slot,
                            start_pos,
                            start_offset,
                        };
                        apply_scroll_drag(tree, scroll_slot, start_offset - dy, screen, damage);
                        return (true, None);
                    }

                    // Below threshold: update pressed indicator only if still inside candidate
                    if candidate_slot != NONE_IDX
                        && tree.nodes[candidate_slot as usize].occupied
                        && is_interactive_leaf_or_row(&tree.nodes[candidate_slot as usize])
                    {
                        let inside = pointer_hits_slot(tree, candidate_slot, pos);
                        if tree.nodes[candidate_slot as usize].pressed != inside {
                            tree.nodes[candidate_slot as usize].pressed = inside;
                            damage.add(tree.nodes[candidate_slot as usize].visible_rect, screen);
                        }
                    }
                    (true, None)
                }
                CaptureKind::Scrolling {
                    scroll_slot,
                    start_pos,
                    start_offset,
                } => {
                    if !tree.nodes[scroll_slot as usize].occupied {
                        pointer.clear(tree, damage, screen);
                        return (false, None);
                    }
                    let dy = pos.y.saturating_sub(start_pos.y);
                    apply_scroll_drag(tree, scroll_slot, start_offset - dy, screen, damage);
                    (true, None)
                }
            }
        }
        RawInput::PointerUp { id, pos } => {
            if pointer.active_id != Some(id) {
                return (false, None);
            }
            let cap = pointer.capture;
            pointer.active_id = None;
            pointer.capture = CaptureKind::None;

            match cap {
                CaptureKind::None => (false, None),
                CaptureKind::Button(slot) => activate_if_hit_on_up(tree, slot, pos, damage, screen),
                CaptureKind::Slider { slot } => {
                    if !tree.nodes[slot as usize].occupied {
                        return (false, None);
                    }
                    let (_, action) = update_slider_from_pos(tree, slot, pos, damage, screen);
                    (true, action)
                }
                CaptureKind::ScrollCandidate { candidate_slot, .. } => {
                    if candidate_slot == NONE_IDX || !tree.nodes[candidate_slot as usize].occupied {
                        return (false, None);
                    }
                    if tree.nodes[candidate_slot as usize].kind == NodeKind::Slider {
                        let (_, action) =
                            update_slider_from_pos(tree, candidate_slot, pos, damage, screen);
                        (true, action)
                    } else {
                        activate_if_hit_on_up(tree, candidate_slot, pos, damage, screen)
                    }
                }
                CaptureKind::Scrolling { .. } => (true, None),
            }
        }
        RawInput::PointerCancel { id } => {
            if pointer.active_id != Some(id) {
                return (false, None);
            }
            pointer.clear(tree, damage, screen);
            (true, None)
        }
    }
}

fn adjust_slider_by_steps(
    tree: &mut RetainedTree,
    focus: &FocusState,
    slot: u16,
    steps: i16,
    damage: &mut DamageSet,
    screen: Rect,
) -> (bool, Option<UiAction>) {
    let _ = focus;
    let n = &tree.nodes[slot as usize];
    let step = n.props.step.max(1) as i32;
    let new_val = (n.props.value as i32 + (steps as i32) * step)
        .clamp(n.props.min as i32, n.props.max as i32) as i16;
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
    let rect = n.layout_rect;
    let knob_w = 12i32;
    let usable = (rect.w as i32 - knob_w).max(1);
    let rel_x = (pos.x - (rect.x + knob_w / 2)).clamp(0, usable);
    let span = (n.props.max as i32 - n.props.min as i32).max(1);
    let raw_val = n.props.min as i32
        + ((rel_x as i64 * span as i64 + (usable as i64 / 2)) / usable as i64) as i32;
    let step = n.props.step.max(1) as i32;
    let offset_from_min = raw_val - n.props.min as i32;
    let snapped_steps = (offset_from_min + step / 2) / step;
    let snapped = (n.props.min as i32 + snapped_steps * step)
        .clamp(n.props.min as i32, n.props.max as i32) as i16;

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
    if slot == NONE_IDX || !tree.nodes[slot as usize].occupied {
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

fn is_interactive_leaf_or_row(n: &crate::node::RetainedNode) -> bool {
    matches!(
        n.kind,
        NodeKind::Button | NodeKind::Toggle | NodeKind::Slider
    ) || n.props.clickable
}

fn pointer_hits_slot(tree: &RetainedTree, slot: u16, pos: Point) -> bool {
    let n = &tree.nodes[slot as usize];
    n.occupied
        && tree.is_effectively_enabled_and_visible(slot)
        && n.layout_rect.intersect(n.clip_rect).contains_point(pos)
}

fn find_scroll_ancestor(tree: &RetainedTree, mut slot: u16, scope_root: u16) -> Option<u16> {
    slot = tree.nodes[slot as usize].parent;
    while slot != NONE_IDX {
        if tree.nodes[slot as usize].kind == NodeKind::ScrollY {
            return Some(slot);
        }
        if slot == scope_root {
            break;
        }
        slot = tree.nodes[slot as usize].parent;
    }
    None
}

pub fn hit_test_in_scope(tree: &RetainedTree, scope_root: u16, pos: Point) -> Option<u16> {
    if scope_root == NONE_IDX || !tree.nodes[scope_root as usize].occupied {
        return None;
    }
    // Collect pre-order list in paint order within scope_root, then scan backwards (reverse paint order)!
    let mut order = [NONE_IDX; MAX_NODES];
    let mut order_len = 0usize;

    let mut stack = [NONE_IDX; MAX_NODES];
    let mut sp = 0usize;
    stack[sp] = scope_root;
    sp += 1;

    while sp > 0 {
        sp -= 1;
        let slot = stack[sp];
        let n = &tree.nodes[slot as usize];
        if !n.occupied || !n.props.visible {
            continue;
        }
        if scope_root == tree.root && n.kind == NodeKind::Modal {
            continue;
        }
        order[order_len] = slot;
        order_len += 1;

        let mut children = [NONE_IDX; MAX_NODES];
        let mut c_cnt = 0usize;
        let mut ch = n.first_child;
        while ch != NONE_IDX {
            children[c_cnt] = ch;
            c_cnt += 1;
            ch = tree.nodes[ch as usize].next_sibling;
        }
        while c_cnt > 0 {
            c_cnt -= 1;
            stack[sp] = children[c_cnt];
            sp += 1;
        }
    }

    for &slot in order[..order_len].iter().rev() {
        let n = &tree.nodes[slot as usize];
        if !n.props.enabled {
            continue;
        }
        let hit_box = n.layout_rect.intersect(n.clip_rect);
        if hit_box.is_empty() || !hit_box.contains_point(pos) {
            continue;
        }
        if is_interactive_leaf_or_row(n) || n.kind == NodeKind::ScrollY || n.kind == NodeKind::Modal
        {
            return Some(slot);
        }
    }
    None
}
