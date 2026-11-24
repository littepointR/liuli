use super::*;
use core::num::NonZeroU16;
use liuli_widgets::{confirm_dialog, number_editor, selector, tabs, toast, virtual_list};

pub(super) const LIST: Key = Key(12);
const ICONS: &[(IconKind, &str)] = &[
    (IconKind::ChevronRight, "Next"),
    (IconKind::Check, "Check"),
    (IconKind::Warning, "Warning"),
    (IconKind::Sliders, "Sliders"),
    (IconKind::Bell, "Bell"),
    (IconKind::Moon, "Moon"),
    (IconKind::Thermometer, "Temperature"),
    (IconKind::Reset, "Reset"),
    (IconKind::Gauge, "Gauge"),
    (IconKind::Waveform, "Waveform"),
    (IconKind::Diagnostics, "Diagnostics"),
];
fn wide(height: Length) -> NodeProps {
    NodeProps {
        width: Length::Fill(1),
        height,
        ..NodeProps::new()
    }
}
fn label(
    ui: &mut Children<'_, '_>,
    key: u32,
    text: TextRef,
    color: Rgb565,
) -> Result<(), BuildError> {
    ui.label(
        Key(key),
        NodeProps {
            text,
            fg: color,
            ..wide(Length::Px(25))
        },
    )
}
fn button(
    ui: &mut Children<'_, '_>,
    key: u32,
    name: &'static str,
    icon: IconKind,
    t: Theme,
) -> Result<(), BuildError> {
    ui.button(
        Key(key),
        NodeProps {
            text: TextRef::Static(name),
            icon,
            ..t.control()
        },
    )
}
pub(super) fn view(m: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
    let t = m.theme();
    let full = wide(Length::Fill(1));
    let viewport = m.height.saturating_sub(66);
    out.root(
        Key(1),
        NodeProps {
            bg: Some(t.background),
            ..full
        },
        |root| {
            root.column(
                Key(2),
                NodeProps {
                    padding: EdgeInsets::all(10),
                    gap: 6,
                    align_cross: Align::Stretch,
                    ..full
                },
                |ui| {
                    ui.row(Key(7), wide(Length::Px(40)), |bar| {
                        let title = PAGES
                            .iter()
                            .find(|p| p.0 == m.page)
                            .map_or("Widget Demo", |p| p.1);
                        label(bar, 8, TextRef::Static(title), t.accent)?;
                        let home = m.page == Page::Home;
                        bar.button(
                            if home { Key(5) } else { Key(3) },
                            NodeProps {
                                text: TextRef::Static(if home { "Theme" } else { "Back" }),
                                height: Length::Px(34),
                                width: Length::Px(64),
                                ..t.control()
                            },
                        )
                    })?;
                    if m.page == Page::List {
                        return virtual_list(
                            ui,
                            [LIST, Key(13), Key(14)],
                            1000,
                            NonZeroU16::new(38).unwrap(),
                            m.offset,
                            viewport,
                            |rows, id| {
                                let key = Key(1000 + id);
                                let text = rows.text_fmt(format_args!("Record {:04}", id + 1))?;
                                rows.button(
                                    key,
                                    NodeProps {
                                        text,
                                        icon: if m.selected == Some(key) {
                                            IconKind::Check
                                        } else {
                                            IconKind::None
                                        },
                                        ..t.control()
                                    },
                                )
                            },
                        );
                    }
                    ui.scroll_y(Key(9), wide(Length::Px(viewport.into())), |scroll| {
                        scroll.column(
                            Key(10),
                            NodeProps {
                                gap: t.spacing,
                                align_cross: Align::Stretch,
                                ..wide(Length::Content)
                            },
                            |body| contents(body, m, t),
                        )
                    })
                },
            )?;
            if m.confirm {
                confirm_dialog(
                    root,
                    [Key(61), Key(62), Key(63), Key(64)],
                    [TextRef::Static("Confirm"), TextRef::Static("Cancel")],
                    t,
                )?;
            }
            Ok(())
        },
    )
}
fn contents(ui: &mut Children<'_, '_>, m: &Model, t: Theme) -> Result<(), BuildError> {
    match m.page {
        Page::Home => {
            for (i, &(_, name, icon)) in PAGES.iter().enumerate() {
                button(ui, 100 + i as u32, name, icon, t)?;
            }
        }
        Page::Inputs => {
            let tally = ui.text_fmt(format_args!("Button   /   {} presses", m.count))?;
            label(ui, 20, tally, t.text)?;
            ui.button(
                Key(30),
                NodeProps {
                    text: TextRef::Static("Press"),
                    enabled: m.enabled,
                    ..t.control()
                },
            )?;
            label(ui, 21, TextRef::Static("Toggle"), t.text)?;
            ui.toggle(
                Key(31),
                NodeProps {
                    checked: m.enabled,
                    text: TextRef::Static("Enable button"),
                    ..t.control()
                },
            )?;
            let level = ui.text_fmt(format_args!("Slider   /   {}%", m.value))?;
            label(ui, 22, level, t.text)?;
            ui.slider(
                Key(32),
                NodeProps {
                    value: m.value,
                    ..t.control()
                },
            )?;
        }
        Page::Data => {
            let level = ui.text_fmt(format_args!("Progress   /   {}%", m.value))?;
            label(ui, 20, level, t.text)?;
            let display = NodeProps {
                value: m.value,
                bg: Some(t.surface),
                accent: t.accent,
                fg: t.text,
                ..wide(Length::Px(32))
            };
            ui.progress_bar(Key(41), display)?;
            label(ui, 21, TextRef::Static("Sparkline"), t.text)?;
            ui.sparkline_ring(
                Key(42),
                &m.samples,
                NodeProps {
                    height: Length::Px(92),
                    ..display
                },
            )?;
            label(ui, 22, TextRef::Static("Custom meter"), t.text)?;
            ui.custom(
                Key(43),
                NodeProps {
                    height: Length::Px(16),
                    ..display
                },
            )?;
            button(ui, 44, "Add sample", IconKind::Waveform, t)?;
        }
        Page::Choices => {
            label(ui, 20, TextRef::Static("Tabs"), t.text)?;
            tabs(
                ui,
                Key(49),
                &[
                    (Key(50), TextRef::Static("Mode")),
                    (Key(51), TextRef::Static("Value")),
                ],
                m.tab,
                t,
            )?;
            if m.tab == Key(50) {
                label(ui, 21, TextRef::Static("Selector"), t.text)?;
                selector(
                    ui,
                    Key(52),
                    &[
                        (Key(53), TextRef::Static("Quiet")),
                        (Key(54), TextRef::Static("Auto")),
                        (Key(55), TextRef::Static("Loud")),
                    ],
                    m.choice,
                    t,
                )?;
            } else {
                let text = ui.text_fmt(format_args!("Numeric editor   /   {}", m.value))?;
                label(ui, 21, text, t.text)?;
                number_editor(ui, Key(57), 0..=100, m.value, 5, t)?;
            }
        }
        Page::Feedback => {
            button(ui, 60, "Open dialog", IconKind::Warning, t)?;
            button(ui, 65, "Show toast", IconKind::Bell, t)?;
            if m.toast_until.is_some() {
                toast(ui, Key(66), TextRef::Static("Saved"), t)?;
            }
        }
        Page::List => {}
        Page::Icons => {
            let selected = ui.text_fmt(format_args!("Selected   /   {}", ICONS[m.icon].1))?;
            label(ui, 20, selected, t.text)?;
            for (row, pair) in ICONS.chunks(2).enumerate() {
                ui.row(
                    Key(220 + row as u32),
                    NodeProps {
                        gap: t.spacing,
                        ..wide(Length::Px(t.control_height.into()))
                    },
                    |r| {
                        for (col, &(icon, name)) in pair.iter().enumerate() {
                            let i = 2 * row + col;
                            r.button(
                                Key(200 + i as u32),
                                NodeProps {
                                    icon,
                                    text: TextRef::Static(name),
                                    border_width: 1,
                                    border_color: (i == m.icon).then_some(t.accent),
                                    ..t.control()
                                },
                            )?;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        Page::Image => {
            ui.stack(
                Key(70),
                NodeProps {
                    bg: Some(t.surface),
                    ..wide(Length::Px(160))
                },
                |layers| {
                    layers.custom(
                        Key(71),
                        NodeProps {
                            theme_id: u16::from(m.angle) | ((m.scale as u16) << 8),
                            value: m.alpha,
                            ..wide(Length::Fill(1))
                        },
                    )
                },
            )?;
            button(ui, 72, "Rotate", IconKind::Reset, t)?;
            let scale = ui.text_fmt(format_args!("Scale   /   {}%", m.scale))?;
            label(ui, 21, scale, t.text)?;
            ui.slider(
                Key(74),
                NodeProps {
                    min: 20,
                    max: 100,
                    value: m.scale,
                    ..t.control()
                },
            )?;
            let alpha = ui.text_fmt(format_args!("Opacity   /   {} of 15", m.alpha))?;
            label(ui, 20, alpha, t.text)?;
            ui.slider(
                Key(73),
                NodeProps {
                    max: 15,
                    value: m.alpha,
                    ..t.control()
                },
            )?;
        }
        #[cfg(feature = "enhanced")]
        Page::Text => {
            ui.custom(
                Key(80),
                NodeProps {
                    focusable: true,
                    clickable: true,
                    theme_id: m.field.revision(),
                    ..wide(Length::Px(52))
                },
            )?;
            button(ui, 81, "Compose", IconKind::Sliders, t)?;
            button(ui, 82, "Commit", IconKind::Check, t)?;
            button(ui, 83, "Cancel preedit", IconKind::Reset, t)?;
            button(ui, 84, "Backspace", IconKind::ChevronRight, t)?;
        }
    }
    Ok(())
}

pub(super) fn paint_image(m: &Model, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    // A compass needle shows both the rotation angle and four-bit opacity.
    const N: u16 = 0x20e5;
    const A: u16 = 0xee0e;
    const B: u16 = 0xef5b;
    static NEEDLE: liuli::core::bitmap::Bitmap = liuli::core::bitmap::Bitmap::new(
        5,
        5,
        &[
            N, N, A, N, N, N, A, A, A, N, N, N, B, N, N, N, N, B, N, N, N, N, B, N, N,
        ],
    )
    .unwrap();
    let size = c.size();
    let side = size.width.min(size.height) * m.scale as u32 / 100;
    c.bitmap(
        Rect::new(
            ((size.width - side) / 2) as i32,
            ((size.height - side) / 2) as i32,
            side,
            side,
        ),
        &NEEDLE,
        m.angle,
        m.alpha as u8,
    )
}

#[cfg(feature = "enhanced")]
pub(super) fn text_field() -> liuli::enhanced::field::TextField<'static> {
    use liuli::enhanced::{
        ShapingFont,
        field::{FieldStyle, TextField},
    };
    let faces = [
        &include_bytes!("../assets/example.ttf")[..],
        &include_bytes!("../assets/Arabic.ttf")[..],
        &include_bytes!("../assets/Devanagari.ttf")[..],
    ]
    .into_iter()
    .map(|bytes| ShapingFont::from_bytes(bytes, 0).expect("bundled demo typeface"))
    .collect();
    TextField::new(
        "告警 A سلام कि".into(),
        faces,
        FieldStyle {
            width: 220,
            height: 52,
            px: 22,
            foreground: Rgb565::new(30, 61, 30),
            background: Rgb565::new(4, 9, 10),
        },
        256,
    )
    .expect("valid initial text")
}
