use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb565;
use liuli_core::capacity::{
    BuildError, DEFAULT_BAND_HEIGHT, DispatchError, FrameMetrics, PresentError, PresentStatus,
    TimeError,
};
use liuli_core::damage::DamageSet;
use liuli_core::focus::{FocusState, PointerState, dispatch_input};
use liuli_core::geometry::Rect;
use liuli_core::input::RawInput;
use liuli_core::node::RetainedTree;
use liuli_core::paint::{CommandList, PaintFocusInfo, emit_commands};
use liuli_core::reconcile::reconcile_into;
use liuli_core::time::{AnimProp, InstantMs, NextWakeup};
use liuli_core::view::{App, Change, NONE_IDX, NodeKind, ViewBuilder};
use liuli_render::{DmaCoordinator, DmaSink, present_sync_with};
use liuli_text::Lf4Font;

use crate::storage::Storage;

pub struct Engine<'s, 'f, A: App> {
    storage: &'s mut Storage,
    model: A::Model,
    font: Lf4Font<'f>,
    screen: Rect,
    band_height: u32,
    view_pending: bool,
    timer_pending_after_dma: bool,
    last_metrics: FrameMetrics,
}

impl<'s, 'f, A: App> Engine<'s, 'f, A> {
    pub fn new_in_place(
        storage: &'s mut Storage,
        model: A::Model,
        font: Lf4Font<'f>,
        screen: Rect,
        now: InstantMs,
    ) -> Result<Self, BuildError> {
        storage.init_in_place();
        let _ = storage.clock.advance_to(now);

        let mut engine = Self {
            storage,
            model,
            font,
            screen,
            band_height: DEFAULT_BAND_HEIGHT as u32,
            view_pending: false,
            timer_pending_after_dma: false,
            last_metrics: FrameMetrics::default(),
        };
        engine.commit_view(now)?;
        engine.storage.damage.mark_full_screen(screen);
        Ok(engine)
    }

    #[inline]
    pub fn model(&self) -> &A::Model {
        &self.model
    }

    #[inline]
    pub fn model_mut(&mut self) -> &mut A::Model {
        &mut self.model
    }

