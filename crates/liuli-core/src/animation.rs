//! Deterministic, allocation-free animation sampled against the engine's monotonic clock.
use crate::time::{ANIM_SAMPLE_INTERVAL_MS, Easing, InstantMs, NextWakeup, ScrollPhysics};

/// A model-owned animation registered by stable widget key in `App::animations`.
pub trait Animation {
    fn advance(&mut self, now: InstantMs) -> bool;
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup;
    fn cancel(&mut self, now: InstantMs);
}

impl Animation for ScrollPhysics {
    fn advance(&mut self, now: InstantMs) -> bool {
        ScrollPhysics::advance(self, now)
    }
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        ScrollPhysics::next_wakeup(self, now)
    }
    fn cancel(&mut self, now: InstantMs) {
        self.release(now);
        self.set_offset(self.offset().clamp(self.min_offset, self.max_offset));
    }
}

/// Normalized time is in thousandths. Interior values may overshoot for spring-like motion.
#[derive(Clone, Copy, Debug)]
pub enum Curve {
    Builtin(Easing),
    Bezier(CubicBezier),
    Custom(fn(u16) -> i16),
    Samples(&'static [i16; 17]),
}

impl Curve {
    pub fn sample(self, time: u16) -> i16 {
        match time {
            0 => 0,
            1000.. => 1000,
            t => match self {
                Self::Builtin(e) => e.apply(t.into()) as i16,
                Self::Bezier(b) => b.sample(t),
                Self::Custom(f) => f(t),
                Self::Samples(points) => StyleCurve::Samples(points).sample(t),
            },
        }
    }
}
impl From<Easing> for Curve {
    fn from(e: Easing) -> Self {
        Self::Builtin(e)
    }
}
impl PartialEq for Curve {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Builtin(a), Self::Builtin(b)) => a == b,
            (Self::Bezier(a), Self::Bezier(b)) => a == b,
            (Self::Samples(a), Self::Samples(b)) => a == b,
            (Self::Custom(a), Self::Custom(b)) => core::ptr::fn_addr_eq(a, b),
            _ => false,
        }
    }
}
impl Eq for Curve {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CubicBezier {
    x1: u16,
    y1: i16,
    x2: u16,
    y2: i16,
}
impl CubicBezier {
    /// X control points must be within 0..=1000. Y may overshoot.
    pub const fn new(x1: u16, y1: i16, x2: u16, y2: i16) -> Option<Self> {
        if x1 > 1000 || x2 > 1000 {
            return None;
        }
        Some(Self { x1, y1, x2, y2 })
    }
    fn sample(self, time: u16) -> i16 {
        // Invert x(t) before evaluating y(t). Fixed-point bisection also handles flat tangents.
        fn axis(t: i64, a: i64, b: i64) -> i64 {
            let u = 4096 - t;
            (3 * u * u * t * a + 3 * u * t * t * b + t * t * t * 1000) / (4096_i64.pow(3))
        }
        let (mut lo, mut hi) = (0, 4096);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if axis(mid, self.x1.into(), self.x2.into()) < time.into() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        axis(lo, self.y1.into(), self.y2.into()) as i16
    }
}

/// A flash-resident curve for declarative widget styles. Samples are equally spaced in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleCurve {
    Bezier(CubicBezier),
    Samples(&'static [i16; 17]),
}
impl StyleCurve {
    pub fn sample(self, time: u16) -> i16 {
        match time {
            0 => 0,
            1000.. => 1000,
            t => match self {
                Self::Bezier(curve) => curve.sample(t),
                Self::Samples(points) => {
                    let position = usize::from(t) * 16;
                    let i = position / 1000;
                    let a = if i == 0 { 0 } else { points[i] };
                    let b = if i == 15 { 1000 } else { points[i + 1] };
                    interpolate(a.into(), b.into(), (position % 1000) as i16) as i16
                }
            },
        }
    }
}
impl From<StyleCurve> for Curve {
    fn from(curve: StyleCurve) -> Self {
        match curve {
            StyleCurve::Bezier(b) => Self::Bezier(b),
            StyleCurve::Samples(s) => Self::Samples(s),
        }
    }
}

pub(crate) fn interpolate(from: i32, to: i32, progress: i16) -> i32 {
    (i64::from(from) + (i64::from(to) - i64::from(from)) * i64::from(progress) / 1000)
        .clamp(i32::MIN.into(), i32::MAX.into()) as i32
}

/// A key's curve leads from this value to the next. Times are normalized to 0..=1000.
#[derive(Clone, Copy, Debug)]
pub struct Keyframe {
    pub at: u16,
    pub value: i32,
    pub curve: Curve,
}
impl Keyframe {
    pub const fn new(at: u16, value: i32, curve: Curve) -> Self {
        Self { at, value, curve }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Keyframes<'a>(&'a [Keyframe]);
impl<'a> Keyframes<'a> {
    pub fn new(keys: &'a [Keyframe]) -> Option<Self> {
        if keys.first()?.at != 0
            || keys.last()?.at != 1000
            || keys.windows(2).any(|w| w[0].at >= w[1].at)
        {
            return None;
        }
        Some(Self(keys))
    }
    pub fn sample(&self, time: u16) -> i32 {
        let idx = self.0.partition_point(|k| k.at <= time) - 1;
        let start = self.0[idx];
        let Some(end) = self.0.get(idx + 1) else {
            return start.value;
        };
        let t = u32::from(time - start.at) * 1000 / u32::from(end.at - start.at);
        interpolate(start.value, end.value, start.curve.sample(t as u16))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Times(core::num::NonZeroU16),
    Forever,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub repeat: Repeat,
    /// Start at the last keyframe, e.g. to play an entry effect as an exit.
    pub reverse: bool,
    /// Reverse direction on alternating traversals.
    pub alternate: bool,
}
impl Timing {
    pub const fn once(duration_ms: u32) -> Self {
        Self {
            duration_ms,
            delay_ms: 0,
            repeat: Repeat::Once,
            reverse: false,
            alternate: false,
        }
    }
}

/// One clock for any number of parallel properties. Sample keyframes or a custom mapping
/// with its progress. Sleeping through frames still lands on the correct traversal and endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Playback {
    timing: Timing,
    begins: InstantMs,
    next: Option<InstantMs>,
    progress: u16,
}
impl Playback {
    pub const IDLE: Self = Self {
        timing: Timing::once(0),
        begins: InstantMs(0),
        next: None,
        progress: 1000,
    };
    pub fn play(&mut self, now: InstantMs, timing: Timing) {
        self.timing = timing;
        self.begins = now.saturating_add(timing.delay_ms.into());
        self.next = Some(self.begins);
        self.progress = if timing.reverse { 1000 } else { 0 };
        self.advance(now);
    }
    pub const fn progress(&self) -> u16 {
        self.progress
    }
    pub const fn running(&self) -> bool {
        self.next.is_some()
    }
    pub(crate) const fn end_once(&self) -> InstantMs {
        self.begins.saturating_add(self.timing.duration_ms as u64)
    }
    fn position(&self, now: InstantMs) -> (u16, Option<InstantMs>) {
        if now < self.begins {
            return (
                if self.timing.reverse { 1000 } else { 0 },
                Some(self.begins),
            );
        }
        let dt = now.saturating_sub(self.begins);
        let dur = u64::from(self.timing.duration_ms);
        let count = match self.timing.repeat {
            Repeat::Once => Some(1),
            Repeat::Times(n) => Some(u64::from(n.get())),
            Repeat::Forever => None,
        };
        if dur == 0 || count.is_some_and(|n| dt >= n * dur) || now.0 == u64::MAX {
            let reverse =
                self.timing.reverse ^ (self.timing.alternate && count.is_some_and(|n| n % 2 == 0));
            return (if reverse { 0 } else { 1000 }, None);
        }
        let within = dt % dur;
        let p = (within * 1000 / dur) as u16;
        let reverse = self.timing.reverse ^ (self.timing.alternate && dt / dur % 2 == 1);
        (
            if reverse { 1000 - p } else { p },
            Some(now.saturating_add(ANIM_SAMPLE_INTERVAL_MS.min(dur - within))),
        )
    }
    /// Finish a finite animation, or the current traversal of a repeating animation.
    pub fn finish(&mut self, now: InstantMs) {
        if !self.running() {
            return;
        }
        let odd = match self.timing.repeat {
            Repeat::Once => false,
            Repeat::Times(n) => n.get() % 2 == 0,
            Repeat::Forever => {
                now.saturating_sub(self.begins) / u64::from(self.timing.duration_ms.max(1)) % 2 == 1
            }
        };
        self.progress = if self.timing.reverse ^ (self.timing.alternate && odd) {
            0
        } else {
            1000
        };
        self.next = None;
    }

