use liuli::prelude::*;
use liuli::render::vector::sin_cos_q12;

use crate::Theme;

/// Column descriptor for [`data_table`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableColumn {
    pub title: &'static str,
    pub width: Length,
    pub align: TextAlign,
}

impl TableColumn {
    #[inline]
    pub const fn new(title: &'static str, width: Length, align: TextAlign) -> Self {
        Self {
            title,
            width,
            align,
        }
    }
}

/// Declare a structured fixed-column header + selectable striped rows table.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn data_table(
    ui: &mut Children<'_, '_>,
    base_key: u32,
    cols: &[TableColumn],
    rows: &[&[&'static str]],
    selected_row: Option<usize>,
    row_height: u16,
    theme: Theme,
) -> Result<(), BuildError> {
    let rh = u32::from(row_height.max(20));
    let table_props = NodeProps::new()
        .width(Length::Fill(1))
        .height(Length::Content)
        .bg(theme.surface)
        .border(theme.border, 1)
        .radius(theme.radius);
    ui.column(Key(base_key), table_props, |tbl| {
        tbl.row(
            Key(base_key + 1),
            NodeProps::new()
                .width(Length::Fill(1))
                .height(Length::Px(rh))
                .padding(EdgeInsets::symmetric(6, 2))
                .gap(4)
                .bg(theme.surface_alt)
                .align_cross(Align::Center),
            |hdr| {
                for (ci, col) in cols.iter().enumerate() {
                    hdr.label(
                        Key(base_key + 10 + ci as u32),
                        NodeProps::new()
                            .width(col.width)
                            .height(Length::Content)
                            .fg(theme.accent)
                            .font(FontId::Small12)
                            .text(TextRef::Static(col.title))
                            .text_align(col.align),
                    )?;
                }
                Ok(())
            },
        )?;

        for (ri, row_cells) in rows.iter().enumerate() {
            let is_sel = selected_row == Some(ri);
            let row_bg = if is_sel {
                theme.accent
            } else if !ri.is_multiple_of(2) {
                theme.surface_alt
            } else {
                theme.surface
            };
            let row_fg = if is_sel { theme.background } else { theme.text };
            let rbase = base_key + 100 + (ri as u32) * 16;
            tbl.row(
                Key(rbase),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Px(rh))
                    .padding(EdgeInsets::symmetric(6, 2))
                    .gap(4)
                    .bg(row_bg)
                    .align_cross(Align::Center)
                    .focusable(true)
                    .clickable(true),
                |r| {
                    for (ci, col) in cols.iter().enumerate() {
                        let cell = row_cells.get(ci).copied().unwrap_or("");
                        r.label(
                            Key(rbase + 1 + ci as u32),
                            NodeProps::new()
                                .width(col.width)
                                .height(Length::Content)
                                .fg(row_fg)
                                .font(theme.font)
                                .text(TextRef::Static(cell))
                                .text_align(col.align),
                        )?;
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    })
}

/// Configuration for a dial [`gauge`] with radial ticks and indicator needle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GaugeSpec {
    pub min: i16,
    pub max: i16,
    pub value: i16,
    pub ticks: u8,
    pub diameter: u16,
    pub label: TextRef,
}

/// Declare a dial gauge widget (`NodeKind::Custom`) painted with [`paint_gauge`].
#[track_caller]
pub fn gauge(
    ui: &mut Children<'_, '_>,
    key: Key,
    spec: GaugeSpec,
    theme: Theme,
) -> Result<(), BuildError> {
    if spec.min > spec.max {
        return Err(BuildError::InvalidRange);
    }
    ui.custom(
        key,
        NodeProps::new()
            .width(Length::Px(u32::from(spec.diameter)))
            .height(Length::Px(u32::from(spec.diameter)))
            .bg(theme.surface_alt)
            .fg(theme.text)
            .accent(theme.accent)
            .font(theme.font)
            .text(spec.label)
            .range(spec.min, spec.max, 1)
            .value(spec.value.clamp(spec.min, spec.max)),
    )
}

/// Render a 270-degree dial gauge with ticks, arc indicator, and needle.
pub fn paint_gauge(
    spec: GaugeSpec,
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let min_dim = sz.width.min(sz.height);
    if min_dim < 12 {
        return Ok(());
    }
    let cx = (sz.width / 2) as i32;
    let cy = (sz.height / 2) as i32;
    let stroke_w = (min_dim / 12).clamp(2, 8) as u8;
    let radius = ((min_dim / 2).saturating_sub(u32::from(stroke_w) + 2)).max(4) as u16;
    let track = props.bg().unwrap_or(Rgb565::new(6, 12, 6));

    canvas.arc(Point::new(cx, cy), radius, 135, 270, stroke_w, true, track)?;
    let span = i64::from(spec.max)
        .saturating_sub(i64::from(spec.min))
        .max(1);
    let rel = (i64::from(spec.value.clamp(spec.min, spec.max)) - i64::from(spec.min)).max(0);
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

    let tick_count = spec.ticks.max(2);
    let r_out = i32::from(radius);
    let r_in = (r_out - 5).max(1);
    for t in 0..tick_count {
        let deg = 135 + (270 * i32::from(t)) / i32::from(tick_count - 1);
        let (s, c) = sin_cos_q12(deg);
        let p0 = Point::new(cx + ((r_in * c) >> 12), cy + ((r_in * s) >> 12));
        let p1 = Point::new(cx + ((r_out * c) >> 12), cy + ((r_out * s) >> 12));
        canvas.line(p0, p1, props.fg, 1, false)?;
    }

    let needle_deg = 135 + i32::from(sweep);
    let (ns, nc) = sin_cos_q12(needle_deg);
    let r_needle = (r_out - 4).max(2);
    let tip = Point::new(cx + ((r_needle * nc) >> 12), cy + ((r_needle * ns) >> 12));
    canvas.line(Point::new(cx, cy), tip, props.fg, 2, true)?;
    canvas.circle(Point::new(cx, cy), 3, Some(props.accent), Some(props.fg), 1)?;

    if !matches!(spec.label, TextRef::Static("")) {
        let ly = cy + (r_out / 2);
        canvas.node_text(
            Rect::new(0, ly, sz.width, sz.height.saturating_sub(ly as u32)),
            props,
            props.fg,
            TextAlign::Center,
        )?;
    }
    Ok(())
}

/// Single series for [`paint_bar_chart`] and [`paint_multi_line_chart`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartSeries<'a> {
    pub name: &'static str,
    pub color: Rgb565,
    pub values: &'a [i16],
}

/// Paint grouped vertical bars across one or more series into `WidgetCanvas`.
pub fn paint_bar_chart(
    series: &[ChartSeries<'_>],
    min: i16,
    max: i16,
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let pad = 6i32;
    let cw = sz.width.saturating_sub((pad * 2) as u32);
    let ch = sz.height.saturating_sub((pad * 2) as u32);
    if cw < 4 || ch < 4 || min >= max || series.is_empty() {
        return Ok(());
    }
    canvas.rounded_rect(
        Rect::new(0, 0, sz.width, sz.height),
        props.bg().unwrap_or(Rgb565::new(2, 4, 2)),
        4,
        props.border_color(),
        props.border_width,
    )?;

    let groups = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    if groups == 0 {
        return Ok(());
    }
    let span = i64::from(max) - i64::from(min);
    let group_w = (cw as i32 / groups as i32).max(2);
    let s_count = series.len() as i32;
    let bar_w = ((group_w - 2) / s_count).max(1) as u32;
    let base_y = pad + ch as i32;

    for g in 0..groups {
        let gx = pad + (g as i32) * group_w;
        for (si, s) in series.iter().enumerate() {
            let Some(&val) = s.values.get(g) else {
                continue;
            };
            let norm = i64::from(val.clamp(min, max)) - i64::from(min);
            let bh = ((norm * i64::from(ch)) / span).clamp(1, i64::from(ch)) as u32;
            let bx = gx + (si as i32) * (bar_w as i32);
            canvas.fill(Rect::new(bx, base_y - bh as i32, bar_w, bh), s.color)?;
        }
    }
    Ok(())
}

/// Paint multi-series anti-aliased polylines with gridlines and sample points into `WidgetCanvas`.
pub fn paint_multi_line_chart(
    series: &[ChartSeries<'_>],
    min: i16,
    max: i16,
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let pad = 8i32;
    let cw = sz.width.saturating_sub((pad * 2) as u32);
    let ch = sz.height.saturating_sub((pad * 2) as u32);
    if cw < 4 || ch < 4 || min >= max || series.is_empty() {
        return Ok(());
    }
    canvas.rounded_rect(
        Rect::new(0, 0, sz.width, sz.height),
        props.bg().unwrap_or(Rgb565::new(2, 4, 2)),
        4,
        props.border_color(),
        props.border_width,
    )?;

    let grid_col = Rgb565::new(6, 12, 6);
    for r in 0..3 {
        let gy = pad + ((ch as i32) * r) / 2;
        canvas.line(
            Point::new(pad, gy),
            Point::new(pad + cw as i32, gy),
            grid_col,
            1,
            false,
        )?;
    }

    let span = i64::from(max) - i64::from(min);
    let base_y = pad + ch as i32;
    for s in series {
        let n = s.values.len();
        if n == 0 {
            continue;
        }
        let point_at = |i: usize| -> Point {
            let x = if n <= 1 {
                pad + (cw as i32) / 2
            } else {
                pad + ((i as i64 * i64::from(cw)) / (n as i64 - 1)) as i32
            };
            let norm = i64::from(s.values[i].clamp(min, max)) - i64::from(min);
            let y_off = ((norm * i64::from(ch)) / span).clamp(0, i64::from(ch)) as i32;
            Point::new(x, base_y - y_off)
        };
        let mut prev = point_at(0);
        canvas.circle(prev, 2, Some(s.color), None, 0)?;
        for i in 1..n {
            let cur = point_at(i);
            canvas.line(prev, cur, s.color, 2, true)?;
            canvas.circle(cur, 2, Some(s.color), None, 0)?;
            prev = cur;
        }
    }
    Ok(())
}

/// Declare an animated circular [`spinner`] backed by `NodeKind::Custom`.
#[track_caller]
pub fn spinner(
    ui: &mut Children<'_, '_>,
    key: Key,
    diameter: u16,
    now: InstantMs,
    theme: Theme,
) -> Result<(), BuildError> {
    let phase = (u32::from(spinner_start_deg(now)) << 8) | spinner_sweep_deg(now) as u32;
    ui.custom(
        key,
        NodeProps::new()
            .width(Length::Px(u32::from(diameter)))
            .height(Length::Px(u32::from(diameter)))
            .bg(theme.surface_alt)
            .accent(theme.accent),
    )?;
    ui.set_custom_rev(key, phase)
}

/// Smooth triangle-wave sweep angle (`60..=240` deg) over a 2.88 s period.
#[inline]
pub fn spinner_sweep_deg(now: InstantMs) -> i16 {
    let cycle = ((now.0 / 8) % 360) as i16;
    let tri = if cycle <= 180 { cycle } else { 360 - cycle };
    60 + tri
}

fn spinner_start_deg(now: InstantMs) -> u16 {
    ((now.0 % 1000) * 360 / 1000) as u16
}

/// Paint an animated indeterminate circular comet arc driven purely by `now`.
pub fn paint_spinner(
    now: InstantMs,
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let min_dim = sz.width.min(sz.height);
    if min_dim < 8 {
        return Ok(());
    }
    let cx = (sz.width / 2) as i32;
    let cy = (sz.height / 2) as i32;
    let stroke_w = (min_dim / 8).clamp(2, 6) as u8;
    let rad = ((min_dim / 2).saturating_sub(u32::from(stroke_w) / 2 + 1)).max(2) as u16;
    let track = props.bg().unwrap_or(Rgb565::new(6, 12, 6));
    let start_deg = spinner_start_deg(now);
    let sweep = spinner_sweep_deg(now);

    canvas.arc(Point::new(cx, cy), rad, 0, 360, stroke_w, false, track)?;
    canvas.arc(
        Point::new(cx, cy),
        rad,
        start_deg as i16,
        sweep,
        stroke_w,
        true,
        props.accent,
    )
}

/// Inline styled fragment for [`paint_rich_text`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSpan<'a> {
    pub text: &'a str,
    pub color: Option<Rgb565>,
    pub font: Option<FontId>,
    pub underline: bool,
}

impl<'a> TextSpan<'a> {
    #[inline]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            color: None,
            font: None,
            underline: false,
        }
    }

    #[inline]
    pub const fn with_color(mut self, color: Rgb565) -> Self {
        self.color = Some(color);
        self
    }

    #[inline]
    pub const fn with_underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }
}

