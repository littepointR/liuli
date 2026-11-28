use crate::capacity::{BuildError, MAX_NODES};
use crate::geometry::{Rect, Rgb565, Size};
use crate::view::Key;

/// Maximum distinct part styles per scene. Identical styles share one entry.
pub use crate::capacity::MAX_PART_STYLES;
const _: () = assert!(MAX_NODES <= u8::MAX as usize);

/// Maximum per-node layout offsets / custom revisions per scene graph.
pub use crate::capacity::MAX_LAYOUT_EXTRAS;

const BINDING_BYTES: usize = match MAX_PART_STYLES {
    0 => 0,
    1..=15 => MAX_NODES.div_ceil(2),
    _ => MAX_NODES,
};
/// Logical sub-parts of built-in widgets that can be styled independently.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum WidgetPart {
    #[default]
    Main = 0,
    Track = 1,
    Indicator = 2,
    Knob = 3,
    Content = 4,
    FocusRing = 5,
    Icon = 6,
    Custom(u32),
}

/// The selection decoration occupies the control or follows its measured content.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum IndicatorSpan {
    #[default]
    Control,
    Content,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum IndicatorShape {
    #[default]
    Bottom,
    Top,
    Fill,
    Hidden,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum IndicatorMotion {
    #[default]
    Reveal,
    Fade,
    Slide,
    Instant,
}

/// Paint geometry only. Neither sizing nor hit testing uses the decoration.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IndicatorLayout {
    pub shape: IndicatorShape,
    pub span: IndicatorSpan,
    pub thickness: u8,
    pub edge_offset: u8,
}

impl IndicatorLayout {
    pub const UNDERLINE: Self = Self {
        shape: IndicatorShape::Bottom,
        span: IndicatorSpan::Control,
        thickness: 3,
        edge_offset: 0,
    };

    /// Both rectangles use the same coordinate space. Insets reduce the horizontal span.
    pub fn bounds(self, control: Rect, content: Rect, inset: u8) -> Rect {
        let span = match self.span {
            IndicatorSpan::Control => control,
            IndicatorSpan::Content => content.intersect(control),
        };
        let area =
            Rect::new(span.x, control.y, span.w, control.h).inset(inset.into(), 0, inset.into(), 0);
        let offset = u32::from(self.edge_offset).min(area.h);
        let h = u32::from(self.thickness).min(area.h - offset);
        match self.shape {
            IndicatorShape::Top => Rect::new(area.x, area.y + offset as i32, area.w, h),
            IndicatorShape::Bottom => Rect::new(
                area.x,
                (area.bottom64() - i64::from(h + offset)) as i32,
                area.w,
                h,
            ),
            IndicatorShape::Fill => area.inset(0, offset, 0, offset),
            IndicatorShape::Hidden => Rect::ZERO,
        }
    }
}

/// Interactive visual state flags for state-aware styling.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct VisualState(pub u8);

impl VisualState {
    pub const NORMAL: Self = Self(0);
    pub const PRESSED: Self = Self(1 << 0);
    pub const FOCUSED: Self = Self(1 << 1);
    pub const EDITING: Self = Self(1 << 2);
    pub const CHECKED: Self = Self(1 << 3);
    pub const DISABLED: Self = Self(1 << 4);

    #[inline]
    pub const fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) == flag.0
    }
}

/// Icon placement inside `NodeKind::Button`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum IconPosition {
    #[default]
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
}

const HAS_TRACK_COLOR: u32 = 1 << 0;
const HAS_TRACK_BORDER_COLOR: u32 = 1 << 1;
const HAS_TRACK_BORDER_WIDTH: u32 = 1 << 2;
const HAS_TRACK_THICKNESS: u32 = 1 << 3;
const HAS_TRACK_RADIUS: u32 = 1 << 4;
const HAS_INDICATOR_COLOR: u32 = 1 << 5;
const HAS_INDICATOR_RADIUS: u32 = 1 << 6;
const HAS_INDICATOR_INSET: u32 = 1 << 7;
const HAS_KNOB_COLOR: u32 = 1 << 8;
const HAS_KNOB_BORDER_COLOR: u32 = 1 << 9;
const HAS_KNOB_BORDER_WIDTH: u32 = 1 << 10;
const HAS_KNOB_SIZE: u32 = 1 << 11;
const HAS_KNOB_RADIUS: u32 = 1 << 12;
const HAS_ICON_COLOR: u32 = 1 << 13;
const HAS_ICON_SIZE: u32 = 1 << 14;
const HAS_ICON_GAP: u32 = 1 << 15;
const HAS_ICON_POS: u32 = 1 << 16;
const HAS_PRESSED_BG: u32 = 1 << 17;
const HAS_PRESSED_FG: u32 = 1 << 18;
const HAS_FOCUSED_BORDER: u32 = 1 << 19;
const HAS_DISABLED_BG: u32 = 1 << 20;
const HAS_DISABLED_FG: u32 = 1 << 21;
const HAS_CHECKED_BG: u32 = 1 << 22;
const HAS_CHECKED_FG: u32 = 1 << 23;
const HAS_HIDE_SCROLLBAR: u32 = 1 << 24;
const HAS_SPARKLINE_WIDTH: u32 = 1 << 25;
const HAS_INDICATOR_LAYOUT: u32 = 1 << 26;
const HAS_INDICATOR_MOTION: u32 = 1 << 27;

