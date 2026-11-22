#![no_std]
#![forbid(unsafe_code)]

pub mod capacity;
pub mod damage;
pub mod focus;
pub mod geometry;
pub mod input;
pub mod layout;
pub mod node;
pub mod paint;
pub mod reconcile;
pub mod time;
pub mod view;

#[cfg(test)]
mod tests {
    use crate::capacity::{BuildError, MAX_DAMAGE, MAX_NODES};
    use crate::damage::DamageSet;
    use crate::focus::{FocusState, PointerState, dispatch_input};
    use crate::geometry::{Point, Rect, Size};
    use crate::input::{ActionKind, NavDir, PointerId, RawInput};
    use crate::layout::{TextMeasureOut, TextMeasurer};
    use crate::node::RetainedTree;
    use crate::reconcile::reconcile_into;
    use crate::time::{ANIM_SCALE, AnimPool, AnimProp, ClockTracker, InstantMs};
    use crate::view::{
        Align, EdgeInsets, Key, Length, NodeKind, NodeProps, ViewBuilder, ViewDraft,
    };

    struct FixedMeasurer;

    impl TextMeasurer for FixedMeasurer {
        fn line_height(&self) -> u32 {
            16
        }

        fn measure_text(
            &self,
            text: &str,
            max_width: u32,
            _max_lines: u8,
            _ellipsis: bool,
        ) -> TextMeasureOut {
            let chars = text.chars().count() as u32;
            let raw_w = chars * 8;
            let w = raw_w.min(max_width);
            let lines = if raw_w == 0 {
                0
            } else if max_width == 0 {
                1
            } else {
                raw_w.div_ceil(max_width.max(8)).max(1)
            };
            let h = lines * 16;
            TextMeasureOut {
                size: Size::new(w, h),
                ink: Rect::new(0, 0, w, h),
                lines: lines as u16,
            }
        }
    }

    #[test]
    fn geometry_extreme_no_wrap() {
        let a = Rect::new(i32::MAX - 10, i32::MAX - 10, 100, 100);
        let b = Rect::new(i32::MAX - 5, i32::MAX - 5, 100, 100);
        assert!(a.intersects(b));
        let u = a.union(b);
        assert!(u.w >= 100);
        assert!(Rect::new(0, 0, 0, 10).is_empty());
        assert!(!Rect::new(0, 0, 10, 10).contains_point(Point::new(10, 5)));
    }

    #[test]
    fn damage_merge_rules_5_4_and_corner() {
        let screen = Rect::new(0, 0, 320, 240);
        let mut dmg = DamageSet::new();

        // Corner touch only: must NOT merge
        dmg.add(Rect::new(0, 0, 10, 10), screen);
        dmg.add(Rect::new(10, 10, 10, 10), screen);
        assert_eq!(dmg.as_slice().len(), 2);

        // Edge touch with union <= 5/4 sum (exact flush 10x10 + 10x10 => union 20x10 = 200 <= 200*5/4)
        dmg.clear();
        dmg.add(Rect::new(0, 0, 10, 10), screen);
        dmg.add(Rect::new(10, 0, 10, 10), screen);
        assert_eq!(dmg.as_slice(), &[Rect::new(0, 0, 20, 10)]);

        // Edge touch with tiny + huge where union > 5/4 sum: must NOT merge
        dmg.clear();
        dmg.add(Rect::new(0, 0, 10, 100), screen); // area 1000
        dmg.add(Rect::new(10, 0, 10, 10), screen); // area 100, union = 20*100 = 2000 > 1100*5/4=1375
        assert_eq!(dmg.as_slice().len(), 2);

        // Overflow > 12 degrades to full screen
        dmg.clear();
        for i in 0..=MAX_DAMAGE {
            dmg.add(Rect::new((i as i32) * 20, (i as i32) * 15, 5, 5), screen);
        }
        assert!(dmg.is_full_screen());
        assert_eq!(dmg.as_slice(), &[screen]);

        // Single viewport rect covering >= 60% does NOT expand into untouched header/footer
        dmg.clear();
        dmg.add(Rect::new(0, 32, 320, 184), screen);
        assert!(!dmg.is_full_screen());
        assert_eq!(dmg.as_slice(), &[Rect::new(0, 32, 320, 184)]);

        // Multiple disjoint rects totaling >= 60% collapse to full screen
        dmg.clear();
        dmg.add(Rect::new(0, 0, 320, 80), screen);
        dmg.add(Rect::new(0, 120, 320, 80), screen);
        assert!(dmg.is_full_screen());
        assert_eq!(dmg.as_slice(), &[screen]);
    }

