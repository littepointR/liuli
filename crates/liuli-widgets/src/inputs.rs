use liuli::prelude::*;

use crate::Theme;

fn checked_progress(props: &StoredProps, state: PaintState) -> i32 {
    if props.has_state_motion() {
        state.anim_progress
    } else {
        i32::from(state.checked) * 1000
    }
}

/// Paint a checkbox box, checkmark stroke, and label onto a `WidgetCanvas`.
pub fn paint_checkbox(
    props: &StoredProps,
    state: PaintState,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let parts = *canvas.parts();
    let accent = parts.indicator_color().unwrap_or(props.accent);
    let box_sz = u32::from(parts.track_thickness().unwrap_or(18))
        .min(sz.height.saturating_sub(4))
        .max(8);
    let by = ((sz.height.saturating_sub(box_sz)) / 2) as i32;
    let bx = 2i32;
    let progress = checked_progress(props, state);
    let fill = liuli::core::paint::blend_rgb565_scale(
        parts
            .track_color()
            .or(canvas.background())
            .unwrap_or(Rgb565::new(4, 8, 4)),
        accent,
        progress,
        1000,
    );
    let border = if state.focused {
        parts.focused_border().unwrap_or(props.focus_color)
    } else if state.checked {
        accent
    } else {
        canvas.foreground()
    };
    canvas.rounded_rect(
        Rect::new(bx, by, box_sz, box_sz),
        fill,
        parts.track_radius().unwrap_or(4),
        Some(parts.track_border_color().unwrap_or(border)),
        parts.track_border_width().unwrap_or(1),
    )?;
    if progress >= 500 {
        let p0 = Point::new(bx + (box_sz as i32) / 4, by + (box_sz as i32) / 2);
        let p1 = Point::new(bx + (box_sz as i32 * 5) / 11, by + (box_sz as i32 * 3) / 4);
        let p2 = Point::new(bx + (box_sz as i32 * 4) / 5, by + (box_sz as i32) / 4);
        let tick = parts
            .icon_color()
            .or(props.bg())
            .unwrap_or(Rgb565::new(31, 63, 31));
        canvas.line(p0, p1, tick, 2, true)?;
        canvas.line(p1, p2, tick, 2, true)?;
    }
    let tx = bx + box_sz as i32 + 8;
    let tw = sz.width.saturating_sub(tx as u32);
    if tw > 0 {
        canvas.node_text(
            Rect::new(tx, 0, tw, sz.height),
            props,
            if state.enabled || parts.disabled_fg().is_some() {
                canvas.foreground()
            } else {
                Rgb565::new(14, 28, 14)
            },
            TextAlign::Start,
        )?;
    }
    Ok(())
}

/// Declare an accessible focusable checkbox row.
#[track_caller]
pub fn checkbox(
    ui: &mut Children<'_, '_>,
    key: Key,
    label: TextRef,
    checked: bool,
    theme: Theme,
) -> Result<(), BuildError> {
    let h = theme.control_height.max(24);
    ui.custom(
        key,
        NodeProps::new()
            .with_parts(theme.parts.resolve(crate::WidgetClass::CHECKBOX))
            .width(Length::Fill(1))
            .height(Length::Px(u32::from(h)))
            .bg(theme.surface_alt)
            .fg(theme.text)
            .accent(theme.accent)
            .font(theme.font)
            .text(label)
            .state_motion(theme.motion.state(MotionState::Checked, None))
            .checked(checked)
            .focusable(true)
            .clickable(true),
    )
}

