use liuli::prelude::*;

/// Convert 8-bit per-channel sRGB components to `Rgb565`.
#[inline]
pub const fn rgb8(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

/// Exact integer sRGB->linearized 33-entry lookup table scaled to `0..=10_000`.
const SRGB_LIN_LUT: [u16; 33] = [
    0, 24, 52, 87, 134, 195, 272, 366, 478, 608, 759, 930, 1122, 1337, 1574, 1835, 2120, 2430,
    2766, 3128, 3516, 3932, 4376, 4848, 5349, 5880, 6440, 7031, 7653, 8306, 8991, 9708, 10000,
];

#[inline]
const fn channel_lin_5bit(v5: u8) -> u32 {
    let idx = if v5 > 31 { 31 } else { v5 } as usize;
    let pos = (idx * 32) / 31;
    let rem = (idx * 32) % 31;
    let a = SRGB_LIN_LUT[pos] as u32;
    if rem == 0 {
        a
    } else {
        let b = SRGB_LIN_LUT[pos + 1] as u32;
        a + ((b - a) * rem as u32) / 31
    }
}

#[inline]
const fn channel_lin_6bit(v6: u8) -> u32 {
    let idx = if v6 > 63 { 63 } else { v6 } as usize;
    let pos = (idx * 32) / 63;
    let rem = (idx * 32) % 63;
    let a = SRGB_LIN_LUT[pos] as u32;
    if rem == 0 {
        a
    } else {
        let b = SRGB_LIN_LUT[pos + 1] as u32;
        a + ((b - a) * rem as u32) / 63
    }
}

/// Relative luminance scaled to `0..=100_000_000`.
#[inline]
pub fn relative_luminance_scaled(color: Rgb565) -> u32 {
    let raw = liuli::core::geometry::rgb565_to_u16(color);
    let r = channel_lin_5bit((raw >> 11) as u8);
    let g = channel_lin_6bit(((raw >> 5) & 0x3F) as u8);
    let b = channel_lin_5bit((raw & 0x1F) as u8);
    2126 * r + 7152 * g + 722 * b
}

/// WCAG relative contrast ratio between two `Rgb565` colors in tenths (e.g. `45` = 4.5:1, `210` = 21.0:1).
pub fn contrast_ratio_x10(a: Rgb565, b: Rgb565) -> u16 {
    let la = relative_luminance_scaled(a);
    let lb = relative_luminance_scaled(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    let num = (hi + 5_000_000) as u64 * 10;
    let den = (lo + 5_000_000) as u64;
    ((num + den / 2) / den) as u16
}

/// Semantic color palette with tonal elevation and interactive feedback roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorScheme {
    pub background: Rgb565,
    pub surface: Rgb565,
    pub surface_raised: Rgb565,
    pub surface_floating: Rgb565,
    pub surface_alt: Rgb565,
    pub border: Rgb565,
    pub border_strong: Rgb565,
    pub text: Rgb565,
    pub text_muted: Rgb565,
    pub text_disabled: Rgb565,
    pub accent: Rgb565,
    pub on_accent: Rgb565,
    pub danger: Rgb565,
    pub on_danger: Rgb565,
    pub success: Rgb565,
    pub warning: Rgb565,
    pub focus: Rgb565,
    pub overlay: Rgb565,
    pub knob: Rgb565,
    pub is_dark: bool,
}

impl ColorScheme {
    pub const LIULI_DARK: Self = Self {
        background: rgb8(16, 20, 28),
        surface: rgb8(24, 32, 44),
        surface_raised: rgb8(30, 40, 56),
        surface_floating: rgb8(38, 50, 70),
        surface_alt: rgb8(36, 46, 62),
        border: rgb8(54, 68, 88),
        border_strong: rgb8(84, 104, 134),
        text: rgb8(240, 246, 252),
        text_muted: rgb8(148, 163, 184),
        text_disabled: rgb8(106, 120, 140),
        accent: rgb8(20, 144, 236),
        on_accent: rgb8(255, 255, 255),
        danger: rgb8(224, 64, 76),
        on_danger: rgb8(255, 255, 255),
        success: rgb8(48, 208, 136),
        warning: rgb8(245, 178, 52),
        focus: rgb8(255, 196, 64),
        overlay: rgb8(6, 10, 16),
        knob: rgb8(252, 254, 255),
        is_dark: true,
    };

    pub const LIULI_LIGHT: Self = Self {
        background: rgb8(236, 241, 246),
        surface: rgb8(255, 255, 255),
        surface_raised: rgb8(255, 255, 255),
        surface_floating: rgb8(255, 255, 255),
        surface_alt: rgb8(226, 233, 238),
        border: rgb8(200, 212, 222),
        border_strong: rgb8(152, 168, 184),
        text: rgb8(18, 26, 38),
        text_muted: rgb8(96, 110, 130),
        text_disabled: rgb8(136, 150, 168),
        accent: rgb8(16, 118, 214),
        on_accent: rgb8(255, 255, 255),
        danger: rgb8(208, 44, 56),
        on_danger: rgb8(255, 255, 255),
        success: rgb8(18, 156, 98),
        warning: rgb8(204, 124, 12),
        focus: rgb8(234, 112, 12),
        overlay: rgb8(16, 24, 36),
        knob: rgb8(252, 254, 255),
        is_dark: false,
    };

    pub const CLASSIC_DARK: Self = Self {
        background: rgb8(14, 16, 20),
        surface: rgb8(24, 28, 34),
        surface_raised: rgb8(32, 36, 44),
        surface_floating: rgb8(40, 46, 56),
        surface_alt: rgb8(38, 44, 54),
        border: rgb8(60, 68, 82),
        border_strong: rgb8(92, 104, 122),
        text: rgb8(244, 246, 248),
        text_muted: rgb8(162, 170, 182),
        text_disabled: rgb8(112, 120, 132),
        accent: rgb8(14, 132, 114),
        on_accent: rgb8(255, 255, 255),
        danger: rgb8(216, 62, 72),
        on_danger: rgb8(255, 255, 255),
        success: rgb8(44, 196, 128),
        warning: rgb8(236, 172, 46),
        focus: rgb8(88, 224, 196),
        overlay: rgb8(4, 6, 8),
        knob: rgb8(252, 254, 255),
        is_dark: true,
    };

    pub const CLASSIC_LIGHT: Self = Self {
        background: rgb8(240, 242, 245),
        surface: rgb8(255, 255, 255),
        surface_raised: rgb8(255, 255, 255),
        surface_floating: rgb8(255, 255, 255),
        surface_alt: rgb8(226, 230, 236),
        border: rgb8(194, 202, 212),
        border_strong: rgb8(140, 152, 166),
        text: rgb8(20, 24, 30),
        text_muted: rgb8(92, 102, 116),
        text_disabled: rgb8(138, 146, 158),
        accent: rgb8(12, 122, 106),
        on_accent: rgb8(255, 255, 255),
        danger: rgb8(204, 46, 56),
        on_danger: rgb8(255, 255, 255),
        success: rgb8(16, 148, 92),
        warning: rgb8(192, 118, 14),
        focus: rgb8(12, 122, 106),
        overlay: rgb8(16, 20, 26),
        knob: rgb8(252, 254, 255),
        is_dark: false,
    };

    pub const INDUSTRIAL_DARK: Self = Self {
        background: rgb8(12, 14, 16),
        surface: rgb8(22, 24, 28),
        surface_raised: rgb8(30, 34, 40),
        surface_floating: rgb8(40, 44, 52),
        surface_alt: rgb8(34, 38, 44),
        border: rgb8(68, 74, 84),
        border_strong: rgb8(108, 116, 128),
        text: rgb8(248, 248, 246),
        text_muted: rgb8(168, 174, 182),
        text_disabled: rgb8(110, 116, 124),
        accent: rgb8(196, 88, 12),
        on_accent: rgb8(255, 255, 255),
        danger: rgb8(216, 56, 56),
        on_danger: rgb8(255, 255, 255),
        success: rgb8(52, 198, 112),
        warning: rgb8(244, 188, 40),
        focus: rgb8(255, 188, 40),
        overlay: rgb8(4, 4, 6),
        knob: rgb8(252, 254, 255),
        is_dark: true,
    };

    pub const INDUSTRIAL_LIGHT: Self = Self {
        background: rgb8(232, 234, 238),
        surface: rgb8(250, 250, 252),
        surface_raised: rgb8(255, 255, 255),
        surface_floating: rgb8(255, 255, 255),
        surface_alt: rgb8(218, 222, 228),
        border: rgb8(174, 180, 190),
        border_strong: rgb8(120, 128, 140),
        text: rgb8(16, 18, 22),
        text_muted: rgb8(84, 90, 100),
        text_disabled: rgb8(132, 138, 148),
        accent: rgb8(184, 76, 8),
        on_accent: rgb8(255, 255, 255),
        danger: rgb8(198, 40, 46),
        on_danger: rgb8(255, 255, 255),
        success: rgb8(20, 142, 82),
        warning: rgb8(182, 106, 8),
        focus: rgb8(184, 76, 8),
        overlay: rgb8(12, 14, 18),
        knob: rgb8(252, 254, 255),
        is_dark: false,
    };

    #[inline]
    pub const fn with_background(mut self, c: Rgb565) -> Self {
        self.background = c;
        self
    }
    #[inline]
    pub const fn with_surface(mut self, c: Rgb565) -> Self {
        self.surface = c;
        self
    }
    #[inline]
    pub const fn with_surface_raised(mut self, c: Rgb565) -> Self {
        self.surface_raised = c;
        self
    }
    #[inline]
    pub const fn with_surface_floating(mut self, c: Rgb565) -> Self {
        self.surface_floating = c;
        self
    }
    #[inline]
    pub const fn with_surface_alt(mut self, c: Rgb565) -> Self {
        self.surface_alt = c;
        self
    }
    #[inline]
    pub const fn with_border(mut self, c: Rgb565) -> Self {
        self.border = c;
        self
    }
    #[inline]
    pub const fn with_border_strong(mut self, c: Rgb565) -> Self {
        self.border_strong = c;
        self
    }
    #[inline]
    pub const fn with_text(mut self, c: Rgb565) -> Self {
        self.text = c;
        self
    }
    #[inline]
    pub const fn with_text_muted(mut self, c: Rgb565) -> Self {
        self.text_muted = c;
        self
    }
    #[inline]
    pub const fn with_text_disabled(mut self, c: Rgb565) -> Self {
        self.text_disabled = c;
        self
    }
    #[inline]
    pub const fn with_accent(mut self, c: Rgb565) -> Self {
        self.accent = c;
        self
    }
    #[inline]
    pub const fn with_on_accent(mut self, c: Rgb565) -> Self {
        self.on_accent = c;
        self
    }
    #[inline]
    pub const fn with_danger(mut self, c: Rgb565) -> Self {
        self.danger = c;
        self
    }
    #[inline]
    pub const fn with_on_danger(mut self, c: Rgb565) -> Self {
        self.on_danger = c;
        self
    }
    #[inline]
    pub const fn with_success(mut self, c: Rgb565) -> Self {
        self.success = c;
        self
    }
    #[inline]
    pub const fn with_warning(mut self, c: Rgb565) -> Self {
        self.warning = c;
        self
    }
    #[inline]
    pub const fn with_focus(mut self, c: Rgb565) -> Self {
        self.focus = c;
        self
    }
    #[inline]
    pub const fn with_overlay(mut self, c: Rgb565) -> Self {
        self.overlay = c;
        self
    }
    #[inline]
    pub const fn with_knob(mut self, c: Rgb565) -> Self {
        self.knob = c;
        self
    }
}

/// Control sizing, rhythm, and inner padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Density {
    pub control_height: u16,
    pub spacing: u16,
    pub pad_x: u16,
    pub pad_y: u16,
    pub card_pad: u16,
    pub title_height: u16,
    pub caption_height: u16,
    pub progress_height: u16,
    pub sparkline_height: u16,
    pub toggle_width: u16,
}

