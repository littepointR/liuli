use super::*;
use liuli::core::bitmap::Bitmap;
use liuli_widgets::{ButtonVariant as V, ThemedUi, tokens::rgb8};

pub(super) static CASSETTE: Bitmap =
    Bitmap::from_asset(include_bytes!("../assets/cassette.l565")).unwrap();

pub(super) fn palette(theme: Theme) -> Theme {
    if theme.preset != ThemePreset::Liuli {
        return theme;
    }
    let mut p = theme.colors;
    if theme.is_dark {
        p.background = rgb8(22, 38, 39);
        p.surface = rgb8(34, 53, 54);
        p.surface_alt = rgb8(46, 69, 69);
        p.surface_raised = p.surface_alt;
        p.surface_floating = rgb8(57, 79, 77);
        p.text = rgb8(245, 236, 211);
        p.text_muted = rgb8(181, 204, 192);
        p.accent = rgb8(246, 183, 109);
        p.on_accent = rgb8(32, 40, 38);
        p.border = rgb8(77, 98, 94);
        p.focus = rgb8(159, 229, 212);
    } else {
        p.background = rgb8(235, 234, 215);
        p.surface = rgb8(248, 247, 229);
        p.surface_alt = rgb8(212, 220, 202);
        p.surface_raised = p.surface;
        p.surface_floating = rgb8(251, 249, 238);
        p.text = rgb8(24, 50, 48);
        p.text_muted = rgb8(61, 87, 81);
        p.accent = rgb8(133, 68, 32);
        p.on_accent = rgb8(255, 248, 223);
        p.border = rgb8(145, 161, 147);
        p.focus = rgb8(0, 100, 94);
    }
    p.border_strong = p.text_muted;
    p.success = if theme.is_dark {
        rgb8(124, 207, 182)
    } else {
        rgb8(39, 105, 87)
    };
    theme.with_colors(p)
}

pub(super) fn group() -> NodeProps {
    NodeProps {
        width: Length::Fill(1),
        height: Length::Content,
        gap: 7,
        align_cross: Align::Stretch,
        ..NodeProps::new()
    }
}
pub(super) fn row_props(t: Theme) -> NodeProps {
    NodeProps {
        width: Length::Fill(1),
        height: Length::Px(t.control_height.into()),
        gap: 6,
        ..NodeProps::new()
    }
}
pub(super) fn link(
    ui: &mut ThemedUi<'_, '_, '_>,
    page: Page,
    label: &'static str,
    v: V,
) -> Result<(), BuildError> {
    ui.button(page.route_key(), label, IconKind::None, v)
}

