#![no_std]
#![forbid(unsafe_code)]
use liuli::prelude::*;

/// Typed route stack. Route variants carry parameters and per-page state.
/// The root always remains present; a failed push gives ownership back.
pub struct Navigator<R: Copy, const N: usize> {
    root: R,
    pages: [Option<R>; N],
    len: usize,
}
impl<R: Copy, const N: usize> Navigator<R, N> {
    pub const fn new(root: R) -> Self {
        Self {
            root,
            pages: [None; N],
            len: 0,
        }
    }
    pub fn current(&self) -> R {
        self.len
            .checked_sub(1)
            .and_then(|i| self.pages[i])
            .unwrap_or(self.root)
    }
    pub fn push(&mut self, page: R) -> Result<(), R> {
        let Some(slot) = self.pages.get_mut(self.len) else {
            return Err(page);
        };
        *slot = Some(page);
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) -> Option<R> {
        self.len = self.len.checked_sub(1)?;
        self.pages[self.len].take()
    }
}

/// A typed result delivered to the page revealed by a successful pop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteReturn<R, T> {
    pub from: R,
    pub to: R,
    pub value: T,
}
impl<R: Copy, const N: usize> Navigator<R, N> {
    pub fn pop_with<T>(&mut self, value: T) -> Result<RouteReturn<R, T>, T> {
        let Some(from) = self.pop() else {
            return Err(value);
        };
        Ok(RouteReturn {
            from,
            to: self.current(),
            value,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateCapacity {
    pub page: u8,
    pub key: Key,
}
/// Bounded component-local state kept in the application model across rebuilds.
pub struct KeyedState<T, const N: usize> {
    entries: [Option<(u8, Key, T)>; N],
}
impl<T, const N: usize> KeyedState<T, N> {
    pub const fn new() -> Self {
        Self {
            entries: [const { None }; N],
        }
    }
    pub fn get_or_insert(
        &mut self,
        page: u8,
        key: Key,
        new: impl FnOnce() -> T,
    ) -> Result<&mut T, StateCapacity> {
        let index = self
            .entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|(p, k, _)| *p == page && *k == key))
            .or_else(|| self.entries.iter().position(Option::is_none))
            .ok_or(StateCapacity { page, key })?;
        Ok(&mut self.entries[index]
            .get_or_insert_with(|| (page, key, new()))
            .2)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(u8, Key) -> bool) {
        for e in &mut self.entries {
            if e.as_ref().is_some_and(|(page, key, _)| !keep(*page, *key)) {
                *e = None;
            }
        }
    }
}
impl<T, const N: usize> Default for KeyedState<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
/// A fixed-height list reserves the offscreen extent with two spacers.
/// Item keys must be stable and distinct from the three container keys.
pub fn virtual_list(
    ui: &mut Children<'_, '_>,
    keys: [Key; 3],
    count: u32,
    row_height: core::num::NonZeroU16,
    offset: i32,
    viewport: u16,
    mut row: impl FnMut(&mut Children<'_, '_>, u32) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    let h = u32::from(row_height.get());
    let extent = count
        .checked_mul(h)
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(BuildError::InvalidRange)?;
    let y = (offset.max(0) as u32).min(extent.saturating_sub(viewport.into()));
    let first = (y / h).saturating_sub(1);
    let end = (y + u32::from(viewport))
        .div_ceil(h)
        .saturating_add(1)
        .min(count);
    let props = NodeProps {
        width: Length::Fill(1),
        height: Length::Px(viewport.into()),
        align_cross: Align::Stretch,
        ..NodeProps::new()
    };
    ui.scroll_y(keys[0], props, |list| {
        let spacer = |height| NodeProps {
            height: Length::Px(height),
            width: Length::Px(1),
            ..NodeProps::new()
        };
        list.label(keys[1], spacer(first * h))?;
        for index in first..end {
            row(list, index)?;
        }
        list.label(keys[2], spacer((count - end) * h))
    })
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub source: liuli::core::style_source::StyleSource,
    pub background: Rgb565,
    pub surface: Rgb565,
    pub text: Rgb565,
    pub accent: Rgb565,
    pub focus: Rgb565,
    pub spacing: u16,
    pub control_height: u16,
    pub font: FontId,
}
impl Theme {
    pub const DARK: Self = Self {
        source: liuli::core::style_source::StyleSource::named("Theme::DARK"),
        background: Rgb565::new(2, 6, 4),
        surface: Rgb565::new(4, 13, 7),
        text: Rgb565::new(30, 62, 28),
        accent: Rgb565::new(1, 49, 20),
        focus: Rgb565::new(30, 48, 4),
        spacing: 4,
        control_height: 32,
        font: FontId::Body16,
    };
    pub const LIGHT: Self = Self {
        source: liuli::core::style_source::StyleSource::named("Theme::LIGHT"),
        background: Rgb565::new(30, 61, 29),
        surface: Rgb565::new(26, 57, 26),
        text: Rgb565::new(2, 7, 3),
        accent: Rgb565::new(2, 29, 14),
        focus: Rgb565::new(21, 18, 1),
        spacing: 4,
        control_height: 32,
        font: FontId::Body16,
    };
    #[track_caller]
    pub fn control(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared(),
            bg: Some(self.surface),
            fg: self.text,
            accent: self.accent,
            focus_color: self.focus,
            font: self.font,
            width: Length::Fill(1),
            height: Length::Px(self.control_height.into()),
            clickable: true,
            focusable: true,
            ..NodeProps::new()
        }
    }
}

/// A threshold meter rendered entirely through the public custom-widget API.
pub fn paint_meter(props: &StoredProps, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = canvas.size();
    canvas.fill(Rect::new(0, 0, size.width, size.height), props.bg_raw)?;
    let range = i32::from(props.max) - i32::from(props.min);
    if range > 0 {
        let filled = (u64::from(size.width)
            * (i32::from(props.value) - i32::from(props.min)).clamp(0, range) as u64
            / range as u64) as u32;
        canvas.fill(Rect::new(0, 0, filled, size.height), props.accent)?;
    }
    Ok(())
}

/// Per-component overrides. An unassigned token inherits its parent theme.
#[derive(Default, Copy, Clone)]
pub struct Style {
    pub background: Option<Rgb565>,
    pub surface: Option<Rgb565>,
    pub text: Option<Rgb565>,
    pub accent: Option<Rgb565>,
    pub focus: Option<Rgb565>,
    pub font: Option<FontId>,
    pub spacing: Option<u16>,
    pub control_height: Option<u16>,
}
impl Theme {
    #[track_caller]
    pub fn with(self, style: Style) -> Self {
        Self {
            source: self.source.scoped([
                style.background.is_some(),
                style.surface.is_some(),
                style.text.is_some(),
                style.accent.is_some(),
                style.focus.is_some(),
                style.font.is_some(),
                style.spacing.is_some(),
                style.control_height.is_some(),
            ]),
            background: style.background.unwrap_or(self.background),
            surface: style.surface.unwrap_or(self.surface),
            text: style.text.unwrap_or(self.text),
            accent: style.accent.unwrap_or(self.accent),
            focus: style.focus.unwrap_or(self.focus),
            font: style.font.unwrap_or(self.font),
            spacing: style.spacing.unwrap_or(self.spacing),
            control_height: style.control_height.unwrap_or(self.control_height),
        }
    }
}
/// Numeric editing via touch drag or rotary steps. Actions carry the clamped value.
pub fn number_editor(
    ui: &mut Children<'_, '_>,
    key: Key,
    range: core::ops::RangeInclusive<i16>,
    value: i16,
    step: i16,
    theme: Theme,
) -> Result<(), BuildError> {
    let (min, max) = range.into_inner();
    if min > max || step <= 0 {
        return Err(BuildError::InvalidRange);
    }
    ui.slider(
        key,
        NodeProps {
            min,
            max,
            value: value.clamp(min, max),
            step,
            ..theme.control()
        },
    )
}
/// A mutually exclusive set of buttons. Each action uses the caller's stable key.
pub fn selector(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.column(
        key,
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |choices| choice_buttons(choices, items, selected, theme),
    )
}
pub fn tabs(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.row(
        key,
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |bar| choice_buttons(bar, items, selected, theme),
    )
}
fn choice_buttons(
    ui: &mut Children<'_, '_>,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    for &(key, text) in items {
        let chosen = key == selected;
        ui.button(
            key,
            NodeProps {
                text,
                checked: chosen,
                border_color: chosen.then_some(theme.accent),
                border_width: 1,
                ..theme.control()
            },
        )?;
    }
    Ok(())
}
/// Modal focus and Back are managed by the engine. The application handles the two action keys.
pub fn confirm_dialog(
    ui: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    ui.modal(
        keys[0],
        NodeProps {
            width: Length::Px(180),
            height: Length::Content,
            bg: Some(theme.surface),
            overlay_alpha: 10,
            ..NodeProps::new()
        },
        |modal| {
            modal.column(
                keys[1],
                NodeProps {
                    width: Length::Fill(1),
                    gap: theme.spacing,
                    ..NodeProps::new()
                },
                |actions| {
                    for (key, text) in [keys[2], keys[3]].into_iter().zip(labels) {
                        actions.button(
                            key,
                            NodeProps {
                                text,
                                ..theme.control()
                            },
                        )?;
                    }
                    Ok(())
                },
            )
        },
    )
}
/// Insert last in an overlay stack. The application owns the dismissal deadline.
pub fn toast(
    ui: &mut Children<'_, '_>,
    key: Key,
    text: TextRef,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.label(
        key,
        NodeProps {
            text,
            width: Length::Fill(1),
            height: Length::Px(theme.control_height.into()),
            bg: Some(theme.surface),
            fg: theme.text,
            font: theme.font,
            text_align: TextAlign::Center,
            ..NodeProps::new()
        },
    )
}