/// Paint a circular radio selector indicator and label onto a `WidgetCanvas`.
pub fn paint_radio(
    props: &StoredProps,
    state: PaintState,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let parts = *canvas.parts();
    let accent = parts.indicator_color().unwrap_or(props.accent);
    let rad = (sz.height.saturating_sub(8) / 2).clamp(5, 10) as u16;
    let cy = (sz.height / 2) as i32;
    let cx = i32::from(rad) + 3;
    let border = if state.focused {
        parts.focused_border().unwrap_or(props.focus_color)
    } else if state.checked {
        accent
    } else {
        canvas.foreground()
    };
    canvas.circle(
        Point::new(cx, cy),
        rad,
        parts.track_color().or(canvas.background()),
        Some(parts.track_border_color().unwrap_or(border)),
        parts.track_border_width().unwrap_or(2),
    )?;
    let dot_r = (i32::from((rad / 2).max(2)) * checked_progress(props, state) / 1000) as u16;
    if dot_r > 0 {
        canvas.circle(Point::new(cx, cy), dot_r, Some(accent), None, 0)?;
    }
    let tx = cx + i32::from(rad) + 8;
    let tw = sz.width.saturating_sub(tx as u32);
    if tw > 0 {
        canvas.node_text(
            Rect::new(tx, 0, tw, sz.height),
            props,
            canvas.foreground(),
            TextAlign::Start,
        )?;
    }
    Ok(())
}

/// Declare an accessible focusable radio button row.
#[track_caller]
pub fn radio(
    ui: &mut Children<'_, '_>,
    key: Key,
    label: TextRef,
    selected: bool,
    theme: Theme,
) -> Result<(), BuildError> {
    let h = theme.control_height.max(24);
    ui.custom(
        key,
        NodeProps::new()
            .with_parts(theme.parts.resolve(crate::WidgetClass::RADIO))
            .width(Length::Fill(1))
            .height(Length::Px(u32::from(h)))
            .bg(theme.surface_alt)
            .fg(theme.text)
            .accent(theme.accent)
            .font(theme.font)
            .text(label)
            .state_motion(theme.motion.state(MotionState::Checked, None))
            .checked(selected)
            .focusable(true)
            .clickable(true),
    )
}

/// Pure value state for a collapsible [`dropdown`] control.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub struct DropdownState {
    pub open: bool,
    pub selected: usize,
}

impl DropdownState {
    #[inline]
    pub const fn new(selected: usize) -> Self {
        Self {
            open: false,
            selected,
        }
    }

    pub fn handle_action(
        &mut self,
        key: Key,
        trigger_key: Key,
        item_base_key: u32,
        option_count: usize,
    ) -> bool {
        if key == trigger_key {
            self.open = !self.open;
            return true;
        }
        if key.0 >= item_base_key {
            let idx = (key.0 - item_base_key) as usize;
            if idx < option_count {
                self.selected = idx;
                self.open = false;
                return true;
            }
        }
        false
    }
}

