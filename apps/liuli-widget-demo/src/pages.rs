use super::*;
use core::num::NonZeroU16;
use liuli_widgets::{ButtonVariant, SurfaceSpec, ThemedUi, virtual_list};

pub(super) const LIST: Key = Key(12);
const ICONS: &[(IconKind, &str)] = &[
    (IconKind::ChevronRight, "Next"),
    (IconKind::Check, "Check"),
    (IconKind::Warning, "Warning"),
    (IconKind::Sliders, "Sliders"),
    (IconKind::Bell, "Bell"),
    (IconKind::Moon, "Moon"),
    (IconKind::Thermometer, "Temp"),
    (IconKind::Reset, "Reset"),
    (IconKind::Gauge, "Gauge"),
    (IconKind::Waveform, "Waveform"),
    (IconKind::Diagnostics, "Diag"),
];
fn wide(height: Length) -> NodeProps {
    NodeProps {
        width: Length::Fill(1),
        height,
        ..NodeProps::new()
    }
}
pub(super) fn view(m: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
    let t = m.theme();
    let full = wide(Length::Fill(1));
    let viewport = m.height.saturating_sub(68);
    out.root(
        Key(1),
        NodeProps {
            bg: Some(t.background),
            ..full
        },
        |root| {
            let mut ui = ThemedUi::new(root, t);
            ui.column(
                Key(2),
                NodeProps {
                    padding: EdgeInsets::all(10),
                    gap: 8,
                    align_cross: Align::Stretch,
                    ..full
                },
                |ui| {
                    ui.row(
                        Key(7),
                        NodeProps {
                            align_cross: Align::Center,
                            gap: 6,
                            ..wide(Length::Px(40))
                        },
                        |bar| {
                            let title = PAGES
                                .iter()
                                .find(|p| p.0 == m.page)
                                .map_or("Widget Demo", |p| p.1);
                            bar.title(Key(8), TextRef::Static(title))?;
                            let home = m.page == Page::Home;
                            bar.button_props(
                                if home { Key(5) } else { Key(3) },
                                ButtonVariant::Secondary,
                                NodeProps {
                                    text: TextRef::Static(if home { "Theme" } else { "Back" }),
                                    icon: if home { IconKind::Moon } else { IconKind::None },
                                    height: Length::Px(34),
                                    width: Length::Px(if home { 86 } else { 64 }),
                                    padding: EdgeInsets::symmetric(8, 3),
                                    ..t.secondary_button()
                                },
                            )
                        },
                    )?;
                    if m.page == Page::List {
                        return virtual_list(
                            ui.raw(),
                            [LIST, Key(13), Key(14)],
                            1000,
                            NonZeroU16::new(38).unwrap(),
                            m.offset,
                            viewport,
                            |rows, id| {
                                let key = Key(1000 + id);
                                let selected = m.selected == Some(key);
                                let text = rows.text_fmt(format_args!("Record {:04}", id + 1))?;
                                let base = if selected {
                                    t.secondary_button()
                                } else {
                                    t.control()
                                };
                                rows.button(
                                    key,
                                    NodeProps {
                                        text,
                                        icon: if selected {
                                            IconKind::Check
                                        } else {
                                            IconKind::None
                                        },
                                        border_color: Some(if selected {
                                            t.accent
                                        } else {
                                            t.border
                                        }),
                                        ..base
                                    },
                                )
                            },
                        );
                    }
                    ui.scroll_y(
                        Key(9),
                        NodeProps {
                            accent: t.accent,
                            ..wide(Length::Px(viewport.into()))
                        },
                        |scroll| {
                            scroll.column(
                                Key(10),
                                NodeProps {
                                    gap: t.spacing,
                                    align_cross: Align::Stretch,
                                    ..wide(Length::Content)
                                },
                                |body| contents(body, m),
                            )
                        },
                    )
                },
            )?;
            if m.confirm {
                ui.centered_confirm_dialog(
                    [Key(61), Key(62), Key(63), Key(64)],
                    [TextRef::Static("Confirm"), TextRef::Static("Cancel")],
                )?;
            }
            Ok(())
        },
    )
}
fn contents(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let t = ui.theme();
    match m.page {
        Page::Home => {
            for (i, &(_, name, icon)) in PAGES.iter().enumerate() {
                ui.button(Key(100 + i as u32), name, icon, ButtonVariant::Surface)?;
            }
        }
        Page::Depth => return depth::page(ui, m),
        Page::Inputs => {
            let tally = ui.text_fmt(format_args!("Button   /   {} presses", m.count))?;
            ui.caption(Key(20), tally)?;
            ui.button_enabled(
                Key(30),
                "Press",
                IconKind::None,
                ButtonVariant::Primary,
                m.enabled,
            )?;
            ui.caption(Key(21), TextRef::Static("Toggle"))?;
            ui.toggle_row(
                Key(33),
                Key(34),
                Key(31),
                TextRef::Static("Enable button"),
                m.enabled,
            )?;
            let level = ui.text_fmt(format_args!("Slider   /   {}%", m.value))?;
            ui.caption(Key(22), level)?;
            ui.slider(Key(32), m.value)?;
        }
        Page::Data => {
            let level = ui.text_fmt(format_args!("Progress   /   {}%", m.value))?;
            ui.caption(Key(20), level)?;
            ui.progress_bar(Key(41), m.value)?;
            ui.caption(Key(21), TextRef::Static("Sparkline"))?;
            ui.sparkline_ring(Key(42), &m.samples)?;
            ui.caption(Key(22), TextRef::Static("Custom meter"))?;
            ui.custom(
                Key(43),
                NodeProps {
                    value: m.value,
                    bg: Some(t.surface_alt),
                    accent: t.accent,
                    fg: t.text,
                    ..wide(Length::Px(14))
                },
            )?;
            ui.button(
                Key(44),
                "Add sample",
                IconKind::Waveform,
                ButtonVariant::Primary,
            )?;
        }
        Page::Choices => {
            ui.caption(Key(20), TextRef::Static("Tabs"))?;
            ui.tabs(
                Key(49),
                &[
                    (Key(50), TextRef::Static("Mode")),
                    (Key(51), TextRef::Static("Value")),
                ],
                m.tab,
            )?;
            if m.tab == Key(50) {
                ui.caption(Key(21), TextRef::Static("Selector"))?;
                ui.selector(
                    Key(52),
                    &[
                        (Key(53), TextRef::Static("Quiet")),
                        (Key(54), TextRef::Static("Auto")),
                        (Key(55), TextRef::Static("Loud")),
                    ],
                    m.choice,
                )?;
            } else {
                let text = ui.text_fmt(format_args!("Numeric editor   /   {}", m.value))?;
                ui.caption(Key(21), text)?;
                ui.number_editor(Key(57), 0..=100, m.value, 5)?;
            }
        }
        Page::Feedback => {
            ui.button(
                Key(60),
                "Open dialog",
                IconKind::Warning,
                ButtonVariant::Primary,
            )?;
            ui.button(
                Key(65),
                "Show toast",
                IconKind::Bell,
                ButtonVariant::Secondary,
            )?;
            if m.toast_until.is_some() {
                ui.toast(Key(66), TextRef::Static("Saved"))?;
            }
        }
        Page::List => {}
        Page::Icons => {
            let selected = ui.text_fmt(format_args!("Selected   /   {}", ICONS[m.icon].1))?;
            ui.caption(Key(20), selected)?;
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
                            let active = i == m.icon;
                            let variant = if active {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Surface
                            };
                            r.button_props(
                                Key(200 + i as u32),
                                variant,
                                NodeProps {
                                    icon,
                                    text: TextRef::Static(name),
                                    padding: EdgeInsets::symmetric(6, 4),
                                    ..t.button(variant)
                                },
                            )?;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        Page::Image => {
            ui.surface_stack(
                Key(70),
                SurfaceSpec::CARD,
                NodeProps {
                    padding: EdgeInsets::all(8),
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
            ui.button(Key(72), "Rotate", IconKind::Reset, ButtonVariant::Primary)?;
            let scale = ui.text_fmt(format_args!("Scale   /   {}%", m.scale))?;
            ui.caption(Key(21), scale)?;
            ui.slider_range(Key(74), 20, 100, 1, m.scale)?;
            let alpha = ui.text_fmt(format_args!("Opacity   /   {} of 15", m.alpha))?;
            ui.caption(Key(20), alpha)?;
            ui.slider_range(Key(73), 0, 15, 1, m.alpha)?;
        }
        #[cfg(feature = "enhanced")]
        Page::Text => {
            ui.surface_column(
                Key(79),
                SurfaceSpec::CARD,
                NodeProps {
                    padding: EdgeInsets::all(4),
                    ..t.card()
                },
                |box_ui| {
                    box_ui.custom(
                        Key(80),
                        NodeProps {
                            focusable: true,
                            clickable: true,
                            theme_id: m.field.revision(),
                            ..wide(Length::Px(44))
                        },
                    )
                },
            )?;
            ui.button(
                Key(81),
                "Compose",
                IconKind::Sliders,
                ButtonVariant::Surface,
            )?;
            ui.button(Key(82), "Commit", IconKind::Check, ButtonVariant::Primary)?;
            ui.button(
                Key(83),
                "Cancel preedit",
                IconKind::Reset,
                ButtonVariant::Outline,
            )?;
            ui.button(
                Key(84),
                "Backspace",
                IconKind::ChevronRight,
                ButtonVariant::Secondary,
            )?;
        }
    }
    Ok(())
}

pub(super) fn paint_image(m: &Model, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    // A compass needle shows both the rotation angle and four-bit opacity.
    const N: u16 = 0x18e5;
    const A: u16 = 0x149d;
    const B: u16 = 0xdf5e;
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
    let t = Theme::default();
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
            width: 200,
            height: 44,
            px: 20,
            foreground: t.text,
            background: t.surface,
        },
        256,
    )
    .expect("valid initial text")
}
