use crate::screen::{MemoryScreen, render_reference_fullscreen};
use liuli::prelude::*;
use liuli::render::BandIter;
use std::collections::VecDeque;

#[derive(Default)]
struct FontTraffic {
    requests: usize,
    bytes: usize,
    cross_band: usize,
    band: usize,
    seen: std::collections::BTreeMap<(u8, u32), usize>,
}
impl liuli::text::storage::GlyphPixels for FontTraffic {
    type Error = std::convert::Infallible;
    fn begin_band(&mut self, _: Rect) {
        self.band += 1;
    }
    fn pixels<'a>(
        &'a mut self,
        slot: FontId,
        g: liuli::text::GlyphBitmap<'a>,
    ) -> Result<&'a [u8], Self::Error> {
        self.requests += 1;
        self.bytes += g.data.len();
        if self
            .seen
            .insert((slot as u8, g.metrics.unicode), self.band)
            .is_some_and(|last| last != self.band)
        {
            self.cross_band += 1;
        }
        Ok(g.data)
    }
}
pub const MAX_PNG_HISTORY: usize = 16;
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FrameLookupError {
    Expired,
    NotFound,
}

#[derive(Clone, Debug)]
pub struct FrameSnapshot {
    pub frame_id: u64,
    pub now_ms: u64,
    pub status: PresentStatus,
    pub damage: Vec<Rect>,
    pub band_rects: Vec<Rect>,
    pub bands: u32,
    pub pixels_sent: u32,
    pub measured_nodes: u32,
    pub commands_replayed: u32,
    pub wakeup: NextWakeup,
    pub last_completed_frame_id: u64,
    pub last_completed_metrics: FrameMetrics,
}

