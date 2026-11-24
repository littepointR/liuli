use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb565;
use liuli_core::capacity::{
    BuildError, DEFAULT_BAND_HEIGHT, DispatchError, FrameMetrics, MAX_NODES, PresentError,
    PresentStatus, TimeError, TransactionError,
};
use liuli_core::damage::DamageSet;
use liuli_core::focus::{
    CaptureKind, FocusState, PointerState, RepeatSource, RepeatState, adjust_slider_by_steps_pub,
    dispatch_input, is_valid_repeat_target, pointer_hits_id_in_scope,
};
use liuli_core::geometry::Rect;
use liuli_core::input::{ActionKind, HoldKey, RawInput, UiAction};
use liuli_core::layout::{ensure_visible, relayout_scroll_subtree};
use liuli_core::node::{NodeId, RetainedTree};
use liuli_core::paint::{CommandList, PaintFocusInfo, emit_commands_with};
use liuli_core::reconcile::reconcile_into;
use liuli_core::time::{
    AnimProp, InstantMs, MAX_REPEAT_STEPS_PER_ADVANCE, NextWakeup, REPEAT_DELAY_MS,
    REPEAT_PERIOD_MS,
};
use liuli_core::view::{App, Change, Key, NodeKind, ViewBuilder};
use liuli_render::{DmaCoordinator, DmaSink};
use liuli_text::{FontCacheStats, FontProvider, FontSet, Lf4Font};

use crate::storage::Storage;
use liuli_core::geometry::Size;
use liuli_core::layout::{TextMeasureOut, TextMeasurer};
use liuli_core::view::{FontId, TextAlign};

struct AppMeasure<'a, A, F>(&'a F, core::marker::PhantomData<A>);
impl<A: App, F: TextMeasurer> TextMeasurer for AppMeasure<'_, A, F> {
    fn custom_size(&self, key: Key, font: FontId, available: Size) -> Size {
        A::measure_custom(key, font, available)
    }
    fn line_height(&self, font: FontId) -> u32 {
        self.0.line_height(font)
    }
    fn measure_text(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
    ) -> TextMeasureOut {
        self.0
            .measure_text(font, text, max_width, max_lines, ellipsis, align)
    }
    fn measure_text_styled(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
        wrap: liuli_core::view::WrapMode,
        letter_spacing: i8,
        line_spacing: i8,
    ) -> TextMeasureOut {
        self.0.measure_text_styled(
            font,
            text,
            max_width,
            max_lines,
            ellipsis,
            align,
            wrap,
            letter_spacing,
            line_spacing,
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct ActiveDma {
    session_token: u32,
    frame_id: u32,
    scene_version: u32,
}

pub struct Engine<'s, 'f, A: App, F: FontProvider + TextMeasurer = FontSet<'f>> {
    storage: &'s mut Storage,
    model: A::Model,
    fonts: F,
    screen: Rect,
    band_height: u32,
    #[cfg(feature = "platform-blend")]
    blend: fn(&mut [u16], Rgb565, u8),
    view_pending: bool,
    timer_pending_after_dma: bool,
    last_build_error: Option<BuildError>,
    build_key: Option<Key>,
    last_metrics: FrameMetrics,
    pending_measured_nodes: u32,
    scene_version: u32,
    next_dma_session: u32,
    active_dma: Option<ActiveDma>,
    #[cfg(feature = "enhanced")]
    pair: liuli_enhanced::gestures::TouchPair,
    has_committed_once: bool,
    active_page: u8,
    last_content_focus: Option<Key>,
    _font_lt: core::marker::PhantomData<&'f ()>,
}

impl<'s, 'f, A: App> Engine<'s, 'f, A, FontSet<'f>> {
    pub fn new_in_place(
        storage: &'s mut Storage,
        model: A::Model,
        font: Lf4Font<'f>,
        screen: Rect,
        now: InstantMs,
    ) -> Result<Self, BuildError> {
        Self::new_in_place_with_fonts(storage, model, FontSet::single(font), screen, now)
    }

    pub fn new_in_place_with_fonts(
        storage: &'s mut Storage,
        model: A::Model,
        fonts: FontSet<'f>,
        screen: Rect,
        now: InstantMs,
    ) -> Result<Self, BuildError> {
        Self::new_in_place_with_provider(storage, model, fonts, screen, now)
    }

    #[inline]
    pub fn font(&self) -> &Lf4Font<'f> {
        self.fonts.body16()
    }

    #[inline]
    pub fn fonts(&self) -> &FontSet<'f> {
        &self.fonts
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
        self.present_sync_with_pixels(
            target,
            scratch,
            finish,
            &mut liuli_text::storage::InlinePixels,
        )
        .map_err(liuli_render::sync::FrameError::into_display)
    }

    pub fn present_sync_with_pixels<T, Fin, G: liuli_text::storage::GlyphPixels>(
        &mut self,
        target: &mut T,
        scratch: &mut [u16],
        finish: Fin,
        glyphs: &mut G,
    ) -> Result<PresentStatus, liuli_render::sync::FrameError<T::Error, G::Error>>
    where
        T: DrawTarget<Color = Rgb565>,
        Fin: FnMut(&mut T) -> Result<(), T::Error>,
    {
        if self.is_dma_busy() {
            return Err(PresentError::DmaBusy.into());
        }
        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;

        let (status, mut m) = liuli_render::sync::present_sync_with_pixels(
            target,
            scratch,
            self.screen,
            self.band_height,
            &self.storage.cmds[active_c_idx],
            &mut self.storage.damage,
            &self.storage.trees[active_t_idx],
            &self.fonts,
            finish,
            #[cfg(feature = "platform-blend")]
            self.blend,
            #[cfg(not(feature = "platform-blend"))]
            liuli_render::raster::blend_solid,
            glyphs,
        )?;
        self.record_present_metrics(status, &mut m);
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
        self.poll_dma_with_provider(dma, sink)
            .map_err(|err| match err {
                liuli_render::sync::FrameError::Display(e) => e,
                liuli_render::sync::FrameError::ShortGlyph { needed, actual } => {
                    PresentError::ShortGlyph { needed, actual }
                }
                liuli_render::sync::FrameError::Font(never) => match never {},
            })
    }
}

impl<'s, 'f, A: App, F: FontProvider + TextMeasurer> Engine<'s, 'f, A, F> {
    pub fn new_in_place_with_provider(
        storage: &'s mut Storage,
        model: A::Model,
        fonts: F,
        screen: Rect,
        now: InstantMs,
    ) -> Result<Self, BuildError> {
        storage.init_in_place();
        let _ = storage.clock.advance_to(now);
        let active_page = A::page_id(&model);

        let mut engine = Self {
            storage,
            model,
            fonts,
            screen,
            #[cfg(feature = "platform-blend")]
            blend: liuli_render::raster::blend_solid,
            band_height: DEFAULT_BAND_HEIGHT as u32,
            view_pending: false,
            timer_pending_after_dma: false,
            last_build_error: None,
            build_key: None,
            last_metrics: FrameMetrics::default(),
            pending_measured_nodes: 0,
            scene_version: 0,
            next_dma_session: 1,
            active_dma: None,
            #[cfg(feature = "enhanced")]
            pair: Default::default(),
            has_committed_once: false,
            active_page,
            last_content_focus: None,
            _font_lt: core::marker::PhantomData,
        };
        engine.commit_view_inner(now)?;
        engine.storage.damage.mark_full_screen(screen);
        Ok(engine)
    }

