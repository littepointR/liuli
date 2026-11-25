use liuli::core::style_source::StyleSource;
use liuli::prelude::*;

use crate::style::{ButtonVariant, Elevation, Material, ResolvedSurface, SurfaceSpec};
use crate::tokens::{
    ColorScheme, ComponentParts, Density, EffectTier, EffectsConfig, ElevationLevel,
    ElevationScale, GlassStyle, InteractionStyle, Motion, Shape, Typography,
};

/// Built-in aesthetic presets combining color palette, shape language, and glass defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemePreset {
    #[default]
    Liuli,
    Classic,
    Industrial,
}

impl ThemePreset {
    pub const ALL: [Self; 3] = [Self::Liuli, Self::Classic, Self::Industrial];

    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Liuli => "Liuli",
            Self::Classic => "Classic",
            Self::Industrial => "Industrial",
        }
    }

    #[inline]
    pub const fn next(self) -> Self {
        match self {
            Self::Liuli => Self::Classic,
            Self::Classic => Self::Industrial,
            Self::Industrial => Self::Liuli,
        }
    }
}

const fn preset_colors(preset: ThemePreset, dark: bool) -> ColorScheme {
    match (preset, dark) {
        (ThemePreset::Liuli, true) => ColorScheme::LIULI_DARK,
        (ThemePreset::Liuli, false) => ColorScheme::LIULI_LIGHT,
        (ThemePreset::Classic, true) => ColorScheme::CLASSIC_DARK,
        (ThemePreset::Classic, false) => ColorScheme::CLASSIC_LIGHT,
        (ThemePreset::Industrial, true) => ColorScheme::INDUSTRIAL_DARK,
        (ThemePreset::Industrial, false) => ColorScheme::INDUSTRIAL_LIGHT,
    }
}

const fn preset_shape(preset: ThemePreset) -> Shape {
    match preset {
        ThemePreset::Liuli => Shape::SOFT,
        ThemePreset::Classic => Shape::ROUNDED,
        ThemePreset::Industrial => Shape::INDUSTRIAL,
    }
}

const fn preset_glass(preset: ThemePreset, colors: ColorScheme) -> GlassStyle {
    let tint_alpha = match (preset, colors.is_dark) {
        (ThemePreset::Industrial, true) => 9,
        (ThemePreset::Industrial, false) => 10,
        (_, true) => 7,
        (_, false) => 9,
    };
    let blur_radius = match preset {
        ThemePreset::Industrial => 3,
        _ => 5,
    };
    let border_color = Some(if colors.is_dark {
        colors.border_strong
    } else {
        colors.surface
    });
    GlassStyle {
        blur_radius,
        tint_color: Some(colors.surface),
        tint_alpha,
        border_color,
        border_width: 1,
    }
}

const fn preset_name(preset: ThemePreset, dark: bool) -> &'static str {
    match (preset, dark) {
        (ThemePreset::Liuli, true) => "Theme::DARK",
        (ThemePreset::Liuli, false) => "Theme::LIGHT",
        (ThemePreset::Classic, true) => "Theme::CLASSIC_DARK",
        (ThemePreset::Classic, false) => "Theme::CLASSIC_LIGHT",
        (ThemePreset::Industrial, true) => "Theme::INDUSTRIAL_DARK",
        (ThemePreset::Industrial, false) => "Theme::INDUSTRIAL_LIGHT",
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub source: StyleSource,
    pub background: Rgb565,
    pub surface: Rgb565,
    pub surface_alt: Rgb565,
    pub border: Rgb565,
    pub text: Rgb565,
    pub text_muted: Rgb565,
    pub accent: Rgb565,
    pub on_accent: Rgb565,
    pub focus: Rgb565,
    pub danger: Rgb565,
    pub success: Rgb565,
    pub overlay: Rgb565,
    pub spacing: u16,
    pub control_height: u16,
    pub radius: u8,
    pub font: FontId,
    pub preset: ThemePreset,
    pub is_dark: bool,
    pub colors: ColorScheme,
    pub density: Density,
    pub shape: Shape,
    pub typography: Typography,
    pub elevations: ElevationScale,
    pub glass: GlassStyle,
    pub interaction: InteractionStyle,
    pub motion: Motion,
    pub effects: EffectsConfig,
    pub parts: ComponentParts,
}

impl Default for Theme {
    fn default() -> Self {
        Self::DARK
    }
}

