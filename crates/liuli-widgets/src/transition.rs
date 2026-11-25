use liuli::prelude::*;

/// The output of a transition mapping. Opacity is 0..=15, a full Y turn is 256 units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionFrame {
    pub layout: LayoutMotion,
    pub opacity: u8,
    pub turn: u8,
}
impl TransitionFrame {
    pub const IDENTITY: Self = Self {
        layout: LayoutMotion::IDENTITY,
        opacity: 15,
        turn: 0,
    };
    /// Call after declaring and decorating the target. Layout motion also moves its hit region.
    pub fn apply(self, ui: &mut Children<'_, '_>, key: Key) -> Result<(), BuildError> {
        ui.set_layout_motion(key, self.layout)?;
        ui.animate_layer(key, self.opacity, self.turn)
    }
}

/// Theme-level timing, preset and curve for an entrance, exit or page change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionStyle {
    pub kind: TransitionKind,
    pub duration_ms: u32,
    pub curve: Curve,
}
impl TransitionStyle {
    pub const fn new(kind: TransitionKind, duration_ms: u32, easing: Easing) -> Self {
        Self {
            kind,
            duration_ms,
            curve: Curve::Builtin(easing),
        }
    }
    pub const fn with_kind(self, kind: TransitionKind) -> Self {
        Self { kind, ..self }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionRole {
    Page,
    Entrance,
    Exit,
}

/// Presets can also be sampled directly for custom drawing.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub enum TransitionKind {
    #[default]
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Fade,
    FloatUp,
    FloatDown,
    FloatLeft,
    FloatRight,
    ZoomIn,
    ZoomOut,
    Pop,
    Pulse,
    Shake,
    Bounce,
    Flip,
}

impl TransitionKind {
    pub const ALL: [Self; 16] = [
        Self::SlideLeft,
        Self::SlideRight,
        Self::SlideUp,
        Self::SlideDown,
        Self::Fade,
        Self::FloatUp,
        Self::FloatDown,
        Self::FloatLeft,
        Self::FloatRight,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::Pop,
        Self::Pulse,
        Self::Shake,
        Self::Bounce,
        Self::Flip,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::SlideLeft => "Slide left",
            Self::SlideRight => "Slide right",
            Self::SlideUp => "Slide up",
            Self::SlideDown => "Slide down",
            Self::Fade => "Fade",
            Self::FloatUp => "Float up",
            Self::FloatDown => "Float down",
            Self::FloatLeft => "Float left",
            Self::FloatRight => "Float right",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::Pop => "Pop",
            Self::Pulse => "Pulse",
            Self::Shake => "Shake",
            Self::Bounce => "Bounce",
            Self::Flip => "Flip",
        }
    }
    pub fn frame(self, progress: i16) -> TransitionFrame {
        let p = i32::from(progress);
        let rem = (1000 - p).clamp(-32767, 32767) as i16;
        let mut f = TransitionFrame::IDENTITY;
        let fade = (3 + (12 * p + 500) / 1000).clamp(0, 15) as u8;
        let t = p.clamp(0, 1000);
        match self {
            Self::SlideLeft => f.layout.x = rem,
            Self::SlideRight => f.layout.x = -rem,
            Self::SlideUp => f.layout.y = rem,
            Self::SlideDown => f.layout.y = -rem,
            Self::Fade => {
                f.opacity = fade;
                if !SurfaceStyle::has_layers_support() {
                    f.layout.grow = -rem / 8;
                }
            }
            Self::FloatUp | Self::FloatDown | Self::FloatLeft | Self::FloatRight => {
                f.opacity = fade;
                match self {
                    Self::FloatUp => f.layout.y = rem / 4,
                    Self::FloatDown => f.layout.y = -rem / 4,
                    Self::FloatLeft => f.layout.x = rem / 4,
                    _ => f.layout.x = -rem / 4,
                }
            }
            Self::ZoomIn => f.layout.grow = -rem / 2,
            Self::ZoomOut => f.layout.grow = rem / 2,
            Self::Pop => f.layout.grow = ((Easing::EaseOutBack.apply(t.into()) - 1000) / 2) as i16,
            Self::Pulse => f.layout.grow = (t * (1000 - t) / 1250) as i16,
            Self::Bounce => {
                f.layout.y = ((Easing::EaseOutBounce.apply(t.into()) - 1000) / 3) as i16
            }
            Self::Shake => {
                // Four alternating excursions, tapering to rest.
                let phase = t * 8 % 2000;
                let wave = if phase < 500 {
                    phase
                } else if phase < 1500 {
                    1000 - phase
                } else {
                    phase - 2000
                };
                f.layout.x = (wave * (1000 - t) / 4000) as i16;
            }
            Self::Flip => {
                f.turn = (i32::from(rem) * 60 / 1000) as u8;
                if !SurfaceStyle::has_layers_support() {
                    f.layout.grow = -rem / 2;
                }
            }
        }
        f
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PageTransition {
    kind: TransitionKind,
    clock: Playback,
    curve: Curve,
    mapping: Option<fn(i16) -> TransitionFrame>,
}
impl PartialEq for PageTransition {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.clock == other.clock
            && self.curve == other.curve
            && match (self.mapping, other.mapping) {
                (Some(a), Some(b)) => core::ptr::fn_addr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
impl Eq for PageTransition {}
impl PageTransition {
    pub const IDLE: Self = Self {
        kind: TransitionKind::SlideLeft,
        clock: Playback::IDLE,
        curve: Curve::Builtin(Easing::EaseOutCubic),
        mapping: None,
    };
    pub const fn new() -> Self {
        Self::IDLE
    }
    pub const fn kind(&self) -> TransitionKind {
        self.kind
    }
    pub const fn with_easing(mut self, easing: Easing) -> Self {
        self.curve = Curve::Builtin(easing);
        self
    }
    pub const fn with_curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }
    /// Apply a live preference change to an already playing transition.
    pub fn apply_motion(&mut self, motion: crate::Motion, now: InstantMs) {
        if motion.reduced {
            self.clock.finish(now);
        }
    }
    pub fn play_styled(
        &mut self,
        motion: crate::Motion,
        role: TransitionRole,
        local: Option<TransitionStyle>,
        now: InstantMs,
    ) {
        let spec = local.unwrap_or(match role {
            TransitionRole::Page => motion.page,
            TransitionRole::Entrance => motion.entrance,
            TransitionRole::Exit => motion.exit,
        });
        self.curve = spec.curve;
        let timing = Timing {
            reverse: role == TransitionRole::Exit,
            ..Timing::once(if motion.reduced { 0 } else { spec.duration_ms })
        };
        self.play(spec.kind, now, timing);
    }

    pub fn start(&mut self, kind: TransitionKind, now: InstantMs, duration_ms: u32) {
        self.play(kind, now, Timing::once(duration_ms));
    }
    pub fn play(&mut self, kind: TransitionKind, now: InstantMs, timing: Timing) {
        self.kind = kind;
        self.mapping = None;
        self.clock.play(now, timing);
    }
    pub fn play_custom(&mut self, map: fn(i16) -> TransitionFrame, now: InstantMs, timing: Timing) {
        self.mapping = Some(map);
        self.clock.play(now, timing);
    }
    fn frame_at_progress(&self, p: u16) -> TransitionFrame {
        let eased = self.curve.sample(p);
        match self.mapping {
            Some(map) => map(eased),
            None => self.kind.frame(eased),
        }
    }
    /// Most recent engine sample. Suitable for declaring the whole animated subtree.
    pub fn frame(&self) -> TransitionFrame {
        self.frame_at_progress(self.clock.progress())
    }
    pub fn is_active(&self, now: InstantMs) -> bool {
        !self.clock.finished_at(now)
    }
    pub fn progress_per_mille(&self, now: InstantMs) -> i32 {
        self.curve.sample(self.clock.progress_at(now)).into()
    }
    pub fn offset_for(&self, screen: Size, now: InstantMs) -> (i16, i16) {
        let m = self.frame_at_progress(self.clock.progress_at(now)).layout;
        let px = |fraction, extent| {
            (i64::from(fraction) * i64::from(extent) / 1000).clamp(i16::MIN.into(), i16::MAX.into())
                as i16
        };
        (px(m.x, screen.width), px(m.y, screen.height))
    }
    pub fn alpha_for(&self, now: InstantMs) -> u8 {
        self.frame_at_progress(self.clock.progress_at(now)).opacity
    }
    pub fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        self.clock
            .next_sample_at(now)
            .map_or(NextWakeup::OnEvent, NextWakeup::At)
    }
    /// Compatibility with callers supplying a pixel viewport. For all properties use frame().apply().
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
impl Animation for PageTransition {
    fn advance(&mut self, now: InstantMs) -> bool {
        self.clock.advance(now)
    }
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        self.clock.next_wakeup(now)
    }
    fn cancel(&mut self, now: InstantMs) {
        self.clock.cancel(now);
    }
}
impl Default for PageTransition {
    fn default() -> Self {
        Self::IDLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_roles_resolve_local_curves_and_finish_in_the_correct_direction() {
        let mut motion = crate::Motion::SNAPPY;
        motion.exit = TransitionStyle::new(TransitionKind::SlideDown, 200, Easing::Linear);
        let mut t = PageTransition::new();
        t.play_styled(motion, TransitionRole::Exit, None, InstantMs(10));
        assert_eq!(t.frame(), TransitionFrame::IDENTITY);
        t.advance(InstantMs(110));
        assert_eq!(t.frame().layout.y, -500);
        t.apply_motion(motion.with_reduced(true), InstantMs(111));
        assert_eq!(t.frame().layout.y, -1000);
        assert_eq!(t.next_wakeup(InstantMs(111)), NextWakeup::OnEvent);
        let local = TransitionStyle {
            kind: TransitionKind::FloatLeft,
            curve: Curve::Custom(|_| 250),
            duration_ms: 800,
        };
        t.play_styled(
            motion,
            TransitionRole::Entrance,
            Some(local),
            InstantMs(120),
        );
        t.advance(InstantMs(170));
        assert_eq!(t.frame(), TransitionKind::FloatLeft.frame(250));
        t.play_styled(
            crate::Motion::REDUCED,
            TransitionRole::Entrance,
            Some(local),
            InstantMs(200),
        );
        assert_eq!(t.frame(), TransitionFrame::IDENTITY);
        assert_eq!(t.next_wakeup(InstantMs(200)), NextWakeup::OnEvent);
    }

    #[test]
    fn custom_curve_and_mapping_share_the_clock_and_freeze_at_event_time() {
        fn lift(p: i16) -> TransitionFrame {
            TransitionFrame {
                layout: LayoutMotion {
                    y: (1000 - p) / 2,
                    ..LayoutMotion::IDENTITY
                },
                opacity: (i32::from(p) * 15 / 1000) as u8,
                turn: (p / 20) as u8,
            }
        }
        let mut t = PageTransition::new()
            .with_curve(Curve::Custom(|p| (u32::from(p).pow(2) / 1000) as i16));
        t.play_custom(lift, InstantMs(10), Timing::once(100));
        t.advance(InstantMs(50));
        assert_eq!(t.frame(), lift(160));
        assert_eq!(t.offset_for(Size::new(300, 100), InstantMs(50)), (0, 42));
        t.cancel(InstantMs(60));
        assert_eq!(t.frame(), lift(250));
        assert_eq!(t.next_wakeup(InstantMs(70)), NextWakeup::OnEvent);
        t.start(TransitionKind::SlideDown, InstantMs(100), 0);
        assert_eq!(t.frame(), TransitionFrame::IDENTITY);
    }
}
