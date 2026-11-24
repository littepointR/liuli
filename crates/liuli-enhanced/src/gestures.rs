use liuli_core::{
    geometry::Point,
    input::{GesturePhase, PointerId, RawInput, TransformGesture},
    node::NodeId,
};

#[derive(Copy, Clone)]
struct Contact {
    id: PointerId,
    origin: Point,
    pos: Point,
}
#[derive(Default)]
pub struct TouchPair {
    owner: Option<NodeId>,
    first: Option<Contact>,
    second: Option<Contact>,
    draining: [Option<PointerId>; 2],
}
pub enum Delivery {
    Pass,
    Consume,
    Gesture(NodeId, TransformGesture),
}
impl TouchPair {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn owner(&self) -> Option<NodeId> {
        self.owner
    }
    /// Cancel the winning recognizer and consume releases from its old contacts.
    pub fn cancel(&mut self) -> Delivery {
        let last = self.event(GesturePhase::Cancel);
        self.draining = [
            self.first.take().map(|p| p.id),
            self.second.take().map(|p| p.id),
        ];
        self.owner = None;
        last
    }
    pub fn feed(&mut self, input: RawInput, hit: Option<NodeId>) -> Delivery {
        if self.draining.iter().any(Option::is_some) {
            match input {
                RawInput::PointerCancel { id } | RawInput::PointerUp { id, .. } => {
                    if let Some(slot) = self.draining.iter_mut().find(|p| **p == Some(id)) {
                        *slot = None;
                        return Delivery::Consume;
                    }
                }
                RawInput::PointerMove { id, .. } if self.draining.contains(&Some(id)) => {
                    return Delivery::Consume;
                }
                RawInput::PointerDown { .. } => return Delivery::Consume,
                _ => {}
            }
        }
        match input {
            RawInput::PointerDown { id, pos } => {
                let Some(owner) = hit else {
                    return if self.second.is_some() {
                        Delivery::Consume
                    } else {
                        Delivery::Pass
                    };
                };
                let next = Contact {
                    id,
                    origin: pos,
                    pos,
                };
                if self.first.is_none() {
                    self.owner = Some(owner);
                    self.first = Some(next);
                    return Delivery::Pass;
                }
                if self.owner == Some(owner)
                    && self.second.is_none()
                    && self.first.is_some_and(|c| c.id != id)
                {
                    let first = self.first.as_mut().unwrap();
                    first.origin = first.pos;
                    self.second = Some(next);
                    return self.event(GesturePhase::Begin);
                }
            }
            RawInput::PointerMove { id, pos } => {
                let contact = self
                    .first
                    .as_mut()
                    .filter(|c| c.id == id)
                    .or_else(|| self.second.as_mut().filter(|c| c.id == id));
                if let Some(c) = contact {
                    c.pos = pos;
                    return self.event(GesturePhase::Update);
                }
            }
            RawInput::PointerUp { id, .. } | RawInput::PointerCancel { id } => {
                if self.first.is_some_and(|c| c.id == id) || self.second.is_some_and(|c| c.id == id)
                {
                    let delivery = self.event(if matches!(input, RawInput::PointerCancel { .. }) {
                        GesturePhase::Cancel
                    } else {
                        GesturePhase::End
                    });
                    let remaining = if self.second.is_some() {
                        [self.first, self.second]
                            .into_iter()
                            .flatten()
                            .find(|c| c.id != id)
                            .map(|c| c.id)
                    } else {
                        None
                    };
                    self.reset();
                    self.draining = [remaining, None];
                    return delivery;
                }
            }
            _ => return Delivery::Pass,
        }
        if self.second.is_some() {
            Delivery::Consume
        } else {
            Delivery::Pass
        }
    }
    fn event(&self, phase: GesturePhase) -> Delivery {
        let (Some(a), Some(b), Some(owner)) = (self.first, self.second, self.owner) else {
            return Delivery::Pass;
        };
        let middle = |a: Point, b: Point| {
            Point::new(
                ((i64::from(a.x) + i64::from(b.x)) / 2) as i32,
                ((i64::from(a.y) + i64::from(b.y)) / 2) as i32,
            )
        };
        let center = middle(a.pos, b.pos);
        let origin = middle(a.origin, b.origin);
        let delta = |a: i32, b: i32| (i64::from(b) - i64::from(a)) as f32;
        let (x0, y0) = (delta(a.origin.x, b.origin.x), delta(a.origin.y, b.origin.y));
        let (x, y) = (delta(a.pos.x, b.pos.x), delta(a.pos.y, b.pos.y));
        let base = libm::hypotf(x0, y0);
        Delivery::Gesture(
            owner,
            TransformGesture {
                phase,
                center,
                translation: Point::new(
                    center.x.saturating_sub(origin.x),
                    center.y.saturating_sub(origin.y),
                ),
                scale: if base > 0. {
                    libm::hypotf(x, y) / base
                } else {
                    1.
                },
                radians: libm::atan2f(x0 * y - y0 * x, x0 * x + y0 * y),
            },
        )
    }
}
