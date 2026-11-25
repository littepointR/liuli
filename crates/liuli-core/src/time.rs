use crate::capacity::{MAX_ANIMATIONS, TimeError};
use crate::geometry::Rect;
use crate::view::Key;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Default)]
pub struct InstantMs(pub u64);

impl InstantMs {
    #[inline]
    pub const fn saturating_add(self, delta: u64) -> Self {
        Self(self.0.saturating_add(delta))
    }

    #[inline]
    pub const fn saturating_sub(self, other: Self) -> u64 {
        self.0.saturating_sub(other.0)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NextWakeup {
    Now,
    At(InstantMs),
    OnEvent,
}

impl NextWakeup {
    pub fn earlier(self, other: Self) -> Self {
        match (self, other) {
            (Self::Now, _) | (_, Self::Now) => Self::Now,
            (Self::At(a), Self::At(b)) => Self::At(if a <= b { a } else { b }),
            (Self::At(a), Self::OnEvent) | (Self::OnEvent, Self::At(a)) => Self::At(a),
            (Self::OnEvent, Self::OnEvent) => Self::OnEvent,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ClockTracker {
    initialized: bool,
    last: InstantMs,
}

impl ClockTracker {
    pub const ZERO: Self = Self {
        initialized: false,
        last: InstantMs(0),
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn advance_to(&mut self, now: InstantMs) -> Result<InstantMs, TimeError> {
        if self.initialized && now < self.last {
            return Err(TimeError::WentBackwards);
        }
        self.initialized = true;
        self.last = now;
        Ok(now)
    }

    pub const fn last(&self) -> Option<InstantMs> {
        if self.initialized {
            Some(self.last)
        } else {
            None
        }
    }
}

impl Default for ClockTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks a wrapping 32-bit hardware cycle/timer counter into a monotonic [`InstantMs`].
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MonotonicClock32 {
    last_raw: u32,
    accum_ticks: u64,
    ticks_per_ms: u32,
    initialized: bool,
}

impl MonotonicClock32 {
    /// Creates a clock converter given `ticks_per_ms` (values `< 1` clamp to `1`).
    #[inline]
    pub const fn new(ticks_per_ms: u32) -> Self {
        Self {
            last_raw: 0,
            accum_ticks: 0,
            ticks_per_ms: if ticks_per_ms == 0 { 1 } else { ticks_per_ms },
            initialized: false,
        }
    }

    /// Creates a clock converter from a counter frequency in Hz (minimum 1 tick/ms).
    #[inline]
    pub const fn from_hz(hz: u32) -> Self {
        Self::new(hz / 1000)
    }

    /// Samples the raw 32-bit counter and returns the monotonic millisecond timestamp.
    pub fn sample(&mut self, raw: u32) -> InstantMs {
        if !self.initialized {
            self.last_raw = raw;
            self.initialized = true;
            return InstantMs(0);
        }
        let delta = u64::from(raw.wrapping_sub(self.last_raw));
        self.accum_ticks = self.accum_ticks.saturating_add(delta);
        self.last_raw = raw;
        self.now()
    }

    /// Returns the most recently sampled monotonic timestamp without reading hardware.
    #[inline]
    pub const fn now(&self) -> InstantMs {
        InstantMs(self.accum_ticks / (self.ticks_per_ms as u64))
    }

    /// Resets the clock back to uninitialized `InstantMs(0)`.
    #[inline]
    pub fn reset(&mut self) {
        self.last_raw = 0;
        self.accum_ticks = 0;
        self.initialized = false;
    }
}

#[cfg(test)]
mod monotonic_clock32_tests {
    use super::*;

    #[test]
    fn handles_32bit_wraparound_subms_remainder_and_zero_hz() {
        // 400 MHz => 400_000 ticks per ms
        let mut clk = MonotonicClock32::from_hz(400_000_000);
        assert_eq!(clk.sample(u32::MAX - 100_000), InstantMs(0));
        // Wrap across u32::MAX with 300_000 ticks -> still < 1 ms (total 300_000)
        assert_eq!(clk.sample(199_999), InstantMs(0));
        // Advance another 100_000 ticks -> total 400_000 ticks => 1 ms without remainder loss
        assert_eq!(clk.sample(299_999), InstantMs(1));
        clk.reset();
        assert_eq!(clk.now(), InstantMs(0));
        assert_eq!(clk.sample(500), InstantMs(0));

        // Zero ticks_per_ms / zero Hz does not panic on division
        let mut fallback = MonotonicClock32::from_hz(0);
        assert_eq!(fallback.sample(10), InstantMs(0));
        assert_eq!(fallback.sample(25), InstantMs(15));
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum AnimProp {
    #[default]
    ToggleKnob = 0,
    PressDepth = 1,
    Custom0 = 2,
    Custom1 = 3,
    Custom2 = 4,
    Custom3 = 5,
}

impl AnimProp {
    #[inline]
    pub const fn custom(channel: u8) -> Self {
        match channel & 0b11 {
            1 => Self::Custom1,
            2 => Self::Custom2,
            3 => Self::Custom3,
            _ => Self::Custom0,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum Easing {
    #[default]
    Linear = 0,
    EaseOutCubic = 1,
}

impl Easing {
    /// Fixed-point `[0..=1000]` easing curve evaluated strictly in `i64`.
    #[inline]
    pub const fn apply(self, q: i64) -> i64 {
        let clamped = if q < 0 {
            0
        } else if q > 1000 {
            1000
        } else {
            q
        };
        match self {
            Self::Linear => clamped,
            Self::EaseOutCubic => {
                let inv = 1000 - clamped;
                1000 - (inv * inv * inv) / 1_000_000
            }
        }
    }
}

/// A scalar animation owned by an application or component. Its units are set by the caller.
/// Sample in App::on_timer and expose the next deadline through App::next_deadline.
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    from: i32,
    to: i32,
    start: InstantMs,
    duration_ms: u32,
    easing: Easing,
}
impl Tween {
    pub const fn at(value: i32) -> Self {
        Self {
            from: value,
            to: value,
            start: InstantMs(0),
            duration_ms: 0,
            easing: Easing::Linear,
        }
    }
    pub fn value(&self, now: InstantMs) -> i32 {
        let elapsed = now.saturating_sub(self.start);
        if elapsed >= u64::from(self.duration_ms) {
            return self.to;
        }
        let progress = (elapsed * 1000 / u64::from(self.duration_ms)) as i64;
        let delta = i64::from(self.to) - i64::from(self.from);
        (i64::from(self.from) + delta * self.easing.apply(progress) / 1000) as i32
    }
    pub fn animate_to(&mut self, now: InstantMs, target: i32, duration_ms: u32, easing: Easing) {
        *self = Self {
            from: self.value(now),
            to: target,
            start: now,
            duration_ms,
            easing,
        };
    }
    pub fn cancel(&mut self, now: InstantMs) {
        *self = Self::at(self.value(now));
    }
    pub fn next_deadline(
        &self,
        now: InstantMs,
        frame_ms: core::num::NonZeroU32,
    ) -> Option<InstantMs> {
        let end = self.start.saturating_add(self.duration_ms.into());
        (self.from != self.to && now < end)
            .then(|| end.min(now.saturating_add(frame_ms.get().into())))
    }
}
#[cfg(test)]
mod tween_tests {
    use super::*;
    #[test]
    fn retarget_and_cancel_take_effect_at_event_time() {
        let mut t = Tween::at(i32::MIN);
        t.animate_to(InstantMs(20), i32::MAX, 100, Easing::Linear);
        assert_eq!(t.value(InstantMs(70)), -1);
        t.animate_to(InstantMs(70), 99, 10, Easing::Linear);
        t.cancel(InstantMs(75));
        assert_eq!(t.value(InstantMs(500)), 49);
        assert_eq!(
            t.next_deadline(InstantMs(75), core::num::NonZeroU32::new(16).unwrap()),
            None
        );
    }
}
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AnimTrack {
    pub active: bool,
    pub easing: Easing,
    pub key: Key,
    pub prop: AnimProp,
    pub start_val: i32,
    pub end_val: i32,
    pub current_val: i32,
    pub start_ms: InstantMs,
    pub duration_ms: u32,
    pub last_sample_ms: InstantMs,
    pub dirty_rect: Rect,
}

impl AnimTrack {
    pub const ZERO: Self = Self {
        active: false,
        easing: Easing::Linear,
        key: Key(0),
        prop: AnimProp::ToggleKnob,
        start_val: 0,
        end_val: 0,
        current_val: 0,
        start_ms: InstantMs(0),
        duration_ms: 0,
        last_sample_ms: InstantMs(0),
        dirty_rect: Rect::ZERO,
    };

    pub fn evaluate_at(&self, now: InstantMs) -> i32 {
        if self.duration_ms == 0 {
            return self.end_val;
        }
        let elapsed = now.saturating_sub(self.start_ms);
        if elapsed >= self.duration_ms as u64 {
            return self.end_val;
        }
        let delta = self.end_val as i64 - self.start_val as i64;
        let dur = self.duration_ms as i64;
        match self.easing {
            Easing::Linear => (self.start_val as i64 + (delta * (elapsed as i64)) / dur) as i32,
            Easing::EaseOutCubic => {
                let q = ((elapsed as i64) * 1000) / dur;
                let eased = self.easing.apply(q);
                (self.start_val as i64 + (delta * eased) / 1000) as i32
            }
        }
    }

    pub fn is_finished_at(&self, now: InstantMs) -> bool {
        self.duration_ms == 0 || now.saturating_sub(self.start_ms) >= self.duration_ms as u64
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AnimOverride {
    pub active: bool,
    pub key: Key,
    pub prop: AnimProp,
    pub value: i32,
}

impl AnimOverride {
    pub const ZERO: Self = Self {
        active: false,
        key: Key(0),
        prop: AnimProp::ToggleKnob,
        value: 0,
    };
}

pub const ANIM_SAMPLE_INTERVAL_MS: u64 = 16;
pub const TOGGLE_ANIM_DURATION_MS: u32 = 160;
pub const ANIM_SCALE: i32 = 1000;
pub const REPEAT_DELAY_MS: u64 = 400;
pub const REPEAT_PERIOD_MS: u64 = 80;
pub const MAX_REPEAT_STEPS_PER_ADVANCE: u16 = 8;

#[derive(Clone, Debug)]
pub struct AnimPool {
    tracks: [AnimTrack; MAX_ANIMATIONS],
    overrides: [AnimOverride; MAX_ANIMATIONS],
}

impl AnimPool {
    pub const ZERO: Self = Self {
        tracks: [AnimTrack::ZERO; MAX_ANIMATIONS],
        overrides: [AnimOverride::ZERO; MAX_ANIMATIONS],
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn reset(&mut self) {
        self.tracks = [AnimTrack::ZERO; MAX_ANIMATIONS];
        self.overrides = [AnimOverride::ZERO; MAX_ANIMATIONS];
    }

    #[inline]
    pub fn copy_from(&mut self, other: &Self) {
        self.tracks = other.tracks;
        self.overrides = other.overrides;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        key: Key,
        prop: AnimProp,
        default_start: i32,
        end_val: i32,
        now: InstantMs,
        duration_ms: u32,
        dirty_rect: Rect,
    ) -> Option<Rect> {
        self.start_with_easing(
            key,
            prop,
            default_start,
            end_val,
            now,
            duration_ms,
            dirty_rect,
            Easing::Linear,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_with_easing(
        &mut self,
        key: Key,
        prop: AnimProp,
        default_start: i32,
        end_val: i32,
        now: InstantMs,
        duration_ms: u32,
        dirty_rect: Rect,
        easing: Easing,
    ) -> Option<Rect> {
        let current = self.current_value(key, prop, now).unwrap_or(default_start);
        self.set_override(key, prop, current);

        if duration_ms == 0 || current == end_val {
            self.remove_track(key, prop);
            self.set_override(key, prop, end_val);
            return if current != end_val {
                Some(dirty_rect)
            } else {
                None
            };
        }

        // Update existing track if present
        for t in &mut self.tracks {
            if t.active && t.key == key && t.prop == prop {
                t.easing = easing;
                t.start_val = current;
                t.end_val = end_val;
                t.current_val = current;
                t.start_ms = now;
                t.duration_ms = duration_ms;
                t.last_sample_ms = now;
                t.dirty_rect = dirty_rect;
                return Some(dirty_rect);
            }
        }

        // Insert into free slot
        for t in &mut self.tracks {
            if !t.active {
                *t = AnimTrack {
                    active: true,
                    easing,
                    key,
                    prop,
                    start_val: current,
                    end_val,
                    current_val: current,
                    start_ms: now,
                    duration_ms,
                    last_sample_ms: now,
                    dirty_rect,
                };
                return Some(dirty_rect);
            }
        }

        // Pool full: jump directly to end value and invalidate dirty_rect
        self.set_override(key, prop, end_val);
        Some(dirty_rect)
    }

    pub fn current_value(&self, key: Key, prop: AnimProp, now: InstantMs) -> Option<i32> {
        for t in &self.tracks {
            if t.active && t.key == key && t.prop == prop {
                return Some(t.evaluate_at(now));
            }
        }
        for o in &self.overrides {
            if o.active && o.key == key && o.prop == prop {
                return Some(o.value);
            }
        }
        None
    }

    pub fn cached_value(&self, key: Key, prop: AnimProp) -> Option<i32> {
        for t in &self.tracks {
            if t.active && t.key == key && t.prop == prop {
                return Some(t.current_val);
            }
        }
        for o in &self.overrides {
            if o.active && o.key == key && o.prop == prop {
                return Some(o.value);
            }
        }
        None
    }

    pub(crate) fn target_value(&self, key: Key, prop: AnimProp) -> Option<i32> {
        self.tracks
            .iter()
            .find(|t| t.active && t.key == key && t.prop == prop)
            .map(|t| t.end_val)
            .or_else(|| self.cached_value(key, prop))
    }

    pub fn update_rect(&mut self, key: Key, prop: AnimProp, rect: Rect) {
        for t in &mut self.tracks {
            if t.active && t.key == key && t.prop == prop {
                t.dirty_rect = rect;
            }
        }
    }

    pub fn cancel_for_key(&mut self, key: Key) {
        for t in &mut self.tracks {
            if t.active && t.key == key {
                t.active = false;
            }
        }
        for o in &mut self.overrides {
            if o.active && o.key == key {
                o.active = false;
            }
        }
    }

    pub fn advance(&mut self, now: InstantMs, mut on_dirty: impl FnMut(Rect)) -> bool {
        let mut changed = false;
        for idx in 0..self.tracks.len() {
            if !self.tracks[idx].active {
                continue;
            }
            let next_val = self.tracks[idx].evaluate_at(now);
            let finished = self.tracks[idx].is_finished_at(now);
            if next_val != self.tracks[idx].current_val || finished {
                self.tracks[idx].current_val = next_val;
                self.tracks[idx].last_sample_ms = now;
                on_dirty(self.tracks[idx].dirty_rect);
                changed = true;
            }
            if finished {
                let key = self.tracks[idx].key;
                let prop = self.tracks[idx].prop;
                let end_val = self.tracks[idx].end_val;
                self.tracks[idx].active = false;
                self.set_override(key, prop, end_val);
            }
        }
        changed
    }

    pub fn has_active(&self) -> bool {
        self.tracks.iter().any(|t| t.active)
    }

    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        let mut earliest: Option<InstantMs> = None;
        for t in &self.tracks {
            if !t.active {
                continue;
            }
            let end_time = t.start_ms.saturating_add(t.duration_ms as u64);
            let target = if now >= end_time {
                return NextWakeup::Now;
            } else {
                let next_sample = t.last_sample_ms.saturating_add(ANIM_SAMPLE_INTERVAL_MS);
                let step = if next_sample < end_time {
                    next_sample
                } else {
                    end_time
                };
                if step <= now {
                    return NextWakeup::Now;
                }
                step
            };
            earliest = Some(match earliest {
                Some(prev) if prev <= target => prev,
                _ => target,
            });
        }
        match earliest {
            Some(t) => NextWakeup::At(t),
            None => NextWakeup::OnEvent,
        }
    }

    fn remove_track(&mut self, key: Key, prop: AnimProp) {
        for t in &mut self.tracks {
            if t.active && t.key == key && t.prop == prop {
                t.active = false;
            }
        }
    }

    fn set_override(&mut self, key: Key, prop: AnimProp, val: i32) {
        for o in &mut self.overrides {
            if o.active && o.key == key && o.prop == prop {
                o.value = val;
                return;
            }
        }
        for o in &mut self.overrides {
            if !o.active {
                *o = AnimOverride {
                    active: true,
                    key,
                    prop,
                    value: val,
                };
                return;
            }
        }
        self.overrides[0] = AnimOverride {
            active: true,
            key,
            prop,
            value: val,
        };
    }
}

impl Default for AnimPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Fixed-point (Q8) scroll inertia, exponential friction, overscroll bounce, and snap helper.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ScrollPhysics {
    /// Current scroll offset in 1/256 px units.
    pub offset_q8: i32,
    /// Current velocity in 1/256 px per 16 ms step.
    pub velocity_q8: i32,
    /// Minimum scrollable offset in pixels.
    pub min_offset: i32,
    /// Maximum scrollable offset in pixels.
    pub max_offset: i32,
    /// Optional snap increment in pixels (`0` disables snapping).
    pub snap_step: u16,
    /// Maximum rubber-band overscroll distance in pixels (`0` disables bounce).
    pub bounce_px: u16,
    last_step_ms: Option<InstantMs>,
    dragging: bool,
}

impl ScrollPhysics {
    const Q_SHIFT: i32 = 8;
    const Q_ONE: i32 = 1 << Self::Q_SHIFT;
    const STEP_MS: u64 = 16;

    pub const ZERO: Self = Self {
        offset_q8: 0,
        velocity_q8: 0,
        min_offset: 0,
        max_offset: 0,
        snap_step: 0,
        bounce_px: 0,
        last_step_ms: None,
        dragging: false,
    };

    #[inline]
    pub const fn new(min_offset: i32, max_offset: i32) -> Self {
        let max_clamped = if max_offset < min_offset {
            min_offset
        } else {
            max_offset
        };
        Self {
            min_offset,
            max_offset: max_clamped,
            ..Self::ZERO
        }
    }

    #[inline]
    pub const fn with_bounce(mut self, bounce_px: u16) -> Self {
        self.bounce_px = bounce_px;
        self
    }

    #[inline]
    pub const fn with_snap(mut self, snap_step: u16) -> Self {
        self.snap_step = snap_step;
        self
    }

    #[inline]
    pub fn offset(&self) -> i32 {
        (self.offset_q8 + (Self::Q_ONE / 2)).div_euclid(Self::Q_ONE)
    }

    #[inline]
    pub fn set_offset(&mut self, offset_px: i32) {
        self.offset_q8 = offset_px
            .clamp(
                self.min_offset.saturating_sub(i32::from(self.bounce_px)),
                self.max_offset.saturating_add(i32::from(self.bounce_px)),
            )
            .saturating_mul(Self::Q_ONE);
        self.velocity_q8 = 0;
    }

    pub fn set_bounds(&mut self, min_offset: i32, max_offset: i32) {
        self.min_offset = min_offset;
        self.max_offset = max_offset.max(min_offset);
        if !self.dragging && self.velocity_q8 == 0 && !self.is_overscrolled() {
            let clamped = self.offset().clamp(self.min_offset, self.max_offset);
            self.offset_q8 = clamped.saturating_mul(Self::Q_ONE);
        }
    }

    #[inline]
    pub fn is_overscrolled(&self) -> bool {
        let min_q8 = self.min_offset.saturating_mul(Self::Q_ONE);
        let max_q8 = self.max_offset.saturating_mul(Self::Q_ONE);
        self.offset_q8 < min_q8 || self.offset_q8 > max_q8
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        !self.dragging && (self.velocity_q8 != 0 || self.is_overscrolled())
    }

    #[inline]
    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        if !self.is_active() {
            return NextWakeup::OnEvent;
        }
        match self.last_step_ms {
            Some(prev) => {
                let target = prev.saturating_add(Self::STEP_MS);
                if target <= now {
                    NextWakeup::Now
                } else {
                    NextWakeup::At(target)
                }
            }
            None => NextWakeup::Now,
        }
    }

    pub fn begin_drag(&mut self, now: InstantMs) {
        self.dragging = true;
        self.velocity_q8 = 0;
        self.last_step_ms = Some(now);
    }

    pub fn drag_by(&mut self, delta_px: i32, now: InstantMs) -> i32 {
        self.dragging = true;
        let min_q8 = self.min_offset.saturating_mul(Self::Q_ONE);
        let max_q8 = self.max_offset.saturating_mul(Self::Q_ONE);
        let bounce_q8 = i32::from(self.bounce_px).saturating_mul(Self::Q_ONE);
        let raw_delta_q8 = delta_px.saturating_mul(Self::Q_ONE);
        let effective_q8 = if (self.offset_q8 <= min_q8 && raw_delta_q8 < 0)
            || (self.offset_q8 >= max_q8 && raw_delta_q8 > 0)
        {
            raw_delta_q8 / 3
        } else {
            raw_delta_q8
        };
        self.offset_q8 = self.offset_q8.saturating_add(effective_q8).clamp(
            min_q8.saturating_sub(bounce_q8),
            max_q8.saturating_add(bounce_q8),
        );
        let dt_ms = self
            .last_step_ms
            .map(|prev| now.0.saturating_sub(prev.0))
            .unwrap_or(Self::STEP_MS)
            .clamp(4, 64) as i32;
        let inst_v = (effective_q8.saturating_mul(Self::STEP_MS as i32)) / dt_ms;
        self.velocity_q8 = (self.velocity_q8 / 2).saturating_add(inst_v / 2);
        self.last_step_ms = Some(now);
        self.offset()
    }

    pub fn release(&mut self, now: InstantMs) {
        self.dragging = false;
        self.last_step_ms = Some(now);
    }

    pub fn release_with_velocity(&mut self, velocity_px_per_sec: i32, now: InstantMs) {
        self.dragging = false;
        self.velocity_q8 =
            ((i64::from(velocity_px_per_sec) * i64::from(Self::Q_ONE) * Self::STEP_MS as i64)
                / 1000)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        self.last_step_ms = Some(now);
    }

    /// Advance inertia/bounce physics using wall-clock timestamp; returns `true` when offset changed.
    pub fn advance(&mut self, now: InstantMs) -> bool {
        if !self.is_active() {
            return false;
        }
        let elapsed = self
            .last_step_ms
            .map(|prev| now.0.saturating_sub(prev.0))
            .unwrap_or(Self::STEP_MS);
        let steps = (elapsed / Self::STEP_MS).min(16);
        if steps == 0 {
            return false;
        }
        self.last_step_ms = Some(InstantMs(
            self.last_step_ms
                .map(|p| p.0.saturating_add(steps * Self::STEP_MS))
                .unwrap_or(now.0),
        ));
        let before = self.offset();
        for _ in 0..steps {
            self.tick_one();
            if !self.is_active() {
                break;
            }
        }
        self.offset() != before || !self.is_active()
    }

    /// Advance a fixed duration in milliseconds (convenience for fixed-timestep loops/tests).
    pub fn step_ms(&mut self, dt_ms: u32) -> i32 {
        let steps = (u64::from(dt_ms) / Self::STEP_MS).clamp(1, 32);
        for _ in 0..steps {
            self.tick_one();
            if !self.is_active() {
                break;
            }
        }
        self.offset()
    }

    fn tick_one(&mut self) {
        let min_q8 = self.min_offset.saturating_mul(Self::Q_ONE);
        let max_q8 = self.max_offset.saturating_mul(Self::Q_ONE);
        let bounce_q8 = i32::from(self.bounce_px).saturating_mul(Self::Q_ONE);

        if self.offset_q8 < min_q8 || self.offset_q8 > max_q8 {
            let target_q8 = self.offset_q8.clamp(min_q8, max_q8);
            let dist = target_q8 - self.offset_q8;
            let pull = if dist.abs() <= 64 {
                dist
            } else {
                let step = dist / 4;
                if step == 0 { dist.signum() * 64 } else { step }
            };
            self.offset_q8 = self.offset_q8.saturating_add(pull);
            self.velocity_q8 = 0;
            return;
        }

        if self.velocity_q8 != 0 {
            self.offset_q8 = self.offset_q8.saturating_add(self.velocity_q8).clamp(
                min_q8.saturating_sub(bounce_q8),
                max_q8.saturating_add(bounce_q8),
            );
            if self.offset_q8 <= min_q8.saturating_sub(bounce_q8)
                || self.offset_q8 >= max_q8.saturating_add(bounce_q8)
            {
                self.velocity_q8 = if self.bounce_px == 0 {
                    0
                } else {
                    -(self.velocity_q8 / 4)
                };
            } else {
                self.velocity_q8 = (i64::from(self.velocity_q8) * 235 / 256) as i32;
            }

            if self.velocity_q8.abs() < 32 && !self.is_overscrolled() {
                self.velocity_q8 = 0;
                if self.snap_step > 1 {
                    let step = i32::from(self.snap_step);
                    let cur = self.offset();
                    let rel = cur - self.min_offset;
                    let snapped = self.min_offset + ((rel + step / 2) / step) * step;
                    self.offset_q8 = snapped
                        .clamp(self.min_offset, self.max_offset)
                        .saturating_mul(Self::Q_ONE);
                }
            }
        }
    }
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self::ZERO
    }
}