    #[inline]
    pub fn font_provider(&self) -> &F {
        &self.fonts
    }

    #[inline]
    pub fn font_cache_stats(&self) -> FontCacheStats {
        self.fonts.cache_stats()
    }

    /// Hot-swaps the active font provider, marks all text nodes for re-measurement, and redraws.
    pub fn reload_fonts(&mut self, fonts: F, now: InstantMs) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        self.storage.clock.advance_to(now)?;
        self.fonts = fonts;
        let active_t_idx = self.storage.active_tree_idx as usize;
        for node in &mut self.storage.trees[active_t_idx].nodes {
            if node.occupied {
                node.text_cache = liuli_core::node::TextCache::ZERO;
                node.dirty_measure = true;
                node.dirty_paint = true;
            }
        }
        self.commit_view_inner(now)
            .map_err(TransactionError::Build)?;
        self.storage.damage.mark_full_screen(self.screen);
        Ok(())
    }

    #[inline]
    fn record_present_metrics(&mut self, status: PresentStatus, m: &mut FrameMetrics) {
        match status {
            PresentStatus::Presented => {
                m.measured_nodes = self.pending_measured_nodes;
                self.pending_measured_nodes = 0;
                self.last_metrics = *m;
            }
            PresentStatus::Idle => {
                m.measured_nodes = 0;
                self.last_metrics = *m;
            }
            PresentStatus::Pending => {
                m.measured_nodes = self.pending_measured_nodes;
                self.last_metrics = *m;
            }
        }
    }

    #[inline]
    #[cfg(feature = "inspector")]
    pub fn style_source(&self, key: Key) -> Option<&liuli_core::style_source::StyleSource> {
        self.storage.draft.inspection.as_ref()?.source(key)
    }
    pub fn model(&self) -> &A::Model {
        &self.model
    }

    #[inline]
    pub const fn is_dma_busy(&self) -> bool {
        self.active_dma.is_some()
    }

    #[inline]
    pub const fn scene_version(&self) -> u32 {
        self.scene_version
    }

    #[inline]
    pub fn try_model_mut(&mut self) -> Result<&mut A::Model, TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        Ok(&mut self.model)
    }

    #[inline]
    pub fn try_with_model<R, Cb>(&mut self, f: Cb) -> Result<R, TransactionError<()>>
    where
        Cb: FnOnce(&mut A::Model) -> R,
    {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        Ok(f(&mut self.model))
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
    pub const fn repeat(&self) -> &RepeatState {
        &self.storage.repeat
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
    pub const fn last_build_error(&self) -> Option<BuildError> {
        self.last_build_error
    }

    #[inline]
    pub const fn band_height(&self) -> u32 {
        self.band_height
    }

    /// Select a bit-exact solid overlay kernel for synchronous presentation.
    #[cfg(feature = "platform-blend")]
    pub fn set_sync_blend(&mut self, blend: fn(&mut [u16], Rgb565, u8)) {
        self.blend = blend;
    }

    /// Repaint the committed scene after the display loses its contents.
    pub fn invalidate_screen(&mut self) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        self.storage.damage.mark_full_screen(self.screen);
        Ok(())
    }

    /// Apply a panel address-window constraint before starting a transfer.
    pub fn align_damage(
        &mut self,
        x: core::num::NonZeroU16,
        y: core::num::NonZeroU16,
    ) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        self.storage.damage.align_windows(self.screen, x, y);
        Ok(())
    }

    /// Pixel-buffer budget for each of the two transactional command lists.
    #[cfg(feature = "enhanced")]
    pub fn set_layer_budget(&mut self, bytes: usize) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        for list in &mut self.storage.cmds {
            list.layer_budget = Some(bytes);
        }
        Ok(())
    }

    pub fn set_band_height(&mut self, height: u32) {
        self.band_height = height.max(1);
    }

    pub fn try_set_focus_ring_visible(
        &mut self,
        visible: bool,
    ) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        let prev = self.storage.focus.ring_visible;
        if prev != visible {
            self.storage.focus.ring_visible = visible;
            if let Err(err) = self.reemit_commands_inner() {
                self.storage.focus.ring_visible = prev;
                return Err(TransactionError::Build(err));
            }
            if let Some(slot) = self.storage.focus.focused_slot(self.storage.active_tree()) {
                let r = self.storage.active_tree().nodes[slot as usize].visible_rect;
                self.storage.damage.add(r, self.screen);
                self.storage.trees[self.storage.active_tree_idx as usize]
                    .expand_effect_damage(&mut self.storage.damage, self.screen);
            }
        }
        Ok(())
    }

    pub fn set_focus_ring_visible(&mut self, visible: bool) {
        let _ = self.try_set_focus_ring_visible(visible);
    }

    pub fn commit_view(&mut self, now: InstantMs) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        self.commit_view_inner(now).map_err(TransactionError::Build)
    }

    /// Page, attempted widget (or text container), and exhausted resource from the last transaction.
    pub fn build_failure(&self) -> Option<(u8, Option<Key>, BuildError)> {
        self.last_build_error
            .map(|error| (A::page_id(&self.model), self.build_key, error))
    }
    #[inline]
    fn commit_view_inner(&mut self, now: InstantMs) -> Result<(), BuildError> {
        #[cfg(feature = "enhanced")]
        let previous = self.pair.owner().and_then(|id| {
            let tree = self.storage.active_tree();
            tree.is_id_valid(id)
                .then(|| (id, tree.nodes[id.slot() as usize].key, self.active_page))
        });
        self.commit_view_once(now)?;
        #[cfg(feature = "enhanced")]
        if let Some((id, key, page)) = previous {
            let tree = self.storage.active_tree();
            let still_eligible = self.active_page == page
                && tree.is_id_valid(id)
                && tree.is_effectively_enabled_and_visible(id.slot())
                && A::accepts_transform(&self.model, key)
                && liuli_core::focus::active_scope_root(tree)
                    .is_some_and(|root| tree.is_descendant_of(id.slot(), root));
            if !still_eligible
                && let liuli_enhanced::gestures::Delivery::Gesture(_, cancel) = self.pair.cancel()
                && let Some(msg) = A::on_transform(&self.model, key, cancel)
                && A::update(&mut self.model, msg, now) == Change::View
            {
                // Cancellation already released the pair. A corrective view cannot cancel it twice.
                self.commit_view_once(now)?;
            }
        }
        Ok(())
    }
    fn commit_view_once(&mut self, now: InstantMs) -> Result<(), BuildError> {
        self.build_key = None;
        let old_page = self.active_page;
        let new_page = A::page_id(&self.model);
        let page_switched = self.has_committed_once && old_page != new_page;

        let active_t_idx = self.storage.active_tree_idx as usize;
        let cand_t_idx = 1 - active_t_idx;
        let active_c_idx = self.storage.active_cmd_idx as usize;
        let cand_c_idx = 1 - active_c_idx;

        let (saved_focus, saved_scroll_key, saved_scroll_y) = if page_switched {
            let old_tree = &self.storage.trees[active_t_idx];
            let f_key = self
                .storage
                .focus
                .focused_key()
                .filter(|&k| old_tree.modal.is_none() && !A::is_tab_key(k))
                .or(self.last_content_focus);
            let mut s_key = None;
            let mut s_y = 0i32;
            for n in &old_tree.nodes {
                if n.occupied && n.kind == NodeKind::ScrollY {
                    s_key = Some(n.key);
                    s_y = n.scroll_y;
                    break;
                }
            }
            (f_key, s_key, s_y)
        } else {
            (None, None, 0)
        };

        {
            let mut vb = ViewBuilder::new(&mut self.storage.draft);
            if let Err(err) = A::view(&self.model, &mut vb) {
                self.build_key = vb.operation_key();
                self.view_pending = true;
                self.last_build_error = Some(err);
                return Err(err);
            }
            if let Err(err) = vb.finish() {
                self.view_pending = true;
                self.last_build_error = Some(err);
                return Err(err);
            }
        }

        self.storage.cand_anims.copy_from(&self.storage.anims);
        let mut cand_focus = self.storage.focus;
        let mut cand_pointer = self.storage.pointer;
        let mut cand_repeat = self.storage.repeat;
        self.storage.cand_damage.clear();

        let focused_slot = cand_focus.focused_slot(&self.storage.trees[active_t_idx]);
        let captured_slot = cand_pointer
            .captured_id()
            .filter(|&id| self.storage.trees[active_t_idx].is_id_valid(id))
            .map(NodeId::slot);

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
            &AppMeasure::<A, F>(&self.fonts, core::marker::PhantomData),
            &mut self.storage.cand_anims,
            focused_slot,
            captured_slot,
            now,
            &mut self.storage.cand_damage,
        );

        let outcome = match res {
            Ok(v) => v,
            Err(err) => {
                self.view_pending = true;
                self.last_build_error = Some(err);
                return Err(err);
            }
        };

        if outcome.capture_target_invalidated {
            cand_pointer.set_active_id(None);
            cand_pointer.capture = CaptureKind::None;
        }

        let was_modal = active_tree.modal.is_some();
        let is_modal = cand_tree.modal.is_some();
        if page_switched {
            cand_pointer.clear(cand_tree, &mut self.storage.cand_damage, self.screen);
            cand_repeat.clear();
            cand_focus.editing = false;
            if !is_modal {
                cand_focus.clear_modal_trigger();
            }

            let restore = A::page_restore(&self.model, new_page);
            if let Some(sk) = restore.scroll_key
                && let Some(scroll_slot) = cand_tree.find_slot_by_key(sk)
                && cand_tree.nodes[scroll_slot as usize].kind == NodeKind::ScrollY
            {
                cand_tree.nodes[scroll_slot as usize].scroll_y = restore.scroll_y;
                relayout_scroll_subtree(cand_tree, scroll_slot, self.screen);
            }

            if is_modal {
                cand_focus.on_modal_opened(cand_tree);
            } else {
                let target_slot = restore
                    .focus_key
                    .filter(|&k| !A::is_tab_key(k))
                    .and_then(|k| cand_tree.find_slot_by_key(k))
                    .filter(|&s| cand_focus.is_slot_eligible_in_scope(cand_tree, s))
                    .or_else(|| cand_focus.first_content_focusable(cand_tree, A::is_tab_key));
                cand_focus.set_focused_key(target_slot.map(|s| cand_tree.nodes[s as usize].key));
                if let Some(slot) = target_slot
                    && let Some(scroll_slot) = ensure_visible(cand_tree, slot, self.screen)
                {
                    self.storage.cand_damage.add(
                        cand_tree.nodes[scroll_slot as usize].visible_rect,
                        self.screen,
                    );
                }
            }
            self.storage.cand_damage.mark_full_screen(self.screen);
        } else if !was_modal && is_modal {
            cand_pointer.clear(cand_tree, &mut self.storage.cand_damage, self.screen);
            cand_repeat.clear();
            cand_focus.on_modal_opened(cand_tree);
        } else if was_modal && !is_modal {
            cand_focus.on_modal_closed_with_filter(
                cand_tree,
                &mut self.storage.cand_damage,
                self.screen,
                A::is_tab_key,
            );
            cand_repeat.clear();
        } else {
            if !self.has_committed_once && !is_modal && cand_focus.focused_key().is_none() {
                let first = cand_focus
                    .first_content_focusable(cand_tree, A::is_tab_key)
                    .map(|s| cand_tree.nodes[s as usize].key);
                cand_focus.set_focused_key(first);
            } else {
                cand_focus.ensure_valid(cand_tree);
            }
            if cand_repeat.is_active()
                && !is_valid_repeat_target(cand_tree, &cand_focus, &cand_pointer, &cand_repeat)
            {
                cand_repeat.clear();
            }
        }

        let focus_info = PaintFocusInfo {
            focused_slot: cand_focus.focused_slot(cand_tree),
            ring_visible: cand_focus.ring_visible,
            editing: cand_focus.editing,
        };

        cand_tree.sync_press_depth(&mut self.storage.cand_anims, now);
        if let Err(err) = emit_commands_with(
            cand_tree,
            &self.storage.cand_anims,
            focus_info,
            self.screen,
            &mut self.storage.cmds[cand_c_idx],
            |key, props, canvas| A::paint_model(&self.model, key, props, canvas),
        ) {
            self.build_key = self.storage.cmds[cand_c_idx].effect_error_key;
            self.view_pending = true;
            self.last_build_error = Some(err);
            return Err(err);
        }

        #[cfg(feature = "inspector")]
        if let Some(i) = self.storage.draft.inspection.as_mut() {
            i.commit();
        }
        active_tree.expand_effect_damage(&mut self.storage.cand_damage, self.screen);
        cand_tree.expand_effect_damage(&mut self.storage.cand_damage, self.screen);
        #[cfg(feature = "enhanced")]
        liuli_core::layers::reuse_groups(
            &mut self.storage.cmds,
            active_c_idx,
            active_tree,
            cand_tree,
        );
        // Atomic commit: swap tree & command indices and apply state + damage
        self.storage.active_tree_idx = cand_t_idx as u8;
        self.storage.active_cmd_idx = cand_c_idx as u8;
        self.storage.anims.copy_from(&self.storage.cand_anims);
        self.storage.focus = cand_focus;
        self.storage.pointer = cand_pointer;
        self.storage.repeat = cand_repeat;
        for &r in self.storage.cand_damage.as_slice() {
            self.storage.damage.add(r, self.screen);
        }
        self.storage.cand_damage.clear();
        self.view_pending = false;
        self.last_build_error = None;
        self.pending_measured_nodes = self
            .pending_measured_nodes
            .saturating_add(outcome.measured_nodes);
        self.last_metrics.measured_nodes = self.pending_measured_nodes;
        self.scene_version = self.scene_version.wrapping_add(1);
        self.has_committed_once = true;
        self.active_page = new_page;

        if page_switched {
            self.last_content_focus = self
                .storage
                .focus
                .focused_key()
                .filter(|&k| !is_modal && !A::is_tab_key(k));
            A::on_page_committed(
                &mut self.model,
                old_page,
                saved_focus,
                saved_scroll_key,
                saved_scroll_y,
            );
        } else if !is_modal
            && let Some(k) = self.storage.focus.focused_key()
            && !A::is_tab_key(k)
        {
            self.last_content_focus = Some(k);
        }

        Ok(())
    }

    pub fn reemit_commands(&mut self) -> Result<(), TransactionError<()>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(()));
        }
        self.reemit_commands_inner()
            .map_err(TransactionError::Build)
    }

    fn reemit_commands_inner(&mut self) -> Result<(), BuildError> {
        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;
        let cand_c_idx = 1 - active_c_idx;
        let tree = &self.storage.trees[active_t_idx];
        let focus_info = PaintFocusInfo {
            focused_slot: self.storage.focus.focused_slot(tree),
            ring_visible: self.storage.focus.ring_visible,
            editing: self.storage.focus.editing,
        };
        self.storage.cand_anims.copy_from(&self.storage.anims);
        tree.sync_press_depth(
            &mut self.storage.cand_anims,
            self.storage.clock.last().unwrap_or_default(),
        );
        if let Err(err) = emit_commands_with(
            tree,
            &self.storage.cand_anims,
            focus_info,
            self.screen,
            &mut self.storage.cmds[cand_c_idx],
            |key, props, canvas| A::paint_model(&self.model, key, props, canvas),
        ) {
            self.build_key = self.storage.cmds[cand_c_idx].effect_error_key;
            self.last_build_error = Some(err);
            return Err(err);
        }
        #[cfg(feature = "enhanced")]
        liuli_core::layers::reuse_groups(&mut self.storage.cmds, active_c_idx, tree, tree);
        self.storage.anims.copy_from(&self.storage.cand_anims);
        self.storage.active_cmd_idx = cand_c_idx as u8;
        self.scene_version = self.scene_version.wrapping_add(1);
        if !self.view_pending {
            self.last_build_error = None;
        }
        Ok(())
    }

    fn sync_toggle_anim_rects(&mut self, active_t_idx: usize) {
        for n in &self.storage.trees[active_t_idx].nodes {
            if n.occupied && n.kind == NodeKind::Toggle {
                self.storage
                    .anims
                    .update_rect(n.key, AnimProp::ToggleKnob, n.visible_rect);
            }
        }
    }

    fn flush_cand_damage(&mut self) {
        self.storage.trees[self.storage.active_tree_idx as usize]
            .expand_effect_damage(&mut self.storage.cand_damage, self.screen);
        for &r in self.storage.cand_damage.as_slice() {
            self.storage.damage.add(r, self.screen);
        }
        self.storage.cand_damage.clear();
    }

    #[allow(clippy::too_many_arguments)]
    fn rollback_fast_path(
        &mut self,
        active_t_idx: usize,
        saved_focus: FocusState,
        saved_pointer: PointerState,
        saved_repeat: RepeatState,
        saved_last_content_focus: Option<Key>,
        saved_pressed_mask: u64,
        saved_scroll_y: &[i32; MAX_NODES],
    ) {
        self.storage.focus = saved_focus;
        self.storage.pointer = saved_pointer;
        self.storage.repeat = saved_repeat;
        self.last_content_focus = saved_last_content_focus;
        let tree = &mut self.storage.trees[active_t_idx];
        let mut scroll_changed_mask = 0u64;
        for (idx, &prev_scroll) in saved_scroll_y.iter().enumerate() {
            if tree.nodes[idx].occupied {
                tree.nodes[idx].pressed = (saved_pressed_mask & (1u64 << idx)) != 0;
                if tree.nodes[idx].scroll_y != prev_scroll {
                    tree.nodes[idx].scroll_y = prev_scroll;
                    if tree.nodes[idx].kind == NodeKind::ScrollY {
                        scroll_changed_mask |= 1u64 << idx;
                    }
                }
            }
        }
        if scroll_changed_mask != 0 {
            for idx in 0..MAX_NODES {
                if (scroll_changed_mask & (1u64 << idx)) != 0 {
                    relayout_scroll_subtree(tree, idx as u16, self.screen);
                }
            }
        }
        self.storage.cand_damage.clear();
    }

    pub fn text_target(&self) -> Option<Key> {
        let tree = self.active_tree();
        let slot = self.storage.focus.focused_slot(tree)?;
        let key = tree.nodes[slot as usize].key;
        (self.storage.focus.is_slot_eligible_in_scope(tree, slot)
            && A::accepts_text(&self.model, key))
        .then_some(key)
    }
    pub fn try_text<'i>(
        &mut self,
        input: liuli_core::input::TextInput<'i>,
        now: InstantMs,
    ) -> Result<bool, TransactionError<liuli_core::input::TextInput<'i>>> {
        if self.is_dma_busy() {
            return Err(TransactionError::Busy(input));
        }
        self.storage.clock.advance_to(now)?;
        let Some(key) = self.text_target() else {
            return Ok(false);
        };
        if let Some(msg) = A::on_text(&self.model, key, input)
            && (A::update(&mut self.model, msg, now) == Change::View || self.view_pending)
        {
            self.commit_view_inner(now)?;
        }
        Ok(true)
    }
    pub fn try_dispatch(
        &mut self,
        input: RawInput,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<bool, TransactionError<RawInput>> {
        if dma_busy || self.is_dma_busy() {
            return Err(TransactionError::Busy(input));
        }
        if self.storage.clock.advance_to(now).is_err() {
            return Err(TransactionError::WentBackwards);
        }

        let active_t_idx = self.storage.active_tree_idx as usize;
        #[cfg(feature = "enhanced")]
        {
            use liuli_enhanced::gestures::Delivery;
            let tree = self.storage.active_tree();
            let owner = match input {
                RawInput::PointerDown { pos, .. } => liuli_core::focus::active_scope_root(tree)
                    .and_then(|scope| {
                        let mut slot = liuli_core::focus::hit_test_in_scope(tree, scope, pos)?;
                        loop {
                            let n = &tree.nodes[slot as usize];
                            if A::accepts_transform(&self.model, n.key) {
                                return Some(n.id(slot));
                            }
                            if slot == scope {
                                return None;
                            }
                            slot = n.parent.get_u16()?;
                        }
                    }),
                _ => None,
            };
            match self.pair.feed(input, owner) {
                Delivery::Pass => {}
                Delivery::Consume => return Ok(true),
                Delivery::Gesture(id, gesture) => {
                    let key = tree.nodes[id.slot() as usize].key;
                    if gesture.phase == liuli_core::input::GesturePhase::Begin {
                        self.storage.pointer.clear(
                            &mut self.storage.trees[active_t_idx],
                            &mut self.storage.damage,
                            self.screen,
                        );
                        self.storage.repeat.clear();
                        self.reemit_commands_inner()?;
                    }
                    if let Some(msg) = A::on_transform(&self.model, key, gesture)
                        && A::update(&mut self.model, msg, now) == Change::View
                    {
                        self.commit_view_inner(now)?;
                    }
                    return Ok(true);
                }
            }
        }
        let saved_focus = self.storage.focus;
        let saved_pointer = self.storage.pointer;
        let saved_repeat = self.storage.repeat;
        let saved_last_content_focus = self.last_content_focus;
        let suppress_pointer_up_action = matches!(
            (input, saved_repeat.source),
            (RawInput::PointerUp { id, .. }, RepeatSource::Pointer)
                if id == saved_repeat.pointer_id && saved_repeat.fired_any
        );

        let mut saved_pressed_mask = 0u64;
        let mut saved_scroll_y = [0i32; MAX_NODES];
        for (idx, n) in self.storage.trees[active_t_idx].nodes.iter().enumerate() {
            if n.occupied {
                if n.pressed {
                    saved_pressed_mask |= 1u64 << idx;
                }
                saved_scroll_y[idx] = n.scroll_y;
            }
        }
        self.storage.cand_damage.clear();

        let (consumed, action) = dispatch_input(
            &mut self.storage.trees[active_t_idx],
            &mut self.storage.focus,
            &mut self.storage.pointer,
            input,
            self.screen,
            &mut self.storage.cand_damage,
        );

        let cur_tree = &self.storage.trees[active_t_idx];
        match input {
            RawInput::PointerDown { id, .. } => {
                if self.storage.pointer.active_id() == Some(id) {
                    let target_id = match self.storage.pointer.capture {
                        CaptureKind::Button(id)
                        | CaptureKind::ScrollCandidate { candidate: id, .. } => Some(id),
                        _ => None,
                    };
                    if let Some(tid) = target_id
                        && cur_tree.is_id_valid(tid)
                        && cur_tree.nodes[tid.slot() as usize].kind == NodeKind::Button
                        && (cur_tree.nodes[tid.slot() as usize].props.repeatable()
                            || cur_tree.nodes[tid.slot() as usize].props.long_press())
                    {
                        self.storage.repeat = RepeatState {
                            source: RepeatSource::Pointer,
                            pointer_id: id,
                            hold_key: HoldKey::Activate,
                            fired_any: false,
                            target_id: tid,
                            key: cur_tree.nodes[tid.slot() as usize].key,
                            adjust_delta: 0,
                            next_fire_ms: now.saturating_add(REPEAT_DELAY_MS),
                        };
                    } else {
                        self.storage.repeat.clear();
                    }
                }
            }
            RawInput::PointerMove { id, pos } => {
                if self.storage.repeat.source == RepeatSource::Pointer
                    && self.storage.repeat.pointer_id == id
                {
                    let valid =
                        is_valid_repeat_target(
                            cur_tree,
                            &self.storage.focus,
                            &self.storage.pointer,
                            &self.storage.repeat,
                        ) && pointer_hits_id_in_scope(cur_tree, self.storage.repeat.target_id, pos);
                    if !valid {
                        self.storage.repeat.clear();
                    }
                }
            }
            RawInput::PointerUp { id, .. } | RawInput::PointerCancel { id } => {
                if self.storage.repeat.source == RepeatSource::Pointer
                    && self.storage.repeat.pointer_id == id
                {
                    self.storage.repeat.clear();
                }
            }
            RawInput::KeyDown(hold_key) => {
                self.storage.repeat.clear();
                if let Some(slot) = self.storage.focus.focused_slot(cur_tree) {
                    let n = &cur_tree.nodes[slot as usize];
                    match hold_key {
                        HoldKey::AdjustPrev | HoldKey::AdjustNext
                            if self.storage.focus.editing && n.kind == NodeKind::Slider =>
                        {
                            self.storage.repeat = RepeatState {
                                source: RepeatSource::Key,
                                pointer_id: liuli_core::input::PointerId(0),
                                hold_key,
                                fired_any: false,
                                target_id: n.id(slot),
                                key: n.key,
                                adjust_delta: if hold_key == HoldKey::AdjustNext {
                                    1
                                } else {
                                    -1
                                },
                                next_fire_ms: now.saturating_add(REPEAT_DELAY_MS),
                            };
                        }
                        HoldKey::Activate if n.kind == NodeKind::Button && n.props.repeatable() => {
                            self.storage.repeat = RepeatState {
                                source: RepeatSource::Key,
                                pointer_id: liuli_core::input::PointerId(0),
                                hold_key,
                                fired_any: false,
                                target_id: n.id(slot),
                                key: n.key,
                                adjust_delta: 0,
                                next_fire_ms: now.saturating_add(REPEAT_DELAY_MS),
                            };
                        }
                        _ => {}
                    }
                }
            }
            RawInput::KeyUp(hold_key) => {
                if self.storage.repeat.source == RepeatSource::Key
                    && self.storage.repeat.hold_key == hold_key
                {
                    self.storage.repeat.clear();
                }
            }
            RawInput::Nav(_) | RawInput::Back => {
                self.storage.repeat.clear();
            }
            RawInput::Activate | RawInput::Adjust(_) => {}
        }

        if cur_tree.modal.is_none()
            && let Some(k) = self.storage.focus.focused_key()
            && !A::is_tab_key(k)
        {
            self.last_content_focus = Some(k);
        }

        let effective_action = if suppress_pointer_up_action {
            None
        } else {
            action
        };
        let had_local_damage = !self.storage.cand_damage.is_empty();

        let message = effective_action
            .and_then(|act| A::on_action(&self.model, act))
            .or_else(|| {
                cur_tree.nodes.iter().enumerate().find_map(|(index, node)| {
                    if node.occupied
                        && node.kind == NodeKind::ScrollY
                        && saved_scroll_y[index] != node.scroll_y
                    {
                        A::on_scroll(&self.model, node.key, node.scroll_y)
                    } else {
                        None
                    }
                })
            });
        if let Some(msg) = message
            && A::update(&mut self.model, msg, now) == Change::View
        {
            self.sync_toggle_anim_rects(active_t_idx);
            self.flush_cand_damage();
            if let Err(err) = self.commit_view_inner(now) {
                if had_local_damage {
                    let _ = self.reemit_commands_inner();
                    self.last_build_error = Some(err);
                }
                return Err(TransactionError::Build(err));
            }
            return Ok(consumed);
        }

        if self.view_pending {
            match self.commit_view_inner(now) {
                Ok(()) => {
                    if had_local_damage {
                        self.storage.damage.mark_full_screen(self.screen);
                    }
                    return Ok(consumed);
                }
                Err(commit_err) => {
                    if had_local_damage {
                        if self.reemit_commands_inner().is_ok() {
                            self.sync_toggle_anim_rects(active_t_idx);
                            self.flush_cand_damage();
                        } else {
                            self.rollback_fast_path(
                                active_t_idx,
                                saved_focus,
                                saved_pointer,
                                saved_repeat,
                                saved_last_content_focus,
                                saved_pressed_mask,
                                &saved_scroll_y,
                            );
                        }
                        self.last_build_error = Some(commit_err);
                    }
                    return Err(TransactionError::Build(commit_err));
                }
            }
        }

        if had_local_damage {
            if let Err(err) = self.reemit_commands_inner() {
                self.rollback_fast_path(
                    active_t_idx,
                    saved_focus,
                    saved_pointer,
                    saved_repeat,
                    saved_last_content_focus,
                    saved_pressed_mask,
                    &saved_scroll_y,
                );
                return Err(TransactionError::Build(err));
            }
            self.sync_toggle_anim_rects(active_t_idx);
            self.flush_cand_damage();
        }

        Ok(consumed)
    }

    pub fn dispatch(
        &mut self,
        input: RawInput,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<bool, DispatchError<RawInput>> {
        match self.try_dispatch(input, now, dma_busy) {
            Ok(consumed) => Ok(consumed),
            Err(TransactionError::Busy(input)) => Err(DispatchError::Busy(input)),
            Err(TransactionError::WentBackwards) => Err(DispatchError::WentBackwards),
            Err(TransactionError::Build(_)) => Ok(false),
        }
    }

    pub fn try_message(
        &mut self,
        msg: A::Msg,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<(), TransactionError<A::Msg>> {
        if dma_busy || self.is_dma_busy() {
            return Err(TransactionError::Busy(msg));
        }
        if self.storage.clock.advance_to(now).is_err() {
            return Err(TransactionError::WentBackwards);
        }
        if A::update(&mut self.model, msg, now) == Change::View || self.view_pending {
            self.commit_view_inner(now)?;
        }
        Ok(())
    }

    pub fn message(
        &mut self,
        msg: A::Msg,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<(), DispatchError<A::Msg>> {
        match self.try_message(msg, now, dma_busy) {
            Ok(()) => Ok(()),
            Err(TransactionError::Busy(msg)) => Err(DispatchError::Busy(msg)),
            Err(TransactionError::WentBackwards) => Err(DispatchError::WentBackwards),
            Err(TransactionError::Build(_)) => Ok(()),
        }
    }

    pub fn try_advance(
        &mut self,
        now: InstantMs,
        dma_busy: bool,
    ) -> Result<bool, TransactionError<()>> {
        self.storage.clock.advance_to(now)?;
        if dma_busy || self.is_dma_busy() {
            self.timer_pending_after_dma = true;
            return Ok(false);
        }
        self.timer_pending_after_dma = false;

        let mut any_change = false;
        let mut build_err: Option<BuildError> = None;
        if A::on_timer(&mut self.model, now) == Change::View {
            match self.commit_view_inner(now) {
                Ok(()) => any_change = true,
                Err(err) => build_err = Some(err),
            }
        }

        if self.storage.repeat.is_active() && now >= self.storage.repeat.next_fire_ms {
            let active_t_idx = self.storage.active_tree_idx as usize;
            if !is_valid_repeat_target(
                &self.storage.trees[active_t_idx],
                &self.storage.focus,
                &self.storage.pointer,
                &self.storage.repeat,
            ) {
                self.storage.repeat.clear();
            } else {
                let overdue =
                    1 + now.saturating_sub(self.storage.repeat.next_fire_ms) / REPEAT_PERIOD_MS;
                let steps = overdue.min(MAX_REPEAT_STEPS_PER_ADVANCE as u64) as u16;
                self.storage.repeat.next_fire_ms = self
                    .storage
                    .repeat
                    .next_fire_ms
                    .saturating_add(overdue.saturating_mul(REPEAT_PERIOD_MS));
                self.storage.repeat.fired_any = true;

                if self.storage.repeat.adjust_delta != 0 {
                    if let Some(slot) = self
                        .storage
                        .focus
                        .focused_slot(&self.storage.trees[active_t_idx])
                    {
                        let delta = self
                            .storage
                            .repeat
                            .adjust_delta
                            .saturating_mul(steps as i16);
                        self.storage.cand_damage.clear();
                        let (_, act_opt) = adjust_slider_by_steps_pub(
                            &mut self.storage.trees[active_t_idx],
                            slot,
                            delta,
                            &mut self.storage.cand_damage,
                            self.screen,
                        );
                        if let Some(act) = act_opt
                            && let Some(msg) = A::on_action(&self.model, act)
                            && A::update(&mut self.model, msg, now) == Change::View
                        {
                            self.flush_cand_damage();
                            match self.commit_view_inner(now) {
                                Ok(()) => any_change = true,
                                Err(err) => {
                                    if build_err.is_none() {
                                        build_err = Some(err);
                                    }
                                }
                            }
                        } else if !self.storage.cand_damage.is_empty() {
                            self.flush_cand_damage();
                            let _ = self.reemit_commands_inner();
                            any_change = true;
                        }
                    }
                } else {
                    let is_hold = self.storage.repeat.source == RepeatSource::Pointer
                        && self.storage.trees[active_t_idx].nodes
                            [self.storage.repeat.target_id.slot() as usize]
                            .props
                            .long_press();
                    let act = UiAction {
                        key: self.storage.repeat.key,
                        kind: if is_hold {
                            ActionKind::LongPress
                        } else {
                            ActionKind::Activate
                        },
                    };
                    let mut view_dirty = false;
                    if is_hold {
                        let node = &mut self.storage.trees[active_t_idx].nodes
                            [self.storage.repeat.target_id.slot() as usize];
                        node.pressed = false;
                        self.storage.damage.add(node.visible_rect, self.screen);
                        self.storage.trees[active_t_idx]
                            .expand_effect_damage(&mut self.storage.damage, self.screen);
                        self.storage.pointer.capture = CaptureKind::None;
                        self.storage.repeat.clear();
                        self.reemit_commands_inner()?;
                        any_change = true;
                    }
                    for _ in 0..if is_hold { 1 } else { steps } {
                        if let Some(msg) = A::on_action(&self.model, act)
                            && A::update(&mut self.model, msg, now) == Change::View
                        {
                            view_dirty = true;
                        }
                    }
                    if view_dirty {
                        match self.commit_view_inner(now) {
                            Ok(()) => any_change = true,
                            Err(err) => {
                                if build_err.is_none() {
                                    build_err = Some(err);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.storage.cand_anims.copy_from(&self.storage.anims);
        self.storage.cand_damage.clear();
        let anim_changed = self.storage.cand_anims.advance(now, |dirty| {
            self.storage.cand_damage.add(dirty, self.screen);
        });
        if anim_changed {
            let active_t_idx = self.storage.active_tree_idx as usize;
            let active_c_idx = self.storage.active_cmd_idx as usize;
            let cand_c_idx = 1 - active_c_idx;
            let tree = &self.storage.trees[active_t_idx];
            let focus_info = PaintFocusInfo {
                focused_slot: self.storage.focus.focused_slot(tree),
                ring_visible: self.storage.focus.ring_visible,
                editing: self.storage.focus.editing,
            };
            if let Err(err) = emit_commands_with(
                tree,
                &self.storage.cand_anims,
                focus_info,
                self.screen,
                &mut self.storage.cmds[cand_c_idx],
                |key, props, canvas| A::paint_model(&self.model, key, props, canvas),
            ) {
                self.build_key = self.storage.cmds[cand_c_idx].effect_error_key;
                self.storage.cand_damage.clear();
                self.last_build_error = Some(err);
                return Err(TransactionError::Build(err));
            }
            #[cfg(feature = "enhanced")]
            liuli_core::layers::reuse_groups(&mut self.storage.cmds, active_c_idx, tree, tree);
            self.storage.active_cmd_idx = cand_c_idx as u8;
            self.scene_version = self.scene_version.wrapping_add(1);
            self.storage.anims.copy_from(&self.storage.cand_anims);
            self.flush_cand_damage();
            any_change = true;
        }

        if let Some(err) = build_err {
            return Err(TransactionError::Build(err));
        }

        Ok(any_change)
    }

    pub fn advance(&mut self, now: InstantMs, dma_busy: bool) -> Result<bool, TimeError> {
        match self.try_advance(now, dma_busy) {
            Ok(changed) => Ok(changed),
            Err(TransactionError::WentBackwards) => Err(TimeError::WentBackwards),
            Err(TransactionError::Build(_)) => Ok(!self.storage.damage.is_empty()),
            Err(TransactionError::Busy(())) => Ok(false),
        }
    }

    pub fn next_wakeup(&self, now: InstantMs, dma_busy: bool) -> NextWakeup {
        if dma_busy || self.is_dma_busy() {
            return NextWakeup::OnEvent;
        }
        if self.timer_pending_after_dma || !self.storage.damage.is_empty() {
            return NextWakeup::Now;
        }
        let anim_wake = self.storage.anims.next_wakeup(now);
        let repeat_wake = self.storage.repeat.next_wakeup(now);
        let app_wake = match A::next_deadline(&self.model, now) {
            Some(t) if t <= now => NextWakeup::Now,
            Some(t) => NextWakeup::At(t),
            None => NextWakeup::OnEvent,
        };
        anim_wake.earlier(repeat_wake).earlier(app_wake)
    }

    pub fn present_sync_streaming<T: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut T,
        scratch: &mut [u16],
    ) -> Result<PresentStatus, liuli_render::sync::FrameError<T::Error, F::Error>> {
        self.present_sync_with_provider(target, scratch, |_| Ok(()))
    }

    pub fn present_sync_with_provider<T, Fin>(
        &mut self,
        target: &mut T,
        scratch: &mut [u16],
        finish: Fin,
    ) -> Result<PresentStatus, liuli_render::sync::FrameError<T::Error, F::Error>>
    where
        T: DrawTarget<Color = Rgb565>,
        Fin: FnMut(&mut T) -> Result<(), T::Error>,
    {
        if self.is_dma_busy() {
            return Err(PresentError::DmaBusy.into());
        }
        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;

        let (status, mut m) = liuli_render::sync::present_sync_with_provider(
            target,
            scratch,
            self.screen,
            self.band_height,
            &self.storage.cmds[active_c_idx],
            &mut self.storage.damage,
            &self.storage.trees[active_t_idx],
            &self.fonts,
            finish,
            #[cfg(feature = "platform-blend")]
            self.blend,
            #[cfg(not(feature = "platform-blend"))]
            liuli_render::raster::blend_solid,
        )?;
        self.record_present_metrics(status, &mut m);
        Ok(status)
    }

    pub fn poll_dma_with_provider<'a, S>(
        &mut self,
        dma: &mut DmaCoordinator<'a>,
        sink: &mut S,
    ) -> Result<PresentStatus, liuli_render::sync::FrameError<S::Error, F::Error>>
    where
        S: DmaSink<'a>,
    {
        if let Some(lock) = self.active_dma {
            if !dma.is_active()
                || dma.session_token() != lock.session_token
                || dma.frame_id() != lock.frame_id
                || self.scene_version != lock.scene_version
            {
                return Err(PresentError::DmaBusy.into());
            }
        } else {
            if dma.is_active() {
                return Err(PresentError::DmaBusy.into());
            }
            if dma.is_faulted() {
                return Err(PresentError::DmaFaulted.into());
            }
            if !self.storage.damage.is_empty() {
                let token = self.next_dma_session;
                self.next_dma_session = self.next_dma_session.wrapping_add(1).max(1);
                dma.begin_frame_with_session(
                    &self.storage.damage,
                    self.screen,
                    self.band_height,
                    token,
                )?;
                self.active_dma = Some(ActiveDma {
                    session_token: token,
                    frame_id: dma.frame_id(),
                    scene_version: self.scene_version,
                });
            }
        }

        let active_t_idx = self.storage.active_tree_idx as usize;
        let active_c_idx = self.storage.active_cmd_idx as usize;

        let step_res = dma.step_with_provider(
            sink,
            &self.storage.cmds[active_c_idx],
            &mut self.storage.damage,
            &self.storage.trees[active_t_idx],
            &self.fonts,
        );
        if !dma.is_active() {
            self.active_dma = None;
        }
        let (status, mut m) = step_res?;
        self.record_present_metrics(status, &mut m);
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use liuli_core::capacity::{BuildError, MAX_DEPTH, MAX_NODES, TransactionError};
    use liuli_core::geometry::{Point, Rect};
    use liuli_core::input::{PointerId, RawInput, UiAction};
    use liuli_core::node::RetainedTree;
    use liuli_core::time::{AnimPool, AnimProp, InstantMs, NextWakeup};
    use liuli_core::view::{
        App, Change, Key, Length, NodeKind, NodeProps, SlotLink, StoredProps, ViewBuilder,
    };
    use liuli_text::Lf4Font;

    use super::Engine;
    use crate::storage::Storage;

    const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

    #[derive(Clone, Copy)]
    struct RollModel {
        checked: bool,
    }

    struct RollApp;

    impl App for RollApp {
        type Model = RollModel;
        type Msg = bool;

        fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
            model.checked = msg;
            Change::View
        }

        fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
            let mut root_p = NodeProps::new();
            root_p.width = Length::Fill(1);
            root_p.height = Length::Fill(1);

            out.root(Key(1), root_p, |ui| {
                let mut scroll_p = NodeProps::new();
                scroll_p.width = Length::Fill(1);
                scroll_p.height = Length::Px(60);
                ui.scroll_y(Key(2), scroll_p, |ui| {
                    let mut col_p = NodeProps::new();
                    col_p.width = Length::Fill(1);
                    col_p.height = Length::Content;
                    ui.column(Key(3), col_p, |ui| {
                        let mut b1 = NodeProps::new();
                        b1.width = Length::Px(100);
                        b1.height = Length::Px(40);
                        b1.focusable = true;
                        b1.clickable = true;
                        ui.button(Key(10), b1)?;

                        let mut t = NodeProps::new();
                        t.width = Length::Px(48);
                        t.height = Length::Px(26);
                        t.checked = model.checked;
                        t.focusable = true;
                        ui.toggle(Key(11), t)?;

                        let mut b2 = NodeProps::new();
                        b2.width = Length::Px(100);
                        b2.height = Length::Px(40);
                        b2.focusable = true;
                        b2.clickable = true;
                        ui.button(Key(12), b2)?;
                        Ok(())
                    })
                })
            })
        }

        fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
            None
        }
    }

    fn inject_deep_dummy_chain(tree: &mut RetainedTree) {
        let root_idx = tree.root.get().unwrap();
        let chain_len = MAX_DEPTH + 2;
        let mut chain = [0usize; MAX_DEPTH + 2];
        let mut found = 0usize;
        for (idx, n) in tree.nodes.iter().enumerate() {
            if !n.occupied {
                chain[found] = idx;
                found += 1;
                if found == chain_len {
                    break;
                }
            }
        }
        assert_eq!(found, chain_len);
        assert!(MAX_NODES >= chain_len);

        let visible_props = StoredProps::from_props(NodeProps::new());
        let head = chain[0];
        let prev_last = tree.nodes[root_idx].last_child;
        if let Some(pl) = prev_last.get() {
            tree.nodes[pl].next_sibling = SlotLink::from_slot(head).unwrap();
        } else {
            tree.nodes[root_idx].first_child = SlotLink::from_slot(head).unwrap();
        }
        tree.nodes[root_idx].last_child = SlotLink::from_slot(head).unwrap();

        for i in 0..chain_len {
            let slot = chain[i];
            tree.nodes[slot].clear_fresh(1);
            tree.nodes[slot].key = Key(9000 + i as u32);
            tree.nodes[slot].kind = NodeKind::Label;
            tree.nodes[slot].props = visible_props;
            tree.nodes[slot].parent =
                SlotLink::from_slot(if i == 0 { root_idx } else { chain[i - 1] }).unwrap();
            tree.nodes[slot].first_child = if i + 1 < chain_len {
                SlotLink::from_slot(chain[i + 1]).unwrap()
            } else {
                SlotLink::NONE
            };
            tree.nodes[slot].last_child = tree.nodes[slot].first_child;
        }
    }

    #[test]
    fn try_dispatch_fast_path_rolls_back_state_on_reemit_failure() {
        let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
        let screen = Rect::new(0, 0, 120, 60);
        let mut storage = Storage::new();
        let mut engine = Engine::<RollApp>::new_in_place(
            &mut storage,
            RollModel { checked: false },
            font,
            screen,
            InstantMs(0),
        )
        .unwrap();

        engine
            .try_dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(20, 15),
                },
                InstantMs(10),
                false,
            )
            .unwrap();
        engine.storage.damage.clear();

        let active_t_idx = engine.storage.active_tree_idx as usize;
        let btn_slot = engine.storage.trees[active_t_idx]
            .find_slot_by_key(Key(10))
            .unwrap() as usize;
        let scroll_slot = engine.storage.trees[active_t_idx]
            .find_slot_by_key(Key(2))
            .unwrap() as usize;

        assert!(engine.storage.trees[active_t_idx].nodes[btn_slot].pressed);
        let saved_focus = *engine.focus();
        let saved_pointer = *engine.pointer();
        let saved_btn_rect = engine.storage.trees[active_t_idx].nodes[btn_slot].layout_rect;
        let saved_scroll_y = engine.storage.trees[active_t_idx].nodes[scroll_slot].scroll_y;

        inject_deep_dummy_chain(&mut engine.storage.trees[active_t_idx]);

        // Vertical drag past DRAG_THRESHOLD_PX mutates scroll_y, child layout_rect, pressed, and capture
        // before reemit_commands fails with BuildError::Depth.
        let err = engine
            .try_dispatch(
                RawInput::PointerMove {
                    id: PointerId(1),
                    pos: Point::new(20, -10),
                },
                InstantMs(20),
                false,
            )
            .unwrap_err();
        assert_eq!(err, TransactionError::Build(BuildError::Depth));
        assert_eq!(engine.last_build_error(), Some(BuildError::Depth));
        assert_eq!(*engine.focus(), saved_focus);
        assert_eq!(*engine.pointer(), saved_pointer);
        assert!(engine.damage().is_empty());
        assert_eq!(
            engine.next_wakeup(InstantMs(20), false),
            NextWakeup::OnEvent
        );
        let tree_after = engine.active_tree();
        assert!(tree_after.nodes[btn_slot].pressed);
        assert_eq!(tree_after.nodes[scroll_slot].scroll_y, saved_scroll_y);
        assert_eq!(tree_after.nodes[btn_slot].layout_rect, saved_btn_rect);
    }

    #[test]
    fn try_advance_rolls_back_anim_and_damage_on_reemit_failure() {
        let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
        let screen = Rect::new(0, 0, 120, 60);
        let mut storage = Storage::new();
        let mut engine = Engine::<RollApp>::new_in_place(
            &mut storage,
            RollModel { checked: false },
            font,
            screen,
            InstantMs(0),
        )
        .unwrap();

        engine.try_message(true, InstantMs(10), false).unwrap();
        engine.storage.damage.clear();
        let mut saved_anims = AnimPool::new();
        saved_anims.copy_from(&engine.storage.anims);

        let active_t_idx = engine.storage.active_tree_idx as usize;
        inject_deep_dummy_chain(&mut engine.storage.trees[active_t_idx]);

        let err = engine.try_advance(InstantMs(60), false).unwrap_err();
        assert_eq!(err, TransactionError::Build(BuildError::Depth));
        assert_eq!(engine.last_build_error(), Some(BuildError::Depth));
        assert!(engine.damage().is_empty());
        assert_eq!(
            engine
                .storage
                .anims
                .cached_value(Key(11), AnimProp::ToggleKnob),
            saved_anims.cached_value(Key(11), AnimProp::ToggleKnob)
        );
        assert_eq!(
            engine.storage.anims.next_wakeup(InstantMs(10)),
            saved_anims.next_wakeup(InstantMs(10))
        );
    }
}
