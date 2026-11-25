use liuli::prelude::*;
use liuli_widgets::{ButtonVariant, SurfaceSpec, ThemedUi};

const END: i32 = 1000;
const DURATIONS: [u32; 3] = [600, 1200, 2400];
const DURATIONS_UI: [(Key, TextRef); 3] = [
    (Key(410), TextRef::Static("0.6 s")),
    (Key(411), TextRef::Static("1.2 s")),
    (Key(412), TextRef::Static("2.4 s")),
];

pub(super) struct Motion {
    pub(super) tween: Tween,
    duration: usize,
}

impl Motion {
    pub(super) fn new() -> Self {
        Self {
            tween: Tween::at(0),
            duration: 1,
        }
    }

    pub(super) fn duration_ms(&self) -> u32 {
        DURATIONS[self.duration]
    }

    pub(super) fn reset(&mut self) {
        self.tween = Tween::at(0);
    }

    fn play(&mut self, now: InstantMs) {
        let position = self.tween.value(now);
        let remaining = (DURATIONS[self.duration] * (END - position) as u32).div_ceil(END as u32);
        self.tween.animate_to(now, END, remaining, Easing::Linear);
    }

    pub(super) fn action(&mut self, key: Key, now: InstantMs) -> Change {
        let playing = self.tween.value(now) != END && self.tween.running();
        match key.0 {
            406 if playing => self.tween.cancel(now),
            406 => {
                if self.tween.value(now) == END {
                    self.reset();
                }
                self.play(now);
            }
            407 => self.reset(),
            410..=412 => {
                self.duration = (key.0 - 410) as usize;
                if playing {
                    self.play(now);
                }
            }
            _ => return Change::Unchanged,
        }
        Change::View
    }

    pub(super) fn page(&self, ui: &mut ThemedUi<'_, '_, '_>) -> Result<(), BuildError> {
        let t = ui.theme();
        ui.card(Key(400), SurfaceSpec::CARD, |card| {
            for (key, label, curve, color) in [
                (401, "Playhead", Easing::Linear, t.accent),
                (403, "Fade envelope", Easing::EaseOutCubic, t.success),
            ] {
                card.caption(Key(key), TextRef::Static(label))?;
                card.custom(
                    Key(key + 1),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Px(32),
                        value: curve.apply(i64::from(self.tween.current())) as i16,
                        fg: color,
                        accent: t.border,
                        ..NodeProps::new()
                    },
                )?;
            }
            Ok(())
        })?;
        ui.row(
            Key(405),
            NodeProps {
                width: Length::Fill(1),
                height: Length::Px(t.control_height.into()),
                gap: t.spacing,
                ..NodeProps::new()
            },
            |row| {
                let label = match (self.tween.running(), self.tween.current()) {
                    (true, _) => "Pause",
                    (_, 0) => "Play",
                    (_, END) => "Replay",
                    _ => "Resume",
                };
                row.button(Key(406), label, IconKind::None, ButtonVariant::Primary)?;
                row.button(
                    Key(407),
                    "Rewind",
                    IconKind::Reset,
                    ButtonVariant::Secondary,
                )
            },
        )?;
        ui.caption(Key(408), TextRef::Static("Fade length"))?;
        ui.tabs(Key(409), &DURATIONS_UI, DURATIONS_UI[self.duration].0)
    }
}

pub(super) fn paint(props: &StoredProps, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = canvas.size();
    let marker = 16.min(size.width).min(size.height);
    if marker == 0 {
        return Ok(());
    }
    let travel = size.width - marker;
    let x = (u64::from(travel) * props.value as u64 / END as u64) as i32;
    let y = ((size.height - marker) / 2) as i32;
    let middle = (size.height / 2) as i32;
    canvas.fill(
        Rect::new((marker / 2) as i32, middle, travel, 2),
        props.accent,
    )?;
    canvas.fill(
        Rect::new((marker / 2) as i32, middle, x as u32, 2),
        props.fg,
    )?;
    let inset = marker / 8;
    canvas.fill(
        Rect::new(x + inset as i32, y, marker - 2 * inset, marker),
        props.fg,
    )?;
    canvas.fill(
        Rect::new(x, y + inset as i32, marker, marker - 2 * inset),
        props.fg,
    )
}
