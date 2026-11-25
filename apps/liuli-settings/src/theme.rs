use liuli::prelude::{FocusRingStyle, NodeProps, Rgb565};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Theme {
    pub id: u16,
    pub bg: Rgb565,
    pub header_bg: Rgb565,
    pub surface: Rgb565,
    pub surface_alt: Rgb565,
    pub border: Rgb565,
    pub text: Rgb565,
    pub text_muted: Rgb565,
    pub accent: Rgb565,
    pub focus: Rgb565,
    pub overlay: Rgb565,
    pub danger: Rgb565,
    pub danger_text: Rgb565,
    pub track_off: Rgb565,
    pub knob: Rgb565,
    pub on_accent: Rgb565,
}

impl Theme {
    /// The two v0.2 reference applications use square navigation outlines.
    pub(crate) fn control(&self) -> NodeProps {
        NodeProps {
            focus_color: self.focus,
            focus_ring: FocusRingStyle {
                rounded: false,
                ..FocusRingStyle::DEFAULT
            },
            ..NodeProps::new()
        }
    }
}

const fn rgb8(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

pub const LIGHT_THEME: Theme = Theme {
    id: 0,
    bg: rgb8(242, 245, 248),
    header_bg: rgb8(255, 255, 255),
    surface: rgb8(255, 255, 255),
    surface_alt: rgb8(232, 238, 244),
    border: rgb8(204, 214, 224),
    text: rgb8(24, 32, 44),
    text_muted: rgb8(96, 110, 128),
    accent: rgb8(24, 119, 242),
    focus: rgb8(234, 118, 0),
    overlay: rgb8(12, 18, 28),
    danger: rgb8(212, 56, 56),
    danger_text: rgb8(204, 44, 44),
    track_off: rgb8(184, 194, 206),
    knob: rgb8(255, 255, 255),
    on_accent: rgb8(255, 255, 255),
};

pub const DARK_THEME: Theme = Theme {
    id: 1,
    bg: rgb8(16, 20, 26),
    header_bg: rgb8(24, 30, 38),
    surface: rgb8(28, 35, 44),
    surface_alt: rgb8(38, 46, 58),
    border: rgb8(62, 74, 90),
    text: rgb8(236, 242, 248),
    text_muted: rgb8(152, 166, 184),
    accent: rgb8(56, 152, 255),
    focus: rgb8(255, 192, 48),
    overlay: rgb8(0, 0, 0),
    danger: rgb8(224, 76, 76),
    danger_text: rgb8(255, 132, 132),
    track_off: rgb8(76, 88, 104),
    knob: rgb8(246, 250, 255),
    on_accent: rgb8(255, 255, 255),
};
