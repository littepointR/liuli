use liuli::prelude::*;

/// Direction / style for [`PageTransition`].
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub enum TransitionKind {
    #[default]
    SlideLeft,
    SlideRight,
    SlideUp,
    Fade,
}

/// Non-allocating transition clock computing integer layout offsets and alpha steps.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct PageTransition {
    kind: TransitionKind,
    start_ms: InstantMs,
    duration_ms: u32,
    easing: Easing,
    active: bool,
}

impl PageTransition {
    pub const IDLE: Self = Self {
        kind: TransitionKind::SlideLeft,
        start_ms: InstantMs(0),
        duration_ms: 0,
        easing: Easing::EaseOutCubic,
        active: false,
    };

    #[inline]
    pub const fn new() -> Self {
        Self::IDLE
    }

    #[inline]
    pub const fn kind(&self) -> TransitionKind {
        self.kind
    }

    #[inline]
    pub fn start(&mut self, kind: TransitionKind, now: InstantMs, duration_ms: u32) {
        self.kind = kind;
        self.start_ms = now;
        self.duration_ms = duration_ms;
        self.active = duration_ms > 0;
    }

    #[inline]
    pub const fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    #[inline]
    pub fn is_active(&self, now: InstantMs) -> bool {
        self.active && now.0.saturating_sub(self.start_ms.0) < u64::from(self.duration_ms)
    }

    /// Returns eased progress in `0..=1000`.
    pub fn progress_per_mille(&self, now: InstantMs) -> i32 {
        if !self.active || self.duration_ms == 0 {
            return 1000;
        }
        let elapsed = now.0.saturating_sub(self.start_ms.0);
        let dur = u64::from(self.duration_ms);
        if elapsed >= dur {
            return 1000;
        }
        let raw = ((elapsed * 1000) / dur) as i64;
        self.easing.apply(raw) as i32
    }

    /// Returns signed `(dx, dy)` entry offset in pixels (`0, 0` once settled).
    pub fn offset_for(&self, screen: Size, now: InstantMs) -> (i16, i16) {
        let p = self.progress_per_mille(now);
        let rem = 1000 - p;
        let w = (screen.width as i64 * i64::from(rem)) / 1000;
        let h = (screen.height as i64 * i64::from(rem)) / 1000;
        let clamped_w = w.clamp(0, i64::from(i16::MAX)) as i16;
        let clamped_h = h.clamp(0, i64::from(i16::MAX)) as i16;
        match self.kind {
            TransitionKind::SlideLeft => (clamped_w, 0),
            TransitionKind::SlideRight => (-clamped_w, 0),
            TransitionKind::SlideUp => (0, clamped_h),
            TransitionKind::Fade => (0, 0),
        }
    }

    /// Returns effective 4-bit opacity (`3..=15`) while active (`15` once settled).
    pub fn alpha_for(&self, now: InstantMs) -> u8 {
        match self.kind {
            TransitionKind::Fade if self.is_active(now) => {
                let p = self.progress_per_mille(now);
                (3 + ((p * 12 + 500) / 1000)).clamp(3, 15) as u8
            }
            _ => 15,
        }
    }

    #[inline]
    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        if !self.is_active(now) {
            NextWakeup::OnEvent
        } else {
            NextWakeup::At(now.saturating_add(16))
        }
    }

    /// Apply computed layout transition offset directly to a declared view node.
    pub fn apply(
        &self,
        ui: &mut Children<'_, '_>,
        target_key: Key,
        screen: Size,
        now: InstantMs,
    ) -> Result<(), BuildError> {
        let (dx, dy) = self.offset_for(screen, now);
        ui.set_offset(target_key, dx, dy)
    }
}

impl Default for PageTransition {
    fn default() -> Self {
        Self::IDLE
    }
}