/// Part dimensions available during intrinsic measurement. Paint-only properties
/// such as selection decorations cannot accidentally become layout dependencies.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct PartMetrics {
    pub icon_size: Option<u8>,
    pub icon_gap: Option<u8>,
    pub icon_position: Option<IconPosition>,
    pub knob_size: Option<(u8, u8)>,
    pub track_thickness: Option<u8>,
}

/// Sparse per-part and per-state style overrides stored in static scene tables.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct WidgetPartStyle {
    pub present: u32,
    #[cfg(feature = "part-painters")]
    painter: Option<&'static crate::paint::PartPainter>,
    pub track_color: Rgb565,
    pub track_border_color: Rgb565,
    pub indicator_color: Rgb565,
    pub knob_color: Rgb565,
    pub knob_border_color: Rgb565,
    pub icon_color: Rgb565,
    pub pressed_bg: Rgb565,
    pub pressed_fg: Rgb565,
    pub focused_border: Rgb565,
    pub disabled_bg: Rgb565,
    pub disabled_fg: Rgb565,
    pub checked_bg: Rgb565,
    pub checked_fg: Rgb565,
    pub track_thickness: u8,
    pub track_radius: u8,
    pub track_border_width: u8,
    pub indicator_radius: u8,
    pub indicator_inset: u8,
    pub knob_width: u8,
    pub knob_height: u8,
    pub knob_radius: u8,
    pub knob_border_width: u8,
    pub icon_size: u8,
    pub icon_gap: u8,
    pub icon_pos: IconPosition,
    pub sparkline_width: u8,
    pub hide_scrollbar: bool,
    indicator_geometry: IndicatorLayout,
    pub indicator_transition: IndicatorMotion,
}

impl Default for IndicatorLayout {
    fn default() -> Self {
        Self::UNDERLINE
    }
}

impl Default for WidgetPartStyle {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(not(feature = "part-painters"))]
const _: () = assert!(core::mem::size_of::<WidgetPartStyle>() <= 52);

impl WidgetPartStyle {
    pub const ZERO: Self = Self {
        present: 0,
        #[cfg(feature = "part-painters")]
        painter: None,
        track_color: Rgb565::new(0, 0, 0),
        track_border_color: Rgb565::new(0, 0, 0),
        indicator_color: Rgb565::new(0, 0, 0),
        knob_color: Rgb565::new(0, 0, 0),
        knob_border_color: Rgb565::new(0, 0, 0),
        icon_color: Rgb565::new(0, 0, 0),
        pressed_bg: Rgb565::new(0, 0, 0),
        pressed_fg: Rgb565::new(0, 0, 0),
        focused_border: Rgb565::new(0, 0, 0),
        disabled_bg: Rgb565::new(0, 0, 0),
        disabled_fg: Rgb565::new(0, 0, 0),
        checked_bg: Rgb565::new(0, 0, 0),
        checked_fg: Rgb565::new(0, 0, 0),
        track_thickness: 0,
        track_radius: 0,
        track_border_width: 0,
        indicator_radius: 0,
        indicator_inset: 0,
        knob_width: 0,
        knob_height: 0,
        knob_radius: 0,
        knob_border_width: 0,
        icon_size: 0,
        icon_gap: 0,
        icon_pos: IconPosition::Left,
        sparkline_width: 0,
        hide_scrollbar: false,
        indicator_geometry: IndicatorLayout {
            thickness: 0,
            ..IndicatorLayout::UNDERLINE
        },
        indicator_transition: IndicatorMotion::Reveal,
    };

    #[inline]
    pub const fn new() -> Self {
        Self::ZERO
    }

    pub const fn with_indicator_layout(mut self, layout: IndicatorLayout) -> Self {
        self.present |= HAS_INDICATOR_LAYOUT;
        self.indicator_geometry = layout;
        self
    }

    pub const fn indicator_layout(&self) -> IndicatorLayout {
        if self.present & HAS_INDICATOR_LAYOUT != 0 {
            self.indicator_geometry
        } else {
            IndicatorLayout::UNDERLINE
        }
    }