/// Declare a collapsible dropdown button and expandable option list.
#[track_caller]
#[allow(clippy::too_many_arguments)]
pub fn dropdown(
    ui: &mut Children<'_, '_>,
    wrap_key: Key,
    trigger_key: Key,
    list_key: Key,
    item_base_key: u32,
    options: &[&'static str],
    state: DropdownState,
    theme: Theme,
) -> Result<(), BuildError> {
    let current = options
        .get(state.selected)
        .copied()
        .or_else(|| options.first().copied())
        .unwrap_or("");
    let col_props = NodeProps::new()
        .width(Length::Fill(1))
        .height(Length::Content)
        .gap(4);
    ui.column(wrap_key, col_props, |col| {
        col.button(
            trigger_key,
            NodeProps::new()
                .width(Length::Fill(1))
                .height(Length::Px(u32::from(theme.control_height)))
                .bg(theme.surface_alt)
                .fg(theme.text)
                .accent(theme.accent)
                .border(theme.border, 1)
                .radius(theme.radius)
                .font(theme.font)
                .text(TextRef::Static(current))
                .icon(IconKind::ChevronRight)
                .focusable(true)
                .clickable(true),
        )?;
        if state.open && !options.is_empty() {
            let panel = NodeProps::new()
                .width(Length::Fill(1))
                .height(Length::Content)
                .padding(EdgeInsets::all(4))
                .gap(2)
                .bg(theme.surface)
                .border(theme.accent, 1)
                .radius(theme.radius);
            col.column(list_key, panel, |list| {
                for (i, &label) in options.iter().enumerate() {
                    let is_sel = i == state.selected;
                    list.button(
                        Key(item_base_key + i as u32),
                        NodeProps::new()
                            .width(Length::Fill(1))
                            .height(Length::Px(u32::from(
                                theme.control_height.saturating_sub(4).max(20),
                            )))
                            .bg(if is_sel {
                                theme.accent
                            } else {
                                theme.surface_alt
                            })
                            .fg(if is_sel { theme.background } else { theme.text })
                            .radius(theme.radius.saturating_sub(2))
                            .font(theme.font)
                            .text(TextRef::Static(label))
                            .text_align(TextAlign::Start)
                            .focusable(true)
                            .clickable(true),
                    )?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })
}

/// Pure state for a vertical wheel [`roller`] picker.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub struct RollerState {
    pub selected: usize,
    pub wrap: bool,
}

impl RollerState {
    #[inline]
    pub const fn new(selected: usize, wrap: bool) -> Self {
        Self { selected, wrap }
    }

    pub fn step(&mut self, delta: i32, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        let next = if self.wrap {
            (self.selected as isize + delta as isize).rem_euclid(count as isize) as usize
        } else {
            (self.selected as isize + delta as isize).clamp(0, count as isize - 1) as usize
        };
        if next != self.selected {
            self.selected = next;
            true
        } else {
            false
        }
    }

    pub fn handle_action(&mut self, action: ActionKind, count: usize) -> bool {
        match action {
            ActionKind::Adjust(steps) => self.step(i32::from(steps), count),
            ActionKind::Activate => self.step(1, count),
            _ => false,
        }
    }
}

/// Declare an interactive wheel picker backed by `NodeKind::Custom`.
#[track_caller]
pub fn roller(
    ui: &mut Children<'_, '_>,
    key: Key,
    width: Length,
    height: u16,
    options: &[&'static str],
    state: RollerState,
    theme: Theme,
) -> Result<(), BuildError> {
    let max_idx = options.len().saturating_sub(1).min(i16::MAX as usize) as i16;
    let cur = state.selected.min(max_idx.max(0) as usize) as i16;
    let cur_str = options.get(cur as usize).copied().unwrap_or("");
    ui.custom(
        key,
        NodeProps::new()
            .width(width)
            .height(Length::Px(u32::from(height)))
            .bg(theme.surface)
            .fg(theme.text)
            .accent(theme.accent)
            .border(theme.border, 1)
            .radius(theme.radius)
            .font(theme.font)
            .text(TextRef::Static(cur_str))
            .range(0, max_idx, 1)
            .value(cur)
            .editable(true)
            .focusable(true)
            .clickable(true),
    )
}

/// Draw a roller's previous/current/next rows and center selection highlight bar.
pub fn paint_roller(
    options: &[&'static str],
    state: RollerState,
    props: &StoredProps,
    paint_state: PaintState,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let row_h = (sz.height / 3).max(16);
    let mid_y = ((sz.height.saturating_sub(row_h)) / 2) as i32;
    canvas.rounded_rect(
        Rect::new(0, 0, sz.width, sz.height),
        props.bg().unwrap_or(Rgb565::new(2, 4, 2)),
        4,
        Some(if paint_state.focused {
            props.focus_color
        } else {
            props.border_color().unwrap_or(props.fg)
        }),
        1,
    )?;
    canvas.rounded_rect(
        Rect::new(2, mid_y, sz.width.saturating_sub(4), row_h),
        props.accent,
        4,
        None,
        0,
    )?;
    if options.is_empty() {
        return Ok(());
    }
    let count = options.len();
    let cur = state.selected.min(count - 1);
    let dim = Rgb565::new(15, 30, 15);
    for (slot_off, row_y, col) in [
        (-1isize, mid_y - row_h as i32, dim),
        (0, mid_y, props.fg),
        (1, mid_y + row_h as i32, dim),
    ] {
        if row_y + (row_h as i32) <= 0 || row_y >= sz.height as i32 {
            continue;
        }
        let target = cur as isize + slot_off;
        let idx = if state.wrap {
            Some(target.rem_euclid(count as isize) as usize)
        } else if (0..count as isize).contains(&target) {
            Some(target as usize)
        } else {
            None
        };
        if let Some(i) = idx {
            canvas.text(
                Rect::new(4, row_y + 2, sz.width.saturating_sub(8), row_h),
                options[i],
                col,
                props.font(),
                TextAlign::Center,
            )?;
        }
    }
    Ok(())
}

/// Zero-heap multi-line UTF-8 text buffer with line/column cursor tracking.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct MultiLineState<const CAP: usize> {
    buf: [u8; CAP],
    len: usize,
    cursor: usize,
    scroll_line: usize,
    rev: u32,
}

impl<const CAP: usize> MultiLineState<CAP> {
    pub const fn new() -> Self {
        Self {
            buf: [0; CAP],
            len: 0,
            cursor: 0,
            scroll_line: 0,
            rev: 1,
        }
    }

    pub fn from_text(initial: &str) -> Self {
        let mut s = Self::new();
        let _ = s.insert_str(initial);
        s
    }

    #[inline]
    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    #[inline]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    #[inline]
    pub const fn revision(&self) -> u32 {
        self.rev
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.text()
            .as_bytes()
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1
    }

    /// Returns `(zero_based_line, zero_based_byte_col_in_line)` at the current cursor.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let before = &self.buf[..self.cursor.min(self.len)];
        let line = before.iter().filter(|&&b| b == b'\n').count();
        let col = match before.iter().rposition(|&b| b == b'\n') {
            Some(pos) => before.len() - (pos + 1),
            None => before.len(),
        };
        (line, col)
    }

    pub fn insert_str(&mut self, s: &str) -> bool {
        let bytes = s.as_bytes();
        let avail = CAP.saturating_sub(self.len);
        if bytes.is_empty() || avail == 0 {
            return false;
        }
        let mut take = bytes.len().min(avail);
        while take > 0 && !s.is_char_boundary(take) {
            take -= 1;
        }
        if take == 0 {
            return false;
        }
        self.buf
            .copy_within(self.cursor..self.len, self.cursor + take);
        self.buf[self.cursor..self.cursor + take].copy_from_slice(&bytes[..take]);
        self.len += take;
        self.cursor += take;
        self.rev = self.rev.wrapping_add(1).max(1);
        true
    }

    pub fn insert_newline(&mut self) -> bool {
        self.insert_str("\n")
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let s = self.text();
        let mut prev = self.cursor - 1;
        while prev > 0 && !s.is_char_boundary(prev) {
            prev -= 1;
        }
        let rem = self.len - self.cursor;
        self.buf.copy_within(self.cursor..self.len, prev);
        self.len = prev + rem;
        self.cursor = prev;
        self.rev = self.rev.wrapping_add(1).max(1);
        true
    }

    pub fn move_horizontal(&mut self, forward: bool) -> bool {
        let s = self.text();
        if forward {
            if self.cursor >= self.len {
                return false;
            }
            let mut next = self.cursor + 1;
            while next < self.len && !s.is_char_boundary(next) {
                next += 1;
            }
            self.cursor = next;
            self.rev = self.rev.wrapping_add(1).max(1);
            true
        } else {
            if self.cursor == 0 {
                return false;
            }
            let mut prev = self.cursor - 1;
            while prev > 0 && !s.is_char_boundary(prev) {
                prev -= 1;
            }
            self.cursor = prev;
            self.rev = self.rev.wrapping_add(1).max(1);
            true
        }
    }

    pub fn move_vertical(&mut self, down: bool) -> bool {
        let (cur_line, cur_col) = self.cursor_line_col();
        let target_line = if down {
            cur_line + 1
        } else if cur_line == 0 {
            return false;
        } else {
            cur_line - 1
        };
        let mut line_idx = 0usize;
        let mut start = 0usize;
        for (i, &b) in self.buf[..self.len].iter().enumerate() {
            if b == b'\n' {
                if line_idx == target_line {
                    let end = i;
                    let mut next = (start + cur_col).min(end);
                    while next > start && !self.text().is_char_boundary(next) {
                        next -= 1;
                    }
                    self.cursor = next;
                    self.rev = self.rev.wrapping_add(1).max(1);
                    return true;
                }
                line_idx += 1;
                start = i + 1;
            }
        }
        if line_idx == target_line {
            let end = self.len;
            let mut next = (start + cur_col).min(end);
            while next > start && !self.text().is_char_boundary(next) {
                next -= 1;
            }
            self.cursor = next;
            self.rev = self.rev.wrapping_add(1).max(1);
            return true;
        }
        false
    }

    pub fn apply_text(&mut self, input: TextInput<'_>) -> bool {
        match input {
            TextInput::Commit(s) => self.insert_str(s),
            TextInput::Backspace => self.backspace(),
            TextInput::Left => self.move_horizontal(false),
            TextInput::Right => self.move_horizontal(true),
            TextInput::Compose(_) | TextInput::Cancel => false,
        }
    }

    pub fn apply_nav(&mut self, dir: NavDir) -> bool {
        match dir {
            NavDir::Left | NavDir::Prev => self.move_horizontal(false),
            NavDir::Right | NavDir::Next => self.move_horizontal(true),
            NavDir::Up => self.move_vertical(false),
            NavDir::Down => self.move_vertical(true),
        }
    }
}

impl<const CAP: usize> Default for MultiLineState<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

/// Declare a focusable multi-line editor container backed by `NodeKind::Custom`.
#[track_caller]
pub fn multi_line_editor<const CAP: usize>(
    ui: &mut Children<'_, '_>,
    key: Key,
    width: Length,
    height: u16,
    state: &MultiLineState<CAP>,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.custom(
        key,
        NodeProps::new()
            .width(width)
            .height(Length::Px(u32::from(height)))
            .bg(theme.surface)
            .fg(theme.text)
            .accent(theme.accent)
            .border(theme.border, 1)
            .radius(theme.radius)
            .font(theme.font)
            .editable(true)
            .focusable(true)
            .clickable(true)
            .pointer_capture(PointerCaptureMode::Free),
    )?;
    ui.set_custom_rev(key, state.revision())
}

/// Paint visible lines and caret indicator for a `MultiLineState` onto `WidgetCanvas`.
pub fn paint_multi_line_editor<const CAP: usize>(
    state: &MultiLineState<CAP>,
    props: &StoredProps,
    paint_state: PaintState,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    let bg = props.bg().unwrap_or(Rgb565::new(2, 4, 2));
    let border = if paint_state.focused {
        props.focus_color
    } else {
        props.border_color().unwrap_or(props.fg)
    };
    canvas.rounded_rect(Rect::new(0, 0, sz.width, sz.height), bg, 4, Some(border), 1)?;

    let line_h = 18i32;
    let pad = 6i32;
    let (cur_line, cur_col) = state.cursor_line_col();
    let max_rows = ((sz.height as i32 - pad * 2) / line_h).max(1) as usize;
    let first_line = state
        .scroll_line
        .max(cur_line.saturating_add(1).saturating_sub(max_rows));
    let inner_w = sz.width.saturating_sub((pad * 2) as u32) as u16;

    for (line_idx, line) in state.text().split('\n').enumerate() {
        if line_idx < first_line {
            continue;
        }
        let vis_row = line_idx - first_line;
        if vis_row >= max_rows {
            break;
        }
        let y = pad + vis_row as i32 * line_h;
        if !line.is_empty() {
            // Paint line chunk cleanly using inline_text for zero-alloc slice rendering (<=12B chunks).
            let mut x = pad;
            let mut rest = line;
            while !rest.is_empty() && x < sz.width as i32 - pad {
                let mut take = rest.len().min(12);
                while !rest.is_char_boundary(take) {
                    take -= 1;
                }
                if take == 0 {
                    break;
                }
                let seg = &rest[..take];
                canvas.text(
                    Rect::new(x, y, u32::from(inner_w), line_h as u32),
                    seg,
                    props.fg,
                    props.font(),
                    TextAlign::Start,
                )?;
                x += (seg.chars().count() as i32) * 8;
                rest = &rest[take..];
            }
        }
        if paint_state.focused && line_idx == cur_line {
            let prefix_chars = line[..cur_col.min(line.len())].chars().count() as i32;
            let cx = (pad + prefix_chars * 8).min(sz.width as i32 - pad);
            canvas.fill(
                Rect::new(cx, y, 2, (line_h - 2).max(4) as u32),
                props.accent,
            )?;
        }
    }
    Ok(())
}