impl Theme {
    pub const DARK: Self = Self::from_parts(
        StyleSource::named("Theme::DARK"),
        ThemePreset::Liuli,
        ColorScheme::LIULI_DARK,
        Density::REGULAR,
        Shape::SOFT,
        Typography::STANDARD,
        ElevationScale::DARK,
        GlassStyle::DARK_FROST,
        InteractionStyle::DARK,
        Motion::STANDARD,
        EffectsConfig::GLASS,
    );

    pub const LIGHT: Self = Self::from_parts(
        StyleSource::named("Theme::LIGHT"),
        ThemePreset::Liuli,
        ColorScheme::LIULI_LIGHT,
        Density::REGULAR,
        Shape::SOFT,
        Typography::STANDARD,
        ElevationScale::LIGHT,
        GlassStyle::LIGHT_FROST,
        InteractionStyle::LIGHT,
        Motion::STANDARD,
        EffectsConfig::GLASS,
    );

    #[inline]
    pub const fn preset(preset: ThemePreset, dark: bool) -> Self {
        let colors = preset_colors(preset, dark);
        let shape = preset_shape(preset);
        let glass = preset_glass(preset, colors);
        let elevations = if dark {
            ElevationScale::DARK
        } else {
            ElevationScale::LIGHT
        };
        let interaction = if dark {
            InteractionStyle::DARK
        } else {
            InteractionStyle::LIGHT
        };
        Self::from_parts(
            StyleSource::named(preset_name(preset, dark)),
            preset,
            colors,
            Density::REGULAR,
            shape,
            Typography::STANDARD,
            elevations,
            glass,
            interaction,
            Motion::STANDARD,
            EffectsConfig::GLASS,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub const fn from_parts(
        source: StyleSource,
        preset: ThemePreset,
        colors: ColorScheme,
        density: Density,
        shape: Shape,
        typography: Typography,
        elevations: ElevationScale,
        glass: GlassStyle,
        interaction: InteractionStyle,
        motion: Motion,
        effects: EffectsConfig,
    ) -> Self {
        Self {
            source,
            background: colors.background,
            surface: colors.surface,
            surface_alt: colors.surface_alt,
            border: colors.border,
            text: colors.text,
            text_muted: colors.text_muted,
            accent: colors.accent,
            on_accent: colors.on_accent,
            focus: colors.focus,
            danger: colors.danger,
            success: colors.success,
            overlay: colors.overlay,
            spacing: density.spacing,
            control_height: density.control_height,
            radius: shape.radius_md,
            font: typography.body,
            preset,
            is_dark: colors.is_dark,
            colors,
            density,
            shape,
            typography,
            elevations,
            glass,
            interaction,
            motion,
            effects,
            parts: ComponentParts::DEFAULT,
        }
    }

    const fn sync_flat(mut self) -> Self {
        self.background = self.colors.background;
        self.surface = self.colors.surface;
        self.surface_alt = self.colors.surface_alt;
        self.border = self.colors.border;
        self.text = self.colors.text;
        self.text_muted = self.colors.text_muted;
        self.accent = self.colors.accent;
        self.on_accent = self.colors.on_accent;
        self.focus = self.colors.focus;
        self.danger = self.colors.danger;
        self.success = self.colors.success;
        self.overlay = self.colors.overlay;
        self.is_dark = self.colors.is_dark;
        self.spacing = self.density.spacing;
        self.control_height = self.density.control_height;
        self.radius = self.shape.radius_md;
        self.font = self.typography.body;
        self
    }

    #[track_caller]
    pub fn with_colors(mut self, colors: ColorScheme) -> Self {
        self.source = self.source.scoped_tokens([
            colors.background != self.colors.background,
            colors.surface != self.colors.surface
                || colors.surface_raised != self.colors.surface_raised
                || colors.surface_floating != self.colors.surface_floating
                || colors.surface_alt != self.colors.surface_alt,
            colors.border != self.colors.border
                || colors.border_strong != self.colors.border_strong,
            colors.text != self.colors.text
                || colors.text_muted != self.colors.text_muted
                || colors.text_disabled != self.colors.text_disabled,
            colors.accent != self.colors.accent || colors.on_accent != self.colors.on_accent,
            colors.danger != self.colors.danger
                || colors.on_danger != self.colors.on_danger
                || colors.success != self.colors.success
                || colors.warning != self.colors.warning,
            colors.focus != self.colors.focus,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        ]);
        self.colors = colors;
        self.sync_flat()
    }

    #[track_caller]
    pub fn with_density(mut self, density: Density) -> Self {
        if density != self.density {
            self.source = self.source.scoped_index(9);
        }
        self.density = density;
        self.sync_flat()
    }

    #[track_caller]
    pub fn with_shape(mut self, shape: Shape) -> Self {
        if shape != self.shape {
            self.source = self.source.scoped_index(10);
        }
        self.shape = shape;
        self.sync_flat()
    }

    #[track_caller]
    pub fn with_typography(mut self, typography: Typography) -> Self {
        if typography != self.typography {
            self.source = self.source.scoped_index(8);
        }
        self.typography = typography;
        self.sync_flat()
    }

    #[track_caller]
    pub fn with_text_gamma(self, gamma: TextGamma) -> Self {
        self.with_typography(self.typography.with_gamma(gamma))
    }

    #[track_caller]
    pub fn with_word_wrap(self, wrap: WrapMode) -> Self {
        self.with_typography(self.typography.with_wrap(wrap))
    }

    #[track_caller]
    pub fn with_subpixel_order(self, subpixel: SubpixelOrder) -> Self {
        self.with_typography(self.typography.with_subpixel(subpixel))
    }

    #[track_caller]
    pub fn with_letter_spacing(self, letter_spacing: i8) -> Self {
        self.with_typography(self.typography.with_letter_spacing(letter_spacing))
    }

    #[track_caller]
    pub fn with_line_spacing(self, line_spacing: i8) -> Self {
        self.with_typography(self.typography.with_line_spacing(line_spacing))
    }

    #[track_caller]
    pub fn with_elevations(mut self, elevations: ElevationScale) -> Self {
        if elevations != self.elevations {
            self.source = self.source.scoped_index(11);
        }
        self.elevations = elevations;
        self
    }

    #[track_caller]
    pub fn with_glass(mut self, glass: GlassStyle) -> Self {
        if glass != self.glass {
            self.source = self.source.scoped_index(12);
        }
        self.glass = glass;
        self
    }

    #[track_caller]
    pub fn with_interaction(mut self, interaction: InteractionStyle) -> Self {
        self.source = self.source.scoped_tokens([
            false,
            false,
            false,
            false,
            false,
            false,
            interaction.focus != self.interaction.focus,
            interaction.press_surface != self.interaction.press_surface
                || interaction.press_accent != self.interaction.press_accent,
            false,
            false,
            false,
            false,
            false,
            false,
        ]);
        self.interaction = interaction;
        self
    }

    #[track_caller]
    pub fn with_motion(mut self, motion: Motion) -> Self {
        if motion != self.motion {
            self.source = self.source.scoped_index(13);
        }
        self.motion = motion;
        self
    }

    #[track_caller]
    pub fn with_effects(mut self, effects: EffectsConfig) -> Self {
        if effects != self.effects {
            self.source = self.source.scoped_index(13);
        }
        self.effects = effects;
        self
    }

    #[track_caller]
    pub fn with_effect_tier(self, tier: EffectTier) -> Self {
        self.with_effects(EffectsConfig::from_tier(tier))
    }

    #[track_caller]
    pub fn with_parts(mut self, parts: ComponentParts) -> Self {
        if parts != self.parts {
            self.source = self.source.scoped_index(10);
        }
        self.parts = parts;
        self
    }

    #[track_caller]
    pub fn map_parts(self, f: impl FnOnce(ComponentParts) -> ComponentParts) -> Self {
        let next = f(self.parts);
        self.with_parts(next)
    }

    #[track_caller]
    pub fn map_colors(self, f: impl FnOnce(ColorScheme) -> ColorScheme) -> Self {
        let next = f(self.colors);
        self.with_colors(next)
    }
    #[track_caller]
    pub fn map_density(self, f: impl FnOnce(Density) -> Density) -> Self {
        let next = f(self.density);
        self.with_density(next)
    }
    #[track_caller]
    pub fn map_shape(self, f: impl FnOnce(Shape) -> Shape) -> Self {
        let next = f(self.shape);
        self.with_shape(next)
    }
    #[track_caller]
    pub fn map_elevations(self, f: impl FnOnce(ElevationScale) -> ElevationScale) -> Self {
        let next = f(self.elevations);
        self.with_elevations(next)
    }
    #[track_caller]
    pub fn map_glass(self, f: impl FnOnce(GlassStyle) -> GlassStyle) -> Self {
        let next = f(self.glass);
        self.with_glass(next)
    }
    #[track_caller]
    pub fn map_interaction(self, f: impl FnOnce(InteractionStyle) -> InteractionStyle) -> Self {
        let next = f(self.interaction);
        self.with_interaction(next)
    }
    #[track_caller]
    pub fn map_motion(self, f: impl FnOnce(Motion) -> Motion) -> Self {
        let next = f(self.motion);
        self.with_motion(next)
    }
    #[track_caller]
    pub fn map_effects(self, f: impl FnOnce(EffectsConfig) -> EffectsConfig) -> Self {
        let next = f(self.effects);
        self.with_effects(next)
    }

    // Single-capability convenience helpers.
    #[track_caller]
    pub fn with_shadows(self, enabled: bool) -> Self {
        self.map_effects(|e| e.with_shadows(enabled))
    }
    #[track_caller]
    pub fn with_press_depth(self, enabled: bool) -> Self {
        self.map_effects(|e| e.with_press_depth(enabled))
    }
    #[track_caller]
    pub fn with_glass_blur(self, enabled: bool) -> Self {
        self.map_effects(|e| e.with_glass_blur(enabled))
    }
    #[track_caller]
    pub fn with_blur_radius(self, radius: u8) -> Self {
        let radius = radius.min(8);
        self.map_glass(|g| g.with_blur_radius(radius))
            .map_effects(|e| e.with_max_blur_radius(radius.max(e.max_blur_radius)))
    }
    #[track_caller]
    pub fn with_glass_tint(self, color: Option<Rgb565>, alpha: u8) -> Self {
        self.map_glass(|g| g.with_tint_color(color).with_tint_alpha(alpha))
    }
    #[track_caller]
    pub fn with_rounded_focus(self, rounded: bool) -> Self {
        self.map_interaction(|i| i.with_rounded_focus(rounded))
    }
    #[track_caller]
    pub fn with_control_height(self, height: u16) -> Self {
        self.map_density(|d| d.with_control_height(height))
    }
    #[track_caller]
    pub fn with_spacing(self, spacing: u16) -> Self {
        self.map_density(|d| d.with_spacing(spacing))
    }
    #[track_caller]
    pub fn with_radius(self, radius: u8) -> Self {
        self.map_shape(|s| s.with_all_radii(radius))
    }
    #[track_caller]
    pub fn with_accent(self, accent: Rgb565) -> Self {
        self.map_colors(|c| c.with_accent(accent))
    }

    fn press_depth_style(self) -> PressDepth {
        if self.effects.press_depth && !self.motion.reduced {
            PressDepth::custom(
                self.motion.press_depth_ms,
                self.motion.press_depth_scale,
                self.motion.press_depth_easing,
            )
        } else {
            PressDepth::OFF
        }
    }

    fn elevation_level(self, height: Elevation) -> Option<ElevationLevel> {
        match height {
            Elevation::Flat => None,
            Elevation::Raised => Some(self.elevations.raised),
            Elevation::Floating => Some(self.elevations.floating),
        }
    }

    fn resolved_shadow(self, height: Elevation) -> Option<Shadow> {
        if !self.effects.shadows || self.effects.shadow_opacity_scale == 0 {
            return None;
        }
        let level = self.elevation_level(height)?;
        let opacity = ((u16::from(level.shadow_opacity.min(15))
            * u16::from(self.effects.shadow_opacity_scale.min(15))
            + 7)
            / 15) as u8;
        if opacity == 0 {
            return None;
        }
        Some(Shadow {
            color: level.shadow_color.unwrap_or(self.overlay),
            x: level.shadow_x,
            y: level.shadow_y,
            blur: level.shadow_blur.min(self.effects.max_shadow_blur),
            opacity,
        })
    }

    /// Surface shadow and press-depth compression for a named elevation height.
    pub fn elevation(self, height: Elevation) -> SurfaceStyle {
        let Some(shadow) = self.resolved_shadow(height) else {
            return SurfaceStyle::FLAT;
        };
        SurfaceStyle::FLAT
            .with_shadow(shadow)
            .with_press_depth(self.press_depth_style())
    }

    /// Resolve a declarative `SurfaceSpec` into `NodeProps` + `SurfaceStyle` honoring all capability gates.
    #[track_caller]
    pub fn resolve_surface(self, spec: SurfaceSpec, mut base: NodeProps) -> ResolvedSurface {
        let level = self.elevation_level(spec.elevation);
        let default_surface = match spec.elevation {
            Elevation::Flat => self.surface,
            Elevation::Raised => {
                if self.effects.elevation_tone {
                    level
                        .and_then(|l| l.surface_override)
                        .unwrap_or(self.colors.surface_raised)
                } else {
                    self.surface
                }
            }
            Elevation::Floating => {
                if self.effects.elevation_tone {
                    level
                        .and_then(|l| l.surface_override)
                        .unwrap_or(self.colors.surface_floating)
                } else {
                    self.surface
                }
            }
        };
        let default_border =
            level
                .and_then(|l| l.border_override)
                .unwrap_or(match spec.elevation {
                    Elevation::Floating if self.effects.elevation_tone => self.colors.border_strong,
                    _ => self.border,
                });
        let default_radius = spec.radius.unwrap_or(match spec.elevation {
            Elevation::Floating => self.shape.radius_lg,
            Elevation::Flat | Elevation::Raised => self.shape.radius_md,
        });

        let mut effect = SurfaceStyle::FLAT;
        if let Some(shadow) = spec.shadow_override {
            if self.effects.shadows {
                effect = effect.with_shadow(Shadow {
                    blur: shadow.blur.min(self.effects.max_shadow_blur),
                    opacity: ((u16::from(shadow.opacity.min(15))
                        * u16::from(self.effects.shadow_opacity_scale.min(15))
                        + 7)
                        / 15) as u8,
                    ..shadow
                });
            }
        } else if let Some(shadow) = self.resolved_shadow(spec.elevation) {
            effect = effect.with_shadow(shadow);
        }
        effect = effect.with_press_depth(if self.motion.reduced {
            PressDepth::OFF
        } else {
            spec.press_depth_override
                .unwrap_or_else(|| self.press_depth_style())
        });

        let mut material_label = "solid";
        let mut active_glass_layer: Option<Layer> = None;
        match spec.material {
            Material::Solid => {
                base.bg = Some(spec.bg_override.unwrap_or(default_surface));
                base.border_color = spec.border_color.or(Some(default_border));
                base.border_width = spec.border_width.unwrap_or(match spec.elevation {
                    Elevation::Flat => self.shape.border_width,
                    Elevation::Raised | Elevation::Floating => self.shape.elevated_border_width,
                });
            }
            Material::Transparent => {
                base.bg = spec.bg_override;
                base.border_color = spec.border_color;
                base.border_width = spec.border_width.unwrap_or(0);
                material_label = "transparent";
            }
            Material::Glass {
                blur_radius,
                tint_color,
                tint_alpha,
            } => {
                let want_blur = self.effects.glass_blur;
                let want_tint = self.effects.glass_tint;
                let blur = if want_blur {
                    blur_radius
                        .unwrap_or(self.glass.blur_radius)
                        .min(self.effects.max_blur_radius)
                        .min(8)
                } else {
                    0
                };
                let t_alpha = if want_tint {
                    tint_alpha.unwrap_or(self.glass.tint_alpha).min(15)
                } else {
                    0
                };
                if SurfaceStyle::has_layers_support() && (blur > 0 || t_alpha > 0) {
                    let t_color = tint_color.or(self.glass.tint_color).unwrap_or(self.surface);
                    base.bg = spec.bg_override;
                    base.border_color = spec
                        .border_color
                        .or(self.glass.border_color)
                        .or(Some(self.colors.border_strong));
                    base.border_width = spec.border_width.unwrap_or(self.glass.border_width);
                    active_glass_layer = Some(Layer {
                        opacity: 15,
                        projection: Projection(0),
                        backdrop_radius: blur,
                        tint_color: t_color,
                        tint_alpha: t_alpha,
                    });
                    material_label = match (blur > 0, t_alpha > 0) {
                        (true, true) => "glass+tint",
                        (true, false) => "glass",
                        (false, true) => "tint",
                        (false, false) => "solid",
                    };
                } else {
                    base.bg = Some(spec.bg_override.unwrap_or(default_surface));
                    base.border_color = spec.border_color.or(Some(default_border));
                    base.border_width = spec
                        .border_width
                        .unwrap_or(self.shape.elevated_border_width);
                    material_label = "solid(fallback)";
                }
            }
        }

        base.radius = default_radius;
        base.style_source = self.source.declared().with_material(material_label);

        let opacity = if self.effects.group_opacity {
            spec.compositor.opacity.min(15)
        } else {
            15
        };
        let projection = if self.effects.projection_3d {
            spec.compositor.projection
        } else {
            Projection(0)
        };
        let need_layer = active_glass_layer.is_some()
            || spec.compositor.isolate
            || opacity < 15
            || projection.0 != 0;
        if SurfaceStyle::has_layers_support() && need_layer {
            let layer = active_glass_layer.unwrap_or(Layer::OPAQUE);
            effect = effect.with_layer(Layer {
                opacity,
                projection,
                ..layer
            });
        }

        ResolvedSurface {
            props: base,
            effect,
        }
    }

    #[track_caller]
    pub fn control(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared().with_variant("control"),
            bg: Some(self.surface),
            border_color: Some(self.border),
            border_width: self.shape.border_width,
            radius: self.shape.radius_md,
            padding: EdgeInsets::symmetric(self.density.pad_x, self.density.pad_y),
            fg: self.text,
            accent: self.accent,
            focus_color: self.focus,
            focus_ring: self.interaction.focus,
            press_feedback: self.interaction.press_surface,
            font: self.typography.body,
            wrap: self.typography.wrap,
            text_gamma: self.typography.gamma,
            subpixel: self.typography.subpixel,
            letter_spacing: self.typography.letter_spacing,
            line_spacing: self.typography.line_spacing,
            width: Length::Fill(1),
            height: Length::Px(self.density.control_height.into()),
            clickable: true,
            focusable: true,
            ..NodeProps::new()
        }
    }

    #[track_caller]
    pub fn button(self, variant: ButtonVariant) -> NodeProps {
        let base = NodeProps {
            parts: self.parts.resolve(crate::WidgetClass::BUTTON),
            style_source: self.source.declared().with_variant(variant.name()),
            ..self.control()
        }
        .state_motion(self.motion.state(MotionState::Pressed, None));
        match variant {
            ButtonVariant::Surface => base,
            ButtonVariant::Primary => NodeProps {
                bg: Some(self.accent),
                fg: self.on_accent,
                border_color: Some(self.accent),
                border_width: self.shape.border_width,
                press_feedback: self.interaction.press_accent,
                ..base
            },
            ButtonVariant::Secondary => NodeProps {
                bg: Some(self.surface_alt),
                fg: self.text,
                border_color: Some(self.border),
                border_width: self.shape.border_width,
                ..base
            },
            ButtonVariant::Outline => NodeProps {
                bg: None,
                fg: self.accent,
                border_color: Some(self.accent),
                border_width: self.shape.border_width,
                press_feedback: PressFeedback::AccentTint(4),
                ..base
            },
            ButtonVariant::Ghost => NodeProps {
                bg: None,
                fg: self.text,
                border_color: None,
                border_width: 0,
                press_feedback: PressFeedback::AccentTint(3),
                ..base
            },
            ButtonVariant::Danger => NodeProps {
                bg: Some(self.danger),
                fg: self.colors.on_danger,
                border_color: Some(self.danger),
                border_width: self.shape.border_width,
                press_feedback: self.interaction.press_accent,
                ..base
            },
        }
    }

    #[track_caller]
    pub fn button_state(self, variant: ButtonVariant, enabled: bool) -> NodeProps {
        if enabled {
            self.button(variant)
        } else {
            NodeProps {
                style_source: self.source.declared().with_variant("disabled"),
                ..self.disabled_button()
            }
        }
    }

    #[track_caller]
    pub fn primary_button(self) -> NodeProps {
        self.button(ButtonVariant::Primary)
    }

    #[track_caller]
    pub fn secondary_button(self) -> NodeProps {
        self.button(ButtonVariant::Secondary)
    }

    #[track_caller]
    pub fn outline_button(self) -> NodeProps {
        self.button(ButtonVariant::Outline)
    }

    #[track_caller]
    pub fn ghost_button(self) -> NodeProps {
        self.button(ButtonVariant::Ghost)
    }

    #[track_caller]
    pub fn danger_button(self) -> NodeProps {
        self.button(ButtonVariant::Danger)
    }

    #[track_caller]
    pub fn disabled_button(self) -> NodeProps {
        NodeProps {
            parts: self.parts.resolve(crate::WidgetClass::BUTTON),
            style_source: self.source.declared().with_variant("disabled"),
            bg: Some(self.surface_alt),
            fg: self.text_muted,
            border_color: Some(self.border),
            border_width: self.shape.border_width,
            press_feedback: PressFeedback::None,
            enabled: false,
            ..self.control()
        }
    }

    #[track_caller]
    pub fn card(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared(),
            bg: Some(self.surface),
            border_color: Some(self.border),
            border_width: self.shape.border_width,
            radius: self.shape.radius_md,
            padding: EdgeInsets::all(self.density.card_pad),
            gap: self.density.spacing,
            align_cross: Align::Stretch,
            width: Length::Fill(1),
            height: Length::Content,
            ..NodeProps::new()
        }
    }

