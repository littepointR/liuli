use liuli_core::capacity::{FrameMetrics, MAX_DAMAGE, PresentError, PresentStatus};
use liuli_core::damage::DamageSet;
use liuli_core::geometry::Rect;
use liuli_core::paint::{CommandList, TextLookup};
use liuli_text::Lf4Font;

use crate::band::BandTarget;
use crate::raster::render_band;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BandSlot {
    A = 0,
    B = 1,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BandMeta {
    pub rect: Rect,
    pub frame: u32,
    pub seq: u32,
    pub slot: BandSlot,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SealError {
    EmptyRect,
    LengthMismatch,
}

/// Owned mutable pixel scratch slot before hand-off to DMA.
///
/// Only `FreeBand` grants `&mut [u16]`. Calling `seal` consumes `FreeBand` and returns a
/// read-only `ReadyBand`.
///
/// ```
/// use liuli_core::geometry::Rect;
/// use liuli_render::{BandSlot, FreeBand};
///
/// let mut words = [0u16; 32];
/// let mut band = FreeBand::new(&mut words, BandSlot::A);
/// band.pixels_mut()[0] = 0x1234;
/// let ready = band.seal(Rect::new(0, 0, 8, 4), 1, 0).unwrap();
/// assert_eq!(ready.pixels()[0], 0x1234);
/// ```
///
/// Using `FreeBand` after `seal` is rejected at compile time:
/// ```compile_fail
/// use liuli_core::geometry::Rect;
/// use liuli_render::{BandSlot, FreeBand};
///
/// let mut words = [0u16; 32];
/// let band = FreeBand::new(&mut words, BandSlot::A);
/// let _ready = band.seal(Rect::new(0, 0, 8, 4), 1, 0).unwrap();
/// let _ = band.slot();
/// ```
pub struct FreeBand<'a> {
    buf: &'a mut [u16],
    slot: BandSlot,
}

impl<'a> FreeBand<'a> {
    pub fn new(buf: &'a mut [u16], slot: BandSlot) -> Self {
        Self { buf, slot }
    }

    #[inline]
    pub const fn slot(&self) -> BandSlot {
        self.slot
    }

    #[inline]
    pub const fn capacity_pixels(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [u16] {
        self.buf
    }

    pub fn seal(self, rect: Rect, frame: u32, seq: u32) -> Result<ReadyBand<'a>, SealError> {
        if rect.is_empty() {
            return Err(SealError::EmptyRect);
        }
        let valid_len = (rect.w as usize)
            .checked_mul(rect.h as usize)
            .ok_or(SealError::LengthMismatch)?;
        if valid_len > self.buf.len() {
            return Err(SealError::LengthMismatch);
        }
        Ok(ReadyBand {
            buf: self.buf,
            meta: BandMeta {
                rect,
                frame,
                seq,
                slot: self.slot,
            },
            valid_len,
        })
    }
}

/// Read-only sealed band ready for DMA submission.
///
/// Neither `Clone`, `Copy`, nor mutable pixel access is available.
///
/// Positive usage:
/// ```
/// use liuli_core::geometry::Rect;
/// use liuli_render::{BandSlot, FreeBand};
///
/// let mut words = [0xFFFFu16; 16];
/// let band = FreeBand::new(&mut words, BandSlot::A);
/// let ready = band.seal(Rect::new(0, 0, 4, 4), 1, 0).unwrap();
/// assert_eq!(ready.pixels().len(), 16);
/// ```
///
/// Mutating pixels through `ReadyBand` fails to compile:
/// ```compile_fail
/// use liuli_core::geometry::Rect;
/// use liuli_render::{BandSlot, FreeBand};
///
/// let mut words = [0u16; 16];
/// let band = FreeBand::new(&mut words, BandSlot::A);
/// let mut ready = band.seal(Rect::new(0, 0, 4, 4), 1, 0).unwrap();
/// ready.pixels_mut()[0] = 1;
/// ```
pub struct ReadyBand<'a> {
    buf: &'a mut [u16],
    meta: BandMeta,
    valid_len: usize,
}

