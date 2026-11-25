use crate::capacity::{BuildError, MAX_NODES, NodeMask};
use crate::damage::DamageSet;
use crate::geometry::Rect;
use crate::layout::{TextMeasurer, layout_tree, refresh_node_ink_rect};
use crate::node::{RetainedTree, TextCache};
use crate::time::{ANIM_SCALE, AnimPool, AnimProp, InstantMs, TOGGLE_ANIM_DURATION_MS};
use crate::view::{NodeKind, SlotLink, ViewDraft};

const NO_MAP: u8 = u8::MAX;
const _: () = assert!(MAX_NODES < NO_MAP as usize);

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
#[inline(never)]
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
) -> Result<ReconcileOutcome, BuildError> {
    let Some(draft_root) = draft.root.get() else {
        return Err(BuildError::NoRoot);
    };
    let has_active = active.root.is_some();

    candidate.reset();
    candidate.effects = draft.effects;
    candidate.curves = draft.curves;
    candidate.layout_extras = draft.layout_extras;
    candidate.generation_hwm = active.generation_hwm;
    let tlen = draft.text_len as usize;
    candidate.text_buf[..tlen].copy_from_slice(&draft.text_buf[..tlen]);
    candidate.text_len = draft.text_len;
    candidate.sparklines = draft.sparklines;

    let d_count = draft.count as usize;
    let mut draft_to_cand = [NO_MAP; MAX_NODES];
    let mut matched_from_old = [NO_MAP; MAX_NODES];
    let mut matched_old_mask = NodeMask::ZERO;
    let mut cand_occupied_mask = NodeMask::ZERO;

    // Pass 1: match nodes with identical Key + Kind, preserving their exact slot and NodeId.
    for d_idx in 0..d_count {
        let d = &draft.nodes[d_idx];
        if let Some(old_slot) = active.find_slot_by_key(d.key) {
            let old = &active.nodes[old_slot as usize];
            if old.kind == d.kind {
                let s_u8 = old_slot as u8;
                draft_to_cand[d_idx] = s_u8;
                matched_old_mask.set(old_slot as usize);
                cand_occupied_mask.set(old_slot as usize);
                matched_from_old[old_slot as usize] = s_u8;
            }
        }
    }

    // Pass 2: assign freed or unused slots to newly inserted / kind-changed nodes.
    let mut next_free = 0usize;
    let mut saw_exhausted = false;
    for slot in draft_to_cand.iter_mut().take(d_count) {
        if *slot != NO_MAP {
            continue;
        }
        loop {
            while next_free < MAX_NODES && cand_occupied_mask.get(next_free) {
                next_free += 1;
            }
            if next_free >= MAX_NODES {
                return Err(if saw_exhausted {
                    BuildError::GenerationExhausted
                } else {
                    BuildError::Nodes
                });
            }
            let prev_gen = candidate.generation_hwm[next_free];
            if prev_gen == u32::MAX {
                saw_exhausted = true;
                next_free += 1;
                continue;
            }
            candidate.generation_hwm[next_free] = prev_gen + 1;
            cand_occupied_mask.set(next_free);
            *slot = next_free as u8;
            next_free += 1;
            break;
        }
    }

    candidate
        .part_styles
        .copy_from_draft(&draft.part_styles, &draft_to_cand[..d_count]);

    // Track deleted or kind-changed nodes for damage, animation cleanup, and focus/capture cleanup.
    let mut outcome = ReconcileOutcome::default();
    for (old_idx, old) in active.nodes.iter().enumerate() {
        if !old.occupied || matched_old_mask.get(old_idx) {
            continue;
        }
        outcome.tree_changed = true;
        if !old.visible_rect.is_empty() {
            damage.add(active.style_bounds(old_idx), screen);
        }
        anims.cancel_for_key(old.key);
        if focused_slot == Some(old_idx as u16) {
            outcome.focus_target_invalidated = true;
        }
        if captured_slot == Some(old_idx as u16) {
            outcome.capture_target_invalidated = true;
        }
    }

    // Populate candidate nodes in-place & rebuild parent/child/sibling pointers.
    let map_link = |link: SlotLink| -> SlotLink {
        match link.get() {
            Some(d_i) => {
                let s = draft_to_cand[d_i];
                if s == NO_MAP {
                    SlotLink::NONE
                } else {
                    SlotLink::from_slot(s as usize).unwrap_or(SlotLink::NONE)
                }
            }
            None => SlotLink::NONE,
        }
    };

    for d_idx in 0..d_count {
        let d = &draft.nodes[d_idx];
        let c_slot = draft_to_cand[d_idx] as usize;
        let old_slot = matched_from_old[c_slot];

        if old_slot != NO_MAP {
            candidate.nodes[c_slot].copy_from(&active.nodes[old_slot as usize]);
            candidate.nodes[c_slot].dirty_measure = false;
            candidate.nodes[c_slot].dirty_paint = false;
        } else {
            let new_gen = candidate.generation_hwm[c_slot];
            candidate.nodes[c_slot].clear_fresh(new_gen);
        }

        let node = &mut candidate.nodes[c_slot];
        node.occupied = true;
        node.key = d.key;
        node.kind = d.kind;
        node.props = d.props;
        if old_slot == NO_MAP && matches!(d.kind, NodeKind::ScrollX | NodeKind::ScrollY) {
            node.scroll_y = d.props.initial_scroll();
        }
        node.motion = d.motion;
        node.motion_ms = d.motion_ms;
        node.parent = map_link(d.parent);
        node.first_child = map_link(d.first_child);
        node.last_child = map_link(d.last_child);
        node.next_sibling = map_link(d.next_sibling);
    }

    let root_c = draft_to_cand[draft_root] as usize;
    candidate.root = SlotLink::from_slot(root_c).unwrap_or(SlotLink::NONE);
    candidate.modal = match draft.modal.get() {
        Some(m) => {
            let mc = draft_to_cand[m] as usize;
            SlotLink::from_slot(mc).unwrap_or(SlotLink::NONE)
        }
        None => SlotLink::NONE,
    };
    candidate.active_count = draft.count;

    // Global theme / modal / initial screen checks.
    let active_theme = match active.root.get() {
        Some(r) => active.nodes[r].props.theme_id,
        None => 0,
    };
    let cand_theme = candidate.nodes[root_c].props.theme_id;
    let old_screen = match active.root.get() {
        Some(r) => active.nodes[r].layout_rect,
        None => Rect::ZERO,
    };
    let modal_changed = active.modal.is_some() != candidate.modal.is_some();
    let global_invalidate =
        !has_active || active_theme != cand_theme || old_screen != screen || modal_changed;
    outcome.modal_changed = modal_changed;

    // Check diffs per node.
    for c_idx in 0..MAX_NODES {
        if !candidate.nodes[c_idx].occupied {
            continue;
        }
        let old_slot = matched_from_old[c_idx];
        if old_slot == NO_MAP {
            outcome.tree_changed = true;
            candidate.nodes[c_idx].dirty_measure = true;
            candidate.nodes[c_idx].dirty_paint = true;
            continue;
        }

        let old = &active.nodes[old_slot as usize];
        let node = &candidate.nodes[c_idx];

        let old_parent_key = old.parent.get().map(|i| active.nodes[i].key);
        let new_parent_key = node.parent.get().map(|i| candidate.nodes[i].key);
        let old_next_key = old.next_sibling.get().map(|i| active.nodes[i].key);
        let new_next_key = node.next_sibling.get().map(|i| candidate.nodes[i].key);
        let old_first_key = old.first_child.get().map(|i| active.nodes[i].key);
        let new_first_key = node.first_child.get().map(|i| candidate.nodes[i].key);

        let order_changed = old_parent_key != new_parent_key
            || old_next_key != new_next_key
            || old_first_key != new_first_key;

        let old_part = active.part_styles.get(usize::from(old_slot));
        let new_part = candidate.part_styles.get(c_idx);
        let part_changed = old_part != new_part;
        let part_btn_measure = old_part.affects_button_measure(new_part);
        let old_extra = active.layout_extras.get(node.key);
        let new_extra = candidate.layout_extras.get(node.key);
        let button_part_measure =
            matches!(node.kind, NodeKind::Button | NodeKind::Tab) && part_btn_measure;

        let op = &old.props;
        let np = &node.props;
        let structural_changed = op.width != np.width
            || op.height != np.height
            || op.padding != np.padding
            || op.gap != np.gap
            || op.align_cross != np.align_cross
            || op.max_lines != np.max_lines
            || op.ellipsis() != np.ellipsis()
            || op.visible() != np.visible()
            || op.icon != np.icon
            || op.border_width != np.border_width
            || op.radius != np.radius
            || op.font() != np.font()
            || op.text_align() != np.text_align()
            || op.wrap() != np.wrap()
            || op.letter_spacing != np.letter_spacing
            || op.line_spacing != np.line_spacing
            || op.stack_align() != np.stack_align()
            || op.paint_mode() != np.paint_mode()
            || op.justify() != np.justify()
            || (node.kind == NodeKind::Grid && op.grid_cols() != np.grid_cols())
            || (node.kind == NodeKind::TabBar && op.tab_layout() != np.tab_layout())
            || (node.kind == NodeKind::Tab && op.min != np.min)
            || old_extra.offset_x != new_extra.offset_x
            || old_extra.offset_y != new_extra.offset_y
            || old_extra.motion != new_extra.motion
            || old_extra.min_w != new_extra.min_w
            || old_extra.max_w != new_extra.max_w
            || old_extra.min_h != new_extra.min_h
            || old_extra.max_h != new_extra.max_h
            || old_extra.margin != new_extra.margin
            || button_part_measure
            || (node.kind == NodeKind::Custom
                && part_changed
                && old_part.metrics() != new_part.metrics());

        let mut text_changed_measure = false;
        let old_text = active.text_str(op.text());
        let new_text = draft.text_str(np.text());
        let text_changed = old_text != new_text;
        let mut updated_cache: Option<TextCache> = None;

        if text_changed {
            if !structural_changed && !order_changed && old.text_cache.valid {
                let wrap_w = old.text_cache.wrap_width;
                let max_lines = if matches!(node.kind, NodeKind::Button | NodeKind::Tab) {
                    np.max_lines.max(1)
                } else {
                    np.max_lines
                };
                let font = np.font();
                let align = np.text_align();
                let new_tm = measurer.measure_text_styled(
                    font,
                    new_text,
                    wrap_w,
                    max_lines,
                    np.ellipsis(),
                    align,
                    np.wrap(),
                    np.letter_spacing,
                    np.line_spacing,
                );
                let size_same = new_tm.size == old.text_cache.size;
                let ink_same_or_inside = new_tm.ink == old.text_cache.rel_ink
                    || (new_tm.ink.x >= 0
                        && new_tm.ink.y >= 0
                        && new_tm.ink.right64() <= old.layout_rect.w as i64
                        && new_tm.ink.bottom64() <= old.layout_rect.h as i64);

                updated_cache = Some(TextCache {
                    wrap_width: wrap_w,
                    size: new_tm.size,
                    rel_ink: new_tm.ink,
                    lines: new_tm.lines,
                    valid: true,
                    font,
                    align,
                    max_lines,
                    ellipsis: np.ellipsis(),
                });
                if !size_same || !ink_same_or_inside {
                    text_changed_measure = true;
                }
            } else {
                text_changed_measure = true;
            }
        }

        let sparkline_changed = node.kind == NodeKind::Sparkline
            && active.sparklines.slice(op.series) != candidate.sparklines.slice(np.series);

        let timing_changed = old.motion_ms != node.motion_ms
            || old.motion.curve(&active.curves) != node.motion.curve(&candidate.curves)
            || op.anim_easing() != np.anim_easing();
        let visual_changed = (node.kind == NodeKind::Custom && op != np)
            || op.bg() != np.bg()
            || op.border_color() != np.border_color()
            || op.fg != np.fg
            || op.accent != np.accent
            || op.focus_color != np.focus_color
            || op.overlay_color != np.overlay_color
            || op.overlay_alpha != np.overlay_alpha
            || op.value != np.value
            || op.min != np.min
            || op.max != np.max
            || old.motion.state() != node.motion.state()
            || timing_changed
            || op.checked() != np.checked()
            || op.enabled() != np.enabled()
            || op.focusable() != np.focusable()
            || op.clickable() != np.clickable()
            || active.effects.get(node.key) != candidate.effects.get(node.key)
            || op.focus_ring_raw != np.focus_ring_raw
            || op.press_feedback_raw != np.press_feedback_raw
            || op.text_style() != np.text_style()
            || op.theme_id != np.theme_id
            || op.paint_mode() != np.paint_mode()
            || op.editable() != np.editable()
            || op.pointer_capture() != np.pointer_capture()
            || part_changed
            || old_extra.custom_rev != new_extra.custom_rev
            || sparkline_changed;

        let binding_changed = old.motion.state() != node.motion.state();
        let toggle_changed = node.motion.state() == crate::view::MotionState::None
            && node.kind == NodeKind::Toggle
            && op.checked() != np.checked();
        let node_key = node.key;
        let old_vis_rect = old.visible_rect;
        let old_checked = op.checked();
        let new_checked = np.checked();
        let anim_easing = np.anim_easing();
        let focus_lost = focused_slot == Some(old_slot as u16)
            && (!np.enabled() || !np.visible() || !np.focusable());
        let cap_lost = captured_slot == Some(old_slot as u16) && (!np.enabled() || !np.visible());

        let focus_ink_changed =
            op.focus_ring_raw != np.focus_ring_raw || op.focusable() != np.focusable();
        let c_node = &mut candidate.nodes[c_idx];
        if let Some(cache) = updated_cache {
            c_node.text_cache = cache;
        }
        if !text_changed_measure && (updated_cache.is_some() || focus_ink_changed) {
            refresh_node_ink_rect(c_node);
        }
        if order_changed || structural_changed || text_changed_measure {
            outcome.tree_changed = true;
            c_node.dirty_measure = true;
            c_node.dirty_paint = true;
        } else if text_changed || visual_changed {
            outcome.tree_changed = true;
            c_node.dirty_paint = true;
        }
        let depth = candidate.effects.get(node_key);
        if depth.shadow.is_none()
            || !depth.press_depth.is_enabled()
            || depth.press_depth.duration_ms() == 0
        {
            anims.clear_property(node_key, AnimProp::PressDepth);
        }
        if binding_changed {
            c_node.reset_style_target();
            anims.clear_property(node_key, AnimProp::Style);
            anims.clear_property(node_key, AnimProp::ToggleKnob);
        }
        if !binding_changed
            && timing_changed
            && c_node.motion_ms > 0
            && let Some(target) = anims.active_target(node_key, AnimProp::Style)
        {
            anims.start_with_curve(
                node_key,
                AnimProp::Style,
                target,
                target,
                now,
                c_node.motion_ms.into(),
                old_vis_rect,
                anim_easing,
                c_node.motion.curve(&candidate.curves),
            );
        }
        if toggle_changed {
            let start_def = if old_checked { ANIM_SCALE } else { 0 };
            let end_val = if new_checked { ANIM_SCALE } else { 0 };
            anims.start_with_easing(
                node_key,
                AnimProp::ToggleKnob,
                start_def,
                end_val,
                now,
                TOGGLE_ANIM_DURATION_MS,
                old_vis_rect,
                anim_easing,
            );
        }
        if focus_lost {
            outcome.focus_target_invalidated = true;
        }
        if cap_lost {
            outcome.capture_target_invalidated = true;
            c_node.pressed = false;
        }
    }

    if global_invalidate {
        candidate.nodes[root_c].dirty_measure |=
            !has_active || old_screen != screen || modal_changed;
        candidate.nodes[root_c].dirty_paint = true;
        outcome.tree_changed = true;
    }

    // Reveal a newly selected page once. Ordinary commits preserve manual browsing.
    for i in 0..d_count {
        let slot = usize::from(draft_to_cand[i]);
        let node = &candidate.nodes[slot];
        if node.kind == NodeKind::Tab
            && node.props.checked()
            && (matched_from_old[slot] == NO_MAP || !active.nodes[slot].props.checked())
            && let Some(parent) = node.parent.get()
            && candidate.nodes[parent].kind == NodeKind::TabBar
        {
            candidate.nodes[parent].scroll_y = -1;
            candidate.nodes[parent].dirty_measure = true;
        }
    }

    // Propagate dirty_measure up parent chain using bitmask.
    let mut dirty_mask = NodeMask::ZERO;
    for (idx, n) in candidate.nodes.iter().enumerate() {
        if n.occupied && n.dirty_measure {
            dirty_mask.set(idx);
        }
    }
    for idx in 0..MAX_NODES {
        if !dirty_mask.get(idx) {
            continue;
        }
        let mut p = candidate.nodes[idx].parent;
        while let Some(p_idx) = p.get() {
            if candidate.nodes[p_idx].dirty_measure {
                break;
            }
            candidate.nodes[p_idx].dirty_measure = true;
            p = candidate.nodes[p_idx].parent;
        }
    }

    outcome.relayout_needed = candidate.nodes[root_c].dirty_measure;
    outcome.measured_nodes =
        layout_tree(candidate, screen, measurer).map_err(BuildError::Layout)?;

    // Update running animation dirty_rect to new visible_rect.
    for n in &candidate.nodes {
        if !n.occupied {
            continue;
        }
        if n.kind == NodeKind::Toggle {
            anims.update_rect(n.key, AnimProp::ToggleKnob, n.visible_rect);
        }
        if n.kind == NodeKind::Custom || n.props.paint_mode() == crate::view::PaintMode::Override {
            for ch in 0..4u8 {
                anims.update_rect(n.key, AnimProp::custom(ch), n.visible_rect);
            }
        }
    }

    // Add dirty rectangles to DamageSet.
    if global_invalidate {
        damage.mark_full_screen(screen);
    } else {
        for (c_idx, n) in candidate.nodes.iter().enumerate() {
            if !n.occupied {
                continue;
            }
            let old_slot = matched_from_old[c_idx];
            let old_vis = if old_slot != NO_MAP {
                active.nodes[old_slot as usize].visible_rect
            } else {
                Rect::ZERO
            };
            if n.dirty_paint || n.dirty_measure || n.visible_rect != old_vis {
                if !old_vis.is_empty() {
                    damage.add(active.style_bounds(usize::from(old_slot)), screen);
                }
                if !n.visible_rect.is_empty() {
                    damage.add(candidate.style_bounds(c_idx), screen);
                }
            }
        }
    }

    Ok(outcome)
}