    #[track_caller]
    pub fn toggle(self, checked: bool) -> NodeProps {
        NodeProps {
            parts: self.parts.resolve(crate::WidgetClass::TOGGLE),
            style_source: self.source.declared(),
            bg: Some(self.surface_alt),
            border_color: Some(self.border),
            border_width: self.shape.border_width,
            radius: self.shape.radius_md,
            accent: self.accent,
            fg: self.colors.knob,
            focus_color: self.focus,
            focus_ring: self.interaction.focus,
            checked,
            width: Length::Px(self.density.toggle_width.into()),
            height: Length::Px(self.density.control_height.into()),
            clickable: true,
            focusable: true,
            ..NodeProps::new()
        }
        .state_motion(self.motion.state(MotionState::Checked, None))
    }

    #[track_caller]
    pub fn slider(self, value: i16) -> NodeProps {
        NodeProps {
            parts: self.parts.resolve(crate::WidgetClass::SLIDER),
            style_source: self.source.declared(),
            bg: Some(self.surface_alt),
            radius: self.shape.radius_md,
            accent: self.accent,
            fg: self.colors.knob,
            focus_color: self.focus,
            focus_ring: self.interaction.focus,
            value,
            width: Length::Fill(1),
            height: Length::Px(self.density.control_height.into()),
            clickable: true,
            focusable: true,
            ..NodeProps::new()
        }
        .state_motion(self.motion.state(MotionState::Value, None))
    }

