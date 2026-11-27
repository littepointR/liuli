//! A controlled tab set. Pages use stable keys for input and typed IDs for application state.
use crate::Theme;
use liuli::prelude::*;

pub use liuli::core::view::TabLayout;

pub struct TabPage<Id, M> {
    pub id: Id,
    pub key: Key,
    pub title: TextRef,
    pub icon: IconKind,
    pub build: fn(&M, Theme, &mut Children<'_, '_>) -> Result<(), BuildError>,
}

impl<Id, M> TabPage<Id, M> {
    pub const fn new(
        id: Id,
        key: Key,
        title: TextRef,
        build: fn(&M, Theme, &mut Children<'_, '_>) -> Result<(), BuildError>,
    ) -> Self {
        Self {
            id,
            key,
            title,
            build,
            icon: IconKind::None,
        }
    }
}

impl<Id, M> TabPage<Id, M> {
    pub const fn with_icon(mut self, icon: IconKind) -> Self {
        self.icon = icon;
        self
    }
}

/// The header, action mapping and lazy panel share the same page declarations.
pub struct Tabs<'a, Id, M> {
    pages: &'a [TabPage<Id, M>],
    selected: &'a TabPage<Id, M>,
}
impl<'a, Id: Copy + Eq, M> Tabs<'a, Id, M> {
    pub fn new(pages: &'a [TabPage<Id, M>], selected: Id) -> Result<Self, BuildError> {
        for (i, p) in pages.iter().enumerate() {
            if pages[..i]
                .iter()
                .any(|other| other.id == p.id || other.key == p.key)
            {
                return Err(BuildError::DuplicateKey);
            }
        }
        let selected = pages
            .iter()
            .find(|p| p.id == selected)
            .ok_or(BuildError::InvalidRange)?;
        Ok(Self { pages, selected })
    }
    pub fn selection(&self, event: UiAction) -> Option<Id> {
        if event.kind != ActionKind::Activate {
            return None;
        }
        self.pages.iter().find(|p| p.key == event.key).map(|p| p.id)
    }
    /// Build the header and lazy panel inside the caller's column.
    pub fn view(
        &self,
        ui: &mut Children<'_, '_>,
        keys: [Key; 2],
        model: &M,
        t: Theme,
    ) -> Result<(), BuildError> {
        self.header(ui, keys[0], TabLayout::Scroll, t)?;
        self.panel(
            ui,
            keys[1],
            NodeProps::new().size(Length::Fill(1), Length::Fill(1)),
            model,
            t,
        )
    }

    pub fn header(
        &self,
        ui: &mut Children<'_, '_>,
        key: Key,
        layout: TabLayout,
        t: Theme,
    ) -> Result<(), BuildError> {
        self.header_with(ui, key, layout, t, |_| Ok(()), |_| Ok(()))
    }
    /// Accessories use their measured widths. The strip receives a separate line when needed.
    pub fn header_with(
        &self,
        ui: &mut Children<'_, '_>,
        key: Key,
        layout: TabLayout,
        t: Theme,
        leading: impl FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
        trailing: impl FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
    ) -> Result<(), BuildError> {
        ui.tab_bar(key, layout, bar_props(t), |bar| {
            leading(bar)?;
            for page in self.pages {
                tab_item(bar, page.key, page.title, page.icon, self.selected.key, t)?;
            }
            trailing(bar)
        })
    }
    pub fn panel(
        &self,
        ui: &mut Children<'_, '_>,
        key: Key,
        props: NodeProps,
        model: &M,
        t: Theme,
    ) -> Result<(), BuildError> {
        ui.tab_panel(key, self.selected.key, props, |panel| {
            (self.selected.build)(model, t, panel)
        })
    }
}

fn bar_props(t: Theme) -> NodeProps {
    NodeProps {
        scroll: Some(t.scroll_policy(ScrollPolicy::FREE)),
        width: Length::Fill(1),
        height: Length::Content,
        gap: t.spacing,
        bg: Some(t.surface),
        style_source: t.source.declared().with_variant("tab-bar"),
        ..NodeProps::new()
    }
}
fn tab_item(
    ui: &mut Children<'_, '_>,
    key: Key,
    title: TextRef,
    icon: IconKind,
    selected: Key,
    t: Theme,
) -> Result<(), BuildError> {
    let checked = key == selected;
    let props = NodeProps {
        parts: t.parts.resolve(crate::WidgetClass::TAB),
        width: Length::Content,
        height: Length::Content,
        text: title,
        icon,
        checked,
        min: t.density.control_height as i16,
        max_lines: 1,
        ellipsis: true,
        focusable: true,
        clickable: true,
        fg: if checked { t.accent } else { t.text },
        accent: t.accent,
        overlay_color: t.border,
        align_cross: Align::Center,
        padding: EdgeInsets::symmetric(t.density.pad_x, t.density.pad_y),
        font: t.typography.body,
        letter_spacing: t.typography.letter_spacing,
        focus_color: t.focus,
        focus_ring: FocusRingStyle {
            offset: 0,
            ..t.interaction.focus
        },
        press_feedback: t.interaction.press_surface,
        style_source: t.source.declared().with_variant("tab"),
        ..NodeProps::new()
    }
    .state_motion(t.motion.state(MotionState::Checked, None));
    ui.leaf(key, NodeKind::Tab, props)
}

/// A header for callers that manage the panel separately.
pub fn tab_strip(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    layout: TabLayout,
    t: Theme,
) -> Result<(), BuildError> {
    ui.tab_bar(key, layout, bar_props(t), |bar| {
        for &(id, title) in items {
            tab_item(bar, id, title, IconKind::None, selected, t)?;
        }
        Ok(())
    })
}
pub fn tabs(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    t: Theme,
) -> Result<(), BuildError> {
    tab_strip(ui, key, items, selected, TabLayout::Scroll, t)
}
/// Compatibility interface for applications explicitly requesting a fixed column count.
pub fn tabs_in_columns(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    columns: u8,
    t: Theme,
) -> Result<(), BuildError> {
    ui.tab_bar(key, TabLayout::Columns(columns), bar_props(t), |bar| {
        for &(id, title) in items {
            tab_item(bar, id, title, IconKind::None, selected, t)?;
        }
        Ok(())
    })
}