impl Density {
    pub const COMPACT: Self = Self {
        control_height: 28,
        spacing: 4,
        pad_x: 8,
        pad_y: 3,
        card_pad: 8,
        title_height: 22,
        caption_height: 18,
        progress_height: 12,
        sparkline_height: 76,
        toggle_width: 48,
    };

    pub const REGULAR: Self = Self {
        control_height: 32,
        spacing: 6,
        pad_x: 10,
        pad_y: 4,
        card_pad: 10,
        title_height: 26,
        caption_height: 22,
        progress_height: 14,
        sparkline_height: 96,
        toggle_width: 52,
    };

    pub const TOUCH: Self = Self {
        control_height: 38,
        spacing: 6,
        pad_x: 10,
        pad_y: 4,
        card_pad: 10,
        title_height: 26,
        caption_height: 22,
        progress_height: 14,
        sparkline_height: 96,
        toggle_width: 52,
    };

    pub const LARGE: Self = Self {
        control_height: 44,
        spacing: 10,
        pad_x: 14,
        pad_y: 6,
        card_pad: 14,
        title_height: 30,
        caption_height: 24,
        progress_height: 16,
        sparkline_height: 112,
        toggle_width: 56,
    };

    #[inline]
    pub const fn with_control_height(mut self, v: u16) -> Self {
        self.control_height = v;
        self
    }
    #[inline]
    pub const fn with_spacing(mut self, v: u16) -> Self {
        self.spacing = v;
        self
    }
    #[inline]
    pub const fn with_pad(mut self, pad_x: u16, pad_y: u16) -> Self {
        self.pad_x = pad_x;
        self.pad_y = pad_y;
        self
    }
    #[inline]
    pub const fn with_card_pad(mut self, v: u16) -> Self {
        self.card_pad = v;
        self
    }
    #[inline]
    pub const fn with_progress_height(mut self, v: u16) -> Self {
        self.progress_height = v;
        self
    }
    #[inline]
    pub const fn with_sparkline_height(mut self, v: u16) -> Self {
        self.sparkline_height = v;
        self
    }
}