pub(super) fn paint_tape(m: &Model, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = c.size();
    let base = size.width.min(size.height * 240 / 136);
    let width = (base * m.scale as u32 / 100).max(1);
    let height = (width * 136 / 240).max(1);
    c.fill(Rect::new(0, 0, size.width, size.height), rgb8(69, 120, 115))?;
    c.bitmap(
        Rect::new(
            ((size.width - width) / 2) as i32,
            ((size.height - height) / 2) as i32,
            width,
            height,
        ),
        &CASSETTE,
        m.angle,
        m.alpha as u8,
    )
}
fn artwork(ui: &mut ThemedUi<'_, '_, '_>, m: &Model, height: u32) -> Result<(), BuildError> {
    ui.custom(
        Key(450),
        NodeProps {
            height: Length::Px(height),
            width: Length::Fill(1),
            theme_id: u16::from(m.angle) | ((m.scale as u16) << 8),
            value: m.alpha,
            ..NodeProps::new()
        },
    )
}
pub(super) fn loaded(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    artwork(ui, m, if m.landscape { 208 } else { 121 })?;
    let name = ui.text_fmt(format_args!("{}", m.deck.loaded))?;
    ui.title(Key(451), name)?;
    let time = ui.text_fmt(format_args!(
        "{}:{:02}  /  {}:{:02}     Tape {:02}",
        m.deck.elapsed() / 60,
        m.deck.elapsed() % 60,
        m.deck.loaded.seconds / 60,
        m.deck.loaded.seconds % 60,
        m.deck.loaded.number
    ))?;
    ui.caption(Key(452), time)?;
    ui.progress_bar(
        Key(453),
        (100 * u64::from(m.deck.elapsed()) / u64::from(m.deck.loaded.seconds)) as i16,
    )
}
fn transport(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let t = ui.theme();
    ui.row(Key(454), row_props(t), |row| {
        row.button(
            Key(90),
            if m.deck.playing.is_some() {
                "Pause"
            } else {
                "Play"
            },
            IconKind::None,
            V::Primary,
        )?;
        link(row, Page::Feedback, "New take", V::Secondary)
    })
}
fn deck_tools(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let t = ui.theme();
    let gain = ui.text_fmt(format_args!(
        "{} input    /    Gain {}%",
        source(m),
        m.value
    ))?;
    ui.caption(Key(455), gain)?;
    let position = (1000 * u64::from(m.deck.elapsed()) / u64::from(m.deck.loaded.seconds)) as i16;
    ui.custom(
        Key(456),
        NodeProps {
            width: Length::Fill(1),
            height: Length::Px(if m.landscape { 100 } else { 72 }),
            value: position,
            max: m.deck.envelope,
            theme_id: m.deck.loaded.key.0 as u16,
            fg: t.success,
            bg: Some(t.surface),
            accent: t.accent,
            ..NodeProps::new()
        },
    )?;
    ui.row(Key(457), row_props(t), |r| {
        link(r, Page::Depth, "Sleeve", V::Surface)?;
        link(r, Page::Motion, "Fade", V::Surface)
    })
}

// A repeatable waveform thumbnail for each simulated tape. Each column is a single paint command.
pub(super) fn waveform(p: &StoredProps, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = c.size();
    c.fill(Rect::new(0, 0, size.width, size.height), p.bg_raw)?;
    if size.width < 12 || size.height < 8 {
        return Ok(());
    }
    let span = size.width - 12;
    let columns = (span / 4).clamp(1, 58);
    let middle = (size.height / 2) as i32;
    let position = span * p.value as u32 / 1000;
    let mut seed = u32::from(p.theme_id);
    for i in 0..columns {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let envelope = 5 + ((i * 3 + p.theme_id as u32) % 17).abs_diff(8);
        let amplitude =
            1 + (size.height - 6) * (5 + (seed >> 26)) * envelope / (69 * 14) * p.max as u32 / 1000;
        let left = i * span / columns;
        let right = (i + 1) * span / columns;
        let color = if left < position { p.accent } else { p.fg };
        c.fill(
            Rect::new(
                6 + left as i32,
                middle - (amplitude / 2) as i32,
                (right - left).saturating_sub(1).max(1),
                amplitude,
            ),
            color,
        )?;
    }
    c.fill(
        Rect::new(6 + position as i32, 3, 1, size.height - 6),
        p.accent,
    )
}

pub(super) fn deck(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    if m.landscape {
        let ui_panel = ui.theme().card();
        ui.row(Key(460), group(), |row| {
            row.column(
                Key(461),
                NodeProps {
                    width: Length::Fill(6),
                    ..group()
                },
                |left| loaded(left, m),
            )?;
            row.column(
                Key(462),
                NodeProps {
                    width: Length::Fill(4),
                    padding: EdgeInsets::all(12),
                    ..ui_panel
                },
                |right| {
                    right.title(Key(463), TextRef::Static("Transport"))?;
                    transport(right, m)?;
                    deck_tools(right, m)
                },
            )
        })?;
        ui.row(Key(466), row_props(ui.theme()), |line| {
            let text = match m.last_cue {
                Some((icon, sec)) => line.text_fmt(format_args!(
                    "Last cue    {}    {}:{:02}",
                    pages::ICONS[icon].1,
                    sec / 60,
                    sec % 60
                ))?,
                None => TextRef::Static("Cue sheet    /    No markers"),
            };
            line.caption(Key(464), text)?;
            line.button_props(
                Key(30),
                V::Outline,
                NodeProps {
                    width: Length::Px(140),
                    text: TextRef::Static("Drop cue"),
                    icon: pages::ICONS[m.icon].0,
                    ..line.theme().button_state(V::Outline, m.enabled)
                },
            )
        })
    } else {
        loaded(ui, m)?;
        transport(ui, m)?;
        deck_tools(ui, m)
    }
}

