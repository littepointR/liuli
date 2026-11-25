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
    Up,
    Down,
    Left,
    Right,
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

/// Active touch contact reported by a board touch controller.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TouchPoint {
    pub id: PointerId,
    pub pos: Point,
}

impl TouchPoint {
    /// Creates a tracked touch contact from a controller-assigned `id` and logical coordinates.
    #[inline]
    pub const fn new(id: u8, pos: Point) -> Self {
        Self {
            id: PointerId(id),
            pos,
        }
    }
}

/// Zero-allocation diff tracker turning raw controller contact snapshots into [`RawInput`] events.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TouchTracker<const MAX_CONTACTS: usize = 5> {
    active: [Option<TouchPoint>; MAX_CONTACTS],
}

impl<const MAX_CONTACTS: usize> TouchTracker<MAX_CONTACTS> {
    /// Creates an empty contact tracker.
    pub const fn new() -> Self {
        Self {
            active: [None; MAX_CONTACTS],
        }
    }

    /// Returns the number of currently active contacts.
    #[inline]
    pub fn active_count(&self) -> usize {
        self.active.iter().filter(|s| s.is_some()).count()
    }

    /// Returns `true` when no contacts are currently pressed.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.active.iter().all(Option::is_none)
    }

    /// Clears tracked state without emitting cancel or release events.
    #[inline]
    pub fn reset(&mut self) {
        self.active = [None; MAX_CONTACTS];
    }

    /// Diffs `contacts` against the previous poll, emitting releases before moves and presses.
    ///
    /// Slot state updates only after `emit` returns `Ok(())`, keeping state consistent if `emit`
    /// aborts early (for example, when DMA locks dispatch).
    pub fn try_update<E>(
        &mut self,
        contacts: &[TouchPoint],
        mut emit: impl FnMut(RawInput) -> Result<(), E>,
    ) -> Result<(), E> {
        for slot in &mut self.active {
            if let Some(old) = *slot
                && !contacts.iter().any(|c| c.id == old.id)
            {
                emit(RawInput::PointerUp {
                    id: old.id,
                    pos: old.pos,
                })?;
                *slot = None;
            }
        }

        for (idx, &pt) in contacts.iter().enumerate() {
            if contacts[..idx].iter().any(|p| p.id == pt.id) {
                continue;
            }
            if let Some(slot) = self
                .active
                .iter_mut()
                .find(|s| s.is_some_and(|cur| cur.id == pt.id))
            {
                if let Some(cur) = *slot
                    && cur.pos != pt.pos
                {
                    emit(RawInput::PointerMove {
                        id: pt.id,
                        pos: pt.pos,
                    })?;
                    *slot = Some(pt);
                }
            } else if let Some(empty) = self.active.iter_mut().find(|s| s.is_none()) {
                emit(RawInput::PointerDown {
                    id: pt.id,
                    pos: pt.pos,
                })?;
                *empty = Some(pt);
            }
        }

        Ok(())
    }

    /// Diffs `contacts` against the previous poll using an infallible callback.
    pub fn update(&mut self, contacts: &[TouchPoint], mut emit: impl FnMut(RawInput)) {
        let _ = self.try_update::<core::convert::Infallible>(contacts, |ev| {
            emit(ev);
            Ok(())
        });
    }

    /// Maps physical `contacts` into logical coordinates for `rotation` and `physical_size` before diffing.
    pub fn try_update_rotated<E>(
        &mut self,
        contacts: &[TouchPoint],
        rotation: crate::geometry::Rotation,
        physical_size: crate::geometry::Size,
        emit: impl FnMut(RawInput) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut mapped = [TouchPoint::new(0, Point::new(0, 0)); MAX_CONTACTS];
        let count = contacts.len().min(MAX_CONTACTS);
        for (dst, &src) in mapped[..count].iter_mut().zip(contacts.iter()) {
            *dst = TouchPoint::new(
                src.id.0,
                rotation.map_point_to_logical(src.pos, physical_size),
            );
        }
        self.try_update(&mapped[..count], emit)
    }

    /// Infallible convenience wrapper around [`Self::try_update_rotated`].
    pub fn update_rotated(
        &mut self,
        contacts: &[TouchPoint],
        rotation: crate::geometry::Rotation,
        physical_size: crate::geometry::Size,
        mut emit: impl FnMut(RawInput),
    ) {
        let _ = self.try_update_rotated::<core::convert::Infallible>(
            contacts,
            rotation,
            physical_size,
            |ev| {
                emit(ev);
                Ok(())
            },
        );
    }

    /// Cancels every currently active contact (for example, on touch bus read failure).
    pub fn try_cancel_all<E>(
        &mut self,
        mut emit: impl FnMut(RawInput) -> Result<(), E>,
    ) -> Result<(), E> {
        for slot in &mut self.active {
            if let Some(old) = *slot {
                emit(RawInput::PointerCancel { id: old.id })?;
                *slot = None;
            }
        }
        Ok(())
    }

    /// Cancels every currently active contact using an infallible callback.
    pub fn cancel_all(&mut self, mut emit: impl FnMut(RawInput)) {
        let _ = self.try_cancel_all::<core::convert::Infallible>(|ev| {
            emit(ev);
            Ok(())
        });
    }
}