impl Default for Density {
    fn default() -> Self {
        Self::REGULAR
    }
}

/// Corner geometry and border thickness scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub radius_sm: u8,
    pub radius_md: u8,
    pub radius_lg: u8,
    pub border_width: u8,
    pub elevated_border_width: u8,
}

impl Shape {
    pub const SHARP: Self = Self {
        radius_sm: 0,
        radius_md: 0,
        radius_lg: 2,
        border_width: 1,
        elevated_border_width: 1,
    };

    pub const INDUSTRIAL: Self = Self {
        radius_sm: 2,
        radius_md: 4,
        radius_lg: 6,
        border_width: 1,
        elevated_border_width: 1,
    };

    pub const SOFT: Self = Self {
        radius_sm: 6,
        radius_md: 8,
        radius_lg: 10,
        border_width: 1,
        elevated_border_width: 1,
    };

    pub const ROUNDED: Self = Self {
        radius_sm: 8,
        radius_md: 12,
        radius_lg: 16,
        border_width: 1,
        elevated_border_width: 1,
    };

    #[inline]
    pub const fn with_radius_sm(mut self, r: u8) -> Self {
        self.radius_sm = r;
        self
    }
    #[inline]
    pub const fn with_radius_md(mut self, r: u8) -> Self {
        self.radius_md = r;
        self
    }
    #[inline]
    pub const fn with_radius_lg(mut self, r: u8) -> Self {
        self.radius_lg = r;
        self
    }
    #[inline]
    pub const fn with_all_radii(mut self, r: u8) -> Self {
        self.radius_sm = r.saturating_sub(2);
        self.radius_md = r;
        self.radius_lg = r.saturating_add(2);
        self
    }
    #[inline]
    pub const fn with_border_width(mut self, w: u8) -> Self {
        self.border_width = w;
        self
    }
    #[inline]
    pub const fn with_elevated_border_width(mut self, w: u8) -> Self {
        self.elevated_border_width = w;
        self
    }
}

