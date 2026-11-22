use liuli::prelude::{InstantMs, TimeError};

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