    #[test]
    fn builder_rejects_errors_atomically() {
        let mut draft = ViewDraft::new();
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.leaf(Key(2), NodeKind::Label, NodeProps::new()).unwrap();
            assert_eq!(
                vb.leaf(Key(2), NodeKind::Label, NodeProps::new()),
                Err(BuildError::DuplicateKey)
            );
        }

        // ScrollY rejecting Fill height on child
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.begin(Key(2), NodeKind::ScrollY, NodeProps::new())
                .unwrap();
            let mut p = NodeProps::new();
            p.height = Length::Fill(1);
            assert_eq!(
                vb.leaf(Key(3), NodeKind::Label, p),
                Err(BuildError::InvalidScrollFill)
            );
        }

        // Nested modal rejected
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.begin(Key(2), NodeKind::Modal, NodeProps::new()).unwrap();
            assert_eq!(
                vb.begin(Key(3), NodeKind::Modal, NodeProps::new()),
                Err(BuildError::NestedModal)
            );
        }

        // Unmatched end rejected
        {
            let mut vb = ViewBuilder::new(&mut draft);
            assert_eq!(vb.end(), Err(BuildError::UnmatchedEnd));
        }

        // Unclosed container rejected on finish
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            assert_eq!(vb.finish(), Err(BuildError::UnclosedContainer));
        }
    }

    #[test]
    fn reconcile_preserves_id_reuses_deleted_slots_and_skips_same_width_text() {
        let screen = Rect::new(0, 0, 320, 240);
        let mut t1 = RetainedTree::new();
        let mut t2 = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();

        // Build initial tree: Root -> Row(10) + Label(20)
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.begin(
                Key(10),
                NodeKind::Column,
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    ..NodeProps::new()
                },
            )
            .unwrap();
            let txt = vb.text_fmt(format_args!("23.0 C")).unwrap();
            vb.leaf(
                Key(20),
                NodeKind::Label,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(24),
                    text: txt,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.leaf(
                Key(30),
                NodeKind::Button,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(32),
                    focusable: true,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.end().unwrap();
            vb.end().unwrap();
            vb.finish().unwrap();
        }
        reconcile_into(
            &draft,
            &t1,
            &mut t2,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();
        let btn_id_before = t2.get_id(Key(30)).unwrap();

        // Swap order of 30 and 20 + change text to same-width "24.5 C" -> NodeId preserved, layout skipped if no reorder,
        // wait: here reorder WILL relayout, so first test pure same-width text update!
        dmg.clear();
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.begin(
                Key(10),
                NodeKind::Column,
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    ..NodeProps::new()
                },
            )
            .unwrap();
            let txt = vb.text_fmt(format_args!("24.5 C")).unwrap();
            vb.leaf(
                Key(20),
                NodeKind::Label,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(24),
                    text: txt,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.leaf(
                Key(30),
                NodeKind::Button,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(32),
                    focusable: true,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.end().unwrap();
            vb.end().unwrap();
            vb.finish().unwrap();
        }
        let (out, _) = reconcile_into(
            &draft,
            &t2,
            &mut t1,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(1000),
            &mut dmg,
        )
        .unwrap();
        assert!(!out.relayout_needed);
        assert_eq!(out.measured_nodes, 0);
        assert_eq!(t1.get_id(Key(30)), Some(btn_id_before));
        assert_eq!(dmg.as_slice(), &[Rect::new(0, 0, 100, 24)]);

        // Now reorder 30 before 20: NodeId of 30 MUST still be identical!
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.begin(
                Key(10),
                NodeKind::Column,
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.leaf(
                Key(30),
                NodeKind::Button,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(32),
                    focusable: true,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            let txt = vb.text_fmt(format_args!("24.5 C")).unwrap();
            vb.leaf(
                Key(20),
                NodeKind::Label,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(24),
                    text: txt,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.end().unwrap();
            vb.end().unwrap();
            vb.finish().unwrap();
        }
        reconcile_into(
            &draft,
            &t1,
            &mut t2,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(1010),
            &mut dmg,
        )
        .unwrap();
        assert_eq!(t2.get_id(Key(30)), Some(btn_id_before));
    }

    #[test]
    fn full_pool_64_replacement_succeeds() {
        let screen = Rect::new(0, 0, 320, 240);
        let mut t1 = RetainedTree::new();
        let mut t2 = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();

        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            for i in 2..=MAX_NODES as u32 {
                vb.leaf(Key(i), NodeKind::Label, NodeProps::new()).unwrap();
            }
            vb.end().unwrap();
            vb.finish().unwrap();
        }
        reconcile_into(
            &draft,
            &t1,
            &mut t2,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();
        assert_eq!(t2.active_count as usize, MAX_NODES);

        // Replace all 63 children with completely new Keys (1002..=1064)
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            for i in 2..=MAX_NODES as u32 {
                vb.leaf(Key(1000 + i), NodeKind::Label, NodeProps::new())
                    .unwrap();
            }
            vb.end().unwrap();
            vb.finish().unwrap();
        }
        reconcile_into(
            &draft,
            &t2,
            &mut t1,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(10),
            &mut dmg,
        )
        .unwrap();
        assert_eq!(t1.active_count as usize, MAX_NODES);
        assert!(t1.get_id(Key(2)).is_none());
        assert!(t1.get_id(Key(1002)).is_some());
    }

    #[test]
    fn layout_fill_remainder_and_scroll_drag_capture() {
        let screen = Rect::new(0, 0, 320, 240);
        let t1 = RetainedTree::new();
        let mut t2 = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();

        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.begin(
                Key(2),
                NodeKind::Column,
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    ..NodeProps::new()
                },
            )
            .unwrap();
            // 100 px row split into 3 equal Fill(1) -> 34, 33, 33
            vb.begin(
                Key(3),
                NodeKind::Row,
                NodeProps {
                    width: Length::Px(100),
                    height: Length::Px(30),
                    align_cross: Align::Stretch,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            for k in [4u32, 5, 6] {
                vb.leaf(
                    Key(k),
                    NodeKind::Button,
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        focusable: true,
                        ..NodeProps::new()
                    },
                )
                .unwrap();
            }
            vb.end().unwrap();

            // ScrollY viewport height 60, containing 2 buttons height 50 each (total 100)
            vb.begin(
                Key(10),
                NodeKind::ScrollY,
                NodeProps {
                    width: Length::Px(200),
                    height: Length::Px(60),
                    padding: EdgeInsets::ZERO,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.leaf(
                Key(11),
                NodeKind::Button,
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Px(50),
                    focusable: true,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.leaf(
                Key(12),
                NodeKind::Button,
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Px(50),
                    focusable: true,
                    ..NodeProps::new()
                },
            )
            .unwrap();
            vb.end().unwrap();

            vb.end().unwrap();
            vb.end().unwrap();
            vb.finish().unwrap();
        }

        reconcile_into(
            &draft,
            &t1,
            &mut t2,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();

        assert_eq!(t2.node_by_key(Key(4)).unwrap().layout_rect.w, 34);
        assert_eq!(t2.node_by_key(Key(5)).unwrap().layout_rect.w, 33);
        assert_eq!(t2.node_by_key(Key(6)).unwrap().layout_rect.w, 33);

        // Drag inside ScrollY beyond 6px threshold must scroll and NOT activate button 11
        let mut focus = FocusState::new();
        let mut pointer = PointerState::new();
        let (_, act1) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut pointer,
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(50, 50),
            },
            screen,
            &mut dmg,
        );
        assert!(act1.is_none());
        let (_, act2) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut pointer,
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(50, 30),
            },
            screen,
            &mut dmg,
        );
        assert!(act2.is_none());
        assert_eq!(t2.node_by_key(Key(10)).unwrap().scroll_y, 20);
        let (_, act3) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut pointer,
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(50, 30),
            },
            screen,
            &mut dmg,
        );
        assert!(act3.is_none());

        // Second pointer ignored while first is active; normal tap activates button
        let (_, act_nav) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut pointer,
            RawInput::Nav(NavDir::Next),
            screen,
            &mut dmg,
        );
        assert!(act_nav.is_none());
        let (_, act_btn) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut pointer,
            RawInput::Activate,
            screen,
            &mut dmg,
        );
        assert_eq!(act_btn.unwrap().kind, ActionKind::Activate);
    }

    #[test]
    fn clock_backwards_and_animation_reverse() {
        let mut clk = ClockTracker::new();
        assert!(clk.advance_to(InstantMs(100)).is_ok());
        assert!(clk.advance_to(InstantMs(99)).is_err());

        let mut anims = AnimPool::new();
        let r = Rect::new(0, 0, 40, 24);
        anims.start(
            Key(7),
            AnimProp::ToggleKnob,
            0,
            ANIM_SCALE,
            InstantMs(0),
            160,
            r,
        );
        assert_eq!(
            anims.current_value(Key(7), AnimProp::ToggleKnob, InstantMs(80)),
            Some(500)
        );

        // Reverse midway at 80ms
        anims.start(
            Key(7),
            AnimProp::ToggleKnob,
            ANIM_SCALE,
            0,
            InstantMs(80),
            160,
            r,
        );
        assert_eq!(
            anims.current_value(Key(7), AnimProp::ToggleKnob, InstantMs(80)),
            Some(500)
        );
        assert_eq!(
            anims.current_value(Key(7), AnimProp::ToggleKnob, InstantMs(160)),
            Some(250)
        );
        assert_eq!(
            anims.current_value(Key(7), AnimProp::ToggleKnob, InstantMs(240)),
            Some(0)
        );
    }
}