impl Default for Shape {
    fn default() -> Self {
        Self::SOFT
    }
}

/// Logical font slots and default text shaping/rendering attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Typography {
    pub title: FontId,
    pub body: FontId,
    pub caption: FontId,
    pub display: FontId,
    pub wrap: WrapMode,
    pub gamma: TextGamma,
    pub subpixel: SubpixelOrder,
    pub letter_spacing: i8,
    pub line_spacing: i8,
}

impl Typography {
    pub const STANDARD: Self = Self {
        title: FontId::Body16,
        body: FontId::Body16,
        caption: FontId::Body16,
        display: FontId::Body16,
        wrap: WrapMode::Char,
        gamma: TextGamma::Linear,
        subpixel: SubpixelOrder::None,
        letter_spacing: 0,
        line_spacing: 0,
    };

    pub const COMPACT_CAPTIONS: Self = Self {
        title: FontId::Body16,
        body: FontId::Body16,
        caption: FontId::Small12,
        display: FontId::Body16,
        wrap: WrapMode::Char,
        gamma: TextGamma::Linear,
        subpixel: SubpixelOrder::None,
        letter_spacing: 0,
        line_spacing: 0,
    };

    #[inline]
    pub const fn all(font: FontId) -> Self {
        Self {
            title: font,
            body: font,
            caption: font,
            display: font,
            wrap: WrapMode::Char,
            gamma: TextGamma::Linear,
            subpixel: SubpixelOrder::None,
            letter_spacing: 0,
            line_spacing: 0,
        }
    }
    #[inline]
    pub const fn with_title(mut self, font: FontId) -> Self {
        self.title = font;
        self
    }
    #[inline]
    pub const fn with_body(mut self, font: FontId) -> Self {
        self.body = font;
        self
    }
    #[inline]
    pub const fn with_caption(mut self, font: FontId) -> Self {
        self.caption = font;
        self
    }
    #[inline]
    pub const fn with_display(mut self, font: FontId) -> Self {
        self.display = font;
        self
    }
    #[inline]
    pub const fn with_wrap(mut self, wrap: WrapMode) -> Self {
        self.wrap = wrap;
        self
    }
    #[inline]
    pub const fn with_gamma(mut self, gamma: TextGamma) -> Self {
        self.gamma = gamma;
        self
    }
    #[inline]
    pub const fn with_subpixel(mut self, subpixel: SubpixelOrder) -> Self {
        self.subpixel = subpixel;
        self
    }
    #[inline]
    pub const fn with_letter_spacing(mut self, letter_spacing: i8) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }
    #[inline]
    pub const fn with_line_spacing(mut self, line_spacing: i8) -> Self {
        self.line_spacing = line_spacing;
        self
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Shadow geometry and optional tonal surface/border overrides for an elevated layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElevationLevel {
    pub shadow_x: i8,
    pub shadow_y: i8,
    pub shadow_blur: u8,
    pub shadow_opacity: u8,
    pub shadow_color: Option<Rgb565>,
    pub surface_override: Option<Rgb565>,
    pub border_override: Option<Rgb565>,
}

impl ElevationLevel {
    pub const fn new(shadow_x: i8, shadow_y: i8, shadow_blur: u8, shadow_opacity: u8) -> Self {
        Self {
            shadow_x,
            shadow_y,
            shadow_blur,
            shadow_opacity: if shadow_opacity > 15 {
                15
            } else {
                shadow_opacity
            },
            shadow_color: None,
            surface_override: None,
            border_override: None,
        }
    }

    #[inline]
    pub const fn with_offset(mut self, x: i8, y: i8) -> Self {
        self.shadow_x = x;
        self.shadow_y = y;
        self
    }
    #[inline]
    pub const fn with_blur(mut self, blur: u8) -> Self {
        self.shadow_blur = blur;
        self
    }
    #[inline]
    pub const fn with_opacity(mut self, opacity: u8) -> Self {
        self.shadow_opacity = if opacity > 15 { 15 } else { opacity };
        self
    }
    #[inline]
    pub const fn with_shadow_color(mut self, color: Option<Rgb565>) -> Self {
        self.shadow_color = color;
        self
    }
    #[inline]
    pub const fn with_surface_override(mut self, color: Option<Rgb565>) -> Self {
        self.surface_override = color;
        self
    }
    #[inline]
    pub const fn with_border_override(mut self, color: Option<Rgb565>) -> Self {
        self.border_override = color;
        self
    }
}

/// Two-step elevation parameters above `Elevation::Flat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElevationScale {
    pub raised: ElevationLevel,
    pub floating: ElevationLevel,
}