impl<'a> ReadyBand<'a> {
    #[inline]
    pub const fn meta(&self) -> BandMeta {
        self.meta
    }

    #[inline]
    pub const fn rect(&self) -> Rect {
        self.meta.rect
    }

    #[inline]
    pub const fn frame(&self) -> u32 {
        self.meta.frame
    }

    #[inline]
    pub const fn seq(&self) -> u32 {
        self.meta.seq
    }

    #[inline]
    pub const fn slot(&self) -> BandSlot {
        self.meta.slot
    }

    #[inline]
    pub fn pixels(&self) -> &[u16] {
        &self.buf[..self.valid_len]
    }

    /// Transfers ownership into a driver-owned `InFlightBand`.
    #[inline]
    pub fn into_in_flight(self) -> InFlightBand<'a> {
        InFlightBand {
            buf: self.buf,
            meta: self.meta,
            valid_len: self.valid_len,
        }
    }

    pub(crate) fn reclaim_unstarted(self) -> FreeBand<'a> {
        FreeBand {
            buf: self.buf,
            slot: self.meta.slot,
        }
    }
}

/// Driver-owned in-flight DMA buffer.
///
/// Exposes only read-only slice inspection until hardware stoppage is confirmed via
/// `confirm_stopped`.
pub struct InFlightBand<'a> {
    buf: &'a mut [u16],
    meta: BandMeta,
    valid_len: usize,
}

impl<'a> InFlightBand<'a> {
    #[inline]
    pub const fn meta(&self) -> BandMeta {
        self.meta
    }

    #[inline]
    pub fn pixels(&self) -> &[u16] {
        &self.buf[..self.valid_len]
    }

    /// Restores mutable ownership only after hardware has definitively stopped accessing the buffer.
    #[inline]
    pub fn confirm_stopped(self) -> FreeBand<'a> {
        FreeBand {
            buf: self.buf,
            slot: self.meta.slot,
        }
    }
}

pub enum StartOutcome<'a, E> {
    Started,
    Busy(ReadyBand<'a>),
    Rejected(E, ReadyBand<'a>),
}

pub enum PollOutcome<'a, E> {
    Pending,
    Completed {
        slot: BandSlot,
        frame: u32,
        seq: u32,
        band: FreeBand<'a>,
    },
    Failed {
        slot: BandSlot,
        frame: u32,
        seq: u32,
        error: E,
        band: Option<FreeBand<'a>>,
    },
}

/// Non-blocking DMA sink accepting moved `ReadyBand` ownership.
///
/// Attempting to submit the same `ReadyBand` a second time after `try_start` moves it fails
/// to compile:
///
/// Positive usage:
/// ```
/// use liuli_core::geometry::Rect;
/// use liuli_render::{BandSlot, DmaSink, FreeBand, PollOutcome, ReadyBand, StartOutcome};
///
/// struct NoopSink;
/// impl<'a> DmaSink<'a> for NoopSink {
///     type Error = ();
///     fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, ()> {
///         StartOutcome::Busy(band)
///     }
///     fn poll_complete(&mut self) -> PollOutcome<'a, ()> {
///         PollOutcome::Pending
///     }
/// }
///
/// let mut words = [0u16; 16];
/// let ready = FreeBand::new(&mut words, BandSlot::A)
///     .seal(Rect::new(0, 0, 4, 4), 1, 0)
///     .unwrap();
/// let mut sink = NoopSink;
/// match sink.try_start(ready) {
///     StartOutcome::Busy(returned) => assert_eq!(returned.seq(), 0),
///     _ => {}
/// }
/// ```
///
/// Compile-fail verification:
/// ```compile_fail
/// use liuli_core::geometry::Rect;
/// use liuli_render::{BandSlot, DmaSink, FreeBand, InFlightBand, PollOutcome, ReadyBand, StartOutcome};
///
/// struct HoldSink<'a>(Option<InFlightBand<'a>>);
/// impl<'a> DmaSink<'a> for HoldSink<'a> {
///     type Error = ();
///     fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, ()> {
///         self.0 = Some(band.into_in_flight());
///         StartOutcome::Started
///     }
///     fn poll_complete(&mut self) -> PollOutcome<'a, ()> {
///         PollOutcome::Pending
///     }
/// }
///
/// let mut words = [0u16; 16];
/// let ready = FreeBand::new(&mut words, BandSlot::A)
///     .seal(Rect::new(0, 0, 4, 4), 1, 0)
///     .unwrap();
/// let mut sink = HoldSink(None);
/// let _ = sink.try_start(ready);
/// let _ = sink.try_start(ready);
/// ```
pub trait DmaSink<'a> {
    type Error;
    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error>;
    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error>;
}