    pub const fn with_indicator_motion(mut self, motion: IndicatorMotion) -> Self {
        self.present |= HAS_INDICATOR_MOTION;
        self.indicator_transition = motion;
        self
    }

    pub fn foreground(&self, state: VisualState, fallback: Rgb565) -> Rgb565 {
        self.state_color(
            state,
            self.disabled_fg(),
            self.pressed_fg(),
            self.checked_fg(),
        )
        .unwrap_or(fallback)
    }

    pub fn background(&self, state: VisualState) -> Option<Rgb565> {
        self.state_color(
            state,
            self.disabled_bg(),
            self.pressed_bg(),
            self.checked_bg(),
        )
    }

    fn state_color(
        &self,
        state: VisualState,
        disabled: Option<Rgb565>,
        pressed: Option<Rgb565>,
        checked: Option<Rgb565>,
    ) -> Option<Rgb565> {
        [
            (VisualState::DISABLED, disabled),
            (VisualState::PRESSED, pressed),
            (VisualState::CHECKED, checked),
        ]
        .into_iter()
        .find_map(|(flag, color)| color.filter(|_| state.contains(flag)))
    }

    #[inline]
    /// Install a static renderer shared by every instance using this style.
    #[cfg(feature = "part-painters")]
    pub const fn with_painter(
        mut self,
        painter: Option<&'static crate::paint::PartPainter>,
    ) -> Self {
        self.present |= 1 << 28;
        self.painter = painter;
        self
    }
    #[cfg(feature = "part-painters")]
    pub(crate) const fn painter(&self) -> Option<&'static crate::paint::PartPainter> {
        self.painter
    }

    pub const fn is_empty(&self) -> bool {
        self.present == 0
    }

    pub const fn metrics(&self) -> PartMetrics {
        PartMetrics {
            icon_size: self.icon_size(),
            icon_gap: self.icon_gap(),
            icon_position: self.icon_pos(),
            knob_size: self.knob_size(),
            track_thickness: self.track_thickness(),
        }
    }

    #[inline]
    pub const fn affects_button_measure(&self, other: &Self) -> bool {
        let m = HAS_ICON_SIZE | HAS_ICON_GAP | HAS_ICON_POS;
        if (self.present & m) != (other.present & m) {
            return true;
        }
        ((self.present & HAS_ICON_SIZE) != 0 && self.icon_size != other.icon_size)
            || ((self.present & HAS_ICON_GAP) != 0 && self.icon_gap != other.icon_gap)
            || ((self.present & HAS_ICON_POS) != 0
                && (self.icon_pos as u8) != (other.icon_pos as u8))
    }

    #[inline]
    pub const fn with_track_color(mut self, color: Rgb565) -> Self {
        self.present |= HAS_TRACK_COLOR;
        self.track_color = color;
        self
    }

    #[inline]
    pub const fn with_track_border(mut self, color: Rgb565, width: u8) -> Self {
        self.present |= HAS_TRACK_BORDER_COLOR | HAS_TRACK_BORDER_WIDTH;
        self.track_border_color = color;
        self.track_border_width = width;
        self
    }

    #[inline]
    pub const fn with_track_size(mut self, thickness: u8) -> Self {
        self.present |= HAS_TRACK_THICKNESS;
        self.track_thickness = thickness;
        self
    }

    #[inline]
    pub const fn with_track_radius(mut self, radius: u8) -> Self {
        self.present |= HAS_TRACK_RADIUS;
        self.track_radius = radius;
        self
    }

    #[inline]
    pub const fn with_indicator_color(mut self, color: Rgb565) -> Self {
        self.present |= HAS_INDICATOR_COLOR;
        self.indicator_color = color;
        self
    }

    #[inline]
    pub const fn with_indicator_radius(mut self, radius: u8) -> Self {
        self.present |= HAS_INDICATOR_RADIUS;
        self.indicator_radius = radius;
        self
    }

    #[inline]
    pub const fn with_indicator_inset(mut self, inset: u8) -> Self {
        self.present |= HAS_INDICATOR_INSET;
        self.indicator_inset = inset;
        self
    }

    #[inline]
    pub const fn with_knob_color(mut self, color: Rgb565) -> Self {
        self.present |= HAS_KNOB_COLOR;
        self.knob_color = color;
        self
    }

    #[inline]
    pub const fn with_knob_border(mut self, color: Rgb565, width: u8) -> Self {
        self.present |= HAS_KNOB_BORDER_COLOR | HAS_KNOB_BORDER_WIDTH;
        self.knob_border_color = color;
        self.knob_border_width = width;
        self
    }

    #[inline]
    pub const fn with_knob_size(mut self, width: u8, height: u8) -> Self {
        self.present |= HAS_KNOB_SIZE;
        self.knob_width = width;
        self.knob_height = height;
        self
    }

    /// Local track bounds and the stable area swept by the slider's thumb.
    pub fn slider_track_bounds(&self, size: Size) -> (Rect, Rect) {
        let (kw, kh) = self.slider_knob_size(size);
        let h = self.track_thickness().map_or_else(
            || size.height.min(6),
            |v| size.height.min(u32::from(v.max(1))),
        );
        let track = Rect::new(
            (kw / 2) as i32,
            ((size.height - h) / 2) as i32,
            size.width - kw,
            h,
        );
        let swept = Rect::new(0, ((size.height - kh) / 2) as i32, size.width, kh);
        (track, track.union(swept))
    }
    pub(crate) fn slider_knob_size(&self, size: Size) -> (u32, u32) {
        let (w, h) = self.knob_size().unwrap_or((12, 20));
        (
            size.width.min(u32::from(w.max(2))),
            size.height.min(u32::from(h.max(2))),
        )
    }

    #[inline]
    pub const fn with_knob_radius(mut self, radius: u8) -> Self {
        self.present |= HAS_KNOB_RADIUS;
        self.knob_radius = radius;
        self
    }

    #[inline]
    pub const fn with_icon_color(mut self, color: Rgb565) -> Self {
        self.present |= HAS_ICON_COLOR;
        self.icon_color = color;
        self
    }

    #[inline]
    pub const fn with_icon_size(mut self, size: u8) -> Self {
        self.present |= HAS_ICON_SIZE;
        self.icon_size = size;
        self
    }

    #[inline]
    pub const fn with_icon_gap(mut self, gap: u8) -> Self {
        self.present |= HAS_ICON_GAP;
        self.icon_gap = gap;
        self
    }

    #[inline]
    pub const fn with_icon_pos(mut self, pos: IconPosition) -> Self {
        self.present |= HAS_ICON_POS;
        self.icon_pos = pos;
        self
    }

    #[inline]
    pub const fn with_pressed_bg(mut self, color: Rgb565) -> Self {
        self.present |= HAS_PRESSED_BG;
        self.pressed_bg = color;
        self
    }

    #[inline]
    pub const fn with_pressed_fg(mut self, color: Rgb565) -> Self {
        self.present |= HAS_PRESSED_FG;
        self.pressed_fg = color;
        self
    }

    #[inline]
    pub const fn with_focused_border(mut self, color: Rgb565) -> Self {
        self.present |= HAS_FOCUSED_BORDER;
        self.focused_border = color;
        self
    }

    #[inline]
    pub const fn with_disabled_bg(mut self, color: Rgb565) -> Self {
        self.present |= HAS_DISABLED_BG;
        self.disabled_bg = color;
        self
    }

    #[inline]
    pub const fn with_disabled_fg(mut self, color: Rgb565) -> Self {
        self.present |= HAS_DISABLED_FG;
        self.disabled_fg = color;
        self
    }

    #[inline]
    pub const fn with_checked_bg(mut self, color: Rgb565) -> Self {
        self.present |= HAS_CHECKED_BG;
        self.checked_bg = color;
        self
    }

    #[inline]
    pub const fn with_checked_fg(mut self, color: Rgb565) -> Self {
        self.present |= HAS_CHECKED_FG;
        self.checked_fg = color;
        self
    }

    #[inline]
    pub const fn with_hide_scrollbar(mut self, hide: bool) -> Self {
        self.present |= HAS_HIDE_SCROLLBAR;
        self.hide_scrollbar = hide;
        self
    }

    #[inline]
    pub const fn with_sparkline_width(mut self, width: u8) -> Self {
        self.present |= HAS_SPARKLINE_WIDTH;
        self.sparkline_width = width;
        self
    }

    #[inline]
    pub const fn track_color(&self) -> Option<Rgb565> {
        if (self.present & HAS_TRACK_COLOR) != 0 {
            Some(self.track_color)
        } else {
            None
        }
    }

    #[inline]
    pub const fn track_border_color(&self) -> Option<Rgb565> {
        if (self.present & HAS_TRACK_BORDER_COLOR) != 0 {
            Some(self.track_border_color)
        } else {
            None
        }
    }

    #[inline]
    pub const fn track_border_width(&self) -> Option<u8> {
        if (self.present & HAS_TRACK_BORDER_WIDTH) != 0 {
            Some(self.track_border_width)
        } else {
            None
        }
    }

    #[inline]
    pub const fn track_thickness(&self) -> Option<u8> {
        if (self.present & HAS_TRACK_THICKNESS) != 0 {
            Some(self.track_thickness)
        } else {
            None
        }
    }

    #[inline]
    pub const fn track_radius(&self) -> Option<u8> {
        if (self.present & HAS_TRACK_RADIUS) != 0 {
            Some(self.track_radius)
        } else {
            None
        }
    }

    #[inline]
    pub const fn indicator_color(&self) -> Option<Rgb565> {
        if (self.present & HAS_INDICATOR_COLOR) != 0 {
            Some(self.indicator_color)
        } else {
            None
        }
    }

    #[inline]
    pub const fn indicator_radius(&self) -> Option<u8> {
        if (self.present & HAS_INDICATOR_RADIUS) != 0 {
            Some(self.indicator_radius)
        } else {
            None
        }
    }

    #[inline]
    pub const fn indicator_inset(&self) -> Option<u8> {
        if (self.present & HAS_INDICATOR_INSET) != 0 {
            Some(self.indicator_inset)
        } else {
            None
        }
    }

    #[inline]
    pub const fn knob_color(&self) -> Option<Rgb565> {
        if (self.present & HAS_KNOB_COLOR) != 0 {
            Some(self.knob_color)
        } else {
            None
        }
    }

    #[inline]
    pub const fn knob_border_color(&self) -> Option<Rgb565> {
        if (self.present & HAS_KNOB_BORDER_COLOR) != 0 {
            Some(self.knob_border_color)
        } else {
            None
        }
    }

    #[inline]
    pub const fn knob_border_width(&self) -> Option<u8> {
        if (self.present & HAS_KNOB_BORDER_WIDTH) != 0 {
            Some(self.knob_border_width)
        } else {
            None
        }
    }

    #[inline]
    pub const fn knob_size(&self) -> Option<(u8, u8)> {
        if (self.present & HAS_KNOB_SIZE) != 0 {
            Some((self.knob_width, self.knob_height))
        } else {
            None
        }
    }

    #[inline]
    pub const fn knob_radius(&self) -> Option<u8> {
        if (self.present & HAS_KNOB_RADIUS) != 0 {
            Some(self.knob_radius)
        } else {
            None
        }
    }

    #[inline]
    pub const fn icon_color(&self) -> Option<Rgb565> {
        if (self.present & HAS_ICON_COLOR) != 0 {
            Some(self.icon_color)
        } else {
            None
        }
    }

    #[inline]
    pub const fn icon_size(&self) -> Option<u8> {
        if (self.present & HAS_ICON_SIZE) != 0 {
            Some(self.icon_size)
        } else {
            None
        }
    }

    #[inline]
    pub const fn icon_gap(&self) -> Option<u8> {
        if (self.present & HAS_ICON_GAP) != 0 {
            Some(self.icon_gap)
        } else {
            None
        }
    }

    #[inline]
    pub const fn icon_pos(&self) -> Option<IconPosition> {
        if (self.present & HAS_ICON_POS) != 0 {
            Some(self.icon_pos)
        } else {
            None
        }
    }

    #[inline]
    pub const fn pressed_bg(&self) -> Option<Rgb565> {
        if (self.present & HAS_PRESSED_BG) != 0 {
            Some(self.pressed_bg)
        } else {
            None
        }
    }

    #[inline]
    pub const fn pressed_fg(&self) -> Option<Rgb565> {
        if (self.present & HAS_PRESSED_FG) != 0 {
            Some(self.pressed_fg)
        } else {
            None
        }
    }

    #[inline]
    pub const fn focused_border(&self) -> Option<Rgb565> {
        if (self.present & HAS_FOCUSED_BORDER) != 0 {
            Some(self.focused_border)
        } else {
            None
        }
    }

    #[inline]
    pub const fn disabled_bg(&self) -> Option<Rgb565> {
        if (self.present & HAS_DISABLED_BG) != 0 {
            Some(self.disabled_bg)
        } else {
            None
        }
    }

    #[inline]
    pub const fn disabled_fg(&self) -> Option<Rgb565> {
        if (self.present & HAS_DISABLED_FG) != 0 {
            Some(self.disabled_fg)
        } else {
            None
        }
    }

    #[inline]
    pub const fn checked_bg(&self) -> Option<Rgb565> {
        if (self.present & HAS_CHECKED_BG) != 0 {
            Some(self.checked_bg)
        } else {
            None
        }
    }

    #[inline]
    pub const fn checked_fg(&self) -> Option<Rgb565> {
        if (self.present & HAS_CHECKED_FG) != 0 {
            Some(self.checked_fg)
        } else {
            None
        }
    }

    #[inline]
    pub const fn scrollbar_hidden(&self) -> bool {
        (self.present & HAS_HIDE_SCROLLBAR) != 0 && self.hide_scrollbar
    }

    #[inline]
    pub const fn sparkline_width(&self) -> Option<u8> {
        if (self.present & HAS_SPARKLINE_WIDTH) != 0 {
            Some(self.sparkline_width)
        } else {
            None
        }
    }

    /// Merge `over` fields onto `self` without allocations.
    pub const fn merge(mut self, over: Self) -> Self {
        let p = over.present;
        if p == 0 {
            return self;
        }
        self.present |= p;
        #[cfg(feature = "part-painters")]
        if p & (1 << 28) != 0 {
            self.painter = over.painter;
        }
        if p & HAS_INDICATOR_LAYOUT != 0 {
            self.indicator_geometry = over.indicator_geometry;
        }
        if p & HAS_INDICATOR_MOTION != 0 {
            self.indicator_transition = over.indicator_transition;
        }
        if (p & HAS_TRACK_COLOR) != 0 {
            self.track_color = over.track_color;
        }
        if (p & HAS_TRACK_BORDER_COLOR) != 0 {
            self.track_border_color = over.track_border_color;
        }
        if (p & HAS_TRACK_BORDER_WIDTH) != 0 {
            self.track_border_width = over.track_border_width;
        }
        if (p & HAS_TRACK_THICKNESS) != 0 {
            self.track_thickness = over.track_thickness;
        }
        if (p & HAS_TRACK_RADIUS) != 0 {
            self.track_radius = over.track_radius;
        }
        if (p & HAS_INDICATOR_COLOR) != 0 {
            self.indicator_color = over.indicator_color;
        }
        if (p & HAS_INDICATOR_RADIUS) != 0 {
            self.indicator_radius = over.indicator_radius;
        }
        if (p & HAS_INDICATOR_INSET) != 0 {
            self.indicator_inset = over.indicator_inset;
        }
        if (p & HAS_KNOB_COLOR) != 0 {
            self.knob_color = over.knob_color;
        }
        if (p & HAS_KNOB_BORDER_COLOR) != 0 {
            self.knob_border_color = over.knob_border_color;
        }
        if (p & HAS_KNOB_BORDER_WIDTH) != 0 {
            self.knob_border_width = over.knob_border_width;
        }
        if (p & HAS_KNOB_SIZE) != 0 {
            self.knob_width = over.knob_width;
            self.knob_height = over.knob_height;
        }
        if (p & HAS_KNOB_RADIUS) != 0 {
            self.knob_radius = over.knob_radius;
        }
        if (p & HAS_ICON_COLOR) != 0 {
            self.icon_color = over.icon_color;
        }
        if (p & HAS_ICON_SIZE) != 0 {
            self.icon_size = over.icon_size;
        }
        if (p & HAS_ICON_GAP) != 0 {
            self.icon_gap = over.icon_gap;
        }
        if (p & HAS_ICON_POS) != 0 {
            self.icon_pos = over.icon_pos;
        }
        if (p & HAS_PRESSED_BG) != 0 {
            self.pressed_bg = over.pressed_bg;
        }
        if (p & HAS_PRESSED_FG) != 0 {
            self.pressed_fg = over.pressed_fg;
        }
        if (p & HAS_FOCUSED_BORDER) != 0 {
            self.focused_border = over.focused_border;
        }
        if (p & HAS_DISABLED_BG) != 0 {
            self.disabled_bg = over.disabled_bg;
        }
        if (p & HAS_DISABLED_FG) != 0 {
            self.disabled_fg = over.disabled_fg;
        }
        if (p & HAS_CHECKED_BG) != 0 {
            self.checked_bg = over.checked_bg;
        }
        if (p & HAS_CHECKED_FG) != 0 {
            self.checked_fg = over.checked_fg;
        }
        if (p & HAS_HIDE_SCROLLBAR) != 0 {
            self.hide_scrollbar = over.hide_scrollbar;
        }
        if (p & HAS_SPARKLINE_WIDTH) != 0 {
            self.sparkline_width = over.sparkline_width;
        }
        self
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PartStyles {
    bindings: [u8; BINDING_BYTES],
    styles: [WidgetPartStyle; MAX_PART_STYLES],
    references: [u8; MAX_PART_STYLES],
}

impl PartStyles {
    pub const EMPTY: Self = Self {
        bindings: [0; BINDING_BYTES],
        styles: [WidgetPartStyle::ZERO; MAX_PART_STYLES],
        references: [0; MAX_PART_STYLES],
    };

    #[inline]
    fn id(&self, node: usize) -> u8 {
        match MAX_PART_STYLES {
            0 => 0,
            1..=15 => (self.bindings[node / 2] >> ((node % 2) * 4)) & 15,
            _ => self.bindings[node],
        }
    }

    fn bind(&mut self, node: usize, id: u8) {
        if MAX_PART_STYLES == 0 {
            return;
        }
        if MAX_PART_STYLES > 15 {
            self.bindings[node] = id;
            return;
        }
        let shift = (node % 2) * 4;
        let packed = &mut self.bindings[node / 2];
        *packed = (*packed & !(15 << shift)) | (id << shift);
    }

    pub(crate) fn copy_from_draft(&mut self, draft: &Self, slots: &[u8]) {
        if MAX_PART_STYLES == 0 {
            return;
        }
        self.styles = draft.styles;
        self.references = draft.references;
        self.bindings.fill(0);
        for (i, &slot) in slots.iter().enumerate() {
            self.bind(usize::from(slot), draft.id(i));
        }
    }

    pub(crate) fn get(&self, node: usize) -> &WidgetPartStyle {
        self.id(node)
            .checked_sub(1)
            .map_or(&WidgetPartStyle::ZERO, |s| &self.styles[usize::from(s)])
    }

    pub(crate) fn merge(&mut self, node: usize, over: WidgetPartStyle) -> Result<(), BuildError> {
        if over.is_empty() {
            return Ok(());
        }
        let id = self.id(node);
        let old = id.checked_sub(1).map(usize::from);
        let merged = self.get(node).merge(over);
        let slot = (0..MAX_PART_STYLES)
            .find(|&s| self.references[s] > 0 && self.styles[s] == merged)
            .or_else(|| old.filter(|&s| self.references[s] == 1))
            .or_else(|| self.references.iter().position(|&n| n == 0))
            .ok_or(BuildError::PartStyles)?;
        if let Some(s) = old {
            self.references[s] -= 1;
        }
        self.styles[slot] = merged;
        self.references[slot] += 1;
        self.bind(node, slot as u8 + 1);
        Ok(())
    }
}

/// Translation as a fraction of the parent's content box and centred resizing of the
/// child's arranged box, in thousandths. Siblings keep their original layout slots.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct LayoutMotion {
    pub x: i16,
    pub y: i16,
    /// Change relative to normal size. Zero keeps the assigned size.
    pub grow: i16,
}
impl LayoutMotion {
    pub const IDENTITY: Self = Self {
        x: 0,
        y: 0,
        grow: 0,
    };
    pub(crate) fn arrange(
        self,
        rect: crate::geometry::Rect,
        parent: crate::geometry::Rect,
    ) -> crate::geometry::Rect {
        if self == Self::IDENTITY {
            return rect;
        }
        let scale = (1000 + i64::from(self.grow)).max(0);
        let axis = |origin, extent, space, offset| {
            let length = (i64::from(extent) * scale / 1000).min(i64::from(u32::MAX));
            let position = i64::from(origin)
                + (i64::from(extent) - length) / 2
                + i64::from(space) * i64::from(offset) / 1000;
            (
                position.clamp(i32::MIN.into(), i32::MAX.into()) as i32,
                length as u32,
            )
        };
        let (x, w) = axis(rect.x, rect.w, parent.w, self.x);
        let (y, h) = axis(rect.y, rect.h, parent.h, self.y);
        crate::geometry::Rect::new(x, y, w, h)
    }
}
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct LayoutExtra {
    pub offset_x: i16,
    pub offset_y: i16,
    pub motion: LayoutMotion,
    pub custom_rev: u32,
    /// Minimum width constraint in px (`0` = unconstrained).
    pub min_w: u16,
    /// Maximum width constraint in px (`0` = unconstrained).
    pub max_w: u16,
    /// Minimum height constraint in px (`0` = unconstrained).
    pub min_h: u16,
    /// Maximum height constraint in px (`0` = unconstrained).
    pub max_h: u16,
    /// Outer margin around the node's layout box.
    pub margin: crate::view::EdgeInsets,
}