impl ElevationScale {
    pub const DARK: Self = Self {
        raised: ElevationLevel::new(0, 3, 4, 9),
        floating: ElevationLevel::new(0, 6, 10, 13),
    };

    pub const LIGHT: Self = Self {
        raised: ElevationLevel::new(0, 2, 4, 4),
        floating: ElevationLevel::new(0, 5, 10, 6),
    };

    pub const SUBTLE: Self = Self {
        raised: ElevationLevel::new(0, 2, 3, 5),
        floating: ElevationLevel::new(0, 4, 6, 8),
    };

    pub const DRAMATIC: Self = Self {
        raised: ElevationLevel::new(0, 4, 6, 10),
        floating: ElevationLevel::new(0, 8, 12, 14),
    };

    #[inline]
    pub const fn with_raised(mut self, level: ElevationLevel) -> Self {
        self.raised = level;
        self
    }
    #[inline]
    pub const fn with_floating(mut self, level: ElevationLevel) -> Self {
        self.floating = level;
        self
    }
}

impl Default for ElevationScale {
    fn default() -> Self {
        Self::DARK
    }
}

/// Frosted backdrop blur and tint scrim configuration for `Material::Glass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlassStyle {
    pub blur_radius: u8,
    pub tint_color: Option<Rgb565>,
    pub tint_alpha: u8,
    pub border_color: Option<Rgb565>,
    pub border_width: u8,
}