pub struct DmaCoordinator<'a> {
    slots: [Option<FreeBand<'a>>; 2],
    queued: Option<ReadyBand<'a>>,
    in_flight: Option<(u32, u32, BandSlot)>,
    faulted: bool,
    active_frame: bool,
    frame_id: u32,
    next_seq: u32,
    rects: [Rect; MAX_DAMAGE],
    rect_count: usize,
    rect_idx: usize,
    cur_y: i32,
    configured_band_h: u32,
    screen: Rect,
    metrics: FrameMetrics,
}

impl<'a> DmaCoordinator<'a> {
    pub fn new(buf_a: &'a mut [u16], buf_b: &'a mut [u16]) -> Self {
        Self {
            slots: [
                Some(FreeBand::new(buf_a, BandSlot::A)),
                Some(FreeBand::new(buf_b, BandSlot::B)),
            ],
            queued: None,
            in_flight: None,
            faulted: false,
            active_frame: false,
            frame_id: 0,
            next_seq: 0,
            rects: [Rect::ZERO; MAX_DAMAGE],
            rect_count: 0,
            rect_idx: 0,
            cur_y: 0,
            configured_band_h: 8,
            screen: Rect::ZERO,
            metrics: FrameMetrics::default(),
        }
    }

    #[inline]
    pub const fn is_active(&self) -> bool {
        self.active_frame
    }

    #[inline]
    pub const fn is_faulted(&self) -> bool {
        self.faulted
    }

    #[inline]
    pub const fn has_in_flight(&self) -> bool {
        self.in_flight.is_some()
    }

    pub fn slot_capacity_ok(&self, screen: Rect) -> bool {
        let min_words = screen.w as usize;
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .all(|b| b.capacity_pixels() >= min_words)
    }

    pub fn begin_frame<E>(
        &mut self,
        damage: &DamageSet,
        screen: Rect,
        configured_band_h: u32,
    ) -> Result<(), PresentError<E>> {
        if self.faulted {
            return Err(PresentError::DmaFaulted);
        }
        if self.active_frame {
            return Err(PresentError::DmaBusy);
        }
        if damage.is_empty() {
            return Err(PresentError::NoDamage);
        }
        let min_words = screen.w as usize;
        for slot in self.slots.iter().flatten() {
            if slot.capacity_pixels() < min_words {
                return Err(PresentError::BufferTooSmall);
            }
        }
        if self.slots[0].is_none() || self.slots[1].is_none() {
            return Err(PresentError::DmaBusy);
        }

        let slice = damage.as_slice();
        self.rect_count = slice.len().min(MAX_DAMAGE);
        self.rects[..self.rect_count].copy_from_slice(&slice[..self.rect_count]);
        self.rect_idx = 0;
        self.cur_y = if self.rect_count > 0 {
            self.rects[0].y
        } else {
            0
        };
        self.configured_band_h = configured_band_h.max(1);
        self.screen = screen;
        self.frame_id = self.frame_id.wrapping_add(1).max(1);
        self.next_seq = 0;
        self.active_frame = true;
        self.metrics = FrameMetrics::default();
        Ok(())
    }