pub(super) fn source(m: &Model) -> &'static str {
    match m.choice.0 {
        54 => "Line",
        55 => "Aux",
        _ => "Mic",
    }
}
pub(super) fn mixer(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let strip = |ui: &mut ThemedUi<'_, '_, '_>| {
        let gain = ui.text_fmt(format_args!("{} gain   {}%", source(m), m.value))?;
        ui.title(Key(20), gain)?;
        ui.slider(Key(32), m.value)?;
        ui.toggle_row(
            Key(33),
            Key(34),
            Key(31),
            TextRef::Static("Cue markers"),
            m.enabled,
        )?;
        let cues = ui.text_fmt(format_args!(
            "{}   /   {} cues",
            pages::ICONS[m.icon].1,
            m.count
        ))?;
        ui.caption(Key(21), cues)?;
        ui.button_enabled(
            Key(30),
            "Drop cue",
            pages::ICONS[m.icon].0,
            V::Secondary,
            m.enabled,
        )
    };
    let patch = |ui: &mut ThemedUi<'_, '_, '_>| {
        ui.caption(Key(22), TextRef::Static("Signal path"))?;
        ui.row(Key(467), row_props(ui.theme()), |r| {
            link(r, Page::Choices, "Routing", V::Surface)?;
            link(r, Page::Data, "Line check", V::Surface)
        })?;
        link(ui, Page::Image, "Deck artwork", V::Outline)
    };
    if m.landscape {
        ui.row(Key(460), group(), |row| {
            row.column(Key(461), row.theme().card(), strip)?;
            row.column(Key(462), row.theme().card(), patch)
        })
    } else {
        strip(ui)?;
        patch(ui)
    }
}

pub(super) fn capture(ui: &mut ThemedUi<'_, '_, '_>, m: &Model) -> Result<(), BuildError> {
    let recording = m.deck.recording.is_some();
    let sec = m.deck.recording_seconds;
    let label = ui.text_fmt(format_args!(
        "{}    {:02}:{:02}",
        if recording {
            "Recording"
        } else if m.capture_saved {
            "Take saved"
        } else {
            "Ready"
        },
        sec / 60,
        sec % 60
    ))?;
    ui.title(Key(20), label)?;
    ui.sparkline_ring(Key(42), &m.samples)?;
    let gain = ui.text_fmt(format_args!("{} input   /   Gain {}%", source(m), m.value))?;
    ui.caption(Key(21), gain)?;
    ui.slider(Key(32), m.value)?;
    ui.button(
        Key(60),
        if recording {
            "Finish take"
        } else {
            "Start recording"
        },
        IconKind::Waveform,
        V::Primary,
    )?;
    if recording {
        let cue = ui.text_fmt(format_args!("Drop cue    {}", m.count))?;
        ui.button_props(
            Key(65),
            V::Surface,
            NodeProps {
                text: cue,
                icon: pages::ICONS[m.icon].0,
                ..ui.theme().button_state(V::Surface, m.enabled)
            },
        )?;
    }
    if m.toast_until.is_some() {
        ui.toast(
            Key(66),
            TextRef::Static(if recording {
                "Cue added"
            } else {
                "Saved to tape library"
            }),
        )?;
    }
    Ok(())
}
