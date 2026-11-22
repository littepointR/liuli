use crate::capacity::{BuildError, MAX_NODES};
use crate::damage::DamageSet;
use crate::geometry::Rect;
use crate::layout::{TextMeasurer, layout_tree};
use crate::node::{RetainedNode, RetainedTree};
use crate::time::{ANIM_SCALE, AnimPool, AnimProp, InstantMs, TOGGLE_ANIM_DURATION_MS};
use crate::view::{Length, NONE_IDX, NodeKind, ViewDraft};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ReconcileOutcome {
    pub tree_changed: bool,
    pub relayout_needed: bool,
    pub measured_nodes: u32,
    pub focus_target_invalidated: bool,
    pub capture_target_invalidated: bool,
    pub modal_changed: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn reconcile_into(
    draft: &ViewDraft,
    active: &RetainedTree,
    candidate: &mut RetainedTree,
    screen: Rect,
    measurer: &dyn TextMeasurer,
    anims: &mut AnimPool,
    focused_slot: Option<u16>,
    captured_slot: Option<u16>,
    now: InstantMs,
    damage: &mut DamageSet,
) -> Result<(ReconcileOutcome, [u16; MAX_NODES]), BuildError> {
    let Some(draft_root) = draft.root else {
        return Err(BuildError::NoRoot);
    };
    let has_active = active.root != NONE_IDX;

    candidate.reset();
    candidate.generation_hwm = active.generation_hwm;
    let tlen = draft.text_len as usize;
    candidate.text_buf[..tlen].copy_from_slice(&draft.text_buf[..tlen]);
    candidate.text_len = draft.text_len;

    let d_count = draft.count as usize;
    let mut draft_to_cand = [NONE_IDX; MAX_NODES];
    let mut old_to_cand = [NONE_IDX; MAX_NODES];
    let mut matched_old = [false; MAX_NODES];
    let mut cand_occupied = [false; MAX_NODES];
    let mut matched_from_old = [NONE_IDX; MAX_NODES];

    // Pass 1: match nodes with identical Key + Kind, preserving their exact slot and NodeId.
    for d_idx in 0..d_count {
        let d = &draft.nodes[d_idx];
        if let Some(old_slot) = active.find_slot_by_key(d.key) {
            let old = &active.nodes[old_slot as usize];
            if old.kind == d.kind {
                draft_to_cand[d_idx] = old_slot;
                old_to_cand[old_slot as usize] = old_slot;
                matched_old[old_slot as usize] = true;
                cand_occupied[old_slot as usize] = true;
                matched_from_old[old_slot as usize] = old_slot;
            }
        }
    }

    // Pass 2: assign freed or unused slots to newly inserted / kind-changed nodes.
    let mut next_free = 0usize;
    for d_idx in 0..d_count {
        if draft_to_cand[d_idx] != NONE_IDX {
            continue;
        }
        while next_free < MAX_NODES && cand_occupied[next_free] {
            next_free += 1;
        }
        if next_free >= MAX_NODES {
            return Err(BuildError::Nodes);
        }
        let slot = next_free as u16;
        let prev_gen = candidate.generation_hwm[next_free];
        if prev_gen == u16::MAX {
            return Err(BuildError::GenerationExhausted);
        }
        let new_gen = prev_gen + 1;
        candidate.generation_hwm[next_free] = new_gen;
        cand_occupied[next_free] = true;
        draft_to_cand[d_idx] = slot;
    }

    // Track deleted or kind-changed nodes for damage, animation cleanup, and focus/capture cleanup.
    let mut outcome = ReconcileOutcome::default();
    for old_idx in 0..MAX_NODES {
        let old = &active.nodes[old_idx];
        if !old.occupied || matched_old[old_idx] {
            continue;
        }
        outcome.tree_changed = true;
        if !old.old_visible_rect.is_empty() {
            damage.add(old.old_visible_rect, screen);
        }
        if !old.visible_rect.is_empty() {
            damage.add(old.visible_rect, screen);
        }
        anims.cancel_for_key(old.key);
        if focused_slot == Some(old_idx as u16) {
            outcome.focus_target_invalidated = true;
        }
        if captured_slot == Some(old_idx as u16) {
            outcome.capture_target_invalidated = true;
        }
    }

    // Populate candidate nodes & rebuild parent/child/sibling pointers.
    for d_idx in 0..d_count {
        let d = &draft.nodes[d_idx];
        let c_slot = draft_to_cand[d_idx] as usize;
        let old_slot = matched_from_old[c_slot];

        let mut node = if old_slot != NONE_IDX {
            let mut prev = active.nodes[old_slot as usize];
            prev.old_visible_rect = prev.visible_rect;
            prev.dirty_measure = false;
            prev.dirty_paint = false;
            prev
        } else {
            let mut fresh = RetainedNode::EMPTY;
            fresh.occupied = true;
            fresh.generation = candidate.generation_hwm[c_slot];
            fresh.dirty_measure = true;
            fresh.dirty_paint = true;
            fresh
        };

        node.occupied = true;
        node.key = d.key;
        node.kind = d.kind;
        node.props = d.props;
        node.parent = if d.parent == NONE_IDX {
            NONE_IDX
        } else {
            draft_to_cand[d.parent as usize]
        };
        node.first_child = if d.first_child == NONE_IDX {
            NONE_IDX
        } else {
            draft_to_cand[d.first_child as usize]
        };
        node.last_child = if d.last_child == NONE_IDX {
            NONE_IDX
        } else {
            draft_to_cand[d.last_child as usize]
        };
        node.next_sibling = if d.next_sibling == NONE_IDX {
            NONE_IDX
        } else {
            draft_to_cand[d.next_sibling as usize]
        };

        candidate.nodes[c_slot] = node;
    }

    candidate.root = draft_to_cand[draft_root as usize];
    candidate.modal = match draft.modal {
        Some(m) => draft_to_cand[m as usize],
        None => NONE_IDX,
    };
    candidate.active_count = draft.count;

    // Global theme / modal / initial screen checks.
    let active_theme = if has_active {
        active.nodes[active.root as usize].props.theme_id
    } else {
        0
    };
    let cand_theme = candidate.nodes[candidate.root as usize].props.theme_id;
    let old_screen = if has_active {
        active.nodes[active.root as usize].layout_rect
    } else {
        Rect::ZERO
    };
    let modal_changed = (active.modal != NONE_IDX) != (candidate.modal != NONE_IDX);
    let global_invalidate =
        !has_active || active_theme != cand_theme || old_screen != screen || modal_changed;
    outcome.modal_changed = modal_changed;

    // Check diffs per node.
    for c_idx in 0..MAX_NODES {
        let node = candidate.nodes[c_idx];
        if !node.occupied {
            continue;
        }
        let old_slot = matched_from_old[c_idx];
        if old_slot == NONE_IDX {
            outcome.tree_changed = true;
            candidate.nodes[c_idx].dirty_measure = true;
            candidate.nodes[c_idx].dirty_paint = true;
            continue;
        }

        let old = &active.nodes[old_slot as usize];

        // Hierarchy / order check: compare parent key & prev/first_child/next_sibling keys
        let old_parent_key =
            (old.parent != NONE_IDX).then(|| active.nodes[old.parent as usize].key);
        let new_parent_key =
            (node.parent != NONE_IDX).then(|| candidate.nodes[node.parent as usize].key);
        let old_next_key =
            (old.next_sibling != NONE_IDX).then(|| active.nodes[old.next_sibling as usize].key);
        let new_next_key = (node.next_sibling != NONE_IDX)
            .then(|| candidate.nodes[node.next_sibling as usize].key);
        let old_first_key =
            (old.first_child != NONE_IDX).then(|| active.nodes[old.first_child as usize].key);
        let new_first_key =
            (node.first_child != NONE_IDX).then(|| candidate.nodes[node.first_child as usize].key);

        if old_parent_key != new_parent_key
            || old_next_key != new_next_key
            || old_first_key != new_first_key
        {
            outcome.tree_changed = true;
            candidate.nodes[c_idx].dirty_measure = true;
            candidate.nodes[c_idx].dirty_paint = true;
        }

        // Structural layout props check
        let op = &old.props;
        let np = &node.props;
        let structural_changed = op.width != np.width
            || op.height != np.height
            || op.padding != np.padding
            || op.gap != np.gap
            || op.align_cross != np.align_cross
            || op.max_lines != np.max_lines
            || op.ellipsis != np.ellipsis
            || op.visible != np.visible
            || op.icon != np.icon
            || op.border_width != np.border_width
            || op.radius != np.radius;

        if structural_changed {
            outcome.tree_changed = true;
            candidate.nodes[c_idx].dirty_measure = true;
            candidate.nodes[c_idx].dirty_paint = true;
        }

        // Text diff check with same-metrics fast path
        let old_text = active.text_str(op.text);
        let new_text = draft.text_str(np.text);
        if old_text != new_text {
            outcome.tree_changed = true;
            candidate.nodes[c_idx].dirty_paint = true;

            if !structural_changed
                && !candidate.nodes[c_idx].dirty_measure
                && let Some((wrap_w, old_sz, old_ink, _)) = old.text_cache
            {
                let max_lines = if node.kind == NodeKind::Button {
                    np.max_lines.max(1)
                } else {
                    np.max_lines
                };
                let new_tm = measurer.measure_text(new_text, wrap_w, max_lines, np.ellipsis);
                let size_same = new_tm.size == old_sz;
                let ink_same_or_inside = new_tm.ink == old_ink
                    || (new_tm.ink.x >= 0
                        && new_tm.ink.y >= 0
                        && new_tm.ink.right64() <= old.layout_rect.w as i64
                        && new_tm.ink.bottom64() <= old.layout_rect.h as i64);

                candidate.nodes[c_idx].text_cache =
                    Some((wrap_w, new_tm.size, new_tm.ink, new_tm.lines));

                if size_same && ink_same_or_inside && !matches!(np.width, Length::Content) {
                    // Fixed-width same-metrics fast path: skip ancestor/page relayout!
                } else if size_same && ink_same_or_inside && matches!(np.width, Length::Content) {
                    // Even Content width keeps exact same size & ink!
                } else {
                    candidate.nodes[c_idx].dirty_measure = true;
                }
            } else {
                candidate.nodes[c_idx].dirty_measure = true;
            }
        }

        // Visual-only props check
        let visual_changed = op.bg != np.bg
            || op.border_color != np.border_color
            || op.fg != np.fg
            || op.accent != np.accent
            || op.focus_color != np.focus_color
            || op.overlay_color != np.overlay_color
            || op.overlay_alpha != np.overlay_alpha
            || op.value != np.value
            || op.min != np.min
            || op.max != np.max
            || op.checked != np.checked
            || op.enabled != np.enabled
            || op.focusable != np.focusable
            || op.clickable != np.clickable
            || op.theme_id != np.theme_id;

        if visual_changed {
            outcome.tree_changed = true;
            candidate.nodes[c_idx].dirty_paint = true;
        }

        if node.kind == NodeKind::Toggle && op.checked != np.checked {
            let start_def = if op.checked { ANIM_SCALE } else { 0 };
            let end_val = if np.checked { ANIM_SCALE } else { 0 };
            anims.start(
                node.key,
                AnimProp::ToggleKnob,
                start_def,
                end_val,
                now,
                TOGGLE_ANIM_DURATION_MS,
                old.visible_rect,
            );
        }

        if focused_slot == Some(old_slot) && (!np.enabled || !np.visible || !np.focusable) {
            outcome.focus_target_invalidated = true;
        }
        if captured_slot == Some(old_slot) && (!np.enabled || !np.visible) {
            outcome.capture_target_invalidated = true;
            candidate.nodes[c_idx].pressed = false;
        }
    }

    if global_invalidate {
        candidate.nodes[candidate.root as usize].dirty_measure = true;
        candidate.nodes[candidate.root as usize].dirty_paint = true;
        outcome.tree_changed = true;
    }

    // Propagate dirty_measure up parent chain using bounded iteration.
    let mut dirty_slots = [NONE_IDX; MAX_NODES];
    let mut dirty_cnt = 0usize;
    for (idx, n) in candidate.nodes.iter().enumerate() {
        if n.occupied && n.dirty_measure {
            dirty_slots[dirty_cnt] = idx as u16;
            dirty_cnt += 1;
        }
    }
    for &s in &dirty_slots[..dirty_cnt] {
        let mut p = candidate.nodes[s as usize].parent;
        while p != NONE_IDX {
            if candidate.nodes[p as usize].dirty_measure {
                break;
            }
            candidate.nodes[p as usize].dirty_measure = true;
            p = candidate.nodes[p as usize].parent;
        }
    }

    outcome.relayout_needed = candidate.nodes[candidate.root as usize].dirty_measure;
    outcome.measured_nodes = layout_tree(candidate, screen, measurer).unwrap_or(0);

    // Update any running toggle animation dirty_rect to new visible_rect.
    for n in &candidate.nodes {
        if n.occupied && n.kind == NodeKind::Toggle {
            anims.update_rect(n.key, AnimProp::ToggleKnob, n.visible_rect);
        }
    }

    // Add dirty rectangles to DamageSet.
    if global_invalidate {
        damage.mark_full_screen(screen);
    } else {
        for n in &candidate.nodes {
            if !n.occupied {
                continue;
            }
            if n.dirty_paint || n.dirty_measure || n.visible_rect != n.old_visible_rect {
                if !n.old_visible_rect.is_empty() {
                    damage.add(n.old_visible_rect, screen);
                }
                if !n.visible_rect.is_empty() {
                    damage.add(n.visible_rect, screen);
                }
            }
        }
    }

    Ok((outcome, old_to_cand))
}