impl LayoutExtra {
    /// Centre a capped control in an allocated track. Its parent can compress it further.
    pub(crate) fn fit_slot(self, rect: Rect) -> Rect {
        let w = if self.max_w > 0 {
            rect.w.min(self.max_w.into())
        } else {
            rect.w
        };
        let h = if self.max_h > 0 {
            rect.h.min(self.max_h.into())
        } else {
            rect.h
        };
        Rect::new(
            rect.x + ((rect.w - w) / 2) as i32,
            rect.y + ((rect.h - h) / 2) as i32,
            w,
            h,
        )
    }

    pub const ZERO: Self = Self {
        offset_x: 0,
        offset_y: 0,
        motion: LayoutMotion::IDENTITY,
        custom_rev: 0,
        min_w: 0,
        max_w: 0,
        min_h: 0,
        max_h: 0,
        margin: crate::view::EdgeInsets::ZERO,
    };

    #[inline]
    pub const fn has_bounds(&self) -> bool {
        self.min_w != 0 || self.max_w != 0 || self.min_h != 0 || self.max_h != 0
    }

    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.offset_x == 0
            && self.motion.x == 0
            && self.motion.y == 0
            && self.motion.grow == 0
            && self.offset_y == 0
            && self.custom_rev == 0
            && !self.has_bounds()
            && self.margin.top == 0
            && self.margin.right == 0
            && self.margin.bottom == 0
            && self.margin.left == 0
    }
}