    pub fn progress_at(&self, now: InstantMs) -> u16 {
        if self.running() {
            self.position(now).0
        } else {
            self.progress
        }
    }
    pub fn finished_at(&self, now: InstantMs) -> bool {
        !self.running() || self.position(now).1.is_none()
    }
    /// Next absolute sampling time for callers driving an analytical clock manually.
    pub fn next_sample_at(&self, now: InstantMs) -> Option<InstantMs> {
        if self.running() {
            self.position(now).1
        } else {
            None
        }
    }
}
impl Default for Playback {
    fn default() -> Self {
        Self::IDLE
    }
}
impl Animation for Playback {
    fn advance(&mut self, now: InstantMs) -> bool {
        if self.next.is_none_or(|due| now < due) {
            return false;
        }
        let (p, next) = self.position(now);
        let changed = self.progress != p || next.is_none();
        self.progress = p;
        self.next = next;
        changed
    }
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        match self.next {
            None => NextWakeup::OnEvent,
            Some(due) if due <= now => NextWakeup::Now,
            Some(due) => NextWakeup::At(due),
        }
    }
    fn cancel(&mut self, now: InstantMs) {
        self.progress = self.progress_at(now);
        self.next = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        effects::PressDepth,
        view::{NodeProps, StoredProps},
    };

