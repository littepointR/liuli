#![no_std]
#![forbid(unsafe_code)]

#[cfg(any(feature = "owned-images", feature = "inspector"))]
extern crate alloc;
pub mod bitmap;
pub mod capacity;
pub mod damage;
pub mod effects;
pub mod focus;
pub mod geometry;
pub mod input;
#[cfg(feature = "layers")]
pub mod layers;
pub mod layout;
pub mod node;
pub mod paint;
pub mod reconcile;
pub mod style_source;
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
        Align, EdgeInsets, Key, Length, NodeKind, NodeProps, SlotLink, StoredProps, ViewBuilder,
        ViewDraft,
    };

    struct FixedMeasurer;

    impl TextMeasurer for FixedMeasurer {
        fn line_height(&self, font: crate::view::FontId) -> u32 {
            match font {
                crate::view::FontId::Body16 => 16,
                crate::view::FontId::Small12 => 12,
            }
        }

        fn measure_text(
            &self,
            font: crate::view::FontId,
            text: &str,
            max_width: u32,
            _max_lines: u8,
            _ellipsis: bool,
            align: crate::view::TextAlign,
        ) -> TextMeasureOut {
            let cw = match font {
                crate::view::FontId::Body16 => 8,
                crate::view::FontId::Small12 => 6,
            };
            let lh = self.line_height(font);
            let chars = text.chars().count() as u32;
            let raw_w = chars * cw;
            let w = raw_w.min(max_width);
            let lines = if raw_w == 0 {
                0
            } else if max_width == 0 {
                1
            } else {
                raw_w.div_ceil(max_width.max(cw)).max(1)
            };
            let h = lines * lh;
            let x_off = match align {
                crate::view::TextAlign::Start => 0,
                crate::view::TextAlign::Center => max_width.saturating_sub(w) as i32 / 2,
                crate::view::TextAlign::End => max_width.saturating_sub(w) as i32,
            };
            TextMeasureOut {
                size: Size::new(w, h),
                ink: Rect::new(x_off, 0, w, h),
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
        let out = reconcile_into(
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

    #[test]
    fn zero_storage_sentinels_and_slot_link_bounds() {
        use crate::geometry::Rgb565;

        assert!(SlotLink::NONE.is_none());
        assert_eq!(SlotLink::NONE.get(), None);
        assert_eq!(SlotLink::from_slot(0).unwrap().get(), Some(0));
        assert_eq!(
            SlotLink::from_slot(MAX_NODES - 1).unwrap().get(),
            Some(MAX_NODES - 1)
        );
        assert!(SlotLink::from_slot(MAX_NODES).is_none());

        let mut focus = FocusState::ZERO;
        assert_eq!(focus.focused_key(), None);
        focus.set_focused_key(Some(Key(0)));
        assert_eq!(focus.focused_key(), Some(Key(0)));

        let mut ptr = PointerState::ZERO;
        assert_eq!(ptr.active_id(), None);
        ptr.set_active_id(Some(PointerId(0)));
        assert_eq!(ptr.active_id(), Some(PointerId(0)));

        let mut clk = ClockTracker::ZERO;
        assert_eq!(clk.last(), None);
        clk.advance_to(InstantMs(0)).unwrap();
        assert_eq!(clk.last(), Some(InstantMs(0)));
        assert!(clk.advance_to(InstantMs(0)).is_ok());

        let none_bg = StoredProps::from_props(NodeProps::new());
        assert_eq!(none_bg.bg(), None);
        assert_eq!(none_bg.border_color(), None);

        let mut black_props = NodeProps::new();
        black_props.bg = Some(Rgb565::new(0, 0, 0));
        black_props.border_color = Some(Rgb565::new(0, 0, 0));
        let black_bg = StoredProps::from_props(black_props);
        assert_eq!(black_bg.bg(), Some(Rgb565::new(0, 0, 0)));
        assert_eq!(black_bg.border_color(), Some(Rgb565::new(0, 0, 0)));
        assert_ne!(none_bg, black_bg);
    }

    #[test]
    fn builder_latches_first_error_and_validates_kinds_roots_and_ranges() {
        let mut draft = ViewDraft::new();

        // First error latches across subsequent errors until finish()
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            vb.leaf(Key(2), NodeKind::Label, NodeProps::new()).unwrap();
            assert_eq!(
                vb.leaf(Key(2), NodeKind::Label, NodeProps::new()),
                Err(BuildError::DuplicateKey)
            );
            assert_eq!(vb.end(), Ok(()));
            assert_eq!(vb.end(), Err(BuildError::UnmatchedEnd));
            assert_eq!(vb.finish(), Err(BuildError::DuplicateKey));
        }

        // begin() rejects non-containers, leaf() rejects containers
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
            assert_eq!(
                vb.begin(Key(2), NodeKind::Label, NodeProps::new()),
                Err(BuildError::InvalidKind)
            );
            assert_eq!(
                vb.leaf(Key(3), NodeKind::Row, NodeProps::new()),
                Err(BuildError::InvalidKind)
            );
            assert_eq!(vb.finish(), Err(BuildError::InvalidKind));
        }

        // Second root fails with MultipleRoots
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.root(Key(1), NodeProps::new(), |_| Ok(())).unwrap();
            assert_eq!(
                vb.root(Key(2), NodeProps::new(), |_| Ok(())),
                Err(BuildError::MultipleRoots)
            );
            assert_eq!(vb.finish(), Err(BuildError::MultipleRoots));
        }

        // Slider range validation: min > max or step <= 0 rejected with InvalidRange
        {
            let mut vb = ViewBuilder::new(&mut draft);
            let mut bad_range = NodeProps::new();
            bad_range.min = 50;
            bad_range.max = 10;
            bad_range.step = 1;
            let err = vb.root(Key(1), NodeProps::new(), |ui| ui.slider(Key(2), bad_range));
            assert_eq!(err, Err(BuildError::InvalidRange));
            assert_eq!(vb.finish(), Err(BuildError::InvalidRange));

            let mut vb2 = ViewBuilder::new(&mut draft);
            let mut bad_step = NodeProps::new();
            bad_step.min = 0;
            bad_step.max = 100;
            bad_step.step = 0;
            assert_eq!(
                vb2.root(Key(1), NodeProps::new(), |ui| ui.slider(Key(2), bad_step)),
                Err(BuildError::InvalidRange)
            );
            bad_step.step = -5;
            let mut vb3 = ViewBuilder::new(&mut draft);
            assert_eq!(
                vb3.root(Key(1), NodeProps::new(), |ui| ui.slider(Key(2), bad_step)),
                Err(BuildError::InvalidRange)
            );
        }

        // Unwinding inside vb.root when inner closure fails keeps depth balanced
        {
            let mut vb = ViewBuilder::new(&mut draft);
            let err = vb.root(Key(1), NodeProps::new(), |ui| {
                ui.column(Key(2), NodeProps::new(), |ui| {
                    ui.label(Key(1), NodeProps::new())
                })
            });
            assert_eq!(err, Err(BuildError::DuplicateKey));
            assert_eq!(vb.finish(), Err(BuildError::DuplicateKey));
        }
    }

    #[test]
    fn small_geometry_toggle_slider_and_scrolly_never_panic() {
        use crate::paint::{CommandList, PaintFocusInfo, emit_commands};

        let mut prev = RetainedTree::new();
        let mut tree = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();
        let mut cmds = CommandList::new();

        for w in 0u32..=30 {
            for h in 0u32..=30 {
                let screen = Rect::new(0, 0, w.max(1), h.max(1));
                for checked in [false, true] {
                    prev.reset();
                    dmg.clear();
                    let mut vb = ViewBuilder::new(&mut draft);
                    vb.root(
                        Key(1),
                        NodeProps {
                            width: Length::Px(w),
                            height: Length::Px(h),
                            ..NodeProps::new()
                        },
                        |ui| {
                            ui.toggle(
                                Key(2),
                                NodeProps {
                                    width: Length::Px(w),
                                    height: Length::Px(h),
                                    checked,
                                    ..NodeProps::new()
                                },
                            )
                        },
                    )
                    .unwrap();
                    vb.finish().unwrap();
                    reconcile_into(
                        &draft,
                        &prev,
                        &mut tree,
                        screen,
                        &FixedMeasurer,
                        &mut anims,
                        None,
                        None,
                        InstantMs(0),
                        &mut dmg,
                    )
                    .unwrap();
                    emit_commands(
                        &tree,
                        &anims,
                        PaintFocusInfo {
                            focused_slot: None,
                            ring_visible: false,
                            editing: false,
                        },
                        screen,
                        &mut cmds,
                    )
                    .unwrap();
                }

                // Slider tiny geometry
                {
                    prev.reset();
                    dmg.clear();
                    let mut vb = ViewBuilder::new(&mut draft);
                    vb.root(
                        Key(1),
                        NodeProps {
                            width: Length::Px(w),
                            height: Length::Px(h),
                            ..NodeProps::new()
                        },
                        |ui| {
                            ui.slider(
                                Key(3),
                                NodeProps {
                                    width: Length::Px(w),
                                    height: Length::Px(h),
                                    value: 50,
                                    min: 0,
                                    max: 100,
                                    step: 5,
                                    ..NodeProps::new()
                                },
                            )
                        },
                    )
                    .unwrap();
                    vb.finish().unwrap();
                    reconcile_into(
                        &draft,
                        &prev,
                        &mut tree,
                        screen,
                        &FixedMeasurer,
                        &mut anims,
                        None,
                        None,
                        InstantMs(0),
                        &mut dmg,
                    )
                    .unwrap();
                    emit_commands(
                        &tree,
                        &anims,
                        PaintFocusInfo {
                            focused_slot: None,
                            ring_visible: false,
                            editing: false,
                        },
                        screen,
                        &mut cmds,
                    )
                    .unwrap();
                }

                // ScrollY overflowing viewport including 9..=15 px heights
                if w >= 7 {
                    prev.reset();
                    dmg.clear();
                    let mut vb = ViewBuilder::new(&mut draft);
                    vb.root(
                        Key(1),
                        NodeProps {
                            width: Length::Px(w),
                            height: Length::Px(h),
                            ..NodeProps::new()
                        },
                        |ui| {
                            ui.scroll_y(
                                Key(4),
                                NodeProps {
                                    width: Length::Px(w),
                                    height: Length::Px(h),
                                    ..NodeProps::new()
                                },
                                |ui| {
                                    ui.label(
                                        Key(5),
                                        NodeProps {
                                            width: Length::Px(w),
                                            height: Length::Px(50),
                                            ..NodeProps::new()
                                        },
                                    )
                                },
                            )
                        },
                    )
                    .unwrap();
                    vb.finish().unwrap();
                    reconcile_into(
                        &draft,
                        &prev,
                        &mut tree,
                        screen,
                        &FixedMeasurer,
                        &mut anims,
                        None,
                        None,
                        InstantMs(0),
                        &mut dmg,
                    )
                    .unwrap();
                    emit_commands(
                        &tree,
                        &anims,
                        PaintFocusInfo {
                            focused_slot: None,
                            ring_visible: false,
                            editing: false,
                        },
                        screen,
                        &mut cmds,
                    )
                    .unwrap();
                }
            }
        }
    }

    #[test]
    fn slider_full_i16_range_and_degenerate_range_safe() {
        use crate::paint::{CommandList, PaintFocusInfo, emit_commands};

        let screen = Rect::new(0, 0, 200, 40);
        let mut prev = RetainedTree::new();
        let mut tree = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();
        let mut cmds = CommandList::new();
        let mut focus = FocusState::new();
        let mut pointer = PointerState::new();

        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.root(
                Key(1),
                NodeProps {
                    width: Length::Px(200),
                    height: Length::Px(40),
                    ..NodeProps::new()
                },
                |ui| {
                    ui.slider(
                        Key(2),
                        NodeProps {
                            width: Length::Px(200),
                            height: Length::Px(30),
                            value: 0,
                            min: i16::MIN,
                            max: i16::MAX,
                            step: 1,
                            focusable: true,
                            ..NodeProps::new()
                        },
                    )
                },
            )
            .unwrap();
            vb.finish().unwrap();
        }

        reconcile_into(
            &draft,
            &prev,
            &mut tree,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();
        emit_commands(
            &tree,
            &anims,
            PaintFocusInfo {
                focused_slot: None,
                ring_visible: false,
                editing: false,
            },
            screen,
            &mut cmds,
        )
        .unwrap();

        let (c1, a1) = dispatch_input(
            &mut tree,
            &mut focus,
            &mut pointer,
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(0, 15),
            },
            screen,
            &mut dmg,
        );
        assert!(c1);
        assert_eq!(a1.unwrap().kind, ActionKind::SetValue(i16::MIN));

        let (c2, a2) = dispatch_input(
            &mut tree,
            &mut focus,
            &mut pointer,
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(199, 15),
            },
            screen,
            &mut dmg,
        );
        assert!(c2);
        assert_eq!(a2.unwrap().kind, ActionKind::SetValue(i16::MAX));

        let _ = dispatch_input(
            &mut tree,
            &mut focus,
            &mut pointer,
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(199, 15),
            },
            screen,
            &mut dmg,
        );

        // Keyboard edit at i16 extremes must clamp safely
        focus.set_focused_key(Some(Key(2)));
        focus.editing = true;
        tree.nodes[tree.find_slot_by_key(Key(2)).unwrap() as usize]
            .props
            .value = i16::MAX;
        let (_, a3) = dispatch_input(
            &mut tree,
            &mut focus,
            &mut pointer,
            RawInput::Adjust(100),
            screen,
            &mut dmg,
        );
        assert!(a3.is_none());

        // Degenerate min == max slider returns (true, None) and paints cleanly
        prev.reset();
        let mut vb2 = ViewBuilder::new(&mut draft);
        vb2.root(
            Key(1),
            NodeProps {
                width: Length::Px(200),
                height: Length::Px(40),
                ..NodeProps::new()
            },
            |ui| {
                ui.slider(
                    Key(2),
                    NodeProps {
                        width: Length::Px(200),
                        height: Length::Px(30),
                        value: 42,
                        min: 42,
                        max: 42,
                        step: 1,
                        focusable: true,
                        ..NodeProps::new()
                    },
                )
            },
        )
        .unwrap();
        vb2.finish().unwrap();
        reconcile_into(
            &draft,
            &prev,
            &mut tree,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();
        emit_commands(
            &tree,
            &anims,
            PaintFocusInfo {
                focused_slot: None,
                ring_visible: false,
                editing: false,
            },
            screen,
            &mut cmds,
        )
        .unwrap();
        let mut focus2 = FocusState::new();
        let mut ptr2 = PointerState::new();
        let (cd, ad) = dispatch_input(
            &mut tree,
            &mut focus2,
            &mut ptr2,
            RawInput::PointerDown {
                id: PointerId(2),
                pos: Point::new(50, 15),
            },
            screen,
            &mut dmg,
        );
        assert!(cd);
        assert!(ad.is_none());
    }

    #[test]
    fn stale_node_id_capture_is_invalidated_on_recycle_and_modal() {
        use crate::focus::CaptureKind;

        let screen = Rect::new(0, 0, 120, 80);
        let mut t1 = RetainedTree::new();
        let mut t2 = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();
        let mut focus = FocusState::new();
        let mut pointer = PointerState::new();

        // Build initial tree with Button Key(10)
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.root(
                Key(1),
                NodeProps {
                    width: Length::Px(120),
                    height: Length::Px(80),
                    ..NodeProps::new()
                },
                |ui| {
                    ui.button(
                        Key(10),
                        NodeProps {
                            width: Length::Px(100),
                            height: Length::Px(40),
                            clickable: true,
                            focusable: true,
                            ..NodeProps::new()
                        },
                    )
                },
            )
            .unwrap();
            vb.finish().unwrap();
        }
        reconcile_into(
            &draft,
            &RetainedTree::new(),
            &mut t1,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();

        let (c_down, _) = dispatch_input(
            &mut t1,
            &mut focus,
            &mut pointer,
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(20, 20),
            },
            screen,
            &mut dmg,
        );
        assert!(c_down);
        let old_id = t1.get_id(Key(10)).unwrap();
        assert_eq!(pointer.capture, CaptureKind::Button(old_id));

        // Replace Key(10) with Button Key(20) in the same recycled slot with bumped generation
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.root(
                Key(1),
                NodeProps {
                    width: Length::Px(120),
                    height: Length::Px(80),
                    ..NodeProps::new()
                },
                |ui| {
                    ui.button(
                        Key(20),
                        NodeProps {
                            width: Length::Px(100),
                            height: Length::Px(40),
                            clickable: true,
                            focusable: true,
                            ..NodeProps::new()
                        },
                    )
                },
            )
            .unwrap();
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
            InstantMs(10),
            &mut dmg,
        )
        .unwrap();
        let new_id = t2.get_id(Key(20)).unwrap();
        assert_eq!(new_id.slot(), old_id.slot());
        assert_ne!(new_id.generation(), old_id.generation());
        assert!(!t2.is_id_valid(old_id));

        // Even with stale CaptureKind::Button(old_id), move/up must neither activate nor press Key(20)
        let mut stale_ptr_move = pointer;
        let (cm, am) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut stale_ptr_move,
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(20, 20),
            },
            screen,
            &mut dmg,
        );
        assert!(!cm);
        assert!(am.is_none());
        assert_eq!(stale_ptr_move.capture, CaptureKind::None);
        assert!(!t2.nodes[new_id.slot() as usize].pressed);

        let mut stale_ptr_up = pointer;
        let (cu, au) = dispatch_input(
            &mut t2,
            &mut focus,
            &mut stale_ptr_up,
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(20, 20),
            },
            screen,
            &mut dmg,
        );
        assert!(!cu);
        assert!(au.is_none());
        assert_eq!(stale_ptr_up.capture, CaptureKind::None);

        // Modal opening behind an active capture must block activation and clear pressed
        let mut t_modal = RetainedTree::new();
        {
            let mut vb = ViewBuilder::new(&mut draft);
            vb.root(
                Key(1),
                NodeProps {
                    width: Length::Px(120),
                    height: Length::Px(80),
                    ..NodeProps::new()
                },
                |ui| {
                    ui.button(
                        Key(10),
                        NodeProps {
                            width: Length::Px(100),
                            height: Length::Px(40),
                            clickable: true,
                            focusable: true,
                            ..NodeProps::new()
                        },
                    )?;
                    ui.modal(
                        Key(99),
                        NodeProps {
                            width: Length::Fill(1),
                            height: Length::Fill(1),
                            ..NodeProps::new()
                        },
                        |ui| ui.label(Key(100), NodeProps::new()),
                    )
                },
            )
            .unwrap();
            vb.finish().unwrap();
        }
        reconcile_into(
            &draft,
            &t1,
            &mut t_modal,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(20),
            &mut dmg,
        )
        .unwrap();
        assert!(t_modal.nodes[old_id.slot() as usize].pressed);
        let mut modal_ptr = pointer;
        let (cmu, amu) = dispatch_input(
            &mut t_modal,
            &mut focus,
            &mut modal_ptr,
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(20, 20),
            },
            screen,
            &mut dmg,
        );
        assert!(!cmu);
        assert!(amu.is_none());
        assert_eq!(modal_ptr.capture, CaptureKind::None);
        assert!(!t_modal.nodes[old_id.slot() as usize].pressed);
    }

    #[test]
    fn progress_bar_values_clamping_degenerate_and_zero_measure_update() {
        use crate::geometry::Rgb565;
        use crate::paint::{CommandList, PaintCmd, PaintFocusInfo, emit_commands};

        let screen = Rect::new(0, 0, 120, 40);
        let mut t1 = RetainedTree::new();
        let mut t2 = RetainedTree::new();
        let mut draft = ViewDraft::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();
        let mut cmds = CommandList::new();

        // min > max rejected with InvalidRange
        {
            let mut vb = ViewBuilder::new(&mut draft);
            let mut bad = NodeProps::new();
            bad.min = 50;
            bad.max = 10;
            assert_eq!(
                vb.root(Key(1), NodeProps::new(), |ui| ui.progress_bar(Key(2), bad)),
                Err(BuildError::InvalidRange)
            );
            assert_eq!(vb.finish(), Err(BuildError::InvalidRange));
        }

        let build_bar = |draft: &mut ViewDraft, w: u32, min: i16, max: i16, val: i16| {
            let mut vb = ViewBuilder::new(draft);
            vb.root(
                Key(1),
                NodeProps {
                    width: Length::Px(120),
                    height: Length::Px(40),
                    ..NodeProps::new()
                },
                |ui| {
                    ui.progress_bar(
                        Key(2),
                        NodeProps {
                            width: Length::Px(w),
                            height: Length::Px(10),
                            min,
                            max,
                            value: val,
                            bg: Some(Rgb565::new(2, 4, 2)),
                            accent: Rgb565::new(0, 63, 0),
                            ..NodeProps::new()
                        },
                    )
                },
            )
            .unwrap();
            vb.finish().unwrap();
        };

        // 0% -> track only (no fill command)
        build_bar(&mut draft, 100, 0, 100, 0);
        reconcile_into(
            &draft,
            &RetainedTree::new(),
            &mut t1,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();
        emit_commands(
            &t1,
            &anims,
            PaintFocusInfo {
                focused_slot: None,
                ring_visible: false,
                editing: false,
            },
            screen,
            &mut cmds,
        )
        .unwrap();
        assert_eq!(
            cmds.as_slice()
                .iter()
                .filter(|c| matches!(c, PaintCmd::FillRoundedRect { .. }))
                .count(),
            1
        );

        // 50% -> track + 50px fill, AND zero measured nodes on value change
        build_bar(&mut draft, 100, 0, 100, 50);
        dmg.clear();
        let out = reconcile_into(
            &draft,
            &t1,
            &mut t2,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(10),
            &mut dmg,
        )
        .unwrap();
        assert_eq!(out.measured_nodes, 0);
        assert_eq!(
            dmg.as_slice(),
            &[t2.node_by_key(Key(2)).unwrap().visible_rect]
        );
        emit_commands(
            &t2,
            &anims,
            PaintFocusInfo {
                focused_slot: None,
                ring_visible: false,
                editing: false,
            },
            screen,
            &mut cmds,
        )
        .unwrap();
        let mut fill_count = 0usize;
        let mut second_w = None;
        for cmd in cmds.as_slice() {
            if let PaintCmd::FillRoundedRect { rect, .. } = *cmd {
                if fill_count == 1 {
                    second_w = Some(rect.w);
                }
                fill_count += 1;
            }
        }
        assert_eq!(fill_count, 2);
        assert_eq!(second_w, Some(50));

        // Over-max, under-min, min==max degenerate (full), and 1px wide
        for (w, min, max, val, expected_fill) in [
            (100u32, 0i16, 100i16, 200i16, Some(100u32)),
            (100, 0, 100, -25, None),
            (100, 42, 42, 42, Some(100)),
            (1, 0, 100, 100, Some(1)),
        ] {
            build_bar(&mut draft, w, min, max, val);
            reconcile_into(
                &draft,
                &RetainedTree::new(),
                &mut t1,
                screen,
                &FixedMeasurer,
                &mut anims,
                None,
                None,
                InstantMs(20),
                &mut dmg,
            )
            .unwrap();
            emit_commands(
                &t1,
                &anims,
                PaintFocusInfo {
                    focused_slot: None,
                    ring_visible: false,
                    editing: false,
                },
                screen,
                &mut cmds,
            )
            .unwrap();
            let got_fill = cmds
                .as_slice()
                .iter()
                .filter_map(|c| match *c {
                    PaintCmd::FillRoundedRect { rect, .. } => Some(rect),
                    _ => None,
                })
                .nth(1)
                .map(|r| r.w);
            assert_eq!(got_fill, expected_fill);
        }
    }

    #[test]
    fn sparkline_capacity_ring_order_and_offscreen_zero_damage() {
        use crate::capacity::MAX_SPARKLINES;
        use crate::view::SampleRing;

        // 1. SampleRing wraparound ordering
        let mut ring: SampleRing<4> = SampleRing::new();
        for v in 1i16..=6 {
            ring.push(v);
        }
        assert_eq!(ring.len(), 4);
        let mut out4 = [0i16; 4];
        assert_eq!(ring.copy_ordered(&mut out4), 4);
        assert_eq!(out4, [3, 4, 5, 6]);
        let mut out2 = [0i16; 2];
        assert_eq!(ring.copy_ordered(&mut out2), 2);
        assert_eq!(out2, [5, 6]);

        // 2. MAX_SPARKLINES = 2 allowed, 3rd fails with Sparklines, min > max fails with InvalidRange
        let mut draft = ViewDraft::new();
        {
            let mut vb = ViewBuilder::new(&mut draft);
            let res = vb.root(Key(1), NodeProps::new(), |ui| {
                for i in 0..MAX_SPARKLINES {
                    ui.sparkline_ring(Key(10 + i as u32), &ring, NodeProps::new())?;
                }
                ui.sparkline(Key(99), &[1, 2, 3], NodeProps::new())
            });
            assert_eq!(res, Err(BuildError::Sparklines));
            assert_eq!(vb.finish(), Err(BuildError::Sparklines));

            let mut vb_rng = ViewBuilder::new(&mut draft);
            let mut bad_p = NodeProps::new();
            bad_p.min = 10;
            bad_p.max = 5;
            assert_eq!(
                vb_rng.root(Key(1), NodeProps::new(), |ui| {
                    ui.sparkline(Key(2), &[7], bad_p)
                }),
                Err(BuildError::InvalidRange)
            );
        }

        // 3. Updating visible Sparkline => measured_nodes == 0 & bounded damage;
        //    Updating scrolled-off-screen Sparkline => 0 damage & measured_nodes == 0
        let screen = Rect::new(0, 0, 120, 60);
        let mut t1 = RetainedTree::new();
        let mut t2 = RetainedTree::new();
        let mut anims = AnimPool::new();
        let mut dmg = DamageSet::new();

        let build_scroll_with_spark = |draft: &mut ViewDraft, s_vis: &[i16], s_off: &[i16]| {
            let mut vb = ViewBuilder::new(draft);
            vb.root(
                Key(1),
                NodeProps {
                    width: Length::Px(120),
                    height: Length::Px(60),
                    ..NodeProps::new()
                },
                |ui| {
                    ui.scroll_y(
                        Key(2),
                        NodeProps {
                            width: Length::Fill(1),
                            height: Length::Fill(1),
                            ..NodeProps::new()
                        },
                        |ui| {
                            ui.sparkline(
                                Key(10),
                                s_vis,
                                NodeProps {
                                    width: Length::Px(100),
                                    height: Length::Px(40),
                                    min: 0,
                                    max: 100,
                                    ..NodeProps::new()
                                },
                            )?;
                            ui.label(
                                Key(11),
                                NodeProps {
                                    width: Length::Px(100),
                                    height: Length::Px(80),
                                    ..NodeProps::new()
                                },
                            )?;
                            ui.sparkline(
                                Key(12),
                                s_off,
                                NodeProps {
                                    width: Length::Px(100),
                                    height: Length::Px(40),
                                    min: 0,
                                    max: 100,
                                    ..NodeProps::new()
                                },
                            )
                        },
                    )
                },
            )
            .unwrap();
            vb.finish().unwrap();
        };

        build_scroll_with_spark(&mut draft, &[10, 20, 30], &[5, 15, 25]);
        reconcile_into(
            &draft,
            &RetainedTree::new(),
            &mut t1,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(0),
            &mut dmg,
        )
        .unwrap();
        assert!(!t1.node_by_key(Key(10)).unwrap().visible_rect.is_empty());
        assert!(t1.node_by_key(Key(12)).unwrap().visible_rect.is_empty());

        // Visible sample update => measured_nodes == 0 and damage == visible sparkline rect
        dmg.clear();
        build_scroll_with_spark(&mut draft, &[10, 80, 30], &[5, 15, 25]);
        let out_vis = reconcile_into(
            &draft,
            &t1,
            &mut t2,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(10),
            &mut dmg,
        )
        .unwrap();
        assert_eq!(out_vis.measured_nodes, 0);
        assert_eq!(
            dmg.as_slice(),
            &[t2.node_by_key(Key(10)).unwrap().visible_rect]
        );

        // Off-screen sample update => measured_nodes == 0 and dmg is empty!
        dmg.clear();
        build_scroll_with_spark(&mut draft, &[10, 80, 30], &[99, 1, 50]);
        let out_off = reconcile_into(
            &draft,
            &t2,
            &mut t1,
            screen,
            &FixedMeasurer,
            &mut anims,
            None,
            None,
            InstantMs(20),
            &mut dmg,
        )
        .unwrap();
        assert_eq!(out_off.measured_nodes, 0);
        assert!(dmg.is_empty());
    }

    #[test]
    fn easing_linear_and_cubic_endpoints_midpoint_875_reverse_and_pool_full() {
        use crate::capacity::MAX_ANIMATIONS;
        use crate::time::Easing;

        assert_eq!(Easing::Linear.apply(0), 0);
        assert_eq!(Easing::Linear.apply(500), 500);
        assert_eq!(Easing::Linear.apply(1000), 1000);

        assert_eq!(Easing::EaseOutCubic.apply(0), 0);
        assert_eq!(Easing::EaseOutCubic.apply(500), 875);
        assert_eq!(Easing::EaseOutCubic.apply(1000), 1000);

        let mut pool = AnimPool::new();
        let r = Rect::new(0, 0, 48, 24);
        pool.start_with_easing(
            Key(50),
            AnimProp::ToggleKnob,
            0,
            ANIM_SCALE,
            InstantMs(0),
            160,
            r,
            Easing::EaseOutCubic,
        );
        assert_eq!(
            pool.current_value(Key(50), AnimProp::ToggleKnob, InstantMs(0)),
            Some(0)
        );
        assert_eq!(
            pool.current_value(Key(50), AnimProp::ToggleKnob, InstantMs(80)),
            Some(875)
        );
        assert_eq!(
            pool.current_value(Key(50), AnimProp::ToggleKnob, InstantMs(160)),
            Some(1000)
        );

        // Reverse midway at 80ms from visual value 875 back toward 0 with EaseOutCubic
        pool.start_with_easing(
            Key(50),
            AnimProp::ToggleKnob,
            ANIM_SCALE,
            0,
            InstantMs(80),
            160,
            r,
            Easing::EaseOutCubic,
        );
        assert_eq!(
            pool.current_value(Key(50), AnimProp::ToggleKnob, InstantMs(80)),
            Some(875)
        );
        // Halfway through reverse (elapsed=80/160 -> eased=875/1000): 875 + (-875 * 875)/1000 = 875 - 765 = 110
        assert_eq!(
            pool.current_value(Key(50), AnimProp::ToggleKnob, InstantMs(160)),
            Some(110)
        );
        assert_eq!(
            pool.current_value(Key(50), AnimProp::ToggleKnob, InstantMs(240)),
            Some(0)
        );

        // Linear midpoint check remains exact 500
        pool.start_with_easing(
            Key(51),
            AnimProp::ToggleKnob,
            0,
            ANIM_SCALE,
            InstantMs(0),
            160,
            r,
            Easing::Linear,
        );
        assert_eq!(
            pool.current_value(Key(51), AnimProp::ToggleKnob, InstantMs(80)),
            Some(500)
        );

        // Pool exhaustion: inserting past MAX_ANIMATIONS jumps straight to end value cleanly
        let mut full = AnimPool::new();
        for i in 0..MAX_ANIMATIONS {
            full.start_with_easing(
                Key(100 + i as u32),
                AnimProp::ToggleKnob,
                0,
                ANIM_SCALE,
                InstantMs(0),
                160,
                r,
                Easing::EaseOutCubic,
            );
        }
        full.start_with_easing(
            Key(999),
            AnimProp::ToggleKnob,
            0,
            ANIM_SCALE,
            InstantMs(0),
            160,
            r,
            Easing::EaseOutCubic,
        );
        assert_eq!(
            full.current_value(Key(999), AnimProp::ToggleKnob, InstantMs(1)),
            Some(ANIM_SCALE)
        );
    }
}
