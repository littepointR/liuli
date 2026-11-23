use core::fmt::{self, Write};

use crate::capacity::{
    BuildError, MAX_DEPTH, MAX_NODES, MAX_SPARKLINE_SAMPLES, MAX_SPARKLINES, MAX_TEXT_BYTES,
};

pub const BUTTON_ICON_SIZE: u32 = 14;
pub const BUTTON_ICON_GAP: u32 = 4;
pub const BUTTON_ICON_SPAN: u32 = BUTTON_ICON_SIZE + BUTTON_ICON_GAP;
use crate::geometry::Rgb565;
use crate::input::UiAction;
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
pub struct SlotLink(u16);

impl SlotLink {
    pub const NONE: Self = Self(0);

    #[inline]
    pub const fn from_slot(slot: usize) -> Option<Self> {
        if slot < MAX_NODES {
            Some(Self((slot + 1) as u16))
        } else {
            None
        }
    }

    #[inline]
    pub const fn from_slot_u16(slot: u16) -> Option<Self> {
        if (slot as usize) < MAX_NODES {
            Some(Self(slot + 1))
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
                Some(idx)
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
}

impl NodeKind {
    pub const fn is_container(self) -> bool {
        matches!(
            self,
            Self::Root | Self::Row | Self::Column | Self::Stack | Self::ScrollY | Self::Modal
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct NodeProps {
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
    pub overlay_color: Rgb565,
    pub overlay_alpha: u8,
    pub text: TextRef,
    pub font: FontId,
    pub text_align: TextAlign,
    pub max_lines: u8,
    pub ellipsis: bool,
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
    pub repeatable: bool,
}

impl Default for NodeProps {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeProps {
    pub const fn new() -> Self {
        Self {
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
            overlay_color: Rgb565::new(0, 0, 0),
            overlay_alpha: 0,
            text: TextRef::Static(""),
            font: FontId::Body16,
            text_align: TextAlign::Start,
            max_lines: 0,
            ellipsis: true,
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
            repeatable: false,
        }
    }
}

const FLAG_HAS_BG: u16 = 1 << 0;
const FLAG_HAS_BORDER: u16 = 1 << 1;
const FLAG_ELLIPSIS: u16 = 1 << 2;
const FLAG_CHECKED: u16 = 1 << 3;
const FLAG_FOCUSABLE: u16 = 1 << 4;
const FLAG_ENABLED: u16 = 1 << 5;
const FLAG_VISIBLE: u16 = 1 << 6;
const FLAG_CLICKABLE: u16 = 1 << 7;
const FLAG_TEXT_DYN: u16 = 1 << 8;
const FLAG_FONT_SMALL12: u16 = 1 << 9;
const TEXT_ALIGN_SHIFT: u16 = 10;
const TEXT_ALIGN_MASK: u16 = 0b11 << TEXT_ALIGN_SHIFT;
const FLAG_EASE_OUT_CUBIC: u16 = 1 << 12;
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
    pub bg_raw: Rgb565,
    pub border_raw: Rgb565,
    pub fg: Rgb565,
    pub accent: Rgb565,
    pub focus_color: Rgb565,
    pub overlay_color: Rgb565,
    pub overlay_alpha: u8,
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
}

impl StoredProps {
    pub const ZERO: Self = Self {
        width: Length::Content,
        height: Length::Content,
        padding: EdgeInsets::ZERO,
        gap: 0,
        align_cross: Align::Start,
        border_width: 0,
        radius: 0,
        bg_raw: Rgb565::new(0, 0, 0),
        border_raw: Rgb565::new(0, 0, 0),
        fg: Rgb565::new(0, 0, 0),
        accent: Rgb565::new(0, 0, 0),
        focus_color: Rgb565::new(0, 0, 0),
        overlay_color: Rgb565::new(0, 0, 0),
        overlay_alpha: 0,
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
    };

    pub fn from_props(p: NodeProps) -> Self {
        let mut flags = 0u16;
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
        if p.font == FontId::Small12 {
            flags |= FLAG_FONT_SMALL12;
        }
        flags |= ((p.text_align as u16) << TEXT_ALIGN_SHIFT) & TEXT_ALIGN_MASK;
        if p.anim_easing == Easing::EaseOutCubic {
            flags |= FLAG_EASE_OUT_CUBIC;
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
        Self {
            width: p.width,
            height: p.height,
            padding: p.padding,
            gap: p.gap,
            align_cross: p.align_cross,
            border_width: p.border_width,
            radius: p.radius,
            bg_raw,
            border_raw,
            fg: p.fg,
            accent: p.accent,
            focus_color: p.focus_color,
            overlay_color: p.overlay_color,
            overlay_alpha: p.overlay_alpha,
            dyn_text,
            static_text,
            max_lines: p.max_lines,
            value: p.value,
            min: p.min,
            max: p.max,
            step: p.step,
            icon: p.icon,
            series: p.series,
            theme_id: p.theme_id,
            flags,
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
        if (self.flags & FLAG_FONT_SMALL12) != 0 {
            FontId::Small12
        } else {
            FontId::Body16
        }
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
        if (self.flags & FLAG_EASE_OUT_CUBIC) != 0 {
            Easing::EaseOutCubic
        } else {
            Easing::Linear
        }
    }

    #[inline]
    pub const fn repeatable(&self) -> bool {
        (self.flags & FLAG_REPEATABLE) != 0
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
    pub nodes: [DraftNode; MAX_NODES],
    pub count: u16,
    pub text_buf: [u8; MAX_TEXT_BYTES],
    pub text_len: u16,
    pub sparklines: SparklinePool,
    pub root: SlotLink,
    pub modal: SlotLink,
}

impl ViewDraft {
    pub const ZERO: Self = Self {
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
        self.count = 0;
        self.text_len = 0;
        self.sparklines.reset();
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
    pub fn scroll_y<F>(&mut self, key: Key, props: NodeProps, f: F) -> Result<(), BuildError>
    where
        F: FnOnce(&mut Children<'a, '_>) -> Result<(), BuildError>,
    {
        self.builder
            .scoped_container(key, NodeKind::ScrollY, props, f)
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
        }
    }

    #[inline]
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
        if !kind.is_container() {
            return Err(self.fail(BuildError::InvalidKind));
        }
        let idx = self.push_node(key, kind, props)?;
        self.stack[self.depth] = idx;
        self.depth += 1;
        Ok(())
    }

    pub fn leaf(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<(), BuildError> {
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
            if let Some(p_idx) = p_link.get()
                && self.draft.nodes[p_idx].kind == NodeKind::ScrollY
                && matches!(props.height, Length::Fill(_))
            {
                return Err(self.fail(BuildError::InvalidScrollFill));
            }
            p_link
        } else {
            SlotLink::NONE
        };

        let Some(link) = SlotLink::from_slot(count) else {
            return Err(self.fail(BuildError::Nodes));
        };
        self.draft.nodes[count] = DraftNode {
            key,
            kind,
            props: StoredProps::from_props(props),
            parent: parent_link,
            first_child: SlotLink::NONE,
            last_child: SlotLink::NONE,
            next_sibling: SlotLink::NONE,
        };
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
