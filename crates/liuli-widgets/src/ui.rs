use core::fmt;
use liuli::prelude::*;

use crate::style::{ButtonVariant, Elevation, SurfaceSpec};
use crate::theme::{Style, Theme};
use crate::tokens::{Density, EffectsConfig, Shape};
use crate::{centered_confirm_dialog, confirm_dialog, number_editor, selector, tabs, toast};

/// Stack-scoped UI builder pairing `Children` with a `Theme` for consistent styling,
/// automatic `SurfaceSpec` material/effect resolution, and clean local theme scoping.
pub struct ThemedUi<'a, 'b, 'p> {
    ui: &'p mut Children<'a, 'b>,
    theme: Theme,
}

impl<'a, 'b, 'p> ThemedUi<'a, 'b, 'p> {
    #[inline]
    pub fn new(ui: &'p mut Children<'a, 'b>, theme: Theme) -> Self {
        Self { ui, theme }
    }

    #[inline]
    pub fn theme(&self) -> Theme {
        self.theme
    }

    #[inline]
    pub fn raw(&mut self) -> &mut Children<'a, 'b> {
        self.ui
    }

    #[inline]
    pub fn with_theme<R>(
        &mut self,
        theme: Theme,
        f: impl FnOnce(&mut ThemedUi<'a, '_, '_>) -> R,
    ) -> R {
        let mut child = ThemedUi { ui: self.ui, theme };
        f(&mut child)
    }

    #[inline]
    #[track_caller]
    pub fn with_style<R>(
        &mut self,
        style: Style,
        f: impl FnOnce(&mut ThemedUi<'a, '_, '_>) -> R,
    ) -> R {
        let next = self.theme.with(style);
        self.with_theme(next, f)
    }

    #[inline]
    #[track_caller]
    pub fn with_effects<R>(
        &mut self,
        effects: EffectsConfig,
        f: impl FnOnce(&mut ThemedUi<'a, '_, '_>) -> R,
    ) -> R {
        let next = self.theme.with_effects(effects);
        self.with_theme(next, f)
    }

    #[inline]
    #[track_caller]
    pub fn with_density<R>(
        &mut self,
        density: Density,
        f: impl FnOnce(&mut ThemedUi<'a, '_, '_>) -> R,
    ) -> R {
        let next = self.theme.with_density(density);
        self.with_theme(next, f)
    }

    #[inline]
    #[track_caller]
    pub fn with_shape<R>(
        &mut self,
        shape: Shape,
        f: impl FnOnce(&mut ThemedUi<'a, '_, '_>) -> R,
    ) -> R {
        let next = self.theme.with_shape(shape);
        self.with_theme(next, f)
    }

    #[inline]
    pub fn row<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        self.ui.row(key, props, |c| f(&mut ThemedUi::new(c, t)))
    }

    #[inline]
    pub fn column<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        self.ui.column(key, props, |c| f(&mut ThemedUi::new(c, t)))
    }

    #[inline]
    pub fn stack<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        self.ui.stack(key, props, |c| f(&mut ThemedUi::new(c, t)))
    }

    #[inline]
    pub fn scroll_y<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        self.ui
            .scroll_y(key, props, |c| f(&mut ThemedUi::new(c, t)))
    }

    /// Declare a themed column container with resolved `SurfaceSpec` props & effects.
    #[track_caller]
    pub fn surface_column<F>(
        &mut self,
        key: Key,
        spec: SurfaceSpec,
        base: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        let resolved = t.resolve_surface(spec, base);
        self.ui
            .column(key, resolved.props, |c| f(&mut ThemedUi::new(c, t)))?;
        self.decorate_if_needed(key, resolved.effect)
    }

    /// Declare a themed row container with resolved `SurfaceSpec` props & effects.
    #[track_caller]
    pub fn surface_row<F>(
        &mut self,
        key: Key,
        spec: SurfaceSpec,
        base: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        let resolved = t.resolve_surface(spec, base);
        self.ui
            .row(key, resolved.props, |c| f(&mut ThemedUi::new(c, t)))?;
        self.decorate_if_needed(key, resolved.effect)
    }

    /// Declare a themed stack container with resolved `SurfaceSpec` props & effects.
    #[track_caller]
    pub fn surface_stack<F>(
        &mut self,
        key: Key,
        spec: SurfaceSpec,
        base: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let t = self.theme;
        let resolved = t.resolve_surface(spec, base);
        self.ui
            .stack(key, resolved.props, |c| f(&mut ThemedUi::new(c, t)))?;
        self.decorate_if_needed(key, resolved.effect)
    }

    /// Declare a standard card container (vertical stretch column with card padding and spacing).
    #[track_caller]
    pub fn card<F>(&mut self, key: Key, spec: SurfaceSpec, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut ThemedUi<'a, '_, '_>) -> Result<(), BuildError>,
    {
        let base = self.theme.card();
        self.surface_column(key, spec, base, f)
    }

    #[track_caller]
    pub fn button(
        &mut self,
        key: Key,
        label: &'static str,
        icon: IconKind,
        variant: ButtonVariant,
    ) -> Result<(), BuildError> {
        self.ui.button(
            key,
            NodeProps {
                text: TextRef::Static(label),
                icon,
                ..self.theme.button(variant)
            },
        )
    }

    #[track_caller]
    pub fn button_enabled(
        &mut self,
        key: Key,
        label: &'static str,
        icon: IconKind,
        variant: ButtonVariant,
        enabled: bool,
    ) -> Result<(), BuildError> {
        self.ui.button(
            key,
            NodeProps {
                text: TextRef::Static(label),
                icon,
                ..self.theme.button_state(variant, enabled)
            },
        )
    }

    #[track_caller]
    pub fn button_props(
        &mut self,
        key: Key,
        variant: ButtonVariant,
        overrides: NodeProps,
    ) -> Result<(), BuildError> {
        let base = self.theme.button(variant);
        self.ui.button(
            key,
            NodeProps {
                style_source: base.style_source,
                ..overrides
            },
        )
    }

    #[track_caller]
    pub fn elevated_button(
        &mut self,
        key: Key,
        label: &'static str,
        icon: IconKind,
        variant: ButtonVariant,
        elevation: Elevation,
    ) -> Result<(), BuildError> {
        self.button(key, label, icon, variant)?;
        self.decorate_if_needed(key, self.theme.elevation(elevation))
    }

    #[track_caller]
    pub fn title(&mut self, key: Key, text: TextRef) -> Result<(), BuildError> {
        self.ui.label(
            key,
            NodeProps {
                text,
                ..self.theme.title()
            },
        )
    }

    #[track_caller]
    pub fn display(&mut self, key: Key, text: TextRef) -> Result<(), BuildError> {
        self.ui.label(
            key,
            NodeProps {
                text,
                ..self.theme.display()
            },
        )
    }

    #[track_caller]
    pub fn paragraph(&mut self, key: Key, text: TextRef) -> Result<(), BuildError> {
        self.ui.label(
            key,
            NodeProps {
                text,
                ..self.theme.paragraph()
            },
        )
    }

    #[track_caller]
    pub fn caption(&mut self, key: Key, text: TextRef) -> Result<(), BuildError> {
        self.ui.label(
            key,
            NodeProps {
                text,
                ..self.theme.caption()
            },
        )
    }

    #[track_caller]
    pub fn label(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.ui.label(key, props)
    }

    #[track_caller]
    pub fn toggle(&mut self, key: Key, checked: bool) -> Result<(), BuildError> {
        self.ui.toggle(key, self.theme.toggle(checked))
    }

    /// Standard labeled card row with a stretched text label and a trailing `Toggle`.
    #[track_caller]
    pub fn toggle_row(
        &mut self,
        row_key: Key,
        label_key: Key,
        toggle_key: Key,
        label: TextRef,
        checked: bool,
    ) -> Result<(), BuildError> {
        let t = self.theme;
        let row_props = NodeProps {
            padding: EdgeInsets::symmetric(t.density.pad_x, t.density.pad_y + 2),
            height: Length::Px(u32::from(t.density.control_height).saturating_add(8)),
            align_cross: Align::Center,
            ..t.card()
        };
        self.ui.row(row_key, row_props, |r| {
            r.label(
                label_key,
                NodeProps {
                    text: label,
                    fg: t.text,
                    font: t.font,
                    wrap: t.typography.wrap,
                    text_gamma: t.typography.gamma,
                    subpixel: t.typography.subpixel,
                    letter_spacing: t.typography.letter_spacing,
                    line_spacing: t.typography.line_spacing,
                    align_cross: Align::Center,
                    width: Length::Fill(1),
                    height: Length::Content,
                    style_source: t.source.declared(),
                    ..NodeProps::new()
                },
            )?;
            r.toggle(toggle_key, t.toggle(checked))
        })
    }

    #[track_caller]
    pub fn slider(&mut self, key: Key, value: i16) -> Result<(), BuildError> {
        self.ui.slider(key, self.theme.slider(value))
    }

    #[track_caller]
    pub fn slider_range(
        &mut self,
        key: Key,
        min: i16,
        max: i16,
        step: i16,
        value: i16,
    ) -> Result<(), BuildError> {
        self.ui.slider(
            key,
            NodeProps {
                min,
                max,
                step,
                ..self.theme.slider(value.clamp(min, max))
            },
        )
    }

    #[track_caller]
    pub fn progress_bar(&mut self, key: Key, value: i16) -> Result<(), BuildError> {
        self.ui.progress_bar(key, self.theme.progress_bar(value))
    }

    #[track_caller]
    pub fn sparkline(&mut self, key: Key, samples: &[i16]) -> Result<(), BuildError> {
        self.ui.sparkline(key, samples, self.theme.sparkline())
    }

    #[track_caller]
    pub fn sparkline_ring<const N: usize>(
        &mut self,
        key: Key,
        ring: &SampleRing<N>,
    ) -> Result<(), BuildError> {
        self.ui.sparkline_ring(key, ring, self.theme.sparkline())
    }

    #[inline]
    pub fn tabs(
        &mut self,
        key: Key,
        items: &[(Key, TextRef)],
        selected: Key,
    ) -> Result<(), BuildError> {
        tabs(self.ui, key, items, selected, self.theme)
    }

    #[inline]
    pub fn selector(
        &mut self,
        key: Key,
        items: &[(Key, TextRef)],
        selected: Key,
    ) -> Result<(), BuildError> {
        selector(self.ui, key, items, selected, self.theme)
    }

    #[inline]
    pub fn number_editor(
        &mut self,
        key: Key,
        range: core::ops::RangeInclusive<i16>,
        value: i16,
        step: i16,
    ) -> Result<(), BuildError> {
        number_editor(self.ui, key, range, value, step, self.theme)
    }

    #[inline]
    pub fn toast(&mut self, key: Key, text: TextRef) -> Result<(), BuildError> {
        toast(self.ui, key, text, self.theme)
    }

    #[inline]
    pub fn confirm_dialog(
        &mut self,
        keys: [Key; 4],
        labels: [TextRef; 2],
    ) -> Result<(), BuildError> {
        confirm_dialog(self.ui, keys, labels, self.theme)
    }

    #[inline]
    pub fn centered_confirm_dialog(
        &mut self,
        keys: [Key; 4],
        labels: [TextRef; 2],
    ) -> Result<(), BuildError> {
        centered_confirm_dialog(self.ui, keys, labels, self.theme)
    }

    #[inline]
    pub fn custom(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.ui.custom(key, props)
    }

    #[inline]
    pub fn decorate(&mut self, key: Key, style: SurfaceStyle) -> Result<(), BuildError> {
        self.ui.decorate(key, style)
    }

    #[inline]
    fn decorate_if_needed(&mut self, key: Key, style: SurfaceStyle) -> Result<(), BuildError> {
        if style.shadow.is_some() || style.layer().is_some() {
            self.ui.decorate(key, style)
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn text_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<TextRef, BuildError> {
        self.ui.text_fmt(args)
    }

    #[inline]
    pub fn text_dyn(&mut self, s: &str) -> Result<TextRef, BuildError> {
        self.ui.text_dyn(s)
    }
}