    #[track_caller]
    pub fn progress_bar(self, value: i16) -> NodeProps {
        NodeProps {
            parts: self.parts.resolve(crate::WidgetClass::PROGRESS),
            style_source: self.source.declared(),
            bg: Some(self.surface_alt),
            border_color: Some(self.border),
            border_width: self.shape.border_width,
            radius: (self.density.progress_height / 2).min(255) as u8,
            accent: self.accent,
            fg: self.text,
            value,
            width: Length::Fill(1),
            height: Length::Px(self.density.progress_height.into()),
            ..NodeProps::new()
        }
        .state_motion(self.motion.state(MotionState::Value, None))
    }

    #[track_caller]
    pub fn sparkline(self) -> NodeProps {
        NodeProps {
            parts: self.parts.resolve(crate::WidgetClass::SPARKLINE),
            style_source: self.source.declared(),
            bg: Some(self.surface),
            border_color: Some(self.border),
            border_width: self.shape.border_width,
            radius: self.shape.radius_md,
            padding: EdgeInsets::all(self.density.card_pad),
            accent: self.accent,
            fg: self.text,
            width: Length::Fill(1),
            height: Length::Px(self.density.sparkline_height.into()),
            ..NodeProps::new()
        }
    }

    #[track_caller]
    pub fn title(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared(),
            fg: self.text,
            font: self.typography.title,
            wrap: self.typography.wrap,
            text_gamma: self.typography.gamma,
            subpixel: self.typography.subpixel,
            letter_spacing: self.typography.letter_spacing,
            line_spacing: self.typography.line_spacing,
            align_cross: Align::Center,
            width: Length::Fill(1),
            height: Length::Px(self.density.title_height.into()),
            ..NodeProps::new()
        }
    }

    #[track_caller]
    pub fn display(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared(),
            fg: self.text,
            font: self.typography.display,
            wrap: self.typography.wrap,
            text_gamma: self.typography.gamma,
            subpixel: self.typography.subpixel,
            letter_spacing: self.typography.letter_spacing,
            line_spacing: self.typography.line_spacing,
            align_cross: Align::Center,
            width: Length::Fill(1),
            height: Length::Content,
            ..NodeProps::new()
        }
    }

    #[track_caller]
    pub fn paragraph(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared(),
            fg: self.text,
            font: self.typography.body,
            wrap: WrapMode::Word,
            text_gamma: self.typography.gamma,
            subpixel: self.typography.subpixel,
            letter_spacing: self.typography.letter_spacing,
            line_spacing: self.typography.line_spacing,
            max_lines: 0,
            ellipsis: false,
            width: Length::Fill(1),
            height: Length::Content,
            ..NodeProps::new()
        }
    }

    #[track_caller]
    pub fn caption(self) -> NodeProps {
        NodeProps {
            style_source: self.source.declared(),
            fg: self.text_muted,
            font: self.typography.caption,
            wrap: self.typography.wrap,
            text_gamma: self.typography.gamma,
            subpixel: self.typography.subpixel,
            letter_spacing: self.typography.letter_spacing,
            line_spacing: self.typography.line_spacing,
            align_cross: Align::Center,
            width: Length::Fill(1),
            height: Length::Px(self.density.caption_height.into()),
            ..NodeProps::new()
        }
    }
}

