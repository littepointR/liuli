use liuli::prelude::*;

/// Theme heights affect paint only. Declaration order continues to govern overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Elevation {
    #[default]
    Flat,
    Raised,
    Floating,
}

/// Surface material semantics with automatic solid degradation when backdrop/tint layers are unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Material {
    #[default]
    Solid,
    Glass {
        blur_radius: Option<u8>,
        tint_color: Option<Rgb565>,
        tint_alpha: Option<u8>,
    },
    Transparent,
}

impl Material {
    pub const GLASS: Self = Self::Glass {
        blur_radius: None,
        tint_color: None,
        tint_alpha: None,
    };

    #[inline]
    pub const fn glass(blur_radius: u8, tint_alpha: u8) -> Self {
        Self::Glass {
            blur_radius: Some(blur_radius),
            tint_color: None,
            tint_alpha: Some(tint_alpha),
        }
    }

    #[inline]
    pub const fn glass_blur(blur_radius: u8) -> Self {
        Self::Glass {
            blur_radius: Some(blur_radius),
            tint_color: None,
            tint_alpha: None,
        }
    }

    #[inline]
    pub const fn glass_tint(tint_color: Rgb565, tint_alpha: u8) -> Self {
        Self::Glass {
            blur_radius: None,
            tint_color: Some(tint_color),
            tint_alpha: Some(tint_alpha),
        }
    }
}

/// Isolated subtree compositing options. Changing opacity or projection preserves `reuse_groups` caches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Compositor {
    pub opacity: u8,
    pub projection: Projection,
    pub isolate: bool,
}

impl Compositor {
    pub const NONE: Self = Self {
        opacity: 15,
        projection: Projection(0),
        isolate: false,
    };

    #[inline]
    pub const fn isolated(opacity: u8, projection: Projection) -> Self {
        Self {
            opacity: if opacity > 15 { 15 } else { opacity },
            projection,
            isolate: true,
        }
    }

    #[inline]
    pub const fn with_opacity(mut self, opacity: u8) -> Self {
        self.opacity = if opacity > 15 { 15 } else { opacity };
        self
    }

    #[inline]
    pub const fn with_projection(mut self, projection: Projection) -> Self {
        self.projection = projection;
        self
    }

    #[inline]
    pub const fn with_isolate(mut self, isolate: bool) -> Self {
        self.isolate = isolate;
        self
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::NONE
    }
}

/// Declarative surface recipe combining material, elevation, compositing, and per-node overrides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSpec {
    pub composition: Option<&'static liuli::prelude::SurfaceComposition>,
    pub material: Material,
    pub elevation: Elevation,
    pub compositor: Compositor,
    pub radius: Option<u8>,
    pub border_color: Option<Rgb565>,
    pub border_width: Option<u8>,
    pub bg_override: Option<Rgb565>,
    pub shadow_override: Option<Shadow>,
    pub press_depth_override: Option<PressDepth>,
}

impl SurfaceSpec {
    pub const CARD: Self = Self {
        composition: None,
        material: Material::Solid,
        elevation: Elevation::Flat,
        compositor: Compositor::NONE,
        radius: None,
        border_color: None,
        border_width: None,
        bg_override: None,
        shadow_override: None,
        press_depth_override: None,
    };

    pub const RAISED: Self = Self {
        elevation: Elevation::Raised,
        ..Self::CARD
    };

    pub const FLOATING: Self = Self {
        elevation: Elevation::Floating,
        ..Self::CARD
    };

    pub const GLASS: Self = Self {
        material: Material::GLASS,
        elevation: Elevation::Raised,
        ..Self::CARD
    };

    #[inline]
    pub const fn with_composition(
        mut self,
        c: &'static liuli::prelude::SurfaceComposition,
    ) -> Self {
        self.composition = Some(c);
        self
    }
    pub const fn with_material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }
    #[inline]
    pub const fn with_elevation(mut self, elevation: Elevation) -> Self {
        self.elevation = elevation;
        self
    }
    #[inline]
    pub const fn with_compositor(mut self, compositor: Compositor) -> Self {
        self.compositor = compositor;
        self
    }
    #[inline]
    pub const fn with_radius(mut self, radius: u8) -> Self {
        self.radius = Some(radius);
        self
    }
    #[inline]
    pub const fn with_border(mut self, color: Rgb565, width: u8) -> Self {
        self.border_color = Some(color);
        self.border_width = Some(width);
        self
    }
    #[inline]
    pub const fn with_bg(mut self, bg: Rgb565) -> Self {
        self.bg_override = Some(bg);
        self
    }
    #[inline]
    pub const fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow_override = Some(shadow);
        self
    }
    #[inline]
    pub const fn with_press_depth(mut self, press_depth: PressDepth) -> Self {
        self.press_depth_override = Some(press_depth);
        self
    }
}

impl Default for SurfaceSpec {
    fn default() -> Self {
        Self::CARD
    }
}

/// Semantic button emphasis hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Surface,
    Primary,
    Secondary,
    Outline,
    Ghost,
    Danger,
}

impl ButtonVariant {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
            Self::Danger => "danger",
        }
    }
}

/// Output of resolving a `SurfaceSpec` against a `Theme`: layout/visual `NodeProps` plus optional `SurfaceStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedSurface {
    pub props: NodeProps,
    pub effect: SurfaceStyle,
}