impl<const MAX_CONTACTS: usize> Default for TouchTracker<MAX_CONTACTS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Nearest selectable value on a step grid anchored at `min`. Both endpoints are stops.
pub fn snap_stepped_value(min: i16, max: i16, step: i16, value: i16) -> i16 {
    if min >= max {
        return min;
    }
    let step = i32::from(step.max(1));
    let value = i32::from(value.clamp(min, max));
    let min = i32::from(min);
    let lower = min + (value - min) / step * step;
    let upper = (lower + step).min(i32::from(max));
    if value - lower < upper - value {
        lower as i16
    } else {
        upper as i16
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn tracker_emits_up_before_down_moves_dedups_and_cancels() {
        let mut tracker = TouchTracker::<2>::new();
        assert!(tracker.is_empty());

        let mut events = Vec::new();
        tracker.update(
            &[
                TouchPoint::new(0, Point::new(10, 20)),
                TouchPoint::new(0, Point::new(99, 99)), // duplicate ID ignored
                TouchPoint::new(1, Point::new(30, 40)),
                TouchPoint::new(2, Point::new(50, 60)), // exceeds MAX_CONTACTS = 2
            ],
            |ev| events.push(ev),
        );
        assert_eq!(tracker.active_count(), 2);
        assert_eq!(
            events,
            vec![
                RawInput::PointerDown {
                    id: PointerId(0),
                    pos: Point::new(10, 20),
                },
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(30, 40),
                },
            ]
        );

        // Stationary poll emits nothing.
        events.clear();
        tracker.update(
            &[
                TouchPoint::new(1, Point::new(30, 40)),
                TouchPoint::new(0, Point::new(10, 20)),
            ],
            |ev| events.push(ev),
        );
        assert!(events.is_empty());

        // Swap contact 1 out for contact 2 and move contact 0 -> Up comes before Move/Down.
        tracker.update(
            &[
                TouchPoint::new(0, Point::new(15, 25)),
                TouchPoint::new(2, Point::new(70, 80)),
            ],
            |ev| events.push(ev),
        );
        assert_eq!(
            events,
            vec![
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(30, 40),
                },
                RawInput::PointerMove {
                    id: PointerId(0),
                    pos: Point::new(15, 25),
                },
                RawInput::PointerDown {
                    id: PointerId(2),
                    pos: Point::new(70, 80),
                },
            ]
        );

        events.clear();
        tracker.cancel_all(|ev| events.push(ev));
        assert!(tracker.is_empty());
        assert_eq!(
            events,
            vec![
                RawInput::PointerCancel { id: PointerId(0) },
                RawInput::PointerCancel { id: PointerId(2) },
            ]
        );
    }

    #[test]
    fn tracker_preserves_remaining_state_on_mid_poll_error() {
        let mut tracker = TouchTracker::<2>::new();
        tracker.update(
            &[
                TouchPoint::new(0, Point::new(1, 2)),
                TouchPoint::new(1, Point::new(3, 4)),
            ],
            |_| {},
        );

        let mut count = 0usize;
        let err = tracker
            .try_cancel_all(|ev| {
                count += 1;
                if count == 2 {
                    return Err("busy");
                }
                assert_eq!(ev, RawInput::PointerCancel { id: PointerId(0) });
                Ok(())
            })
            .unwrap_err();
        assert_eq!(err, "busy");
        assert_eq!(tracker.active_count(), 1);

        let mut remaining = Vec::new();
        tracker.cancel_all(|ev| remaining.push(ev));
        assert!(tracker.is_empty());
        assert_eq!(
            remaining,
            vec![RawInput::PointerCancel { id: PointerId(1) }]
        );
    }

    #[test]
    fn tracker_maps_rotated_touch_coordinates() {
        let mut tracker = TouchTracker::<2>::new();
        let mut events = Vec::new();
        let phys = crate::geometry::Size::new(240, 320);
        // Physical (219, 10) at Deg90 => logical (10, 20)
        tracker.update_rotated(
            &[TouchPoint::new(0, Point::new(219, 10))],
            crate::geometry::Rotation::Deg90,
            phys,
            |ev| events.push(ev),
        );
        assert_eq!(
            events,
            vec![RawInput::PointerDown {
                id: PointerId(0),
                pos: Point::new(10, 20),
            }]
        );
    }
}