/// Per-component overrides. An unassigned token inherits its parent theme.
#[derive(Default, Copy, Clone)]
pub struct Style {
    pub background: Option<Rgb565>,
    pub surface: Option<Rgb565>,
    pub surface_alt: Option<Rgb565>,
    pub border: Option<Rgb565>,
    pub text: Option<Rgb565>,
    pub text_muted: Option<Rgb565>,
    pub accent: Option<Rgb565>,
    pub on_accent: Option<Rgb565>,
    pub focus: Option<Rgb565>,
    pub danger: Option<Rgb565>,
    pub success: Option<Rgb565>,
    pub overlay: Option<Rgb565>,
    pub font: Option<FontId>,
    pub spacing: Option<u16>,
    pub control_height: Option<u16>,
    pub radius: Option<u8>,
    pub colors: Option<ColorScheme>,
    pub density: Option<Density>,
    pub shape: Option<Shape>,
    pub typography: Option<Typography>,
    pub elevations: Option<ElevationScale>,
    pub glass: Option<GlassStyle>,
    pub interaction: Option<InteractionStyle>,
    pub motion: Option<Motion>,
    pub effects: Option<EffectsConfig>,
    pub parts: Option<ComponentParts>,
}

impl Theme {
    #[track_caller]
    pub fn with(self, style: Style) -> Self {
        let base_colors = style.colors.unwrap_or(self.colors);
        let colors = ColorScheme {
            background: style.background.unwrap_or(base_colors.background),
            surface: style.surface.unwrap_or(base_colors.surface),
            surface_alt: style.surface_alt.unwrap_or(base_colors.surface_alt),
            border: style.border.unwrap_or(base_colors.border),
            text: style.text.unwrap_or(base_colors.text),
            text_muted: style.text_muted.unwrap_or(base_colors.text_muted),
            accent: style.accent.unwrap_or(base_colors.accent),
            on_accent: style.on_accent.unwrap_or(base_colors.on_accent),
            focus: style.focus.unwrap_or(base_colors.focus),
            danger: style.danger.unwrap_or(base_colors.danger),
            success: style.success.unwrap_or(base_colors.success),
            overlay: style.overlay.unwrap_or(base_colors.overlay),
            ..base_colors
        };
        let base_density = style.density.unwrap_or(self.density);
        let density = Density {
            spacing: style.spacing.unwrap_or(base_density.spacing),
            control_height: style.control_height.unwrap_or(base_density.control_height),
            ..base_density
        };
        let base_shape = style.shape.unwrap_or(self.shape);
        let shape = if let Some(r) = style.radius {
            Shape {
                radius_md: r,
                ..base_shape
            }
        } else {
            base_shape
        };
        let base_typo = style.typography.unwrap_or(self.typography);
        let typography = if let Some(f) = style.font {
            Typography {
                body: f,
                ..base_typo
            }
        } else {
            base_typo
        };
        let elevations = style.elevations.unwrap_or(self.elevations);
        let glass = style.glass.unwrap_or(self.glass);
        let interaction = style.interaction.unwrap_or(self.interaction);
        let mut motion = style.motion.unwrap_or(self.motion);
        motion.reduced |= self.motion.reduced;
        let effects = style.effects.unwrap_or(self.effects);
        let parts = style.parts.unwrap_or(self.parts);

        let mut next = self;
        next.source = next.source.scoped_tokens([
            style.background.is_some()
                || style
                    .colors
                    .is_some_and(|c| c.background != self.background),
            style.surface.is_some()
                || style.surface_alt.is_some()
                || style.colors.is_some_and(|c| c.surface != self.surface),
            style.border.is_some() || style.colors.is_some_and(|c| c.border != self.border),
            style.text.is_some() || style.text_muted.is_some() || style.colors.is_some(),
            style.accent.is_some() || style.on_accent.is_some(),
            style.danger.is_some() || style.success.is_some(),
            style.focus.is_some() || style.interaction.is_some(),
            style.interaction.is_some(),
            style.font.is_some() || style.typography.is_some(),
            style.spacing.is_some() || style.control_height.is_some() || style.density.is_some(),
            style.radius.is_some() || style.shape.is_some() || style.parts.is_some(),
            style.elevations.is_some(),
            style.glass.is_some(),
            style.motion.is_some() || style.effects.is_some(),
        ]);
        next.colors = colors;
        next.density = density;
        next.shape = shape;
        next.typography = typography;
        next.elevations = elevations;
        next.glass = glass;
        next.interaction = interaction;
        next.motion = motion;
        next.effects = effects;
        next.parts = parts;
        next.sync_flat()
    }
}
