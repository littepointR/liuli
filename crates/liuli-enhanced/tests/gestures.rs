use liuli_core::{
    geometry::Point,
    input::{GesturePhase, PointerId, RawInput},
    node::NodeId,
};
use liuli_enhanced::gestures::{Delivery, TouchPair};
#[test]
fn two_contacts_rotate_scale_and_cancel_without_a_leftover_tap() {
    let mut pair = TouchPair::default();
    let a = PointerId(23);
    let b = PointerId(2);
    let owner = Some(NodeId::new(3, 1));
    assert!(matches!(
        pair.feed(
            RawInput::PointerDown {
                id: a,
                pos: Point::zero()
            },
            owner
        ),
        Delivery::Pass
    ));
    assert!(
        matches!(pair.feed(RawInput::PointerDown{id:b,pos:Point::new(10,0)},owner),Delivery::Gesture(_,g) if g.phase==GesturePhase::Begin)
    );
    let Delivery::Gesture(_, g) = pair.feed(
        RawInput::PointerMove {
            id: b,
            pos: Point::new(0, 20),
        },
        None,
    ) else {
        panic!("missing update")
    };
    assert_eq!(g.scale, 2.);
    assert!((g.radians - core::f32::consts::FRAC_PI_2).abs() < 0.001);
    assert_eq!(g.translation, Point::new(-5, 10));
    assert!(
        matches!(pair.feed(RawInput::PointerCancel{id:a},None),Delivery::Gesture(_,g) if g.phase==GesturePhase::Cancel)
    );
    assert!(matches!(
        pair.feed(
            RawInput::PointerUp {
                id: b,
                pos: Point::new(0, 20)
            },
            None
        ),
        Delivery::Consume
    ));
    assert_eq!(pair.owner(), None);
}