// Keep the 16-bit geometry records contiguous. Only the revision needs 32 bits;
// callers work on an aligned copy, including on targets that forbid unaligned loads.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct PackedLayoutExtra {
    offset: (i16, i16),
    motion: LayoutMotion,
    revision: [u16; 2],
    widths: [u16; 2],
    heights: [u16; 2],
    margin: crate::view::EdgeInsets,
}
impl PackedLayoutExtra {
    const fn new(v: LayoutExtra) -> Self {
        Self {
            offset: (v.offset_x, v.offset_y),
            motion: v.motion,
            revision: [v.custom_rev as u16, (v.custom_rev >> 16) as u16],
            widths: [v.min_w, v.max_w],
            heights: [v.min_h, v.max_h],
            margin: v.margin,
        }
    }
    fn unpack(self) -> LayoutExtra {
        LayoutExtra {
            offset_x: self.offset.0,
            offset_y: self.offset.1,
            motion: self.motion,
            custom_rev: u32::from(self.revision[0]) | (u32::from(self.revision[1]) << 16),
            min_w: self.widths[0],
            max_w: self.widths[1],
            min_h: self.heights[0],
            max_h: self.heights[1],
            margin: self.margin,
        }
    }
}
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LayoutExtras {
    keys: [Key; MAX_LAYOUT_EXTRAS],
    items: [PackedLayoutExtra; MAX_LAYOUT_EXTRAS],
    len: [u8; (MAX_LAYOUT_EXTRAS != 0) as usize],
}

