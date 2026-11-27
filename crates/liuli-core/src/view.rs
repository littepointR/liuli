use core::fmt::{self, Write};

use crate::capacity::{
    BuildError, MAX_DEPTH, MAX_NODES, MAX_SPARKLINE_SAMPLES, MAX_SPARKLINES, MAX_TEXT_BYTES,
};

pub const BUTTON_ICON_SIZE: u32 = 14;
pub const BUTTON_ICON_GAP: u32 = 4;
pub const BUTTON_ICON_SPAN: u32 = BUTTON_ICON_SIZE + BUTTON_ICON_GAP;
use crate::geometry::{Rect, Rgb565, Size};
use crate::input::{PointerCaptureMode, PointerEvent, UiAction};
use crate::paint::{PaintOverride, PaintState};
use crate::time::{Easing, InstantMs};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Change {
    Unchanged,
    View,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Key(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct PageRestore {
    pub scroll_key: Option<Key>,
    pub scroll_y: i32,
    pub focus_key: Option<Key>,
}

pub trait App {
    type Model;
    type Msg;

    fn update(model: &mut Self::Model, msg: Self::Msg, now: InstantMs) -> Change;
    fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError>;
    fn on_action(model: &Self::Model, event: UiAction) -> Option<Self::Msg>;

    /// Visit model-owned animations, including inactive ones. The engine advances them,
    /// coalesces wakeups and cancels a binding when its node leaves the committed tree.
    fn animations(
        _model: &mut Self::Model,
        _bind: &mut dyn FnMut(Key, &mut dyn crate::animation::Animation),
    ) {
    }

    fn accepts_text(_model: &Self::Model, _key: Key) -> bool {
        false
    }
    fn on_text(
        _model: &Self::Model,
        _key: Key,
        _input: crate::input::TextInput<'_>,
    ) -> Option<Self::Msg> {
        None
    }
    /// Apply borrowed text directly to model storage or dispatch the application's text message.
    fn update_text(
        model: &mut Self::Model,
        key: Key,
        input: crate::input::TextInput<'_>,
        now: InstantMs,
    ) -> Option<Change> {
        Self::on_text(model, key, input).map(|msg| Self::update(model, msg, now))
    }

    /// Intrinsic content size of a custom leaf, excluding padding.
    fn measure_custom(
        _key: Key,
        _font: FontId,
        _available: crate::geometry::Size,
    ) -> crate::geometry::Size {
        crate::geometry::Size::zero()
    }

    /// Style-aware intrinsic measurement. The same resolved dimensions reach the painter.
    fn measure_custom_styled(
        key: Key,
        font: FontId,
        available: crate::geometry::Size,
        _metrics: crate::part_style::PartMetrics,
    ) -> crate::geometry::Size {
        Self::measure_custom(key, font, available)
    }

    /// Emit bounded paint commands for a custom leaf. All paint dependencies
    /// must be represented by its retained properties so damage stays correct.
    fn paint_custom(
        _key: Key,
        _props: &StoredProps,
        _canvas: &mut crate::paint::WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        Ok(())
    }

    /// Paint immutable resources held by the current model. Update the custom node's theme_id
    /// (or another retained visual property) whenever its resource changes.
    fn paint_model(
        _model: &Self::Model,
        key: Key,
        props: &StoredProps,
        canvas: &mut crate::paint::WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        Self::paint_custom(key, props, canvas)
    }

    /// Override or augment rendering of a built-in widget when `paint_mode == PaintMode::Override`.
    fn paint_override(
        _model: &Self::Model,
        _key: Key,
        _kind: NodeKind,
        _props: &StoredProps,
        _state: PaintState,
        _canvas: &mut crate::paint::WidgetCanvas<'_>,
    ) -> Result<PaintOverride, BuildError> {
        Ok(PaintOverride::Default)
    }

    /// Receive widget-local pointer down/move/up/cancel coordinates for nodes with `pointer_capture`.
    fn on_pointer(_model: &Self::Model, _key: Key, _event: PointerEvent) -> Option<Self::Msg> {
        None
    }

    /// Compute custom child rectangles and container size for `NodeKind::CustomContainer`.
    fn layout_custom(
        _key: Key,
        _child_sizes: &[Size],
        _available: Size,
        _out_rects: &mut [Rect],
    ) -> Size {
        Size::zero()
    }
    /// Opt this node into two-contact gesture arbitration.
    fn accepts_transform(_model: &Self::Model, _key: Key) -> bool {
        false
    }
    fn on_transform(
        _model: &Self::Model,
        _key: Key,
        _gesture: crate::input::TransformGesture,
    ) -> Option<Self::Msg> {
        None
    }

    /// Update virtual content when dragging, focus navigation or scroll animation changes its offset.
    fn on_scroll(_model: &Self::Model, _key: Key, _offset: i32) -> Option<Self::Msg> {
        None
    }

    /// Optionally bind a controller for programmatic commands. The engine supplies measured
    /// bounds, drag samples, release, animation ticks and wakeups automatically.
    fn scroll_controller(
        _model: &mut Self::Model,
        _key: Key,
    ) -> Option<&mut crate::time::ScrollPhysics> {
        None
    }

    fn next_deadline(_model: &Self::Model, _now: InstantMs) -> Option<InstantMs> {
        None
    }

    fn on_timer(_model: &mut Self::Model, _now: InstantMs) -> Change {
        Change::Unchanged
    }

    fn page_id(_model: &Self::Model) -> u8 {
        0
    }

    fn is_tab_key(_key: Key) -> bool {
        false
    }

    fn page_restore(_model: &Self::Model, _page: u8) -> PageRestore {
        PageRestore::default()
    }

    fn on_page_committed(
        _model: &mut Self::Model,
        _old_page: u8,
        _saved_focus: Option<Key>,
        _saved_scroll_key: Option<Key>,
        _saved_scroll_y: i32,
    ) {
    }
}

/// 1-based slot link so `0` represents an empty link and types using it remain all-zero in `.bss`.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct SlotLink(u8);
const _: () = assert!(MAX_NODES <= u8::MAX as usize);

impl SlotLink {
    pub const NONE: Self = Self(0);

    #[inline]
    pub const fn from_slot(slot: usize) -> Option<Self> {
        if slot < MAX_NODES {
            Some(Self((slot + 1) as u8))
        } else {
            None
        }
    }

    #[inline]
    pub const fn from_slot_u16(slot: u16) -> Option<Self> {
        if (slot as usize) < MAX_NODES {
            Some(Self((slot + 1) as u8))
        } else {
            None
        }
    }

    #[inline]
    pub const fn get(self) -> Option<usize> {
        if self.0 == 0 {
            None
        } else {
            let idx = (self.0 - 1) as usize;
            if idx < MAX_NODES { Some(idx) } else { None }
        }
    }

    #[inline]
    pub const fn get_u16(self) -> Option<u16> {
        if self.0 == 0 {
            None
        } else {
            let idx = self.0 - 1;
            if (idx as usize) < MAX_NODES {
                Some(idx as u16)
            } else {
                None
            }
        }
    }

    #[inline]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_some(self) -> bool {
        self.0 != 0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct TextRange {
    pub start: u16,
    pub len: u16,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TextRef {
    Dynamic(TextRange),
    Static(&'static str),
}

impl Default for TextRef {
    fn default() -> Self {
        Self::Static("")
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum Length {
    #[default]
    Content = 0,
    Px(u32) = 1,
    Fill(u16) = 2,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum Align {
    #[default]
    Start = 0,
    Center = 1,
    End = 2,
    Stretch = 3,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct EdgeInsets {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl EdgeInsets {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    #[inline]
    pub const fn all(v: u16) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    #[inline]
    pub const fn symmetric(horizontal: u16, vertical: u16) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    #[inline]
    pub const fn horizontal(self) -> u32 {
        self.left as u32 + self.right as u32
    }

    #[inline]
    pub const fn vertical(self) -> u32 {
        self.top as u32 + self.bottom as u32
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum FontId {
    #[default]
    Body16 = 0,
    Small12 = 1,
    Title20 = 2,
    Display28 = 3,
}

impl FontId {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0b11 {
            1 => Self::Small12,
            2 => Self::Title20,
            3 => Self::Display28,
            _ => Self::Body16,
        }
    }
}

/// Line-wrapping strategy for `PaintCmd::Text` and `TextMeasurer`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum WrapMode {
    /// Greedy per-character wrapping (default; matches baseline behavior).
    #[default]
    Char = 0,
    /// Word-boundary wrapping for ASCII and CJK kinsoku punctuation rules.
    Word = 1,
}

/// Perceptual sRGB gamma compensation for alpha-blended glyph coverage.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum TextGamma {
    /// Linear coverage alpha (default; preserves baseline goldens).
    #[default]
    Linear = 0,
    /// Contrast-aware gamma compensation for crisp light/dark glyph weight.
    Perceptual = 1,
}

/// Physical RGB LCD subpixel stripe order for horizontal subpixel AA.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum SubpixelOrder {
    /// Grayscale coverage AA (best for rotated/AMOLED Pentile panels; default).
    #[default]
    None = 0,
    /// Horizontal Red-Green-Blue stripe order (e.g. standard RGB LCDs).
    Rgb = 1,
    /// Horizontal Blue-Green-Red stripe order.
    Bgr = 2,
}

/// Packed 6-bit text rendering and decoration flags (`u8`, bottom 6 bits).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct TextStyleFlags(pub u8);

impl TextStyleFlags {
    pub const DEFAULT: Self = Self(0);

    #[inline]
    pub const fn wrap(self) -> WrapMode {
        if (self.0 & 0x01) != 0 {
            WrapMode::Word
        } else {
            WrapMode::Char
        }
    }

    #[inline]
    pub const fn underline(self) -> bool {
        (self.0 & 0x02) != 0
    }

    #[inline]
    pub const fn strikethrough(self) -> bool {
        (self.0 & 0x04) != 0
    }

    #[inline]
    pub const fn gamma(self) -> TextGamma {
        if (self.0 & 0x08) != 0 {
            TextGamma::Perceptual
        } else {
            TextGamma::Linear
        }
    }

    #[inline]
    pub const fn subpixel(self) -> SubpixelOrder {
        match (self.0 >> 4) & 0x03 {
            1 => SubpixelOrder::Rgb,
            2 => SubpixelOrder::Bgr,
            _ => SubpixelOrder::None,
        }
    }

    #[inline]
    pub const fn with_wrap(mut self, wrap: WrapMode) -> Self {
        self.0 = (self.0 & !0x01) | ((wrap as u8) & 0x01);
        self
    }

    #[inline]
    pub const fn with_underline(mut self, enabled: bool) -> Self {
        self.0 = (self.0 & !0x02) | ((enabled as u8) << 1);
        self
    }

    #[inline]
    pub const fn with_strikethrough(mut self, enabled: bool) -> Self {
        self.0 = (self.0 & !0x04) | ((enabled as u8) << 2);
        self
    }

    #[inline]
    pub const fn with_gamma(mut self, gamma: TextGamma) -> Self {
        self.0 = (self.0 & !0x08) | (((gamma as u8) & 0x01) << 3);
        self
    }

    #[inline]
    pub const fn with_subpixel(mut self, order: SubpixelOrder) -> Self {
        self.0 = (self.0 & !0x30) | (((order as u8) & 0x03) << 4);
        self
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum TextAlign {
    #[default]
    Start = 0,
    Center = 1,
    End = 2,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum IconKind {
    #[default]
    None = 0,
    ChevronRight = 1,
    Check = 2,
    Warning = 3,
    Sliders = 4,
    Bell = 5,
    Moon = 6,
    Thermometer = 7,
    Reset = 8,
    Gauge = 9,
    Waveform = 10,
    Diagnostics = 11,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum StackAlign {
    #[default]
    TopStart = 0,
    TopCenter = 1,
    TopEnd = 2,
    CenterStart = 3,
    Center = 4,
    CenterEnd = 5,
    BottomStart = 6,
    BottomCenter = 7,
    BottomEnd = 8,
}

impl StackAlign {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0x0F {
            1 => Self::TopCenter,
            2 => Self::TopEnd,
            3 => Self::CenterStart,
            4 => Self::Center,
            5 => Self::CenterEnd,
            6 => Self::BottomStart,
            7 => Self::BottomCenter,
            8 => Self::BottomEnd,
            _ => Self::TopStart,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum PaintMode {
    #[default]
    Default = 0,
    Override = 1,
}

impl PaintMode {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0b11 {
            1 => Self::Override,
            _ => Self::Default,
        }
    }
}

/// Main-axis child distribution inside `Row` / `Column` containers when no `Length::Fill` children consume remaining space.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum Justify {
    #[default]
    Start = 0,
    Center = 1,
    End = 2,
    SpaceBetween = 3,
    SpaceAround = 4,
    SpaceEvenly = 5,
}

impl Justify {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0b111 {
            1 => Self::Center,
            2 => Self::End,
            3 => Self::SpaceBetween,
            4 => Self::SpaceAround,
            5 => Self::SpaceEvenly,
            _ => Self::Start,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum NodeKind {
    #[default]
    Label = 0,
    Root = 1,
    Row = 2,
    Column = 3,
    Stack = 4,
    ScrollY = 5,
    Button = 6,
    Toggle = 7,
    Slider = 8,
    Modal = 9,
    ProgressBar = 10,
    Sparkline = 11,
    Custom = 12,
    Wrap = 13,
    CustomContainer = 14,
    ScrollX = 15,
    Grid = 16,
    TabBar = 17,
    Tab = 18,
    TabPanel = 19,
}

impl NodeKind {
    pub const fn scrolls_x(self) -> bool {
        matches!(self, Self::ScrollX | Self::TabBar)
    }

    pub const fn is_scroll_view(self) -> bool {
        self.scrolls_x() || matches!(self, Self::ScrollY)
    }

    pub(crate) const fn layout_kind(self) -> Self {
        match self {
            Self::TabPanel => Self::Column,
            other => other,
        }
    }

    pub const fn is_container(self) -> bool {
        matches!(
            self,
            Self::Root
                | Self::Row
                | Self::Column
                | Self::Stack
                | Self::ScrollY
                | Self::ScrollX
                | Self::Modal
                | Self::Wrap
                | Self::CustomContainer
                | Self::Grid
                | Self::TabBar
                | Self::TabPanel
        )
    }
}

/// Overflow policy for one semantic tab bar. Track widths are measured using the active font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabLayout {
    #[default]
    Scroll,
    Wrap {
        max_rows: u8,
    },
    Columns(u8),
}

const SCROLL_OFFSET_PRESENT: i16 = i16::MIN;

fn pack_payload(bits: u32) -> (i16, i16) {
    let [a, b, c, d] = bits.to_le_bytes();
    (i16::from_le_bytes([a, b]), i16::from_le_bytes([c, d]))
}
fn unpack_payload(lo: i16, hi: i16) -> u32 {
    let [a, b] = lo.to_le_bytes();
    let [c, d] = hi.to_le_bytes();
    u32::from_le_bytes([a, b, c, d])
}

/// Visual press feedback applied to a button or container background when `pressed`.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum PressFeedback {
    /// Legacy 3-step RGB565 darken (preserved for baseline goldens).
    #[default]
    Default,
    /// Disable pressed fill color change.
    None,
    /// Blend towards black by `amount` out of 15.
    Darken(u8),
    /// Blend towards white by `amount` out of 15.
    Lighten(u8),
    /// Blend towards `accent` by `amount` out of 15.
    AccentTint(u8),
}

impl PressFeedback {
    #[inline]
    pub const fn to_raw(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::None => 1,
            Self::Darken(a) => (1 << 4) | (if a > 15 { 15 } else { a }),
            Self::Lighten(a) => (2 << 4) | (if a > 15 { 15 } else { a }),
            Self::AccentTint(a) => (3 << 4) | (if a > 15 { 15 } else { a }),
        }
    }

    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        let mode = raw >> 4;
        let amount = raw & 0x0F;
        match mode {
            0 => {
                if raw == 1 {
                    Self::None
                } else {
                    Self::Default
                }
            }
            1 => Self::Darken(amount),
            2 => Self::Lighten(amount),
            3 => Self::AccentTint(amount),
            _ => Self::Default,
        }
    }
}

/// A focus outline. By default a 2 px stroke follows the widget's corners.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FocusRingStyle {
    pub enabled: bool,
    pub width: u8,
    pub offset: u8,
    /// Follow the widget's radius. Set to false to force square corners.
    pub rounded: bool,
}

impl FocusRingStyle {
    pub const DEFAULT: Self = Self {
        enabled: true,
        width: 2,
        offset: 2,
        rounded: true,
    };
    pub const OFF: Self = Self {
        enabled: false,
        width: 2,
        offset: 2,
        rounded: false,
    };

    #[inline]
    pub const fn to_raw(self) -> u8 {
        if self.enabled && self.width == 2 && self.offset == 2 && self.rounded {
            return 0;
        }
        let w = if self.width < 1 {
            1
        } else if self.width > 3 {
            3
        } else {
            self.width
        };
        let o = if self.offset > 3 { 3 } else { self.offset };
        0x80 | (self.enabled as u8) | (w << 1) | (o << 3) | ((self.rounded as u8) << 5)
    }

    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        if raw == 0 {
            return Self::DEFAULT;
        }
        Self {
            enabled: (raw & 0x01) != 0,
            width: (raw >> 1) & 0x03,
            offset: (raw >> 3) & 0x03,
            rounded: (raw & 0x20) != 0,
        }
    }

    /// Outer stroke bounds and radius before clipping to the viewport.
    pub fn outline_geometry(self, rect: Rect, clip: Rect, radius: u8) -> (Rect, u8) {
        let offset = u32::from(self.offset.min(3));
        if !self.rounded || radius == 0 {
            return (rect.union(rect.expand(offset).intersect(clip)), 0);
        }
        // All four arcs stay concentric with the button, including at viewport edges.
        let clearance = (rect.left64() - clip.left64())
            .min(rect.top64() - clip.top64())
            .min(clip.right64() - rect.right64())
            .min(clip.bottom64() - rect.bottom64());
        let expansion = clearance.clamp(0, i64::from(offset)) as u32;
        let body_radius = u32::from(radius).min(rect.w / 2).min(rect.h / 2);
        (
            rect.expand(expansion),
            (body_radius + expansion).min(255) as u8,
        )
    }
}

impl Default for FocusRingStyle {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// State that supplies a normalized 0..=1000 target to a widget painter.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum MotionState {
    #[default]
    None,
    Pressed,
    Checked,
    Focused,
    Value,
}

/// A paint-only transition. A zero duration still publishes the exact target.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct StateMotion {
    pub state: MotionState,
    pub duration_ms: u16,
    pub easing: Easing,
    pub curve: Option<&'static crate::animation::StyleCurve>,
}

impl StateMotion {
    pub const fn new(state: MotionState, duration_ms: u16, easing: Easing) -> Self {
        Self {
            state,
            duration_ms,
            easing,
            curve: None,
        }
    }
    pub const fn with_curve(mut self, curve: &'static crate::animation::StyleCurve) -> Self {
        self.curve = Some(curve);
        self
    }
}

/// Three low bits select the state. The remaining bits index a tree-local palette of custom curves.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct MotionBinding(u8);
impl MotionBinding {
    pub(crate) const EMPTY: Self = Self(0);
    fn new(state: MotionState, slot: u8) -> Self {
        Self(state as u8 | (slot << 3))
    }
    pub(crate) fn state(self) -> MotionState {
        match self.0 & 7 {
            1 => MotionState::Pressed,
            2 => MotionState::Checked,
            3 => MotionState::Focused,
            4 => MotionState::Value,
            _ => MotionState::None,
        }
    }
    pub(crate) fn curve(
        self,
        curves: &CurvePalette,
    ) -> Option<&'static crate::animation::StyleCurve> {
        (self.0 >> 3)
            .checked_sub(1)
            .and_then(|i| curves[i as usize])
    }
}
pub(crate) type CurvePalette =
    [Option<&'static crate::animation::StyleCurve>; crate::capacity::MAX_ANIMATIONS];
const _: () = assert!(crate::capacity::MAX_ANIMATIONS < 32);

/// Interaction policy for a horizontal or vertical viewport, including a scrolling tab bar.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ScrollPolicy {
    pub inertia: bool,
    pub snap_step: u16,
}
impl ScrollPolicy {
    pub const FREE: Self = Self {
        inertia: true,
        snap_step: 0,
    };
    pub const DIRECT: Self = Self {
        inertia: false,
        snap_step: 0,
    };
    pub const fn snap(step: u16) -> Self {
        Self {
            snap_step: step,
            ..Self::FREE
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct NodeProps {
    pub scroll: Option<ScrollPolicy>,
    pub style_source: crate::style_source::StyleSource,
    /// Declarative part defaults and local overrides, interned by the scene builder.
    pub parts: crate::part_style::WidgetPartStyle,
    pub width: Length,
    pub height: Length,
    pub padding: EdgeInsets,
    pub gap: u16,
    pub align_cross: Align,
    pub bg: Option<Rgb565>,
    pub border_color: Option<Rgb565>,
    pub border_width: u8,
    pub radius: u8,
    pub fg: Rgb565,
    pub accent: Rgb565,
    pub focus_color: Rgb565,
    pub focus_ring: FocusRingStyle,
    pub press_feedback: PressFeedback,
    pub overlay_color: Rgb565,
    pub overlay_alpha: u8,
    pub text: TextRef,
    pub font: FontId,
    pub text_align: TextAlign,
    pub max_lines: u8,
    pub ellipsis: bool,
    pub wrap: WrapMode,
    pub text_gamma: TextGamma,
    pub subpixel: SubpixelOrder,
    pub underline: bool,
    pub strikethrough: bool,
    pub letter_spacing: i8,
    pub line_spacing: i8,
    pub value: i16,
    pub min: i16,
    pub max: i16,
    pub step: i16,
    pub checked: bool,
    pub focusable: bool,
    pub enabled: bool,
    pub visible: bool,
    pub clickable: bool,
    pub icon: IconKind,
    pub theme_id: u16,
    pub series: u8,
    pub anim_easing: Easing,
    pub motion_state: MotionState,
    pub motion_ms: u16,
    pub motion_curve: Option<&'static crate::animation::StyleCurve>,
    pub repeatable: bool,
    pub swipeable: bool,
    pub long_press: bool,
    pub pointer_capture: PointerCaptureMode,
    pub editable: bool,
    pub paint_mode: PaintMode,
    pub stack_align: StackAlign,
    pub justify: Justify,
    pub grid_cols: u8,
}

impl Default for NodeProps {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeProps {
    #[cfg_attr(feature = "inspector", track_caller)]
    pub const fn new() -> Self {
        Self {
            scroll: None,
            style_source: crate::style_source::StyleSource::named("NodeProps").declared(),
            parts: crate::part_style::WidgetPartStyle::ZERO,
            width: Length::Content,
            height: Length::Content,
            padding: EdgeInsets::ZERO,
            gap: 0,
            align_cross: Align::Start,
            bg: None,
            border_color: None,
            border_width: 0,
            radius: 0,
            fg: Rgb565::new(31, 63, 31),
            accent: Rgb565::new(4, 34, 29),
            focus_color: Rgb565::new(31, 48, 0),
            focus_ring: FocusRingStyle::DEFAULT,
            press_feedback: PressFeedback::Default,
            overlay_color: Rgb565::new(0, 0, 0),
            overlay_alpha: 0,
            text: TextRef::Static(""),
            font: FontId::Body16,
            text_align: TextAlign::Start,
            max_lines: 0,
            ellipsis: true,
            wrap: WrapMode::Char,
            text_gamma: TextGamma::Linear,
            subpixel: SubpixelOrder::None,
            underline: false,
            strikethrough: false,
            letter_spacing: 0,
            line_spacing: 0,
            value: 0,
            min: 0,
            max: 100,
            step: 1,
            checked: false,
            focusable: false,
            enabled: true,
            visible: true,
            clickable: false,
            icon: IconKind::None,
            theme_id: 0,
            series: 0,
            anim_easing: Easing::Linear,
            motion_state: MotionState::None,
            motion_ms: 0,
            motion_curve: None,
            repeatable: false,
            swipeable: false,
            long_press: false,
            pointer_capture: PointerCaptureMode::None,
            editable: false,
            paint_mode: PaintMode::Default,
            stack_align: StackAlign::TopStart,
            justify: Justify::Start,
            grid_cols: 0,
        }
    }

    /// Merge a local override over the declaration's theme defaults.
    pub const fn with_parts(mut self, style: crate::part_style::WidgetPartStyle) -> Self {
        self.parts = self.parts.merge(style);
        self
    }

    #[inline]
    /// Initial offset for a scroll viewport when its keyed node is mounted.
    /// Subsequent commits preserve live input unless a scroll controller is bound.
    pub fn initial_scroll_offset(mut self, offset: i32) -> Self {
        self.step = SCROLL_OFFSET_PRESENT;
        (self.min, self.max) = pack_payload(offset.max(0) as u32);
        self
    }

    pub const fn scroll_policy(mut self, policy: ScrollPolicy) -> Self {
        self.scroll = Some(policy);
        self
    }

    pub const fn state_motion(mut self, motion: StateMotion) -> Self {
        self.motion_state = motion.state;
        self.motion_ms = motion.duration_ms;
        self.motion_curve = motion.curve;
        self.anim_easing = motion.easing;
        self
    }

    pub const fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    #[inline]
    pub const fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }

    #[inline]
    pub const fn size(mut self, width: Length, height: Length) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    #[inline]
    pub const fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    #[inline]
    pub const fn gap(mut self, gap: u16) -> Self {
        self.gap = gap;
        self
    }

    #[inline]
    pub const fn align_cross(mut self, align: Align) -> Self {
        self.align_cross = align;
        self
    }

    #[inline]
    pub const fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    #[inline]
    pub const fn grid_cols(mut self, cols: u8) -> Self {
        self.grid_cols = cols;
        self
    }

    #[inline]
    pub const fn bg(mut self, bg: Rgb565) -> Self {
        self.bg = Some(bg);
        self
    }

    #[inline]
    pub const fn fg(mut self, fg: Rgb565) -> Self {
        self.fg = fg;
        self
    }

    #[inline]
    pub const fn accent(mut self, accent: Rgb565) -> Self {
        self.accent = accent;
        self
    }

    #[inline]
    pub const fn border(mut self, color: Rgb565, width: u8) -> Self {
        self.border_color = Some(color);
        self.border_width = width;
        self
    }

    #[inline]
    pub const fn radius(mut self, radius: u8) -> Self {
        self.radius = radius;
        self
    }

    #[inline]
    pub const fn text(mut self, text: TextRef) -> Self {
        self.text = text;
        self
    }

    #[inline]
    pub const fn font(mut self, font: FontId) -> Self {
        self.font = font;
        self
    }

    #[inline]
    pub const fn text_align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    #[inline]
    pub const fn max_lines(mut self, max_lines: u8) -> Self {
        self.max_lines = max_lines;
        self
    }

    #[inline]
    pub const fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    #[inline]
    pub const fn clickable(mut self, clickable: bool) -> Self {
        self.clickable = clickable;
        self
    }

    #[inline]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[inline]
    pub const fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    #[inline]
    pub const fn range(mut self, min: i16, max: i16, step: i16) -> Self {
        self.min = min;
        self.max = max;
        self.step = step;
        self
    }

    #[inline]
    pub const fn value(mut self, value: i16) -> Self {
        self.value = value;
        self
    }

    #[inline]
    pub const fn icon(mut self, icon: IconKind) -> Self {
        self.icon = icon;
        self
    }

    #[inline]
    pub const fn stack_align(mut self, align: StackAlign) -> Self {
        self.stack_align = align;
        self
    }

    #[inline]
    pub const fn paint_mode(mut self, mode: PaintMode) -> Self {
        self.paint_mode = mode;
        self
    }

    #[inline]
    pub const fn pointer_capture(mut self, mode: PointerCaptureMode) -> Self {
        self.pointer_capture = mode;
        self
    }

    #[inline]
    pub const fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }
}

const EXT_POINTER_CAPTURE_MASK: u16 = 0b11;
const EXT_FLAG_EDITABLE: u16 = 1 << 2;
const EXT_PAINT_MODE_SHIFT: u16 = 3;
const EXT_PAINT_MODE_MASK: u16 = 0b11 << EXT_PAINT_MODE_SHIFT;
const EXT_STACK_ALIGN_SHIFT: u16 = 5;
const EXT_STACK_ALIGN_MASK: u16 = 0x0F << EXT_STACK_ALIGN_SHIFT;
const EXT_JUSTIFY_SHIFT: u16 = 9;
const EXT_JUSTIFY_MASK: u16 = 0b111 << EXT_JUSTIFY_SHIFT;

const FLAG_HAS_BG: u16 = 1 << 0;
const FLAG_HAS_BORDER: u16 = 1 << 1;
const FLAG_ELLIPSIS: u16 = 1 << 2;
const FLAG_CHECKED: u16 = 1 << 3;
const FLAG_FOCUSABLE: u16 = 1 << 4;
const FLAG_ENABLED: u16 = 1 << 5;
const FLAG_VISIBLE: u16 = 1 << 6;
const FLAG_CLICKABLE: u16 = 1 << 7;
const FLAG_TEXT_DYN: u16 = 1 << 8;
const FLAG_STATE_MOTION: u16 = 1 << 9;
const TEXT_ALIGN_SHIFT: u16 = 10;
const TEXT_ALIGN_MASK: u16 = 0b11 << TEXT_ALIGN_SHIFT;
const FLAG_EASE_OUT_CUBIC: u16 = 1 << 12;
const FLAG_SWIPE: u16 = 1 << 14;
const FLAG_LONG_PRESS: u16 = 1 << 15;
const FLAG_REPEATABLE: u16 = 1 << 13;

/// Zero-initializable node property representation stored in static pools (`Storage`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct StoredProps {
    pub width: Length,
    pub height: Length,
    pub padding: EdgeInsets,
    pub gap: u16,
    pub align_cross: Align,
    pub border_width: u8,
    pub radius: u8,
    pub focus_ring_raw: u8,
    pub press_feedback_raw: u8,
    pub bg_raw: Rgb565,
    pub border_raw: Rgb565,
    pub fg: Rgb565,
    pub accent: Rgb565,
    pub focus_color: Rgb565,
    pub overlay_color: Rgb565,
    pub overlay_alpha: u8,
    pub text_style_raw: u8,
    pub letter_spacing: i8,
    pub line_spacing: i8,
    pub dyn_text: TextRange,
    pub static_text: Option<&'static str>,
    pub max_lines: u8,
    pub value: i16,
    pub min: i16,
    pub max: i16,
    pub step: i16,
    pub icon: IconKind,
    pub series: u8,
    pub theme_id: u16,
    pub flags: u16,
    pub ext_flags: u16,
}

impl StoredProps {
    /// Viewports reuse the value and line-count payloads, which only leaves consume.
    pub fn scroll_policy(&self) -> Option<ScrollPolicy> {
        (self.max_lines != 0).then_some(ScrollPolicy {
            inertia: self.max_lines == 2,
            snap_step: self.value as u16,
        })
    }

    fn for_kind(kind: NodeKind, props: NodeProps) -> Self {
        let mut stored = Self::from_props(props);
        if kind.is_scroll_view() {
            stored.max_lines = props.scroll.map_or(0, |p| if p.inertia { 2 } else { 1 });
            stored.value = props.scroll.map_or(0, |p| p.snap_step as i16);
        }
        stored
    }
    pub const ZERO: Self = Self {
        width: Length::Content,
        height: Length::Content,
        padding: EdgeInsets::ZERO,
        gap: 0,
        align_cross: Align::Start,
        border_width: 0,
        radius: 0,
        focus_ring_raw: 0,
        press_feedback_raw: 0,
        bg_raw: Rgb565::new(0, 0, 0),
        border_raw: Rgb565::new(0, 0, 0),
        fg: Rgb565::new(0, 0, 0),
        accent: Rgb565::new(0, 0, 0),
        focus_color: Rgb565::new(0, 0, 0),
        overlay_color: Rgb565::new(0, 0, 0),
        overlay_alpha: 0,
        text_style_raw: 0,
        letter_spacing: 0,
        line_spacing: 0,
        dyn_text: TextRange { start: 0, len: 0 },
        static_text: None,
        max_lines: 0,
        value: 0,
        min: 0,
        max: 0,
        step: 0,
        icon: IconKind::None,
        series: 0,
        theme_id: 0,
        flags: 0,
        ext_flags: 0,
    };

    pub fn from_props(p: NodeProps) -> Self {
        let mut flags = if p.motion_state == MotionState::None {
            0
        } else {
            FLAG_STATE_MOTION
        };
        let bg_raw = match p.bg {
            Some(c) => {
                flags |= FLAG_HAS_BG;
                c
            }
            None => Rgb565::new(0, 0, 0),
        };
        let border_raw = match p.border_color {
            Some(c) => {
                flags |= FLAG_HAS_BORDER;
                c
            }
            None => Rgb565::new(0, 0, 0),
        };
        if p.ellipsis {
            flags |= FLAG_ELLIPSIS;
        }
        if p.checked {
            flags |= FLAG_CHECKED;
        }
        if p.focusable {
            flags |= FLAG_FOCUSABLE;
        }
        if p.enabled {
            flags |= FLAG_ENABLED;
        }
        if p.visible {
            flags |= FLAG_VISIBLE;
        }
        if p.clickable {
            flags |= FLAG_CLICKABLE;
        }
        let text_style_flags = TextStyleFlags::DEFAULT
            .with_wrap(p.wrap)
            .with_underline(p.underline)
            .with_strikethrough(p.strikethrough)
            .with_gamma(p.text_gamma)
            .with_subpixel(p.subpixel);
        let text_style_raw = (text_style_flags.0 & 0x3F) | (((p.font as u8) & 0x03) << 6);
        flags |= ((p.text_align as u16) << TEXT_ALIGN_SHIFT) & TEXT_ALIGN_MASK;
        if p.anim_easing as u8 & 1 != 0 {
            flags |= FLAG_EASE_OUT_CUBIC;
        }
        if p.swipeable {
            flags |= FLAG_SWIPE;
        }
        if p.long_press {
            flags |= FLAG_LONG_PRESS;
        }
        if p.repeatable {
            flags |= FLAG_REPEATABLE;
        }
        let (dyn_text, static_text) = match p.text {
            TextRef::Dynamic(r) => {
                flags |= FLAG_TEXT_DYN;
                (r, None)
            }
            TextRef::Static(s) => (
                TextRange { start: 0, len: 0 },
                if s.is_empty() { None } else { Some(s) },
            ),
        };
        let mut ext_flags = (p.pointer_capture as u16) & EXT_POINTER_CAPTURE_MASK;
        ext_flags |= ((p.anim_easing as u16) >> 1) << 12;
        if p.editable {
            ext_flags |= EXT_FLAG_EDITABLE;
        }
        ext_flags |= ((p.paint_mode as u16) << EXT_PAINT_MODE_SHIFT) & EXT_PAINT_MODE_MASK;
        ext_flags |= ((p.stack_align as u16) << EXT_STACK_ALIGN_SHIFT) & EXT_STACK_ALIGN_MASK;
        ext_flags |= ((p.justify as u16) << EXT_JUSTIFY_SHIFT) & EXT_JUSTIFY_MASK;
        Self {
            width: p.width,
            height: p.height,
            padding: p.padding,
            gap: p.gap,
            align_cross: p.align_cross,
            border_width: p.border_width,
            radius: p.radius,
            focus_ring_raw: p.focus_ring.to_raw(),
            press_feedback_raw: p.press_feedback.to_raw(),
            bg_raw,
            border_raw,
            fg: p.fg,
            accent: p.accent,
            focus_color: p.focus_color,
            overlay_color: p.overlay_color,
            overlay_alpha: p.overlay_alpha,
            text_style_raw,
            letter_spacing: p.letter_spacing,
            line_spacing: p.line_spacing,
            dyn_text,
            static_text,
            max_lines: p.max_lines,
            value: p.value,
            min: p.min,
            max: p.max,
            step: p.step,
            icon: p.icon,
            series: if p.grid_cols > 0 {
                p.grid_cols
            } else {
                p.series
            },
            theme_id: p.theme_id,
            flags,
            ext_flags,
        }
    }

    #[inline]
    pub const fn bg(&self) -> Option<Rgb565> {
        if (self.flags & FLAG_HAS_BG) != 0 {
            Some(self.bg_raw)
        } else {
            None
        }
    }

    #[inline]
    pub const fn border_color(&self) -> Option<Rgb565> {
        if (self.flags & FLAG_HAS_BORDER) != 0 {
            Some(self.border_raw)
        } else {
            None
        }
    }

    #[inline]
    pub const fn ellipsis(&self) -> bool {
        (self.flags & FLAG_ELLIPSIS) != 0
    }

    #[inline]
    pub const fn checked(&self) -> bool {
        (self.flags & FLAG_CHECKED) != 0
    }

    #[inline]
    pub const fn focusable(&self) -> bool {
        (self.flags & FLAG_FOCUSABLE) != 0
    }

    #[inline]
    pub const fn enabled(&self) -> bool {
        (self.flags & FLAG_ENABLED) != 0
    }

    #[inline]
    pub const fn visible(&self) -> bool {
        (self.flags & FLAG_VISIBLE) != 0
    }

    #[inline]
    pub const fn has_state_motion(&self) -> bool {
        self.flags & FLAG_STATE_MOTION != 0
    }

    pub const fn clickable(&self) -> bool {
        (self.flags & FLAG_CLICKABLE) != 0
    }

    #[inline]
    pub const fn text(&self) -> TextRef {
        if (self.flags & FLAG_TEXT_DYN) != 0 {
            TextRef::Dynamic(self.dyn_text)
        } else if let Some(s) = self.static_text {
            TextRef::Static(s)
        } else {
            TextRef::Static("")
        }
    }

    #[inline]
    pub const fn font(&self) -> FontId {
        FontId::from_raw(self.text_style_raw >> 6)
    }

    #[inline]
    pub const fn text_style(&self) -> TextStyleFlags {
        TextStyleFlags(self.text_style_raw & 0x3F)
    }

    #[inline]
    pub const fn wrap(&self) -> WrapMode {
        self.text_style().wrap()
    }

    #[inline]
    pub const fn text_gamma(&self) -> TextGamma {
        self.text_style().gamma()
    }

    #[inline]
    pub const fn subpixel(&self) -> SubpixelOrder {
        self.text_style().subpixel()
    }

    #[inline]
    pub const fn underline(&self) -> bool {
        self.text_style().underline()
    }

    #[inline]
    pub const fn strikethrough(&self) -> bool {
        self.text_style().strikethrough()
    }

    #[inline]
    pub const fn letter_spacing(&self) -> i8 {
        self.letter_spacing
    }

    #[inline]
    pub const fn line_spacing(&self) -> i8 {
        self.line_spacing
    }

    #[inline]
    pub const fn text_align(&self) -> TextAlign {
        match (self.flags & TEXT_ALIGN_MASK) >> TEXT_ALIGN_SHIFT {
            1 => TextAlign::Center,
            2 => TextAlign::End,
            _ => TextAlign::Start,
        }
    }

    #[inline]
    pub const fn anim_easing(&self) -> Easing {
        Easing::from_id(
            ((self.ext_flags >> 12) as u8) << 1 | ((self.flags & FLAG_EASE_OUT_CUBIC) != 0) as u8,
        )
    }

    #[inline]
    pub const fn swipeable(&self) -> bool {
        self.flags & FLAG_SWIPE != 0
    }
    pub const fn long_press(&self) -> bool {
        self.flags & FLAG_LONG_PRESS != 0
    }
    pub const fn repeatable(&self) -> bool {
        (self.flags & FLAG_REPEATABLE) != 0
    }

    #[inline]
    pub const fn focus_ring(&self) -> FocusRingStyle {
        FocusRingStyle::from_raw(self.focus_ring_raw)
    }

    #[inline]
    pub const fn press_feedback(&self) -> PressFeedback {
        PressFeedback::from_raw(self.press_feedback_raw)
    }

    #[inline]
    pub const fn pointer_capture(&self) -> PointerCaptureMode {
        PointerCaptureMode::from_raw((self.ext_flags & EXT_POINTER_CAPTURE_MASK) as u8)
    }

    #[inline]
    pub const fn editable(&self) -> bool {
        (self.ext_flags & EXT_FLAG_EDITABLE) != 0
    }

    #[inline]
    pub const fn paint_mode(&self) -> PaintMode {
        PaintMode::from_raw(((self.ext_flags & EXT_PAINT_MODE_MASK) >> EXT_PAINT_MODE_SHIFT) as u8)
    }

    #[inline]
    pub const fn stack_align(&self) -> StackAlign {
        StackAlign::from_raw(
            ((self.ext_flags & EXT_STACK_ALIGN_MASK) >> EXT_STACK_ALIGN_SHIFT) as u8,
        )
    }

    #[inline]
    pub const fn justify(&self) -> Justify {
        Justify::from_raw(((self.ext_flags & EXT_JUSTIFY_MASK) >> EXT_JUSTIFY_SHIFT) as u8)
    }

    #[inline]
    pub(crate) fn key_payload(&self) -> Key {
        Key(unpack_payload(self.min, self.max))
    }
    pub(crate) fn initial_scroll(&self) -> i32 {
        if self.step == SCROLL_OFFSET_PRESENT {
            unpack_payload(self.min, self.max) as i32
        } else {
            0
        }
    }
    pub fn tab_layout(&self) -> TabLayout {
        if self.grid_cols() > 0 {
            TabLayout::Columns(self.grid_cols())
        } else if self.max > 1 {
            TabLayout::Wrap {
                max_rows: self.max as u8,
            }
        } else {
            TabLayout::Scroll
        }
    }

    pub const fn grid_cols(&self) -> u8 {
        self.series
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SampleRing<const N: usize> {
    buf: [i16; N],
    head: usize,
    len: usize,
}

impl<const N: usize> SampleRing<N> {
    pub const ZERO: Self = Self {
        buf: [0; N],
        head: 0,
        len: 0,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    #[inline]
    pub fn push(&mut self, value: i16) {
        if N == 0 {
            return;
        }
        self.buf[self.head] = value;
        self.head = (self.head + 1) % N;
        if self.len < N {
            self.len += 1;
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    pub fn copy_ordered(&self, out: &mut [i16]) -> usize {
        if N == 0 || self.len == 0 || out.is_empty() {
            return 0;
        }
        let take = self.len.min(out.len());
        let start = (self.head + N - take) % N;
        for (i, dst) in out[..take].iter_mut().enumerate() {
            *dst = self.buf[(start + i) % N];
        }
        take
    }
}

impl<const N: usize> Default for SampleRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SparklineData {
    pub samples: [i16; MAX_SPARKLINE_SAMPLES],
    pub len: u8,
}

impl SparklineData {
    pub const ZERO: Self = Self {
        samples: [0; MAX_SPARKLINE_SAMPLES],
        len: 0,
    };

    #[inline]
    pub fn as_slice(&self) -> &[i16] {
        let len = (self.len as usize).min(MAX_SPARKLINE_SAMPLES);
        &self.samples[..len]
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SparklinePool {
    pub slots: [SparklineData; MAX_SPARKLINES],
    pub count: u8,
}

impl SparklinePool {
    pub const ZERO: Self = Self {
        slots: [SparklineData::ZERO; MAX_SPARKLINES],
        count: 0,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn reset(&mut self) {
        self.count = 0;
        for slot in &mut self.slots {
            slot.len = 0;
        }
    }

    pub fn push_slice(&mut self, samples: &[i16]) -> Result<u8, BuildError> {
        let idx = self.count as usize;
        if idx >= MAX_SPARKLINES {
            return Err(BuildError::Sparklines);
        }
        let start = samples.len().saturating_sub(MAX_SPARKLINE_SAMPLES);
        let tail = &samples[start..];
        let slot = &mut self.slots[idx];
        slot.samples[..tail.len()].copy_from_slice(tail);
        slot.len = tail.len() as u8;
        self.count += 1;
        Ok(idx as u8)
    }

    pub fn push_ring<const N: usize>(&mut self, ring: &SampleRing<N>) -> Result<u8, BuildError> {
        let idx = self.count as usize;
        if idx >= MAX_SPARKLINES {
            return Err(BuildError::Sparklines);
        }
        let slot = &mut self.slots[idx];
        let copied = ring.copy_ordered(&mut slot.samples);
        slot.len = copied as u8;
        self.count += 1;
        Ok(idx as u8)
    }

    #[inline]
    pub fn slice(&self, series: u8) -> &[i16] {
        let idx = series as usize;
        if idx < self.count as usize && idx < MAX_SPARKLINES {
            self.slots[idx].as_slice()
        } else {
            &[]
        }
    }
}

impl Default for SparklinePool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct DraftNode {
    pub(crate) motion: MotionBinding,
    pub(crate) motion_ms: u16,
    pub key: Key,
    pub kind: NodeKind,
    pub props: StoredProps,
    pub parent: SlotLink,
    pub first_child: SlotLink,
    pub last_child: SlotLink,
    pub next_sibling: SlotLink,
}

impl DraftNode {
    pub const EMPTY: Self = Self {
        motion: MotionBinding::EMPTY,
        motion_ms: 0,
        key: Key(0),
        kind: NodeKind::Label,
        props: StoredProps::ZERO,
        parent: SlotLink::NONE,
        first_child: SlotLink::NONE,
        last_child: SlotLink::NONE,
        next_sibling: SlotLink::NONE,
    };
}

pub struct ViewDraft {
    pub(crate) curves: crate::view::CurvePalette,
    pub effects: crate::effects::Effects,
    pub part_styles: crate::part_style::PartStyles,
    pub layout_extras: crate::part_style::LayoutExtras,
    #[cfg(feature = "inspector")]
    pub inspection: Option<alloc::boxed::Box<crate::style_source::StyleInspection>>,
    pub nodes: [DraftNode; MAX_NODES],
    pub count: u16,
    pub text_buf: [u8; MAX_TEXT_BYTES],
    pub text_len: u16,
    pub sparklines: SparklinePool,
    pub root: SlotLink,
    pub modal: SlotLink,
}

impl ViewDraft {
    pub fn part_style(&self, key: Key) -> &crate::part_style::WidgetPartStyle {
        self.nodes[..usize::from(self.count)]
            .iter()
            .position(|n| n.key == key)
            .map_or(&crate::part_style::WidgetPartStyle::ZERO, |i| {
                self.part_styles.get(i)
            })
    }

    pub const ZERO: Self = Self {
        effects: crate::effects::Effects::EMPTY,
        curves: [None; crate::capacity::MAX_ANIMATIONS],
        part_styles: crate::part_style::PartStyles::EMPTY,
        layout_extras: crate::part_style::LayoutExtras::EMPTY,
        #[cfg(feature = "inspector")]
        inspection: None,
        nodes: [DraftNode::EMPTY; MAX_NODES],
        count: 0,
        text_buf: [0; MAX_TEXT_BYTES],
        text_len: 0,
        sparklines: SparklinePool::ZERO,
        root: SlotLink::NONE,
        modal: SlotLink::NONE,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "inspector")]
        if let Some(i) = self.inspection.as_mut() {
            i.staging.clear();
        }
        self.count = 0;
        self.text_len = 0;
        self.sparklines.reset();
        self.effects = crate::effects::Effects::EMPTY;
        self.curves = [None; crate::capacity::MAX_ANIMATIONS];
        self.part_styles = crate::part_style::PartStyles::EMPTY;
        self.layout_extras = crate::part_style::LayoutExtras::EMPTY;
        self.root = SlotLink::NONE;
        self.modal = SlotLink::NONE;
    }

    pub fn text_str(&self, text: TextRef) -> &str {
        match text {
            TextRef::Static(s) => s,
            TextRef::Dynamic(r) => {
                let start = r.start as usize;
                let end = start.saturating_add(r.len as usize);
                if end <= self.text_len as usize {
                    core::str::from_utf8(&self.text_buf[start..end]).unwrap_or("")
                } else {
                    ""
                }
            }
        }
    }
}

impl Default for ViewDraft {
    fn default() -> Self {
        Self::new()
    }
}

struct DraftTextWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
    overflow: bool,
}

impl Write for DraftTextWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.overflow {
            return Err(fmt::Error);
        }
        let bytes = s.as_bytes();
        if self.len + bytes.len() > self.buf.len() {
            self.overflow = true;
            return Err(fmt::Error);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

pub struct ViewBuilder<'a> {
    draft: &'a mut ViewDraft,
    stack: [SlotLink; MAX_DEPTH],
    depth: usize,
    first_error: Option<BuildError>,
    operation_key: Option<Key>,
}

/// Scoped child builder passed into container closures.
///
/// ```
/// use liuli_core::view::{Key, NodeProps, ViewBuilder, ViewDraft};
///
/// let mut draft = ViewDraft::new();
/// let mut vb = ViewBuilder::new(&mut draft);
/// vb.root(Key(1), NodeProps::new(), |ui| {
///     ui.column(Key(2), NodeProps::new(), |ui| {
///         ui.label(Key(3), NodeProps::new())?;
///         Ok(())
///     })
/// }).unwrap();
/// vb.finish().unwrap();
/// ```
///
/// ```compile_fail
/// use liuli_core::view::{Key, NodeProps, ViewBuilder, ViewDraft};
///
/// let mut draft = ViewDraft::new();
/// let mut vb = ViewBuilder::new(&mut draft);
/// let _ = vb.root(Key(1), NodeProps::new(), |ui| {
///     ui.end()
/// });
/// ```
///
/// ```compile_fail
/// use liuli_core::view::{Key, NodeProps, ViewBuilder, ViewDraft};
///
/// let mut draft = ViewDraft::new();
/// let mut vb = ViewBuilder::new(&mut draft);
/// let _ = vb.root(Key(1), NodeProps::new(), |ui| {
///     ui.finish()
/// });
/// ```
pub struct Children<'a, 'b> {
    builder: &'b mut ViewBuilder<'a>,
}

impl<'a, 'b> Children<'a, 'b> {
    #[inline]
    fn ensure_key(&mut self, key: Key) -> Result<(), BuildError> {
        let b = &mut self.builder;
        b.operation_key = Some(key);
        if !b.draft.nodes[..usize::from(b.draft.count)]
            .iter()
            .any(|n| n.key == key)
        {
            return Err(b.fail(BuildError::InvalidRange));
        }
        Ok(())
    }

    /// Decorate an already declared node without changing its layout or order.
    pub fn decorate(
        &mut self,
        key: Key,
        style: crate::effects::SurfaceStyle,
    ) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        self.builder
            .draft
            .effects
            .add(key, style)
            .map_err(|err| self.builder.fail(err))
    }

    /// Attach or merge part-level and state-aware style overrides on an already declared node.
    pub fn style_parts(
        &mut self,
        key: Key,
        style: crate::part_style::WidgetPartStyle,
    ) -> Result<(), BuildError> {
        let b = &mut self.builder;
        b.operation_key = Some(key);
        let Some(i) = b.draft.nodes[..usize::from(b.draft.count)]
            .iter()
            .position(|n| n.key == key)
        else {
            return Err(b.fail(BuildError::InvalidRange));
        };
        b.draft.part_styles.merge(i, style).map_err(|e| b.fail(e))
    }

    /// Apply a signed layout pixel offset on any declared child (clipped by its parent container).
    pub fn set_offset(&mut self, key: Key, dx: i16, dy: i16) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        self.builder
            .draft
            .layout_extras
            .set_offset(key, dx, dy)
            .map_err(|err| self.builder.fail(err))
    }

    /// Translate relative to the real parent viewport and resize around the child's centre.
    pub fn set_layout_motion(
        &mut self,
        key: Key,
        motion: crate::part_style::LayoutMotion,
    ) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        self.builder
            .draft
            .layout_extras
            .set_motion(key, motion)
            .map_err(|err| self.builder.fail(err))
    }

    /// Compose opacity and a Y-axis turn with the node's declared surface.
    pub fn animate_layer(&mut self, key: Key, opacity: u8, turn: u8) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        if opacity > 15 {
            return Err(self.builder.fail(BuildError::InvalidRange));
        }
        #[cfg(feature = "layers")]
        {
            self.builder
                .draft
                .effects
                .animate_layer(key, opacity, turn)
                .map_err(|err| self.builder.fail(err))
        }
        #[cfg(not(feature = "layers"))]
        {
            let _ = turn;
            Ok(())
        }
    }

    /// Set a clean invalidation revision token for custom/overridden widgets.
    pub fn set_custom_rev(&mut self, key: Key, rev: u32) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        self.builder
            .draft
            .layout_extras
            .set_custom_rev(key, rev)
            .map_err(|err| self.builder.fail(err))
    }

    /// Constrain minimum/maximum dimensions for an already declared node.
    pub fn set_layout_constraints(
        &mut self,
        key: Key,
        min_w: Option<u16>,
        max_w: Option<u16>,
        min_h: Option<u16>,
        max_h: Option<u16>,
    ) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        self.builder
            .draft
            .layout_extras
            .set_constraints(key, min_w, max_w, min_h, max_h)
            .map_err(|err| self.builder.fail(err))
    }

    /// Attach outer margins around an already declared node.
    pub fn set_margin(&mut self, key: Key, margin: EdgeInsets) -> Result<(), BuildError> {
        self.ensure_key(key)?;
        self.builder
            .draft
            .layout_extras
            .set_margin(key, margin)
            .map_err(|err| self.builder.fail(err))
    }

    #[inline]
    pub fn wrap<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder.scoped_container(key, NodeKind::Wrap, props, f)
    }

    #[inline]
    pub fn custom_container<F>(
        &mut self,
        key: Key,
        props: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::CustomContainer, props, f)
    }

    #[inline]
    pub fn row<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder.scoped_container(key, NodeKind::Row, props, f)
    }

    #[inline]
    pub fn column<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::Column, props, f)
    }

    #[inline]
    pub fn stack<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::Stack, props, f)
    }

    #[inline]
    pub fn tab_bar<F>(
        &mut self,
        key: Key,
        layout: TabLayout,
        mut props: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        (props.max, props.grid_cols) = match layout {
            TabLayout::Scroll => (1, 0),
            TabLayout::Wrap { max_rows } => (i16::from(max_rows.max(1)), 0),
            TabLayout::Columns(n) => (1, n.max(1)),
        };
        self.builder
            .scoped_container(key, NodeKind::TabBar, props, f)
    }

    pub fn tab_panel<F>(
        &mut self,
        key: Key,
        active_tab: Key,
        mut props: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        // Panels have no numeric range. Reuse that space for a full-width stable key.
        (props.min, props.max) = pack_payload(active_tab.0);
        self.builder
            .scoped_container(key, NodeKind::TabPanel, props, f)
    }

    #[inline]
    pub fn scroll_y<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::ScrollY, props, f)
    }

    #[inline]
    pub fn scroll_x<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::ScrollX, props, f)
    }

    #[inline]
    pub fn grid<F>(
        &mut self,
        key: Key,
        cols: u8,
        mut props: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        props.grid_cols = cols.max(1);
        self.builder.scoped_container(key, NodeKind::Grid, props, f)
    }

    #[inline]
    pub fn modal<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::Modal, props, f)
    }

    #[inline]
    pub fn leaf(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<(), BuildError> {
        self.builder.leaf(key, kind, props)
    }

    #[inline]
    pub fn custom(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.builder.leaf(key, NodeKind::Custom, props)
    }

    pub fn label(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.builder.leaf(key, NodeKind::Label, props)
    }

    #[inline]
    pub fn button(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.builder.leaf(key, NodeKind::Button, props)
    }

    #[inline]
    pub fn toggle(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.builder.leaf(key, NodeKind::Toggle, props)
    }

    #[inline]
    pub fn slider(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.builder.leaf(key, NodeKind::Slider, props)
    }

    #[inline]
    pub fn progress_bar(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.builder.progress_bar(key, props)
    }

    #[inline]
    pub fn sparkline(
        &mut self,
        key: Key,
        samples: &[i16],
        props: NodeProps,
    ) -> Result<(), BuildError> {
        self.builder.sparkline(key, samples, props)
    }

    #[inline]
    pub fn sparkline_ring<const N: usize>(
        &mut self,
        key: Key,
        ring: &SampleRing<N>,
        props: NodeProps,
    ) -> Result<(), BuildError> {
        self.builder.sparkline_ring(key, ring, props)
    }

    #[inline]
    pub fn text_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<TextRef, BuildError> {
        self.builder.text_fmt(args)
    }

    #[inline]
    pub fn text_dyn(&mut self, s: &str) -> Result<TextRef, BuildError> {
        self.builder.text_dyn(s)
    }
}

