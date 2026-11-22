use crate::geometry::Point;
use crate::view::Key;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NavDir {
    Prev,
    Next,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PointerId(pub u8);

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RawInput {
    Nav(NavDir),
    Activate,
    Adjust(i16),
    Back,
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
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct UiAction {
    pub key: Key,
    pub kind: ActionKind,
}