impl LayoutExtras {
    pub const EMPTY: Self = Self {
        keys: [Key(0); MAX_LAYOUT_EXTRAS],
        items: [PackedLayoutExtra::new(LayoutExtra::ZERO); MAX_LAYOUT_EXTRAS],
        len: [0; (MAX_LAYOUT_EXTRAS != 0) as usize],
    };

    #[inline]
    pub fn get(&self, key: Key) -> LayoutExtra {
        self.keys[..usize::from(self.len.first().copied().unwrap_or(0))]
            .iter()
            .position(|&k| k == key)
            .map_or(LayoutExtra::ZERO, |i| self.items[i].unpack())
    }

    fn upsert(&mut self, key: Key, f: impl FnOnce(&mut LayoutExtra)) -> Result<(), BuildError> {
        let used = usize::from(self.len.first().copied().unwrap_or(0));
        if let Some(i) = self.keys[..used].iter().position(|&k| k == key) {
            let mut value = self.items[i].unpack();
            f(&mut value);
            self.items[i] = PackedLayoutExtra::new(value);
            return Ok(());
        }
        let mut next = LayoutExtra::ZERO;
        f(&mut next);
        if next.is_zero() {
            return Ok(());
        }
        let entry = self.items.get_mut(used).ok_or(BuildError::LayoutExtras)?;
        self.keys[used] = key;
        *entry = PackedLayoutExtra::new(next);
        if let Some(n) = self.len.first_mut() {
            *n += 1;
        }
        Ok(())
    }

