use liuli::prelude::*;
use liuli::render::vector::{atan2_deg_q4, sin_cos_q12};

use crate::Theme;

/// Interactive 270-degree rotary dial backed by `NodeKind::Custom` with free pointer capture.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn arc_knob(
    ui: &mut Children<'_, '_>,
    key: Key,
    diameter: u16,
    range: core::ops::RangeInclusive<i16>,
    value: i16,
    step: i16,
    label: TextRef,
    theme: Theme,
) -> Result<(), BuildError> {
    let (min, max) = range.into_inner();
    if min > max || step <= 0 {
        return Err(BuildError::InvalidRange);
    }
    ui.custom(
        key,
        NodeProps {
            text: label,
            bg: Some(theme.surface_alt),
            fg: theme.text,
            accent: theme.accent,
            focus_color: theme.focus,
            focus_ring: theme.interaction.focus,
            font: theme.font,
            min,
            max,
            step,
            value: value.clamp(min, max),
            width: Length::Px(diameter.into()),
            height: Length::Px(diameter.into()),
            pointer_capture: PointerCaptureMode::Free,
            editable: true,
            clickable: true,
            focusable: true,
            style_source: theme.source.declared(),
            ..NodeProps::new()
        },
    )
}

/// Draw a 270-degree arc knob gauge and indicator dot into a `WidgetCanvas`.
pub fn paint_arc_knob(
    props: &StoredProps,
    state: PaintState,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let size = canvas.size();
    let min_dim = size.width.min(size.height);
    if min_dim < 4 {
        return Ok(());
    }
    let stroke_w = (min_dim / 9).clamp(3, 10) as u8;
    let cx = (size.width / 2) as i32;
    let cy = (size.height / 2) as i32;
    let radius = ((min_dim / 2).saturating_sub(u32::from(stroke_w) / 2 + 2)).max(2) as u16;
    let track_col = props.bg().unwrap_or(Rgb565::new(8, 16, 8));

    canvas.arc(
        Point::new(cx, cy),
        radius,
        135,
        270,
        stroke_w,
        true,
        track_col,
    )?;

    let min = i64::from(props.min);
    let max = i64::from(props.max);
    let span = (max - min).max(1);
    let rel = (i64::from(props.value).clamp(min, max.max(min)) - min).max(0);
    let sweep = ((270 * rel + span / 2) / span) as i16;
    if sweep > 0 {
        canvas.arc(
            Point::new(cx, cy),
            radius,
            135,
            sweep,
            stroke_w,
            true,
            props.accent,
        )?;
    }

    let angle_deg = 135 + i32::from(sweep);
    let (sin_q12, cos_q12) = sin_cos_q12(angle_deg);
    let dot_x = cx + ((i32::from(radius) * cos_q12) >> 12);
    let dot_y = cy + ((i32::from(radius) * sin_q12) >> 12);
    let dot_r = u16::from(stroke_w).div_ceil(2).max(2);
    let dot_col = if state.editing {
        props.focus_color
    } else {
        props.fg
    };
    canvas.circle(
        Point::new(dot_x, dot_y),
        dot_r,
        Some(dot_col),
        Some(props.accent),
        1,
    )?;

    if !matches!(props.text(), TextRef::Static("")) {
        let text_h = 18u32.min(size.height);
        let text_y = cy.saturating_sub((text_h / 2) as i32);
        canvas.node_text(
            Rect::new(0, text_y, size.width, text_h),
            props,
            props.fg,
            TextAlign::Center,
        )?;
    }
    Ok(())
}

