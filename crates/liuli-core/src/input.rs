use crate::geometry::{Point, Size};
use crate::view::Key;

/// Phase of a continuous single-pointer gesture routed to an interactive widget.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum PointerPhase {
    Down = 0,
    Move = 1,
    Up = 2,
    Cancel = 3,
}

/// Widget-local pointer interaction event delivered to `App::on_pointer`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PointerEvent {
    pub phase: PointerPhase,
    pub local: Point,
    pub delta: Point,
    pub size: Size,
}

/// How single-pointer drags are arbitrated inside scroll containers.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum PointerCaptureMode {
    #[default]
    None = 0,
    Free = 1,
    LockX = 2,
    LockY = 3,
}

impl PointerCaptureMode {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw & 0b11 {
            1 => Self::Free,
            2 => Self::LockX,
            3 => Self::LockY,
            _ => Self::None,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NavDir {
    Prev,
    Next,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PointerId(pub u8);

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum HoldKey {
    #[default]
    Activate = 0,
    AdjustPrev = 1,
    AdjustNext = 2,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RawInput {
    Nav(NavDir),
    Activate,
    Adjust(i16),
    Back,
    KeyDown(HoldKey),
    KeyUp(HoldKey),
    PointerDown { id: PointerId, pos: Point },
    PointerMove { id: PointerId, pos: Point },
    PointerUp { id: PointerId, pos: Point },
    PointerCancel { id: PointerId },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ActionKind {
    Activate,
    SetValue(i16),
    Toggle,
    Dismiss,
    SwipeLeft,
    SwipeRight,
    LongPress,
    Adjust(i16),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct UiAction {
    pub key: Key,
    pub kind: ActionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GesturePhase {
    Begin,
    Update,
    End,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformGesture {
    pub phase: GesturePhase,
    pub center: Point,
    /// Translation in display pixels relative to the initial pair.
    pub translation: Point,
    pub scale: f32,
    pub radians: f32,
}

/// Text services deliver borrowed UTF-8; each update replaces the previous preedit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInput<'a> {
    Compose(&'a str),
    Commit(&'a str),
    Cancel,
    Backspace,
    Left,
    Right,
}