    pub fn set_offset(&mut self, key: Key, dx: i16, dy: i16) -> Result<(), BuildError> {
        self.upsert(key, |e| {
            e.offset_x = dx;
            e.offset_y = dy;
        })
    }

    pub fn set_motion(&mut self, key: Key, motion: LayoutMotion) -> Result<(), BuildError> {
        self.upsert(key, |e| e.motion = motion)
    }

    pub fn set_custom_rev(&mut self, key: Key, rev: u32) -> Result<(), BuildError> {
        self.upsert(key, |e| {
            e.custom_rev = rev;
        })
    }

    pub fn set_constraints(
        &mut self,
        key: Key,
        min_w: Option<u16>,
        max_w: Option<u16>,
        min_h: Option<u16>,
        max_h: Option<u16>,
    ) -> Result<(), BuildError> {
        if min_w.is_none() && max_w.is_none() && min_h.is_none() && max_h.is_none() {
            return Ok(());
        }
        self.upsert(key, |e| {
            if let Some(v) = min_w {
                e.min_w = v;
            }
            if let Some(v) = max_w {
                e.max_w = v;
            }
            if let Some(v) = min_h {
                e.min_h = v;
            }
            if let Some(v) = max_h {
                e.max_h = v;
            }
        })
    }

    pub fn set_margin(
        &mut self,
        key: Key,
        margin: crate::view::EdgeInsets,
    ) -> Result<(), BuildError> {
        self.upsert(key, |e| {
            e.margin = margin;
        })
    }
}
