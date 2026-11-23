use std::time::Instant;

use liuli::prelude::{InstantMs, NextWakeup, TimeError};

#[derive(Copy, Clone, Debug)]
pub struct VirtualClock {
    now: InstantMs,
}

impl VirtualClock {
    pub const fn new(start: InstantMs) -> Self {
        Self { now: start }
    }

    #[inline]
    pub const fn now(&self) -> InstantMs {
        self.now
    }

    pub fn advance_to(&mut self, target: InstantMs) -> Result<InstantMs, TimeError> {
        if target < self.now {
            return Err(TimeError::WentBackwards);
        }
        self.now = target;
        Ok(self.now)
    }

    pub fn advance_by(&mut self, delta_ms: u64) -> InstantMs {
        self.now = self.now.saturating_add(delta_ms);
        self.now
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ClockController {
    paused: bool,
    rate_x: u32,
    virtual_now: InstantMs,
    last_wall: Instant,
}

impl ClockController {
    pub fn new(start: InstantMs, wall_now: Instant) -> Self {
        Self {
            paused: false,
            rate_x: 1,
            virtual_now: start,
            last_wall: wall_now,
        }
    }

    #[inline]
    pub const fn now(&self) -> InstantMs {
        self.virtual_now
    }

    #[inline]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    #[inline]
    pub const fn rate(&self) -> u32 {
        self.rate_x
    }

    pub fn sample_wall(&mut self, wall_now: Instant) -> InstantMs {
        let elapsed_ms = wall_now
            .saturating_duration_since(self.last_wall)
            .as_millis() as u64;
        self.last_wall = wall_now;
        if !self.paused && elapsed_ms > 0 {
            self.virtual_now = self
                .virtual_now
                .saturating_add(elapsed_ms.saturating_mul(self.rate_x as u64));
        }
        self.virtual_now
    }

    pub fn pause(&mut self, wall_now: Instant) {
        if !self.paused {
            let _ = self.sample_wall(wall_now);
            self.paused = true;
        }
        self.last_wall = wall_now;
    }

    pub fn resume(&mut self, wall_now: Instant) {
        self.paused = false;
        self.last_wall = wall_now;
    }

    pub fn set_rate(&mut self, rate: u32, wall_now: Instant) -> Result<(), String> {
        if !matches!(rate, 1 | 4 | 16) {
            return Err(format!(
                "unsupported clock rate {rate}; expected 1, 4, or 16"
            ));
        }
        let _ = self.sample_wall(wall_now);
        self.rate_x = rate;
        self.last_wall = wall_now;
        Ok(())
    }

    pub fn step_ms(&mut self, delta_ms: u64, wall_now: Instant) -> InstantMs {
        self.virtual_now = self.virtual_now.saturating_add(delta_ms);
        self.last_wall = wall_now;
        self.virtual_now
    }

    pub fn jump_next_deadline(&mut self, wakeup: NextWakeup, wall_now: Instant) -> InstantMs {
        if let NextWakeup::At(target) = wakeup
            && target > self.virtual_now
        {
            self.virtual_now = target;
        }
        self.last_wall = wall_now;
        self.virtual_now
    }
}