    pub fn recover_from_fault(&mut self, buf_a: &'a mut [u16], buf_b: &'a mut [u16]) {
        self.slots[0] = Some(FreeBand::new(buf_a, BandSlot::A));
        self.slots[1] = Some(FreeBand::new(buf_b, BandSlot::B));
        self.queued = None;
        self.in_flight = None;
        self.active_frame = false;
        self.faulted = false;
    }

    pub fn step<S, L>(
        &mut self,
        sink: &mut S,
        cmds: &CommandList,
        live_damage: &mut DamageSet,
        text_lookup: &L,
        font: &Lf4Font<'_>,
    ) -> Result<(PresentStatus, FrameMetrics), PresentError<S::Error>>
    where
        S: DmaSink<'a>,
        L: TextLookup + ?Sized,
    {
        if self.faulted {
            return Err(PresentError::DmaFaulted);
        }

        // 1. Poll in-flight completion if any.
        if let Some((exp_frame, exp_seq, exp_slot)) = self.in_flight {
            match sink.poll_complete() {
                PollOutcome::Pending => {}
                PollOutcome::Completed {
                    slot,
                    frame,
                    seq,
                    band,
                } => {
                    if frame == exp_frame && seq == exp_seq && slot == exp_slot {
                        self.in_flight = None;
                        let band_idx = band.slot() as usize;
                        self.slots[band_idx] = Some(band);
                    } else {
                        // Stale or mismatched completion: ignore and keep waiting for expected slot
                    }
                }
                PollOutcome::Failed {
                    slot,
                    frame,
                    seq,
                    error,
                    band,
                } => {
                    if frame == exp_frame && seq == exp_seq && slot == exp_slot {
                        self.in_flight = None;
                        if let Some(q) = self.queued.take() {
                            let free = q.reclaim_unstarted();
                            let s = free.slot() as usize;
                            self.slots[s] = Some(free);
                        }
                        self.active_frame = false;
                        live_damage.mark_full_screen(self.screen);

                        if let Some(free) = band {
                            let s = free.slot() as usize;
                            self.slots[s] = Some(free);
                            return Err(PresentError::Target(error));
                        } else {
                            self.faulted = true;
                            return Err(PresentError::DmaFaulted);
                        }
                    }
                }
            }
        }

        if !self.active_frame {
            return Ok((PresentStatus::Idle, self.metrics));
        }

        // 2. If DMA is idle and we have a queued ReadyBand, try to start it.
        if self.in_flight.is_none()
            && let Some(ready) = self.queued.take()
        {
            let meta = ready.meta();
            match sink.try_start(ready) {
                StartOutcome::Started => {
                    self.in_flight = Some((meta.frame, meta.seq, meta.slot));
                    self.metrics.bands += 1;
                    self.metrics.pixels_sent = self
                        .metrics
                        .pixels_sent
                        .saturating_add(meta.rect.w.saturating_mul(meta.rect.h));
                }
                StartOutcome::Busy(returned) => {
                    self.queued = Some(returned);
                }
                StartOutcome::Rejected(err, returned) => {
                    let free = returned.reclaim_unstarted();
                    let s = free.slot() as usize;
                    self.slots[s] = Some(free);
                    self.active_frame = false;
                    live_damage.mark_full_screen(self.screen);
                    return Err(PresentError::Target(err));
                }
            }
        }

        // 3. Render next band into a free slot while at most one is in flight and one is queued.
        for _ in 0..2 {
            if self.queued.is_some() {
                break;
            }
            let Some(free_idx) = self.take_free_slot_idx() else {
                break;
            };
            let free_cap = self.slots[free_idx].as_ref().unwrap().capacity_pixels();
            let Some(band_rect) = self.next_band_rect(free_cap) else {
                break;
            };

            let mut free = self.slots[free_idx].take().unwrap();
            {
                let mut target = match BandTarget::new(free.pixels_mut(), band_rect) {
                    Ok(t) => t,
                    Err(e) => {
                        self.slots[free_idx] = Some(free);
                        self.abort_keep_damage(live_damage);
                        return Err(e);
                    }
                };
                self.metrics.commands_replayed = self
                    .metrics
                    .commands_replayed
                    .saturating_add(render_band(&mut target, cmds, text_lookup, font));
            }
            let seq = self.next_seq;
            self.next_seq += 1;
            let ready = match free.seal(band_rect, self.frame_id, seq) {
                Ok(r) => r,
                Err(_) => {
                    self.abort_keep_damage(live_damage);
                    return Err(PresentError::BufferTooSmall);
                }
            };

            if self.in_flight.is_none() {
                let meta = ready.meta();
                match sink.try_start(ready) {
                    StartOutcome::Started => {
                        self.in_flight = Some((meta.frame, meta.seq, meta.slot));
                        self.metrics.bands += 1;
                        self.metrics.pixels_sent = self
                            .metrics
                            .pixels_sent
                            .saturating_add(meta.rect.w.saturating_mul(meta.rect.h));
                    }
                    StartOutcome::Busy(returned) => {
                        self.queued = Some(returned);
                        break;
                    }
                    StartOutcome::Rejected(err, returned) => {
                        let free = returned.reclaim_unstarted();
                        let s = free.slot() as usize;
                        self.slots[s] = Some(free);
                        self.active_frame = false;
                        live_damage.mark_full_screen(self.screen);
                        return Err(PresentError::Target(err));
                    }
                }
            } else {
                self.queued = Some(ready);
                break;
            }
        }

        // 4. Check if frame is completely finished.
        if self.in_flight.is_none() && self.queued.is_none() && !self.has_more_bands() {
            live_damage.clear();
            self.active_frame = false;
            return Ok((PresentStatus::Presented, self.metrics));
        }

        Ok((PresentStatus::Pending, self.metrics))
    }

