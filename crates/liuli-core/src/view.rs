use core::fmt::{self, Write};

use crate::capacity::{BuildError, MAX_DEPTH, MAX_NODES, MAX_TEXT_BYTES};
use crate::geometry::Rgb565;
use crate::input::UiAction;
use crate::time::InstantMs;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Change {
    Unchanged,
    View,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Key(pub u32);

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
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct TextRange {
    pub start: u16,
    pub len: u16,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TextRef {
    Static(&'static str),
    Dynamic(TextRange),
}

impl Default for TextRef {
    fn default() -> Self {
        Self::Static("")
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum Length {
    Px(u32),
    #[default]
    Content,
    Fill(u16),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum IconKind {
    #[default]
    None,
    ChevronRight,
    Check,
    Warning,
    Sliders,
    Bell,
    Moon,
    Thermometer,
    Reset,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NodeKind {
    Root,
    Row,
    Column,
    Stack,
    ScrollY,
    Label,
    Button,
    Toggle,
    Slider,
    Modal,
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
}

impl Default for NodeProps {
    fn default() -> Self {
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
        }
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
        }
    }
}

pub const NONE_IDX: u16 = u16::MAX;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct DraftNode {
    pub key: Key,
    pub kind: NodeKind,
    pub props: NodeProps,
    pub parent: u16,
    pub first_child: u16,
    pub last_child: u16,
    pub next_sibling: u16,
}

impl DraftNode {
    pub const EMPTY: Self = Self {
        key: Key(0),
        kind: NodeKind::Label,
        props: NodeProps::new(),
        parent: NONE_IDX,
        first_child: NONE_IDX,
        last_child: NONE_IDX,
        next_sibling: NONE_IDX,
    };
}

pub struct ViewDraft {
    pub nodes: [DraftNode; MAX_NODES],
    pub count: u16,
    pub text_buf: [u8; MAX_TEXT_BYTES],
    pub text_len: u16,
    pub root: Option<u16>,
    pub modal: Option<u16>,
}

impl ViewDraft {
    pub const fn new() -> Self {
        Self {
            nodes: [DraftNode::EMPTY; MAX_NODES],
            count: 0,
            text_buf: [0; MAX_TEXT_BYTES],
            text_len: 0,
            root: None,
            modal: None,
        }
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.text_len = 0;
        self.root = None;
        self.modal = None;
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
    stack: [u16; MAX_DEPTH],
    depth: usize,
    failed: bool,
}

impl<'a> ViewBuilder<'a> {
    pub fn new(draft: &'a mut ViewDraft) -> Self {
        draft.reset();
        Self {
            draft,
            stack: [NONE_IDX; MAX_DEPTH],
            depth: 0,
            failed: false,
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
            self.failed = true;
            return Err(BuildError::TextBytes);
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
            self.failed = true;
            return Err(BuildError::TextBytes);
        }
        self.draft.text_buf[start..start + bytes.len()].copy_from_slice(bytes);
        self.draft.text_len = (start + bytes.len()) as u16;
        Ok(TextRef::Dynamic(TextRange {
            start: start as u16,
            len: bytes.len() as u16,
        }))
    }

    pub fn begin(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<(), BuildError> {
        let idx = self.push_node(key, kind, props)?;
        self.stack[self.depth] = idx;
        self.depth += 1;
        Ok(())
    }

    pub fn leaf(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<(), BuildError> {
        self.push_node(key, kind, props)?;
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), BuildError> {
        if self.depth == 0 {
            self.failed = true;
            return Err(BuildError::UnmatchedEnd);
        }
        self.depth -= 1;
        self.stack[self.depth] = NONE_IDX;
        Ok(())
    }

    pub fn finish(self) -> Result<(), BuildError> {
        if self.failed {
            return Err(BuildError::UnmatchedEnd);
        }
        if self.depth != 0 {
            return Err(BuildError::UnclosedContainer);
        }
        if self.draft.root.is_none() {
            return Err(BuildError::NoRoot);
        }
        Ok(())
    }

    fn push_node(&mut self, key: Key, kind: NodeKind, props: NodeProps) -> Result<u16, BuildError> {
        if self.depth >= MAX_DEPTH {
            self.failed = true;
            return Err(BuildError::Depth);
        }
        let count = self.draft.count as usize;
        if count >= MAX_NODES {
            self.failed = true;
            return Err(BuildError::Nodes);
        }
        for i in 0..count {
            if self.draft.nodes[i].key == key {
                self.failed = true;
                return Err(BuildError::DuplicateKey);
            }
        }

        if self.depth == 0 && self.draft.root.is_some() {
            self.failed = true;
            return Err(BuildError::UnmatchedEnd);
        }

        if kind == NodeKind::Modal {
            if self.draft.modal.is_some() {
                self.failed = true;
                return Err(BuildError::NestedModal);
            }
            for &anc_idx in &self.stack[..self.depth] {
                if self.draft.nodes[anc_idx as usize].kind == NodeKind::Modal {
                    self.failed = true;
                    return Err(BuildError::NestedModal);
                }
            }
        }

        let parent_idx = if self.depth > 0 {
            let p = self.stack[self.depth - 1];
            if self.draft.nodes[p as usize].kind == NodeKind::ScrollY
                && matches!(props.height, Length::Fill(_))
            {
                self.failed = true;
                return Err(BuildError::InvalidScrollFill);
            }
            p
        } else {
            NONE_IDX
        };

        let idx = count as u16;
        self.draft.nodes[count] = DraftNode {
            key,
            kind,
            props,
            parent: parent_idx,
            first_child: NONE_IDX,
            last_child: NONE_IDX,
            next_sibling: NONE_IDX,
        };
        self.draft.count += 1;

        if parent_idx == NONE_IDX {
            self.draft.root = Some(idx);
        } else {
            let p = parent_idx as usize;
            let prev_last = self.draft.nodes[p].last_child;
            if prev_last == NONE_IDX {
                self.draft.nodes[p].first_child = idx;
            } else {
                self.draft.nodes[prev_last as usize].next_sibling = idx;
            }
            self.draft.nodes[p].last_child = idx;
        }

        if kind == NodeKind::Modal {
            self.draft.modal = Some(idx);
        }

        Ok(idx)
    }
}