    #[inline]
    pub fn font(&self) -> &Lf4Font<'f> {
        &self.font
    }

    #[inline]
    pub const fn screen(&self) -> Rect {
        self.screen
    }

    #[inline]
    pub fn active_tree(&self) -> &RetainedTree {
        self.storage.active_tree()
    }

    #[inline]
    pub fn active_commands(&self) -> &CommandList {
        self.storage.active_cmds()
    }

    #[inline]
    pub fn damage(&self) -> &DamageSet {
        &self.storage.damage
    }

    #[inline]
    pub const fn focus(&self) -> &FocusState {
        &self.storage.focus
    }

    #[inline]
    pub const fn pointer(&self) -> &PointerState {
        &self.storage.pointer
    }

    #[inline]
    pub const fn last_metrics(&self) -> FrameMetrics {
        self.last_metrics
    }

    #[inline]
    pub const fn view_pending(&self) -> bool {
        self.view_pending
    }

    #[inline]
    pub const fn band_height(&self) -> u32 {
        self.band_height
    }

    pub fn set_band_height(&mut self, height: u32) {
        self.band_height = height.max(1);
    }

    pub fn set_focus_ring_visible(&mut self, visible: bool) {
        if self.storage.focus.ring_visible != visible {
            self.storage.focus.ring_visible = visible;
            if let Some(slot) = self.storage.focus.focused_slot(self.storage.active_tree()) {
                let r = self.storage.active_tree().nodes[slot as usize].visible_rect;
                self.storage.damage.add(r, self.screen);
            }
            let _ = self.reemit_commands();
        }
    }

    pub fn commit_view(&mut self, now: InstantMs) -> Result<(), BuildError> {
        {
            let mut vb = ViewBuilder::new(&mut self.storage.draft);
            if let Err(err) = A::view(&self.model, &mut vb) {
                self.view_pending = true;
                return Err(err);
            }
            if let Err(err) = vb.finish() {
                self.view_pending = true;
                return Err(err);
            }
        }

        let active_t_idx = self.storage.active_tree_idx as usize;
        let cand_t_idx = 1 - active_t_idx;
        let active_c_idx = self.storage.active_cmd_idx as usize;
        let cand_c_idx = 1 - active_c_idx;

        let mut cand_anims = self.storage.anims.clone();
        let mut cand_focus = self.storage.focus;
        let mut cand_pointer = self.storage.pointer;
        self.storage.cand_damage.clear();

        let focused_slot = cand_focus.focused_slot(&self.storage.trees[active_t_idx]);
        let captured_slot = cand_pointer.captured_slot();

        let (tree_pair_a, tree_pair_b) = self.storage.trees.split_at_mut(1);
        let (active_tree, cand_tree): (&RetainedTree, &mut RetainedTree) = if active_t_idx == 0 {
            (&tree_pair_a[0], &mut tree_pair_b[0])
        } else {
            (&tree_pair_b[0], &mut tree_pair_a[0])
        };

        let res = reconcile_into(
            &self.storage.draft,
            active_tree,
            cand_tree,
            self.screen,
            &self.font,
            &mut cand_anims,
            focused_slot,
            captured_slot,
            now,
            &mut self.storage.cand_damage,
        );

        let (outcome, _) = match res {
            Ok(v) => v,
            Err(err) => {
                self.view_pending = true;
                return Err(err);
            }
        };

        if outcome.capture_target_invalidated {
            cand_pointer.active_id = None;
            cand_pointer.capture = liuli_core::focus::CaptureKind::None;
        }

        let was_modal = active_tree.modal != NONE_IDX;
        let is_modal = cand_tree.modal != NONE_IDX;
        if !was_modal && is_modal {
            cand_pointer.clear(cand_tree, &mut self.storage.cand_damage, self.screen);
            cand_focus.on_modal_opened(cand_tree);
        } else if was_modal && !is_modal {
            cand_focus.on_modal_closed(cand_tree, &mut self.storage.cand_damage, self.screen);
        } else if outcome.focus_target_invalidated || cand_focus.focused_key.is_none() {
            cand_focus.ensure_valid(cand_tree);
        } else {
            cand_focus.ensure_valid(cand_tree);
        }

        let focus_info = PaintFocusInfo {
            focused_slot: cand_focus.focused_slot(cand_tree),
            ring_visible: cand_focus.ring_visible,
            editing: cand_focus.editing,
        };

        if let Err(err) = emit_commands(
            cand_tree,
            &cand_anims,
            focus_info,
            self.screen,
            &mut self.storage.cmds[cand_c_idx],
        ) {
            self.view_pending = true;
            return Err(err);
        }

        // Atomic commit: swap tree & command indices and apply state + damage
        self.storage.active_tree_idx = cand_t_idx as u8;
        self.storage.active_cmd_idx = cand_c_idx as u8;
        self.storage.anims = cand_anims;
        self.storage.focus = cand_focus;
        self.storage.pointer = cand_pointer;
        for &r in self.storage.cand_damage.as_slice() {
            self.storage.damage.add(r, self.screen);
        }
        self.storage.cand_damage.clear();
        self.view_pending = false;
        self.last_metrics.measured_nodes = outcome.measured_nodes;

        Ok(())
    }

    pub fn reemit_commands(&mut self) -> Result<(), BuildError> {
        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;
        let cand_c_idx = 1 - active_c_idx;
        let tree = &self.storage.trees[active_t_idx];
        let focus_info = PaintFocusInfo {
            focused_slot: self.storage.focus.focused_slot(tree),
            ring_visible: self.storage.focus.ring_visible,
            editing: self.storage.focus.editing,
        };
        emit_commands(
            tree,
            &self.storage.anims,
            focus_info,
            self.screen,
            &mut self.storage.cmds[cand_c_idx],
        )?;
        self.storage.active_cmd_idx = cand_c_idx as u8;
        Ok(())
    }

    pub fn dispatch(
        &mut self,
        input: RawInput,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<bool, DispatchError<RawInput>> {
        if dma_busy {
            return Err(DispatchError::Busy(input));
        }
        if self.storage.clock.advance_to(now).is_err() {
            return Err(DispatchError::WentBackwards);
        }

        let active_t_idx = self.storage.active_tree_idx as usize;
        self.storage.cand_damage.clear();

        let (consumed, action) = dispatch_input(
            &mut self.storage.trees[active_t_idx],
            &mut self.storage.focus,
            &mut self.storage.pointer,
            input,
            self.screen,
            &mut self.storage.cand_damage,
        );

        // Update toggle animation dirty rects in case ScrollY offset moved them
        for n in &self.storage.trees[active_t_idx].nodes {
            if n.occupied && n.kind == NodeKind::Toggle {
                self.storage
                    .anims
                    .update_rect(n.key, AnimProp::ToggleKnob, n.visible_rect);
            }
        }

        let had_local_damage = !self.storage.cand_damage.is_empty();
        for &r in self.storage.cand_damage.as_slice() {
            self.storage.damage.add(r, self.screen);
        }
        self.storage.cand_damage.clear();

        let mut rebuilt = false;
        if let Some(act) = action
            && let Some(msg) = A::on_action(&self.model, act)
            && A::update(&mut self.model, msg, now) == Change::View
        {
            let _ = self.commit_view(now);
            rebuilt = true;
        }

        if !rebuilt && (had_local_damage || self.view_pending) {
            if self.view_pending {
                let _ = self.commit_view(now);
            } else {
                let _ = self.reemit_commands();
            }
        }

        Ok(consumed)
    }

    pub fn message(
        &mut self,
        msg: A::Msg,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<(), DispatchError<A::Msg>> {
        if dma_busy {
            return Err(DispatchError::Busy(msg));
        }
        if self.storage.clock.advance_to(now).is_err() {
            return Err(DispatchError::WentBackwards);
        }
        if A::update(&mut self.model, msg, now) == Change::View || self.view_pending {
            let _ = self.commit_view(now);
        }
        Ok(())
    }

    pub fn advance(&mut self, now: InstantMs, dma_busy: bool) -> Result<bool, TimeError> {
        self.storage.clock.advance_to(now)?;
        if dma_busy {
            self.timer_pending_after_dma = true;
            return Ok(false);
        }
        self.timer_pending_after_dma = false;

        let mut any_change = false;
        if A::on_timer(&mut self.model, now) == Change::View && self.commit_view(now).is_ok() {
            any_change = true;
        }

        let anim_changed = self.storage.anims.advance(now, |dirty| {
            self.storage.damage.add(dirty, self.screen);
        });
        if anim_changed {
            let _ = self.reemit_commands();
            any_change = true;
        }

        Ok(any_change)
    }

    pub fn next_wakeup(&self, now: InstantMs, dma_busy: bool) -> NextWakeup {
        if dma_busy {
            return NextWakeup::OnEvent;
        }
        if self.timer_pending_after_dma || !self.storage.damage.is_empty() {
            return NextWakeup::Now;
        }
        let anim_wake = self.storage.anims.next_wakeup(now);
        let app_wake = match A::next_deadline(&self.model, now) {
            Some(t) if t <= now => NextWakeup::Now,
            Some(t) => NextWakeup::At(t),
            None => NextWakeup::OnEvent,
        };
        anim_wake.earlier(app_wake)
    }

    pub fn present_sync<T: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut T,
        scratch: &mut [u16],
    ) -> Result<PresentStatus, PresentError<T::Error>> {
        self.present_sync_with(target, scratch, |_| Ok(()))
    }

    pub fn present_sync_with<T, Fin>(
        &mut self,
        target: &mut T,
        scratch: &mut [u16],
        finish: Fin,
    ) -> Result<PresentStatus, PresentError<T::Error>>
    where
        T: DrawTarget<Color = Rgb565>,
        Fin: FnMut(&mut T) -> Result<(), T::Error>,
    {
        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;

        let (status, mut m) = present_sync_with(
            target,
            scratch,
            self.screen,
            self.band_height,
            &self.storage.cmds[active_c_idx],
            &mut self.storage.damage,
            &self.storage.trees[active_t_idx],
            &self.font,
            finish,
        )?;
        m.measured_nodes = self.last_metrics.measured_nodes;
        self.last_metrics = m;
        Ok(status)
    }

    pub fn poll_dma<'a, S>(
        &mut self,
        dma: &mut DmaCoordinator<'a>,
        sink: &mut S,
    ) -> Result<PresentStatus, PresentError<S::Error>>
    where
        S: DmaSink<'a>,
    {
        if !dma.is_active() && !self.storage.damage.is_empty() {
            dma.begin_frame(&self.storage.damage, self.screen, self.band_height)?;
        }

        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;

        let (status, mut m) = dma.step(
            sink,
            &self.storage.cmds[active_c_idx],
            &mut self.storage.damage,
            &self.storage.trees[active_t_idx],
            &self.font,
        )?;
        m.measured_nodes = self.last_metrics.measured_nodes;
        self.last_metrics = m;
        Ok(status)
    }
}