/// Declare a rich-text custom container rendered with [`paint_rich_text`].
#[track_caller]
pub fn rich_text(
    ui: &mut Children<'_, '_>,
    key: Key,
    width: Length,
    height: u16,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.custom(
        key,
        NodeProps::new()
            .width(width)
            .height(Length::Px(u32::from(height)))
            .fg(theme.text)
            .accent(theme.accent)
            .font(theme.font),
    )
}

/// Render inline styled [`TextSpan`]s with horizontal wrapping and optional underlines.
pub fn paint_rich_text(
    spans: &[TextSpan<'_>],
    props: &StoredProps,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let max_w = sz.width as i32;
    let line_h = 18i32;
    let mut x = 0i32;
    let mut y = 0i32;

    for span in spans {
        let color = span.color.unwrap_or(props.fg);
        let font = span.font.unwrap_or_else(|| props.font());
        let char_w = match font {
            FontId::Small12 => 7i32,
            FontId::Body16 => 8i32,
            FontId::Title20 => 10i32,
            FontId::Display28 => 14i32,
        };
        let mut rest = span.text;
        while !rest.is_empty() && y < sz.height as i32 {
            let avail_chars = ((max_w - x) / char_w).max(0) as usize;
            if avail_chars == 0 {
                x = 0;
                y += line_h;
                continue;
            }
            let take_chars = avail_chars.min(12);
            let mut take_bytes = 0usize;
            for (cnt, (idx, ch)) in rest.char_indices().enumerate() {
                if cnt >= take_chars {
                    break;
                }
                take_bytes = idx + ch.len_utf8();
            }
            if take_bytes == 0 {
                break;
            }
            let seg = &rest[..take_bytes];
            let seg_w = (seg.chars().count() as i32) * char_w;
            canvas.text(
                Rect::new(x, y, seg_w.max(char_w) as u32, line_h as u32),
                seg,
                color,
                font,
                TextAlign::Start,
            )?;
            if span.underline {
                canvas.line(
                    Point::new(x, y + line_h - 3),
                    Point::new(x + seg_w, y + line_h - 3),
                    color,
                    1,
                    false,
                )?;
            }
            x += seg_w;
            rest = &rest[take_bytes..];
            if !rest.is_empty() && x + char_w > max_w {
                x = 0;
                y += line_h;
            }
        }
    }
    Ok(())
}
