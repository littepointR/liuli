use liuli_core::focus::{ScrollGesture, ScrollPhase};
use liuli_core::node::{NodeId, RetainedNode, RetainedTree};
use liuli_core::time::{InstantMs, NextWakeup, ScrollPhysics};
use liuli_core::view::{App, Key, ScrollPolicy};

/// One native trajectory for the primary pointer. Inactive viewports retain only
/// their layout offset. A bound controller also accepts application commands.
#[derive(Clone, Copy, Default)]
pub(crate) struct TouchScroll {
    pub owner: Option<(NodeId, Key)>,
    pub pressed_at: InstantMs,
    origin: i32,
    native: ScrollPhysics,
}

impl TouchScroll {
    pub fn controller<'a, A: App>(
        &'a mut self,
        model: &'a mut A::Model,
        id: NodeId,
        key: Key,
    ) -> Option<(&'a mut ScrollPhysics, i32)> {
        if let Some(controller) = A::scroll_controller(model, key) {
            return Some((controller, 0));
        }
        (self.owner == Some((id, key))).then_some((&mut self.native, self.origin))
    }

    pub fn notify_offsets<A: App>(
        &mut self,
        tree: &RetainedTree,
        model: &mut A::Model,
        now: InstantMs,
    ) {
        for (slot, node) in tree.nodes.iter().enumerate() {
            if node.occupied
                && let Some(limit) = node.scroll_limit()
                && let Some((motion, origin)) =
                    self.controller::<A>(model, node.id(slot as u16), node.key)
            {
                if motion.is_active() && node.props.scroll_policy().is_some_and(|p| !p.inertia) {
                    motion.settle_immediately(now);
                }
                let offset = motion.offset().saturating_add(origin).clamp(0, limit);
                if offset != node.scroll_y
                    && let Some(msg) = A::on_scroll(model, node.key, offset)
                {
                    A::update(model, msg, now);
                }
            }
        }
    }

    pub fn sample<A: App>(
        &mut self,
        tree: &RetainedTree,
        model: &mut A::Model,
        now: InstantMs,
        advance: bool,
    ) -> (bool, NextWakeup) {
        let mut changed = false;
        let mut wake = NextWakeup::OnEvent;
        for (slot, node) in tree.nodes.iter().enumerate() {
            if !node.occupied || !node.kind.is_scroll_view() {
                continue;
            }
            let Some((motion, origin)) =
                self.controller::<A>(model, node.id(slot as u16), node.key)
            else {
                continue;
            };
            let moved = advance && motion.is_active() && motion.advance(now);
            wake = wake.earlier(motion.next_wakeup(now));
            if moved {
                let offset = motion
                    .offset()
                    .saturating_add(origin)
                    .clamp(0, node.scroll_limit().unwrap_or(0));
                changed |= offset != node.scroll_y;
            }
        }
        (changed, wake)
    }

    pub fn did_commit<A: App>(
        &mut self,
        model: &mut A::Model,
        active_tree: &RetainedTree,
        cand_tree: &RetainedTree,
        page_switched: bool,
        revealed_tabs: liuli_core::capacity::NodeMask,
    ) {
        let scope = liuli_core::focus::active_scope_root(cand_tree);
        if let Some((id, _)) = self.owner
            && (page_switched
                || !cand_tree.is_id_valid(id)
                || !cand_tree.is_effectively_enabled_and_visible(id.slot())
                || !scope.is_some_and(|root| cand_tree.is_descendant_of(id.slot(), root)))
        {
            self.stop::<A>(model);
        }
        for (idx, node) in active_tree.nodes.iter().enumerate() {
            if node.occupied
                && node.kind.is_scroll_view()
                && (page_switched || !cand_tree.is_id_valid(node.id(idx as u16)))
                && let Some(controller) = A::scroll_controller(model, node.key)
            {
                controller.set_offset(controller.offset());
            }
        }
        for (idx, node) in cand_tree.nodes.iter().enumerate() {
            if node.occupied
                && let Some(limit) = node.scroll_limit()
                && let Some((controller, origin)) =
                    self.controller::<A>(model, node.id(idx as u16), node.key)
            {
                controller.set_bounds(-origin, limit.saturating_sub(origin));
                let policy = node.props.scroll_policy();
                if let Some(policy) = policy {
                    controller.snap_step = policy.snap_step;
                }
                if controller.offset().saturating_add(origin) != node.scroll_y
                    || controller.is_overscrolled()
                    || revealed_tabs.get(idx)
                    || (controller.is_active()
                        && (policy.is_some_and(|p| !p.inertia)
                            || !cand_tree.is_effectively_enabled_and_visible(idx as u16)
                            || !scope
                                .is_some_and(|root| cand_tree.is_descendant_of(idx as u16, root))))
                {
                    controller.set_offset(node.scroll_y - origin);
                }
            }
        }
    }

    pub fn stop<A: App>(&mut self, model: &mut A::Model) {
        if let Some((_, key)) = self.owner.take()
            && let Some(controller) = A::scroll_controller(model, key)
        {
            controller.set_offset(controller.offset());
        }
        self.native = ScrollPhysics::ZERO;
    }

    pub fn input<A: App>(
        &mut self,
        model: &mut A::Model,
        id: NodeId,
        node: &RetainedNode,
        gesture: ScrollGesture,
        now: InstantMs,
    ) -> i32 {
        let limit = node.scroll_limit().unwrap_or(0);
        let policy = node.props.scroll_policy().unwrap_or(ScrollPolicy::FREE);
        if gesture.phase == ScrollPhase::Begin {
            self.stop::<A>(model);
            self.owner = Some((id, node.key));
            // Relative Q8 coordinates retain subpixel precision deep inside a large virtual list.
            // Align the origin to the snap grid so local and content stops coincide.
            let step = i32::from(policy.snap_step.max(1));
            self.origin = node.scroll_y / step * step;
            self.native = ScrollPhysics::ZERO.with_snap(policy.snap_step);
        } else if self.owner != Some((id, node.key)) {
            return node.scroll_y;
        }
        let pressed_at = self.pressed_at;
        let Some((motion, origin)) = self.controller::<A>(model, id, node.key) else {
            return node.scroll_y;
        };
        if gesture.phase == ScrollPhase::Begin {
            motion.set_bounds(-origin, limit - origin);
            motion.set_offset(node.scroll_y - origin);
            motion.begin_drag(pressed_at);
        }
        let desired = gesture.offset.clamp(0, limit) - origin;
        let delta = desired.saturating_sub(motion.offset());
        if gesture.phase == ScrollPhase::Cancel {
            motion.set_offset(node.scroll_y - origin);
        } else {
            if delta != 0 {
                motion.drag_by(delta, now);
            }
            if gesture.phase == ScrollPhase::Release {
                if policy.inertia {
                    motion.release(now);
                } else {
                    motion.settle_immediately(now);
                }
            }
        }
        motion.offset().saturating_add(origin).clamp(0, limit)
    }
}