impl GlassStyle {
    pub const DARK_FROST: Self = Self {
        blur_radius: 5,
        tint_color: Some(rgb8(16, 24, 38)),
        tint_alpha: 7,
        border_color: Some(rgb8(116, 148, 184)),
        border_width: 1,
    };

    pub const LIGHT_FROST: Self = Self {
        blur_radius: 5,
        tint_color: Some(rgb8(244, 248, 254)),
        tint_alpha: 9,
        border_color: Some(rgb8(255, 255, 255)),
        border_width: 1,
    };

    pub const SUBTLE: Self = Self {
        blur_radius: 3,
        tint_color: None,
        tint_alpha: 5,
        border_color: None,
        border_width: 1,
    };

    pub const CRYSTAL: Self = Self {
        blur_radius: 7,
        tint_color: None,
        tint_alpha: 6,
        border_color: None,
        border_width: 1,
    };

    pub const TINT_ONLY: Self = Self {
        blur_radius: 0,
        tint_color: None,
        tint_alpha: 8,
        border_color: None,
        border_width: 1,
    };

    #[inline]
    pub const fn with_blur_radius(mut self, radius: u8) -> Self {
        self.blur_radius = if radius > 8 { 8 } else { radius };
        self
    }
    #[inline]
    pub const fn with_tint_color(mut self, color: Option<Rgb565>) -> Self {
        self.tint_color = color;
        self
    }
    #[inline]
    pub const fn with_tint_alpha(mut self, alpha: u8) -> Self {
        self.tint_alpha = if alpha > 15 { 15 } else { alpha };
        self
    }
    #[inline]
    pub const fn with_border_color(mut self, color: Option<Rgb565>) -> Self {
        self.border_color = color;
        self
    }
    #[inline]
    pub const fn with_border_width(mut self, width: u8) -> Self {
        self.border_width = width;
        self
    }
}