    #[test]
    fn curves_retain_their_id_and_custom_beziers_invert_time() {
        for e in Easing::ALL {
            assert_eq!((e.apply(-1), e.apply(1001)), (0, 1000));
            let p = NodeProps {
                anim_easing: e,
                ..NodeProps::new()
            };
            assert_eq!(StoredProps::from_props(p).anim_easing(), e);
            assert_eq!(PressDepth::custom(120, 4, e).easing(), e);
        }
        assert!(Easing::EaseOutBack.apply(600) > 1000);
        assert!(CubicBezier::new(1001, 0, 30, 500).is_none());
        // x(t)=t^3, y(t)=1-(1-t)^3. x=1/8 corresponds to t=1/2.
        assert_eq!(
            Curve::Bezier(CubicBezier::new(0, 1000, 0, 1000).unwrap()).sample(125),
            875
        );
        let custom = Curve::Custom(|_| 1300);
        assert_eq!(
            (custom.sample(0), custom.sample(50), custom.sample(1000)),
            (0, 1300, 1000)
        );
        assert_eq!(interpolate(i32::MIN, i32::MAX, 1300), i32::MAX);
    }

    #[test]
    fn delayed_reversed_pairs_skip_cycles_and_deliver_the_last_frame_once() {
        let mut clock = Playback::IDLE;
        clock.play(
            InstantMs(4),
            Timing {
                duration_ms: 50,
                delay_ms: 20,
                reverse: true,
                alternate: true,
                repeat: Repeat::Times(core::num::NonZeroU16::new(4).unwrap()),
            },
        );
        assert_eq!(clock.progress(), 1000);
        assert_eq!(
            clock.next_wakeup(InstantMs(4)),
            NextWakeup::At(InstantMs(24))
        );
        assert!(!clock.advance(InstantMs(23)));
        assert_eq!(clock.progress_at(InstantMs(74)), 0);
        assert!(clock.advance(InstantMs(149)));
        assert_eq!(clock.progress(), 500);
        assert!(clock.advance(InstantMs(223)));
        assert_eq!(
            clock.next_wakeup(InstantMs(223)),
            NextWakeup::At(InstantMs(224))
        );
        assert!(clock.advance(InstantMs(300)));
        assert_eq!(clock.progress(), 1000);
        assert!(!clock.advance(InstantMs(301)));
        assert_eq!(clock.next_wakeup(InstantMs(301)), NextWakeup::OnEvent);
        clock.play(
            InstantMs(400),
            Timing {
                repeat: Repeat::Forever,
                ..Timing::once(100)
            },
        );
        clock.cancel(InstantMs(725));
        assert_eq!(clock.progress_at(InstantMs(900)), 250);
        clock.play(
            InstantMs(910),
            Timing {
                repeat: Repeat::Forever,
                ..Timing::once(0)
            },
        );
        assert!(!clock.running());
    }

    #[test]
    fn parallel_tracks_share_time_and_invalid_key_order_is_rejected() {
        let linear = Curve::Builtin(Easing::Linear);
        let x = [
            Keyframe::new(0, 0, linear),
            Keyframe::new(500, 100, linear),
            Keyframe::new(1000, 100, linear),
        ];
        let alpha = [
            Keyframe::new(0, 0, linear),
            Keyframe::new(500, 0, linear),
            Keyframe::new(1000, 14, linear),
        ];
        let x = Keyframes::new(&x).unwrap();
        let a = Keyframes::new(&alpha).unwrap();
        for (t, xy) in [
            (250, (50, 0)),
            (500, (100, 0)),
            (750, (100, 7)),
            (1001, (100, 14)),
        ] {
            assert_eq!((x.sample(t), a.sample(t)), xy);
        }
        assert!(Keyframes::new(&[alpha[0], alpha[0], alpha[2]]).is_none());
        assert!(Keyframes::new(&alpha[1..]).is_none());
        assert!(Keyframes::new(&[]).is_none());
    }
}
