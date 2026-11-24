use crate::capacity::BuildError;
use crate::geometry::Rgb565;
use crate::view::Key;

/// Maximum per-node part style overrides per scene graph.
pub const MAX_PART_STYLES: usize = 12;

/// Maximum per-node layout offsets / custom revisions per scene graph.
pub const MAX_LAYOUT_EXTRAS: usize = 12;

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

/// Sparse per-part and per-state style overrides stored in static scene tables.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct WidgetPartStyle {
    pub present: u32,
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
}

const _: () = assert!(core::mem::size_of::<WidgetPartStyle>() <= 44);

impl WidgetPartStyle {
    pub const ZERO: Self = Self {
        present: 0,
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
    };

    #[inline]
    pub const fn new() -> Self {
        Self::ZERO
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.present == 0
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
    items: [(Key, WidgetPartStyle); MAX_PART_STYLES],
    len: u8,
}

impl PartStyles {
    pub const EMPTY: Self = Self {
        items: [(Key(0), WidgetPartStyle::ZERO); MAX_PART_STYLES],
        len: 0,
    };

    #[inline]
    pub fn get(&self, key: Key) -> WidgetPartStyle {
        self.get_ref(key).copied().unwrap_or(WidgetPartStyle::ZERO)
    }

    #[inline]
    pub fn get_ref(&self, key: Key) -> Option<&WidgetPartStyle> {
        for item in &self.items[..usize::from(self.len)] {
            if item.0 == key {
                return Some(&item.1);
            }
        }
        None
    }

    /// Compare per-node part overrides by reference without copying `WidgetPartStyle` onto caller frames.
    #[inline(never)]
    pub fn diff_key(old: &Self, new: &Self, key: Key) -> (bool, bool) {
        match (old.get_ref(key), new.get_ref(key)) {
            (None, None) => (false, false),
            (Some(o), Some(n)) => (o != n, o.affects_button_measure(n)),
            (Some(o), None) => (
                !o.is_empty(),
                o.affects_button_measure(&WidgetPartStyle::ZERO),
            ),
            (None, Some(n)) => (
                !n.is_empty(),
                WidgetPartStyle::ZERO.affects_button_measure(n),
            ),
        }
    }

    pub fn set(&mut self, key: Key, style: WidgetPartStyle) -> Result<(), BuildError> {
        if style.is_empty() {
            return Ok(());
        }
        let used = usize::from(self.len);
        for item in &mut self.items[..used] {
            if item.0 == key {
                item.1 = item.1.merge(style);
                return Ok(());
            }
        }
        let entry = self.items.get_mut(used).ok_or(BuildError::Effects)?;
        *entry = (key, style);
        self.len += 1;
        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct LayoutExtra {
    pub offset_x: i16,
    pub offset_y: i16,
    pub custom_rev: u32,
}

impl LayoutExtra {
    pub const ZERO: Self = Self {
        offset_x: 0,
        offset_y: 0,
        custom_rev: 0,
    };
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LayoutExtras {
    items: [(Key, LayoutExtra); MAX_LAYOUT_EXTRAS],
    len: u8,
}

impl LayoutExtras {
    pub const EMPTY: Self = Self {
        items: [(Key(0), LayoutExtra::ZERO); MAX_LAYOUT_EXTRAS],
        len: 0,
    };

    #[inline]
    pub fn get(&self, key: Key) -> LayoutExtra {
        for item in &self.items[..usize::from(self.len)] {
            if item.0 == key {
                return item.1;
            }
        }
        LayoutExtra::ZERO
    }

    pub fn set_offset(&mut self, key: Key, dx: i16, dy: i16) -> Result<(), BuildError> {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        let used = usize::from(self.len);
        for item in &mut self.items[..used] {
            if item.0 == key {
                item.1.offset_x = dx;
                item.1.offset_y = dy;
                return Ok(());
            }
        }
        let entry = self.items.get_mut(used).ok_or(BuildError::Effects)?;
        *entry = (
            key,
            LayoutExtra {
                offset_x: dx,
                offset_y: dy,
                custom_rev: 0,
            },
        );
        self.len += 1;
        Ok(())
    }

    pub fn set_custom_rev(&mut self, key: Key, rev: u32) -> Result<(), BuildError> {
        if rev == 0 {
            return Ok(());
        }
        let used = usize::from(self.len);
        for item in &mut self.items[..used] {
            if item.0 == key {
                item.1.custom_rev = rev;
                return Ok(());
            }
        }
        let entry = self.items.get_mut(used).ok_or(BuildError::Effects)?;
        *entry = (
            key,
            LayoutExtra {
                offset_x: 0,
                offset_y: 0,
                custom_rev: rev,
            },
        );
        self.len += 1;
        Ok(())
    }
}