/// Headless or interactive host for an application, using its own resources and display size.
pub struct AppSession<'s, 'f, A: App> {
    engine: Engine<'s, 'f, A>,
    screen: MemoryScreen,
    scratch: Vec<u16>,
    completed_frame_id: u64,
    oldest_retained_frame: u64,
    last_completed_metrics: FrameMetrics,
    png_history: VecDeque<(u64, Vec<u8>, String)>,
    trigger: String,
    peaks: [usize; 3],
    snapshot: FrameSnapshot,
    on_frame: Option<fn(&mut A::Model, FrameMetrics, InstantMs)>,
}
impl<'s, 'f, A: App> AppSession<'s, 'f, A> {
    pub fn new(
        storage: &'s mut Storage,
        model: A::Model,
        fonts: FontSet<'f>,
        size: Size,
        start_now: InstantMs,
        verify_ref: bool,
    ) -> Result<Self, String> {
        Self::with_frame_callback(storage, model, fonts, size, start_now, verify_ref, None)
    }
    pub fn with_frame_callback(
        storage: &'s mut Storage,
        model: A::Model,
        fonts: FontSet<'f>,
        size: Size,
        start_now: InstantMs,
        verify_ref: bool,
        on_frame: Option<fn(&mut A::Model, FrameMetrics, InstantMs)>,
    ) -> Result<Self, String> {
        if size.width == 0 || size.height == 0 {
            return Err("display dimensions must be positive".into());
        }
        storage.draft.inspection = Some(Box::default());
        let mut engine = Engine::new_in_place_with_fonts(
            storage,
            model,
            fonts,
            Rect::new(0, 0, size.width, size.height),
            start_now,
        )
        .map_err(|e| format!("initial view: {e:?}"))?;
        engine
            .try_set_focus_ring_visible(true)
            .map_err(|e| format!("focus: {e:?}"))?;
        let placeholder_snap = FrameSnapshot {
            frame_id: 0,
            now_ms: start_now.0,
            status: PresentStatus::Idle,
            damage: Vec::new(),
            band_rects: Vec::new(),
            bands: 0,
            pixels_sent: 0,
            measured_nodes: 0,
            commands_replayed: 0,
            wakeup: NextWakeup::OnEvent,
            last_completed_frame_id: 0,
            last_completed_metrics: FrameMetrics::default(),
        };

        let mut session = Self {
            engine,
            screen: MemoryScreen::new(size.width, size.height),
            scratch: vec![0; size.width as usize * DEFAULT_BAND_HEIGHT],
            completed_frame_id: 0,
            oldest_retained_frame: 1,
            last_completed_metrics: FrameMetrics::default(),
            png_history: VecDeque::with_capacity(MAX_PNG_HISTORY),
            trigger: "initial".into(),
            peaks: [0; 3],
            snapshot: placeholder_snap,
            on_frame,
        };
        session.present_and_record(start_now, verify_ref)?;
        Ok(session)
    }
    pub fn engine(&self) -> &Engine<'s, 'f, A> {
        &self.engine
    }
    pub fn screen(&self) -> &MemoryScreen {
        &self.screen
    }
    pub fn snapshot(&self) -> &FrameSnapshot {
        &self.snapshot
    }
    pub fn step(
        &mut self,
        now: InstantMs,
        input: Option<RawInput>,
        verify_ref: bool,
    ) -> Result<&FrameSnapshot, String> {
        self.trigger = match input {
            None => "timer".into(),
            Some(event) => {
                let tree = self.engine.active_tree();
                let slot = match event {
                    RawInput::PointerDown { pos, .. }
                    | RawInput::PointerMove { pos, .. }
                    | RawInput::PointerUp { pos, .. } => {
                        liuli::core::focus::active_scope_root(tree)
                            .and_then(|root| liuli::core::focus::hit_test_in_scope(tree, root, pos))
                    }
                    _ => self.engine.focus().focused_slot(tree),
                };
                let mut keys = Vec::new();
                let mut next = slot;
                while let Some(slot) = next {
                    let n = &tree.nodes[slot as usize];
                    keys.push(n.key.0);
                    next = n.parent.get_u16();
                }
                format!(
                    "{event:?} hit_path={keys:?} capture_before={:?}",
                    self.engine.pointer().captured_id()
                )
            }
        };
        self.engine
            .try_advance(now, false)
            .map_err(|e| format!("advance: {e:?}, view={:?}", self.engine.build_failure()))?;
        if let Some(input) = input {
            self.engine
                .try_dispatch(input, now, false)
                .map_err(|e| format!("dispatch: {e:?}, view={:?}", self.engine.build_failure()))?;
        }
        self.present_and_record(now, verify_ref)?;
        Ok(&self.snapshot)
    }
    pub fn text(
        &mut self,
        now: InstantMs,
        input: TextInput<'_>,
        verify: bool,
    ) -> Result<&FrameSnapshot, String> {
        self.trigger = format!("text {:?} focus={:?}", input, self.engine.text_target());
        self.engine
            .try_advance(now, false)
            .map_err(|e| format!("advance: {e:?}"))?;
        if !self
            .engine
            .try_text(input, now)
            .map_err(|e| format!("text: {e:?}, view={:?}", self.engine.build_failure()))?
        {
            return Err("no focused text field".into());
        }
        self.present_and_record(now, verify)?;
        Ok(&self.snapshot)
    }
    pub fn message(
        &mut self,
        now: InstantMs,
        msg: A::Msg,
        verify_ref: bool,
    ) -> Result<&FrameSnapshot, String>
    where
        A::Msg: std::fmt::Debug,
    {
        self.trigger = format!("message {msg:?}");
        self.engine
            .try_advance(now, false)
            .map_err(|e| format!("advance: {e:?}, view={:?}", self.engine.build_failure()))?;
        self.engine
            .try_message(msg, now, false)
            .map_err(|e| format!("message: {e:?}, view={:?}", self.engine.build_failure()))?;
        self.present_and_record(now, verify_ref)?;
        Ok(&self.snapshot)
    }
    /// Recover a rejected view by repairing the model once and retrying its commit.
    pub fn recover(
        &mut self,
        now: InstantMs,
        repair: impl FnOnce(&mut A::Model),
    ) -> Result<&FrameSnapshot, String> {
        self.engine
            .try_with_model(repair)
            .map_err(|e| format!("repair: {e:?}"))?;
        self.engine
            .commit_view(now)
            .map_err(|e| format!("retry: {e:?}, view={:?}", self.engine.build_failure()))?;
        self.present_and_record(now, true)?;
        Ok(&self.snapshot)
    }
    fn present_and_record(&mut self, now: InstantMs, verify_ref: bool) -> Result<(), String> {
        let pre_damage: Vec<Rect> = self.engine.damage().as_slice().to_vec();
        let band_rects: Vec<Rect> = if pre_damage.is_empty() {
            Vec::new()
        } else {
            BandIter::new::<()>(
                &pre_damage,
                self.engine.screen(),
                self.engine.band_height(),
                self.scratch.len(),
            )
            .map_err(|e| format!("BandIter error: {e:?}"))?
            .collect()
        };

        let before = (!pre_damage.is_empty()).then(|| self.screen.words().to_owned());
        let mut fonts = FontTraffic::default();
        let status = self
            .engine
            .present_sync_with_pixels(&mut self.screen, &mut self.scratch, |_| Ok(()), &mut fonts)
            .map_err(|err| format!("present failed: {err:?}"))?;
        let metrics = self.engine.last_metrics();
        if status == PresentStatus::Presented {
            if verify_ref {
                let ref_scr = render_reference_fullscreen(
                    self.engine.active_commands(),
                    self.engine.active_tree(),
                    self.engine.fonts(),
                );
                if let Some((x, y, got, exp)) = self.screen.diff_first_mismatch(&ref_scr) {
                    return Err(format!(
                        "band/incremental render diverged from full-screen reference at ({x},{y}): got 0x{got:04X}, expected 0x{exp:04X}"
                    ));
                }
            }

            self.completed_frame_id += 1;
            self.last_completed_metrics = metrics;
            let png = self.screen.to_png();
            if self.png_history.len() == MAX_PNG_HISTORY {
                self.png_history.pop_front();
            }
            let changed = before
                .iter()
                .flat_map(|v| v.iter())
                .zip(self.screen.words())
                .filter(|(a, b)| a != b)
                .count();
            let tree = self.engine.active_tree();
            let current = [
                tree.active_count as usize,
                self.engine.active_commands().count,
                tree.text_len as usize,
            ];
            for (peak, used) in self.peaks.iter_mut().zip(current) {
                *peak = (*peak).max(used);
            }
            let mut report = format!(
                "frame={} time_ms={} trigger={}\nchanged_pixels={} transferred_pixels={} bytes={} bands={} measured_nodes={}\nnodes={} commands={} text_bytes={} peak_nodes={} peak_commands={} peak_text_bytes={}\nfocus={:?} capture={:?} damage={:?}",
                self.completed_frame_id,
                now.0,
                self.trigger,
                changed,
                metrics.pixels_sent,
                u64::from(metrics.pixels_sent) * 2,
                metrics.bands,
                metrics.measured_nodes,
                current[0],
                current[1],
                current[2],
                self.peaks[0],
                self.peaks[1],
                self.peaks[2],
                self.engine.focus().focused_key(),
                self.engine.pointer().captured_id(),
                pre_damage
            );
            report.push_str(&format!("\ninline_font_requests={} requested_bitmap_bytes={} cross_band={} external_read_bytes=0",fonts.requests,fonts.bytes,fonts.cross_band));
            self.png_history
                .push_back((self.completed_frame_id, png, report));
            self.oldest_retained_frame = self
                .png_history
                .front()
                .map(|(id, _, _)| *id)
                .unwrap_or(self.completed_frame_id + 1);

            if let Some(callback) = self.on_frame {
                self.engine
                    .try_with_model(|model| callback(model, metrics, now))
                    .map_err(|err| format!("frame callback failed: {err:?}"))?;
            }
        }

        let wakeup = self.engine.next_wakeup(now, false);
        self.snapshot = FrameSnapshot {
            frame_id: self.completed_frame_id,
            now_ms: now.0,
            status,
            damage: pre_damage,
            band_rects,
            bands: metrics.bands,
            pixels_sent: metrics.pixels_sent,
            measured_nodes: metrics.measured_nodes,
            commands_replayed: metrics.commands_replayed,
            wakeup,
            last_completed_frame_id: self.completed_frame_id,
            last_completed_metrics: self.last_completed_metrics,
        };
        Ok(())
    }

    pub fn frame_report(&self, id: u64) -> Option<&str> {
        self.png_history
            .iter()
            .find(|(frame, _, _)| *frame == id)
            .map(|(_, _, text)| text.as_str())
    }
    pub fn png_for_frame(&self, frame_id: Option<u64>) -> Result<(u64, &[u8]), FrameLookupError> {
        match frame_id {
            None => self
                .png_history
                .back()
                .map(|(id, bytes, _)| (*id, bytes.as_slice()))
                .ok_or(FrameLookupError::NotFound),
            Some(id) => {
                if let Some((_, bytes, _)) = self.png_history.iter().find(|(fid, _, _)| *fid == id)
                {
                    Ok((id, bytes.as_slice()))
                } else if id > 0 && id < self.oldest_retained_frame {
                    Err(FrameLookupError::Expired)
                } else {
                    Err(FrameLookupError::NotFound)
                }
            }
        }
    }
}
pub(crate) fn rects_to_json(rects: &[Rect]) -> String {
    let mut out = String::from("[");
    for (i, r) in rects.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            r.x, r.y, r.w, r.h
        ));
    }
    out.push(']');
    out
}
