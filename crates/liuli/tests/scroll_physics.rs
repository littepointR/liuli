use liuli::prelude::{InstantMs, NextWakeup, ScrollPhysics};

#[test]
fn snap_approaches_the_same_anchor_at_different_frame_rates() {
    // Anchors are -30, 70, 170 and the partial content edge at 245.
    for (start, velocity, expected) in [(70, 750, 245), (245, -900, 70), (130, 0, 170)] {
        let mut scroll = ScrollPhysics::new(-30, 245).with_snap(100);
        scroll.set_offset(start);
        scroll.release_with_velocity(velocity, InstantMs(0));
        let mut batched = scroll;
        let mut previous = start;
        let mut previous_speed = i32::MAX;
        for step in 1..=96 {
            let now = InstantMs(step * 16);
            scroll.advance(now);
            let offset = scroll.offset();
            let speed = (offset - previous) * (expected - start).signum();
            assert!((0..=previous_speed.saturating_add(1)).contains(&speed));
            previous = offset;
            previous_speed = speed;
            if step % 3 == 0 {
                batched.advance(now);
                assert_eq!(scroll.offset_q8, batched.offset_q8);
            }
        }
        assert_eq!(scroll.offset(), expected);
        assert_eq!(scroll.next_wakeup(InstantMs(1536)), NextWakeup::OnEvent);
    }
}

#[test]
fn snap_respects_resize_drag_and_explicit_position() {
    let mut scroll = ScrollPhysics::new(0, 500).with_snap(100);
    scroll.release_with_velocity(1100, InstantMs(0));
    scroll.advance(InstantMs(32));
    let before_resize = scroll.offset();
    scroll.set_bounds(0, 75);
    assert_eq!(scroll.offset(), before_resize);
    for step in 3..=64 {
        scroll.advance(InstantMs(step * 16));
        assert!(scroll.offset() <= 75);
    }
    assert_eq!(scroll.offset(), 75);
    assert!(!scroll.is_active());

    scroll.release_with_velocity(-1100, InstantMs(1040));
    scroll.advance(InstantMs(1056));
    scroll.begin_drag(InstantMs(1072));
    let dragged = scroll.drag_by(2, InstantMs(1088));
    assert!(!scroll.advance(InstantMs(1200)));
    assert_eq!(scroll.offset(), dragged);
    scroll.release(InstantMs(1200));
    assert!(scroll.is_active());
    scroll.set_offset(23);
    assert!(!scroll.advance(InstantMs(1400)));
    assert_eq!(scroll.offset(), 23);
}
