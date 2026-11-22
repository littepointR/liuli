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
    last: Option<InstantMs>,
}

impl ClockTracker {
    pub const fn new() -> Self {
        Self { last: None }
    }

    pub fn advance_to(&mut self, now: InstantMs) -> Result<InstantMs, TimeError> {
        if let Some(prev) = self.last
            && now < prev
        {
            return Err(TimeError::WentBackwards);
        }
        self.last = Some(now);
        Ok(now)
    }

    pub const fn last(&self) -> Option<InstantMs> {
        self.last
    }
}

impl Default for ClockTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AnimProp {
    ToggleKnob,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AnimTrack {
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
    pub fn evaluate_at(&self, now: InstantMs) -> i32 {
        if self.duration_ms == 0 {
            return self.end_val;
        }
        let elapsed = now.saturating_sub(self.start_ms);
        if elapsed >= self.duration_ms as u64 {
            return self.end_val;
        }
        let delta = self.end_val as i64 - self.start_val as i64;
        let num = delta * (elapsed as i64);
        let dur = self.duration_ms as i64;
        (self.start_val as i64 + (num / dur)) as i32
    }

    pub fn is_finished_at(&self, now: InstantMs) -> bool {
        self.duration_ms == 0 || now.saturating_sub(self.start_ms) >= self.duration_ms as u64
    }
}

pub const ANIM_SAMPLE_INTERVAL_MS: u64 = 16;
pub const TOGGLE_ANIM_DURATION_MS: u32 = 160;
pub const ANIM_SCALE: i32 = 1000;

#[derive(Clone, Debug)]
pub struct AnimPool {
    tracks: [Option<AnimTrack>; MAX_ANIMATIONS],
    overrides: [Option<(Key, AnimProp, i32)>; MAX_ANIMATIONS],
}

impl AnimPool {
    pub const fn new() -> Self {
        Self {
            tracks: [None; MAX_ANIMATIONS],
            overrides: [None; MAX_ANIMATIONS],
        }
    }

    pub fn reset(&mut self) {
        self.tracks = [None; MAX_ANIMATIONS];
        self.overrides = [None; MAX_ANIMATIONS];
    }

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
        for slot in &mut self.tracks {
            if let Some(t) = slot
                && t.key == key
                && t.prop == prop
            {
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
        for slot in &mut self.tracks {
            if slot.is_none() {
                *slot = Some(AnimTrack {
                    key,
                    prop,
                    start_val: current,
                    end_val,
                    current_val: current,
                    start_ms: now,
                    duration_ms,
                    last_sample_ms: now,
                    dirty_rect,
                });
                return Some(dirty_rect);
            }
        }

        // Pool full: jump directly to end value and invalidate dirty_rect
        self.set_override(key, prop, end_val);
        Some(dirty_rect)
    }

    pub fn current_value(&self, key: Key, prop: AnimProp, now: InstantMs) -> Option<i32> {
        for t in self.tracks.iter().flatten() {
            if t.key == key && t.prop == prop {
                return Some(t.evaluate_at(now));
            }
        }
        for o in self.overrides.iter().flatten() {
            if o.0 == key && o.1 == prop {
                return Some(o.2);
            }
        }
        None
    }

    pub fn cached_value(&self, key: Key, prop: AnimProp) -> Option<i32> {
        for t in self.tracks.iter().flatten() {
            if t.key == key && t.prop == prop {
                return Some(t.current_val);
            }
        }
        for o in self.overrides.iter().flatten() {
            if o.0 == key && o.1 == prop {
                return Some(o.2);
            }
        }
        None
    }

    pub fn update_rect(&mut self, key: Key, prop: AnimProp, rect: Rect) {
        for t in self.tracks.iter_mut().flatten() {
            if t.key == key && t.prop == prop {
                t.dirty_rect = rect;
            }
        }
    }

    pub fn cancel_for_key(&mut self, key: Key) {
        for slot in &mut self.tracks {
            if let Some(t) = slot
                && t.key == key
            {
                *slot = None;
            }
        }
        for slot in &mut self.overrides {
            if let Some((k, _, _)) = slot
                && *k == key
            {
                *slot = None;
            }
        }
    }

    pub fn advance(&mut self, now: InstantMs, mut on_dirty: impl FnMut(Rect)) -> bool {
        let mut changed = false;
        for idx in 0..self.tracks.len() {
            let Some(mut track) = self.tracks[idx] else {
                continue;
            };
            let next_val = track.evaluate_at(now);
            let finished = track.is_finished_at(now);
            if next_val != track.current_val || finished {
                track.current_val = next_val;
                track.last_sample_ms = now;
                on_dirty(track.dirty_rect);
                changed = true;
            }
            if finished {
                self.tracks[idx] = None;
                self.set_override(track.key, track.prop, track.end_val);
            } else {
                self.tracks[idx] = Some(track);
            }
        }
        changed
    }

    pub fn has_active(&self) -> bool {
        self.tracks.iter().any(|t| t.is_some())
    }

    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        let mut earliest: Option<InstantMs> = None;
        for t in self.tracks.iter().flatten() {
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
        for slot in &mut self.tracks {
            if let Some(t) = slot
                && t.key == key
                && t.prop == prop
            {
                *slot = None;
            }
        }
    }

    fn set_override(&mut self, key: Key, prop: AnimProp, val: i32) {
        for slot in &mut self.overrides {
            if let Some((k, p, v)) = slot
                && *k == key
                && *p == prop
            {
                *v = val;
                return;
            }
        }
        for slot in &mut self.overrides {
            if slot.is_none() {
                *slot = Some((key, prop, val));
                return;
            }
        }
    }
}

impl Default for AnimPool {
    fn default() -> Self {
        Self::new()
    }
}