/// Convert a widget-local `PointerEvent` on an `arc_knob` into a quantized `[min..=max]` value.
pub fn arc_knob_from_pointer(ev: PointerEvent, min: i16, max: i16, step: i16) -> i16 {
    if min >= max {
        return min;
    }
    let cx = (ev.size.width / 2) as i32;
    let cy = (ev.size.height / 2) as i32;
    let dx = ev.local.x - cx;
    let dy = ev.local.y - cy;
    let deg_q4 = atan2_deg_q4(dy, dx);

    // Sweep spans 135 deg -> 405 deg (= 45 deg), with bottom 90 deg dead-zone split at 90 deg.
    let rel_q4 = if (45 * 16..90 * 16).contains(&deg_q4) {
        270 * 16
    } else if (90 * 16..135 * 16).contains(&deg_q4) {
        0
    } else if deg_q4 < 45 * 16 {
        deg_q4 + 360 * 16 - 135 * 16
    } else {
        deg_q4 - 135 * 16
    };

    let step = i64::from(step.max(1));
    let span = i64::from(max) - i64::from(min);
    let max_rel_q4 = 270i64 * 16;
    let raw_offset = (span * i64::from(rel_q4) + max_rel_q4 / 2) / max_rel_q4;
    let stepped = ((raw_offset + step / 2) / step) * step;
    (i64::from(min) + stepped).clamp(i64::from(min), i64::from(max)) as i16
}

/// Layer-2 `paint_override` helper that adds tick marks and a circular vector thumb to `NodeKind::Slider`.
pub fn paint_ticked_slider(
    props: &StoredProps,
    state: PaintState,
    ticks: u8,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<PaintOverride, BuildError> {
    let size = canvas.size();
    if size.width < 12 || size.height < 6 {
        return Ok(PaintOverride::Default);
    }
    let pad_x = 6i32;
    let usable_w = size.width.saturating_sub(12);
    let tick_count = ticks.max(2);
    if usable_w > 0 {
        let y0 = (size.height as i32 / 2) + 4;
        let y1 = ((size.height as i32) - 2).max(y0);
        let tick_col = if state.editing {
            props.focus_color
        } else {
            props.fg
        };
        for i in 0..tick_count {
            let tx = pad_x + ((i32::from(i) * usable_w as i32) / i32::from(tick_count - 1));
            canvas.line(Point::new(tx, y0), Point::new(tx, y1), tick_col, 1, false)?;
        }
    }

    canvas.paint_slider_track()?;

    let min = i64::from(props.min);
    let max = i64::from(props.max);
    let span = (max - min).max(1);
    let rel = (i64::from(props.value).clamp(min, max.max(min)) - min).max(0);
    let cx = pad_x + ((i64::from(usable_w) * rel) / span) as i32;
    let cy = (size.height / 2) as i32;
    let knob_r = (size.height / 2).saturating_sub(3).clamp(4, 9) as u16;
    let border_col = if state.editing {
        props.focus_color
    } else {
        props.accent
    };
    canvas.circle(
        Point::new(cx, cy),
        knob_r,
        Some(props.fg),
        Some(border_col),
        if state.editing { 2 } else { 1 },
    )?;
    Ok(PaintOverride::Handled)
}

/// Composite primary button with an overlaid top-end badge aligned inside a `Stack`.
#[track_caller]
pub fn badge_button(
    ui: &mut Children<'_, '_>,
    keys: [Key; 3],
    label: TextRef,
    badge_text: TextRef,
    badge_bg: Rgb565,
    theme: Theme,
) -> Result<(), BuildError> {
    let mut stack_p = NodeProps::new();
    stack_p.width = Length::Fill(1);
    stack_p.height = Length::Px(u32::from(theme.control_height).saturating_add(4));
    stack_p.padding = EdgeInsets {
        top: 4,
        right: 4,
        bottom: 0,
        left: 0,
    };
    ui.stack(keys[0], stack_p, |s| {
        s.button(
            keys[1],
            NodeProps {
                text: label,
                width: Length::Fill(1),
                height: Length::Fill(1),
                ..theme.primary_button()
            },
        )?;
        s.label(
            keys[2],
            NodeProps {
                text: badge_text,
                width: Length::Content,
                height: Length::Content,
                padding: EdgeInsets::symmetric(5, 1),
                bg: Some(badge_bg),
                fg: theme.on_accent,
                radius: 6,
                font: FontId::Small12,
                text_align: TextAlign::Center,
                align_cross: Align::Center,
                stack_align: StackAlign::TopEnd,
                ..NodeProps::new()
            },
        )?;
        s.set_offset(keys[2], -4, -4)
    })
}