impl Default for GlassStyle {
    fn default() -> Self {
        Self::DARK_FROST
    }
}

/// Interactive focus ring geometry, press feedback, and modal scrim tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionStyle {
    pub focus: FocusRingStyle,
    pub press_surface: PressFeedback,
    pub press_accent: PressFeedback,
    pub modal_scrim_alpha: u8,
}

impl InteractionStyle {
    pub const DARK: Self = Self {
        focus: FocusRingStyle::DEFAULT,
        press_surface: PressFeedback::Lighten(2),
        press_accent: PressFeedback::Darken(3),
        modal_scrim_alpha: 10,
    };

    pub const LIGHT: Self = Self {
        focus: FocusRingStyle::DEFAULT,
        press_surface: PressFeedback::Darken(2),
        press_accent: PressFeedback::Darken(3),
        modal_scrim_alpha: 10,
    };

    #[inline]
    pub const fn with_focus(mut self, focus: FocusRingStyle) -> Self {
        self.focus = focus;
        self
    }
    #[inline]
    pub const fn with_rounded_focus(mut self, rounded: bool) -> Self {
        self.focus.rounded = rounded;
        self
    }
    #[inline]
    pub const fn with_focus_width(mut self, width: u8) -> Self {
        self.focus.width = width;
        self
    }
    #[inline]
    pub const fn with_focus_offset(mut self, offset: u8) -> Self {
        self.focus.offset = offset;
        self
    }
    #[inline]
    pub const fn with_press_surface(mut self, feedback: PressFeedback) -> Self {
        self.press_surface = feedback;
        self
    }
    #[inline]
    pub const fn with_press_accent(mut self, feedback: PressFeedback) -> Self {
        self.press_accent = feedback;
        self
    }
    #[inline]
    pub const fn with_modal_scrim_alpha(mut self, alpha: u8) -> Self {
        self.modal_scrim_alpha = if alpha > 15 { 15 } else { alpha };
        self
    }
}

impl Default for InteractionStyle {
    fn default() -> Self {
        Self::DARK
    }
}

/// Durations and easing curves used by themed controls and depth animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    pub toggle_easing: Easing,
    pub press_depth_ms: u16,
    pub press_depth_scale: u8,
    pub press_depth_easing: Easing,
    pub toast_ms: u64,
    pub flip_ms: u32,
}

impl Motion {
    pub const STANDARD: Self = Self {
        toggle_easing: Easing::EaseOutCubic,
        press_depth_ms: 120,
        press_depth_scale: 15,
        press_depth_easing: Easing::EaseOutCubic,
        toast_ms: 2000,
        flip_ms: 640,
    };

    pub const SNAPPY: Self = Self {
        toggle_easing: Easing::EaseOutCubic,
        press_depth_ms: 70,
        press_depth_scale: 12,
        press_depth_easing: Easing::EaseOutCubic,
        toast_ms: 1500,
        flip_ms: 420,
    };

    pub const REDUCED: Self = Self {
        toggle_easing: Easing::Linear,
        press_depth_ms: 0,
        press_depth_scale: 10,
        press_depth_easing: Easing::Linear,
        toast_ms: 2500,
        flip_ms: 0,
    };

    #[inline]
    pub const fn with_toggle_easing(mut self, easing: Easing) -> Self {
        self.toggle_easing = easing;
        self
    }
    #[inline]
    pub const fn with_press_depth_ms(mut self, ms: u16) -> Self {
        self.press_depth_ms = ms;
        self
    }
    #[inline]
    pub const fn with_press_depth_scale(mut self, scale: u8) -> Self {
        self.press_depth_scale = if scale > 30 { 30 } else { scale };
        self
    }
    #[inline]
    pub const fn with_press_depth_easing(mut self, easing: Easing) -> Self {
        self.press_depth_easing = easing;
        self
    }
    #[inline]
    pub const fn with_toast_ms(mut self, ms: u64) -> Self {
        self.toast_ms = ms;
        self
    }
    #[inline]
    pub const fn with_flip_ms(mut self, ms: u32) -> Self {
        self.flip_ms = ms;
        self
    }
}

