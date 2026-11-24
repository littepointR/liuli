use super::*;
use core::num::NonZeroU16;
use liuli_widgets::{ButtonVariant, SurfaceSpec, ThemedUi, virtual_list};

pub(super) const LIST: Key = Key(12);
pub(super) const ICONS: &[(IconKind, &str)] = &[
    (IconKind::ChevronRight, "Start"),
    (IconKind::Check, "Keep"),
    (IconKind::Warning, "Noise"),
    (IconKind::Sliders, "Mix"),
    (IconKind::Bell, "Chime"),
    (IconKind::Moon, "Night"),
    (IconKind::Thermometer, "Warm"),
    (IconKind::Reset, "Retake"),
    (IconKind::Gauge, "Peak"),
    (IconKind::Waveform, "Room"),
    (IconKind::Diagnostics, "Click"),
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
    let padding = EdgeInsets::all(10);
    let gap = t.spacing;
    let tabs_height = if !m.landscape && m.page.is_tab() {
        t.control_height + 8
    } else {
        0
    };
    let viewport = m.height.saturating_sub(68 + tabs_height);
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
                    padding,
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
                            let title = PAGES.iter().find(|p| p.0 == m.page).map_or(
                                if m.landscape {
                                    "Spool    /    Field recorder"
                                } else {
                                    "Spool"
                                },
                                |p| p.1,
                            );
                            bar.title(Key(8), TextRef::Static(title))?;
                            if m.landscape && m.page.is_tab() {
                                bar.column(
                                    Key(95),
                                    NodeProps {
                                        width: Length::Px(280),
                                        height: Length::Content,
                                        ..NodeProps::new()
                                    },
                                    |nav| navigation(nav, m),
                                )?;
                            }
                            let home = m.page == Page::Home;
                            bar.button_props(
                                if home { Key(5) } else { Key(3) },
                                ButtonVariant::Secondary,
                                NodeProps {
                                    text: TextRef::Static(if home {
                                        if m.dark { "Day" } else { "Night" }
                                    } else {
                                        "Back"
                                    }),
                                    icon: if home { IconKind::Moon } else { IconKind::None },
                                    height: Length::Px(34),
                                    width: Length::Px(if home { 86 } else { 64 }),
                                    padding: EdgeInsets::symmetric(8, 3),
                                    ..t.secondary_button()
                                },
                            )
                        },
                    )?;
                    if !m.landscape && m.page.is_tab() {
                        navigation(ui, m)?;
                    }
                    if m.page == Page::List {
                        return if m.landscape {
                            ui.row(Key(470), studio::group(), |columns| {
                                columns.column(
                                    Key(471),
                                    NodeProps {
                                        width: Length::Fill(3),
                                        gap: 8,
                                        ..studio::group()
                                    },
                                    |left| tape_rows(left, m, viewport),
                                )?;
                                columns.column(
                                    Key(472),
                                    NodeProps {
                                        width: Length::Fill(2),
                                        ..t.card()
                                    },
                                    |right| studio::loaded(right, m),
                                )
                            })
                        } else {
                            tape_rows(ui, m, viewport)
                        };
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
                                    gap,
                                    align_cross: Align::Stretch,
                                    ..wide(Length::Content)
                                },
                                |body| {
                                    if m.landscape
                                        && !matches!(
                                            m.page,
                                            Page::Home | Page::Inputs | Page::Depth
                                        )
                                    {
                                        body.row(Key(470), studio::group(), |columns| {
                                            columns.column(
                                                Key(471),
                                                NodeProps {
                                                    width: Length::Fill(3),
                                                    ..t.card()
                                                },
                                                |left| contents(left, m),
                                            )?;
                                            columns.column(
                                                Key(472),
                                                NodeProps {
                                                    width: Length::Fill(2),
                                                    ..t.card()
                                                },
                                                |right| studio::loaded(right, m),
                                            )
                                        })
                                    } else {
                                        contents(body, m)
                                    }
                                },
                            )
                        },
                    )
                },
            )?;
            if m.confirm.is_some() {
                ui.centered_confirm_dialog(
                    [Key(61), Key(62), Key(63), Key(64)],
                    [TextRef::Static("Save take"), TextRef::Static("Discard")],
                )?;
            }
            Ok(())
        },
    )
}
fn navigation(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    ui.tabs(
        Key(96),
        &[
            (Page::Home.route_key(), TextRef::Static("Deck")),
            (Page::List.route_key(), TextRef::Static("Tapes")),
            (Page::Inputs.route_key(), TextRef::Static("Mixer")),
        ],
        m.page.route_key(),
    )
}
fn tape_rows(ui: &mut ThemedUi<'_, '_, '_>, m: &Model, viewport: u16) -> Result<(), BuildError> {
    let t = ui.theme();
    ui.row(Key(97), studio::row_props(t), |r| {
        studio::link(r, Page::Icons, "Cue stamps", ButtonVariant::Secondary)?;
        #[cfg(feature = "enhanced")]
        studio::link(r, Page::Text, "Notebook", ButtonVariant::Secondary)?;
        Ok(())
    })?;
    virtual_list(
        ui.raw(),
        [LIST, Key(13), Key(14)],
        m.deck.count(),
        NonZeroU16::new(t.control_height).unwrap(),
        m.offset,
        viewport.saturating_sub(t.control_height + 8),
        |rows, id| {
            let take = m.deck.row(id);
            let key = take.key;
            let selected = m.selected == Some(key);
            let text = rows.text_fmt(format_args!("{:02}   {}", take.number, take))?;
            let base = if selected {
                t.secondary_button()
            } else {
                t.control()
            };
            rows.row(
                key,
                NodeProps {
                    clickable: true,
                    focusable: true,
                    checked: selected,
                    padding: EdgeInsets::symmetric(8, 0),
                    align_cross: Align::Center,
                    bg: Some(if selected {
                        t.surface_alt
                    } else if id % 2 == 0 {
                        t.surface
                    } else {
                        t.background
                    }),
                    border_width: 0,
                    border_color: None,
                    ..base
                },
                |line| {
                    line.label(
                        Key(3000 + 2 * key.0),
                        NodeProps {
                            text,
                            fg: if selected { t.accent } else { t.text },
                            ellipsis: true,
                            ..t.caption()
                        },
                    )?;
                    let duration = line.text_fmt(format_args!(
                        "{}:{:02}",
                        take.seconds / 60,
                        take.seconds % 60
                    ))?;
                    line.label(
                        Key(3001 + 2 * key.0),
                        NodeProps {
                            text: duration,
                            text_align: TextAlign::End,
                            width: Length::Px(43),
                            ..t.caption()
                        },
                    )
                },
            )
        },
    )
}
fn contents(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let t = ui.theme();
    match m.page {
        Page::Home => return studio::deck(ui, m),
        Page::Depth => return depth::page(ui, m),
        Page::Motion => return m.motion.page(ui),
        Page::Inputs => return studio::mixer(ui, m),
        Page::Data => {
            let level = ui.text_fmt(format_args!("Input level   /   {}%", m.level))?;
            ui.caption(Key(20), level)?;
            ui.progress_bar(Key(41), m.level)?;
            ui.caption(Key(21), TextRef::Static("Signal history"))?;
            ui.sparkline_ring(Key(42), &m.samples)?;
            ui.caption(Key(22), TextRef::Static("Peak level"))?;
            ui.custom(
                Key(43),
                NodeProps {
                    value: m.level,
                    bg: Some(t.surface_alt),
                    accent: t.accent,
                    fg: t.text,
                    ..wide(Length::Px(14))
                },
            )?;
            ui.button(
                Key(44),
                "Send test tone",
                IconKind::Waveform,
                ButtonVariant::Primary,
            )?;
        }
        Page::Choices => {
            ui.tabs(
                Key(49),
                &[
                    (Key(50), TextRef::Static("Source")),
                    (Key(51), TextRef::Static("Trim")),
                ],
                m.tab,
            )?;
            if m.tab == Key(50) {
                ui.caption(Key(21), TextRef::Static("Select an input"))?;
                ui.selector(
                    Key(52),
                    &[
                        (Key(53), TextRef::Static("Mic")),
                        (Key(54), TextRef::Static("Line")),
                        (Key(55), TextRef::Static("Aux")),
                    ],
                    m.choice,
                )?;
            } else {
                let text = ui.text_fmt(format_args!("Input gain   /   {}%", m.value))?;
                ui.caption(Key(21), text)?;
                ui.number_editor(Key(57), 0..=100, m.value, 5)?;
            }
        }
        Page::Feedback => return studio::capture(ui, m),
        Page::List => {}
        Page::Icons => {
            let selected = ui.text_fmt(format_args!("Current stamp   /   {}", ICONS[m.icon].1))?;
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
            ui.row(Key(76), studio::row_props(t), |r| {
                r.button(Key(81), "Draft tag", IconKind::None, ButtonVariant::Surface)?;
                r.button(Key(82), "Add tag", IconKind::None, ButtonVariant::Secondary)
            })?;
            ui.row(Key(77), studio::row_props(t), |r| {
                r.button(
                    Key(83),
                    "Clear draft",
                    IconKind::None,
                    ButtonVariant::Surface,
                )?;
                r.button(Key(84), "Backspace", IconKind::None, ButtonVariant::Surface)
            })?;
            let gamma_label = if m.text_gamma == TextGamma::Perceptual {
                "Ink / Soft"
            } else {
                "Ink / Crisp"
            };
            ui.button(
                Key(85),
                gamma_label,
                IconKind::Sliders,
                ButtonVariant::Outline,
            )?;
            ui.surface_column(
                Key(86),
                SurfaceSpec::CARD,
                NodeProps {
                    padding: EdgeInsets::all(8),
                    gap: 4,
                    ..t.card()
                },
                |card| {
                    card.toggle_row(
                        Key(861),
                        Key(862),
                        Key(863),
                        TextRef::Static("Tone logged"),
                        m.room_tone_logged,
                    )?;
                    card.label(
                        Key(87),
                        NodeProps {
                            text: TextRef::Static("Capture thirty seconds before the next take."),
                            line_spacing: 2,
                            max_lines: 3,
                            ..t.paragraph()
                        },
                    )?;
                    card.label(
                        Key(89),
                        NodeProps {
                            text: TextRef::Static("Log the room tone"),
                            underline: !m.room_tone_logged,
                            strikethrough: m.room_tone_logged,
                            height: Length::Content,
                            ..t.caption()
                        },
                    )
                },
            )?;
        }
    }
    Ok(())
}

pub(super) fn paint_image(m: &Model, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = c.size();
    let width = size.width.min(size.height * 240 / 136) * m.scale as u32 / 100;
    let height = width * 136 / 240;
    c.bitmap(
        Rect::new(
            ((size.width - width) / 2) as i32,
            ((size.height - height) / 2) as i32,
            width,
            height,
        ),
        &studio::CASSETTE,
        m.angle,
        m.alpha as u8,
    )
}

#[cfg(feature = "enhanced")]
pub(super) fn text_field(t: Theme) -> liuli::enhanced::field::TextField<'static> {
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
        "Pier A / سلام कि".into(),
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