impl<'a> ViewBuilder<'a> {
    pub fn new(draft: &'a mut ViewDraft) -> Self {
        draft.reset();
        Self {
            draft,
            stack: [SlotLink::NONE; MAX_DEPTH],
            depth: 0,
            first_error: None,
            operation_key: None,
        }
    }

    #[inline]
    pub const fn operation_key(&self) -> Option<Key> {
        self.operation_key
    }
    fn current_container(&self) -> Option<Key> {
        self.depth
            .checked_sub(1)
            .and_then(|d| self.stack[d].get())
            .map(|slot| self.draft.nodes[slot].key)
    }

    fn fail(&mut self, err: BuildError) -> BuildError {
        if self.first_error.is_none() {
            self.first_error = Some(err);
        }
        err
    }

    #[inline]
    pub fn root<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.scoped_container(key, NodeKind::Root, props, f)
    }

    #[inline]
    fn scoped_container<F>(
        &mut self,
        key: Key,
        kind: NodeKind,
        props: NodeProps,
        f: F,
    ) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        let saved_depth = self.depth;
        self.begin(key, kind, props)?;
        let res = {
            let mut children = Children { builder: self };
            f(&mut children)
        };
        while self.depth > saved_depth {
            let _ = self.end();
        }
        match res {
            Ok(()) => Ok(()),
            Err(e) => Err(self.fail(e)),
        }
    }

    pub fn text_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<TextRef, BuildError> {
        if self.first_error.is_none() {
            self.operation_key = self.current_container();
        }
        let start = self.draft.text_len as usize;
        let mut writer = DraftTextWriter {
            buf: &mut self.draft.text_buf,
            len: start,
            overflow: false,
        };
        if writer.write_fmt(args).is_err() || writer.overflow {
            return Err(self.fail(BuildError::TextBytes));
        }
        let end = writer.len;
        let len = end - start;
        self.draft.text_len = end as u16;
        Ok(TextRef::Dynamic(TextRange {
            start: start as u16,
            len: len as u16,
        }))
    }

    pub fn text_dyn(&mut self, s: &str) -> Result<TextRef, BuildError> {
        if self.first_error.is_none() {
            self.operation_key = self.current_container();
        }
        let start = self.draft.text_len as usize;
        let bytes = s.as_bytes();
        if start + bytes.len() > self.draft.text_buf.len() {
            return Err(self.fail(BuildError::TextBytes));
        }
        self.draft.text_buf[start..start + bytes.len()].copy_from_slice(bytes);
        self.draft.text_len = (start + bytes.len()) as u16;
        Ok(TextRef::Dynamic(TextRange {
            start: start as u16,
            len: bytes.len() as u16,
        }))
    }

    pub fn begin(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<(), BuildError> {
        if self.first_error.is_none() {
            self.operation_key = Some(key);
        }
        if !kind.is_container() {
            return Err(self.fail(BuildError::InvalidKind));
        }
        let idx = self.push_node(key, kind, props)?;
        self.stack[self.depth] = idx;
        self.depth += 1;
        Ok(())
    }

    pub fn leaf(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<(), BuildError> {
        if self.first_error.is_none() {
            self.operation_key = Some(key);
        }
        if kind.is_container() {
            return Err(self.fail(BuildError::InvalidKind));
        }
        self.push_node(key, kind, props)?;
        Ok(())
    }

    #[inline]
    pub fn progress_bar(&mut self, key: Key, props: NodeProps) -> Result<(), BuildError> {
        self.leaf(key, NodeKind::ProgressBar, props)
    }

    pub fn sparkline(
        &mut self,
        key: Key,
        samples: &[i16],
        mut props: NodeProps,
    ) -> Result<(), BuildError> {
        if props.min > props.max {
            return Err(self.fail(BuildError::InvalidRange));
        }
        match self.draft.sparklines.push_slice(samples) {
            Ok(series) => {
                props.series = series;
                self.leaf(key, NodeKind::Sparkline, props)
            }
            Err(err) => Err(self.fail(err)),
        }
    }

    pub fn sparkline_ring<const N: usize>(
        &mut self,
        key: Key,
        ring: &SampleRing<N>,
        mut props: NodeProps,
    ) -> Result<(), BuildError> {
        if props.min > props.max {
            return Err(self.fail(BuildError::InvalidRange));
        }
        match self.draft.sparklines.push_ring(ring) {
            Ok(series) => {
                props.series = series;
                self.leaf(key, NodeKind::Sparkline, props)
            }
            Err(err) => Err(self.fail(err)),
        }
    }

    pub fn end(&mut self) -> Result<(), BuildError> {
        if self.depth == 0 {
            return Err(self.fail(BuildError::UnmatchedEnd));
        }
        self.depth -= 1;
        self.stack[self.depth] = SlotLink::NONE;
        Ok(())
    }

    pub fn finish(self) -> Result<(), BuildError> {
        if let Some(err) = self.first_error {
            return Err(err);
        }
        if self.depth != 0 {
            return Err(BuildError::UnclosedContainer);
        }
        if self.draft.root.is_none() {
            return Err(BuildError::NoRoot);
        }
        Ok(())
    }

    fn push_node(
        &mut self,
        key: Key,
        kind: NodeKind,
        props: NodeProps,
    ) -> Result<SlotLink, BuildError> {
        if self.depth >= MAX_DEPTH {
            return Err(self.fail(BuildError::Depth));
        }
        let count = self.draft.count as usize;
        if count >= MAX_NODES {
            return Err(self.fail(BuildError::Nodes));
        }
        for i in 0..count {
            if self.draft.nodes[i].key == key {
                return Err(self.fail(BuildError::DuplicateKey));
            }
        }

        if self.depth == 0 && self.draft.root.is_some() {
            return Err(self.fail(BuildError::MultipleRoots));
        }

        if kind == NodeKind::Slider && (props.min > props.max || props.step <= 0) {
            return Err(self.fail(BuildError::InvalidRange));
        }
        if matches!(kind, NodeKind::ProgressBar | NodeKind::Sparkline) && props.min > props.max {
            return Err(self.fail(BuildError::InvalidRange));
        }

        if kind == NodeKind::Modal {
            if self.draft.modal.is_some() {
                return Err(self.fail(BuildError::NestedModal));
            }
            for &anc_link in &self.stack[..self.depth] {
                if let Some(anc_idx) = anc_link.get()
                    && self.draft.nodes[anc_idx].kind == NodeKind::Modal
                {
                    return Err(self.fail(BuildError::NestedModal));
                }
            }
        }

        let parent_link = if self.depth > 0 {
            let p_link = self.stack[self.depth - 1];
            if let Some(p_idx) = p_link.get() {
                let pk = self.draft.nodes[p_idx].kind;
                if (pk == NodeKind::ScrollY && matches!(props.height, Length::Fill(_)))
                    || (pk == NodeKind::ScrollX && matches!(props.width, Length::Fill(_)))
                {
                    return Err(self.fail(BuildError::InvalidScrollFill));
                }
            }
            p_link
        } else {
            SlotLink::NONE
        };

        let Some(link) = SlotLink::from_slot(count) else {
            return Err(self.fail(BuildError::Nodes));
        };
        let mut curve_slot = 0;
        if props.motion_state != MotionState::None
            && props.motion_ms > 0
            && let Some(curve) = props.motion_curve
        {
            let curves = &mut self.draft.curves;
            let Some(i) = curves
                .iter()
                .position(|c| *c == Some(curve))
                .or_else(|| curves.iter().position(Option::is_none))
            else {
                return Err(self.fail(BuildError::MotionCurves));
            };
            curves[i] = Some(curve);
            curve_slot = i as u8 + 1;
        }
        self.draft
            .part_styles
            .merge(count, props.parts)
            .map_err(|e| self.fail(e))?;
        self.draft.nodes[count] = DraftNode {
            motion: MotionBinding::new(props.motion_state, curve_slot),
            motion_ms: props.motion_ms,
            key,
            kind,
            props: StoredProps::for_kind(kind, props),
            parent: parent_link,
            first_child: SlotLink::NONE,
            last_child: SlotLink::NONE,
            next_sibling: SlotLink::NONE,
        };
        #[cfg(feature = "inspector")]
        if let Some(i) = self.draft.inspection.as_mut() {
            i.staging.push((key, props.style_source));
        }
        self.draft.count += 1;

        if let Some(p) = parent_link.get() {
            let prev_last = self.draft.nodes[p].last_child;
            if let Some(pl) = prev_last.get() {
                self.draft.nodes[pl].next_sibling = link;
            } else {
                self.draft.nodes[p].first_child = link;
            }
            self.draft.nodes[p].last_child = link;
        } else {
            self.draft.root = link;
        }

        if kind == NodeKind::Modal {
            self.draft.modal = link;
        }

        Ok(link)
    }
}