impl Default for Motion {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Convenience preset tiers that construct an `EffectsConfig`. Every capability remains
/// individually overridable on `EffectsConfig` afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectTier {
    Flat,
    Depth,
    Tinted,
    Glass,
}

/// Fine-grained visual capability gates and hardware budget limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectsConfig {
    pub shadows: bool,
    pub max_shadow_blur: u8,
    pub shadow_opacity_scale: u8,
    pub press_depth: bool,
    pub elevation_tone: bool,
    pub glass_blur: bool,
    pub max_blur_radius: u8,
    pub glass_tint: bool,
    pub group_opacity: bool,
    pub projection_3d: bool,
}

impl EffectsConfig {
    pub const FLAT: Self = Self {
        shadows: false,
        max_shadow_blur: 0,
        shadow_opacity_scale: 0,
        press_depth: false,
        elevation_tone: true,
        glass_blur: false,
        max_blur_radius: 0,
        glass_tint: false,
        group_opacity: false,
        projection_3d: false,
    };

    pub const DEPTH: Self = Self {
        shadows: true,
        max_shadow_blur: 12,
        shadow_opacity_scale: 15,
        press_depth: true,
        elevation_tone: true,
        glass_blur: false,
        max_blur_radius: 0,
        glass_tint: false,
        group_opacity: false,
        projection_3d: false,
    };

    pub const TINTED: Self = Self {
        shadows: true,
        max_shadow_blur: 12,
        shadow_opacity_scale: 15,
        press_depth: true,
        elevation_tone: true,
        glass_blur: false,
        max_blur_radius: 0,
        glass_tint: true,
        group_opacity: true,
        projection_3d: true,
    };

    pub const GLASS: Self = Self {
        shadows: true,
        max_shadow_blur: 12,
        shadow_opacity_scale: 15,
        press_depth: true,
        elevation_tone: true,
        glass_blur: true,
        max_blur_radius: 8,
        glass_tint: true,
        group_opacity: true,
        projection_3d: true,
    };

    #[inline]
    pub const fn from_tier(tier: EffectTier) -> Self {
        match tier {
            EffectTier::Flat => Self::FLAT,
            EffectTier::Depth => Self::DEPTH,
            EffectTier::Tinted => Self::TINTED,
            EffectTier::Glass => Self::GLASS,
        }
    }

    #[inline]
    pub const fn with_shadows(mut self, enabled: bool) -> Self {
        self.shadows = enabled;
        self
    }
    #[inline]
    pub const fn with_max_shadow_blur(mut self, blur: u8) -> Self {
        self.max_shadow_blur = blur;
        self
    }
    #[inline]
    pub const fn with_shadow_opacity_scale(mut self, scale: u8) -> Self {
        self.shadow_opacity_scale = if scale > 15 { 15 } else { scale };
        self
    }
    #[inline]
    pub const fn with_press_depth(mut self, enabled: bool) -> Self {
        self.press_depth = enabled;
        self
    }
    #[inline]
    pub const fn with_elevation_tone(mut self, enabled: bool) -> Self {
        self.elevation_tone = enabled;
        self
    }
    #[inline]
    pub const fn with_glass_blur(mut self, enabled: bool) -> Self {
        self.glass_blur = enabled;
        self
    }
    #[inline]
    pub const fn with_max_blur_radius(mut self, radius: u8) -> Self {
        self.max_blur_radius = if radius > 8 { 8 } else { radius };
        self
    }
    #[inline]
    pub const fn with_glass_tint(mut self, enabled: bool) -> Self {
        self.glass_tint = enabled;
        self
    }
    #[inline]
    pub const fn with_group_opacity(mut self, enabled: bool) -> Self {
        self.group_opacity = enabled;
        self
    }
    #[inline]
    pub const fn with_projection_3d(mut self, enabled: bool) -> Self {
        self.projection_3d = enabled;
        self
    }
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self::GLASS
    }
}
