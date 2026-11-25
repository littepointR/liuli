use super::*;
use liuli_widgets::{ButtonVariant, Compositor, Elevation, Material, SurfaceSpec, ThemedUi};

pub struct Depth {
    pub presses: u16,
    pub turn: i32,
    motion: Tween,
    back: bool,
    pub opacity: u8,
    glass: bool,
    stripes: bool,
    floating: bool,
}
impl Depth {
    pub(super) fn new() -> Self {
        Self {
            presses: 0,
            turn: 0,
            motion: Tween::at(0),
            back: false,
            opacity: 15,
            glass: true,
            stripes: false,
            floating: true,
        }
    }
    pub(super) fn stop(&mut self, now: InstantMs) {
        self.motion.cancel(now);
        self.turn = self.motion.value(now);
    }
    pub(super) fn action(&mut self, action: UiAction, now: InstantMs) -> Change {
        match (action.key.0, action.kind) {
            (305, ActionKind::Activate) => self.presses = self.presses.wrapping_add(1),
            (306, ActionKind::Activate) => self.floating = !self.floating,
            (307, ActionKind::Activate) => {
                self.back = !self.back;
                self.motion
                    .animate_to(now, if self.back { 128 } else { 0 }, 640, Easing::Linear);
            }
            (308, ActionKind::SetValue(v)) => self.opacity = v.clamp(0, 15) as u8,
            (309, ActionKind::Activate) => self.glass = !self.glass,
            (310, ActionKind::Activate) => self.stripes = !self.stripes,
            _ => return Change::Unchanged,
        }
        Change::View
    }
}
impl Animation for Depth {
    fn advance(&mut self, now: InstantMs) -> bool {
        let changed = self.motion.advance(now);
        self.turn = self.motion.current();
        changed
    }
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        self.motion.next_wakeup(now)
    }
    fn cancel(&mut self, now: InstantMs) {
        self.stop(now);
    }
}
pub(super) fn backdrop(m: &Model, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = canvas.size();
    let colors = if m.dark {
        [
            liuli_widgets::rgb8(99, 61, 48),
            liuli_widgets::rgb8(186, 120, 70),
            liuli_widgets::rgb8(118, 80, 59),
        ]
    } else {
        [
            liuli_widgets::rgb8(201, 164, 119),
            liuli_widgets::rgb8(234, 203, 153),
            liuli_widgets::rgb8(179, 142, 110),
        ]
    };
    for x in (0..size.width).step_by(9) {
        let i = (x / 9 + u32::from(m.depth.stripes)) as usize;
        canvas.fill(Rect::new(x as i32, 0, 9, size.height), colors[i % 3])?;
    }
    Ok(())
}
pub(super) fn page(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let t = ui.theme();
    let d = &m.depth;
    let proj = Projection(if d.turn > 64 {
        (d.turn - 128) as u8
    } else {
        d.turn as u8
    });
    let elevation = if d.floating {
        Elevation::Floating
    } else {
        Elevation::Raised
    };
    let material = if d.glass {
        Material::GLASS
    } else {
        Material::Solid
    };
    let spec = SurfaceSpec {
        material,
        elevation,
        compositor: Compositor::isolated(d.opacity, proj),
        ..SurfaceSpec::CARD
    };

    ui.stack(
        Key(300),
        NodeProps {
            width: Length::Fill(1),
            height: Length::Px(152),
            ..NodeProps::new()
        },
        |stack| {
            stack.custom(
                Key(301),
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    theme_id: u16::from(d.stripes),
                    ..NodeProps::new()
                },
            )?;
            stack.column(
                Key(302),
                NodeProps {
                    padding: EdgeInsets::symmetric(18, 18),
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    ..NodeProps::new()
                },
                |inset| {
                    inset.surface_column(
                        Key(303),
                        spec,
                        NodeProps {
                            height: Length::Px(108),
                            ..t.card()
                        },
                        |card| {
                            let text = card.text_fmt(format_args!(
                                "{} / Stamp {:02}",
                                if d.turn > 64 { "B" } else { "A" },
                                d.presses
                            ))?;
                            card.title(Key(304), text)?;
                            card.button_props(
                                Key(305),
                                ButtonVariant::Primary,
                                NodeProps {
                                    width: Length::Px(88),
                                    text: TextRef::Static("Stamp"),
                                    ..t.primary_button()
                                },
                            )
                        },
                    )
                },
            )
        },
    )?;
    ui.button(
        Key(306),
        if d.floating {
            "Height / Floating"
        } else {
            "Height / Raised"
        },
        IconKind::None,
        ButtonVariant::Surface,
    )?;
    #[cfg(feature = "enhanced")]
    {
        ui.button(
            Key(307),
            "Turn sleeve",
            IconKind::Reset,
            ButtonVariant::Primary,
        )?;
        let label = ui.text_fmt(format_args!("Opacity   /   {} of 15", d.opacity))?;
        ui.caption(Key(311), label)?;
        ui.slider_range(Key(308), 0, 15, 1, i16::from(d.opacity))?;
        ui.button(
            Key(309),
            if d.glass { "Glass / On" } else { "Glass / Off" },
            IconKind::None,
            ButtonVariant::Surface,
        )?;
    }
    ui.button(
        Key(310),
        "Shift stripes",
        IconKind::None,
        ButtonVariant::Secondary,
    )?;

    // Granular Theme & Effect capability controls (below fold inside ScrollY).
    ui.caption(Key(312), TextRef::Static("Sleeve finish"))?;
    let preset_lbl = ui.text_fmt(format_args!("Preset   /   {}", m.preset.name()))?;
    ui.button_props(
        Key(313),
        ButtonVariant::Surface,
        NodeProps {
            text: preset_lbl,
            ..t.control()
        },
    )?;
    let shape_lbl = ui.text_fmt(format_args!("Shape   /   {}", m.shape_name()))?;
    ui.button_props(
        Key(314),
        ButtonVariant::Surface,
        NodeProps {
            text: shape_lbl,
            ..t.control()
        },
    )?;
    ui.button(
        Key(315),
        if m.effects.shadows {
            "Shadows / On"
        } else {
            "Shadows / Off"
        },
        IconKind::None,
        ButtonVariant::Surface,
    )?;
    ui.button(
        Key(316),
        if m.effects.press_depth {
            "Press depth / On"
        } else {
            "Press depth / Off"
        },
        IconKind::None,
        ButtonVariant::Surface,
    )?;
    #[cfg(feature = "enhanced")]
    {
        ui.button(
            Key(317),
            if m.effects.glass_tint {
                "Glass tint / On"
            } else {
                "Glass tint / Off"
            },
            IconKind::None,
            ButtonVariant::Surface,
        )?;
        let blur_lbl = ui.text_fmt(format_args!("Blur radius   /   {} px", m.blur_radius))?;
        ui.caption(Key(318), blur_lbl)?;
        ui.slider_range(Key(319), 0, 8, 1, i16::from(m.blur_radius))?;
        let tint_val = m.tint_alpha.unwrap_or(t.glass.tint_alpha);
        let tint_lbl = ui.text_fmt(format_args!("Tint alpha   /   {} of 15", tint_val))?;
        ui.caption(Key(320), tint_lbl)?;
        ui.slider_range(Key(321), 0, 15, 1, i16::from(tint_val))?;
    }
    ui.button(
        Key(322),
        if m.rounded_focus {
            "Focus ring / Rounded"
        } else {
            "Focus ring / Sharp"
        },
        IconKind::None,
        ButtonVariant::Outline,
    )
}