    fn take_free_slot_idx(&self) -> Option<usize> {
        if self.slots[0].is_some() {
            Some(0)
        } else if self.slots[1].is_some() {
            Some(1)
        } else {
            None
        }
    }

    fn has_more_bands(&self) -> bool {
        let mut idx = self.rect_idx;
        let mut y = self.cur_y;
        while idx < self.rect_count {
            let r = self.rects[idx];
            if !r.is_empty() && (y as i64) < r.bottom64() {
                return true;
            }
            idx += 1;
            if idx < self.rect_count {
                y = self.rects[idx].y;
            }
        }
        false
    }

    fn next_band_rect(&mut self, buf_cap: usize) -> Option<Rect> {
        while self.rect_idx < self.rect_count {
            let r = self.rects[self.rect_idx];
            if r.is_empty() || (self.cur_y as i64) >= r.bottom64() {
                self.rect_idx += 1;
                if self.rect_idx < self.rect_count {
                    self.cur_y = self.rects[self.rect_idx].y;
                }
                continue;
            }
            let remaining_h = (r.bottom64() - self.cur_y as i64) as u32;
            let max_rows = (buf_cap / (r.w as usize)).max(1) as u32;
            let bh = self.configured_band_h.min(remaining_h).min(max_rows);
            let band = Rect::new(r.x, self.cur_y, r.w, bh);
            self.cur_y = (self.cur_y as i64 + bh as i64) as i32;
            return Some(band);
        }
        None
    }

    fn abort_keep_damage(&mut self, live_damage: &mut DamageSet) {
        if let Some(q) = self.queued.take() {
            let free = q.reclaim_unstarted();
            let s = free.slot() as usize;
            self.slots[s] = Some(free);
        }
        self.active_frame = false;
        live_damage.mark_full_screen(self.screen);
    }
}
