use core::convert::Infallible;

use embedded_graphics_core::Pixel;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use liuli::prelude::*;

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

struct VecTarget {
    w: u32,
    h: u32,
    fb: Vec<u16>,
}

impl VecTarget {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            fb: vec![0; (w * h) as usize],
        }
    }
}

impl OriginDimensions for VecTarget {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for VecTarget {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.x < self.w as i32 && p.y >= 0 && p.y < self.h as i32 {
                self.fb[(p.y as usize) * (self.w as usize) + (p.x as usize)] = c.into_storage();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum FaultMode {
    Normal,
    DuplicateKey,
    UnmatchedEnd,
    DepthOverflow,
    TextOverflow,
    NodeOverflow,
    KindChangedToLabel,
}

struct FaultModel {
    mode: FaultMode,
    counter: u16,
}

#[derive(Eq, PartialEq, Debug)]
enum FaultMsg {
    SetMode(FaultMode),
}

struct FaultApp;

impl App for FaultApp {
    type Model = FaultModel;
    type Msg = FaultMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            FaultMsg::SetMode(m) => {
                model.mode = m;
                model.counter += 1;
                Change::View
            }
        }
    }

    fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let mut root_p = NodeProps::new();
        root_p.bg = Some(Rgb565::new(2, 4, 2));
        root_p.width = Length::Px(120);
        root_p.height = Length::Px(80);
        out.begin(Key(1), NodeKind::Root, root_p)?;

        match model.mode {
            FaultMode::Normal => {
                let mut col_p = NodeProps::new();
                col_p.width = Length::Fill(1);
                col_p.height = Length::Fill(1);
                out.begin(Key(2), NodeKind::Column, col_p)?;

                let mut bp = NodeProps::new();
                bp.width = Length::Px(80);
                bp.height = Length::Px(30);
                bp.bg = Some(Rgb565::new(8, 20, 12));
                bp.fg = Rgb565::new(31, 63, 31);
                bp.focusable = true;
                bp.text = out.text_fmt(format_args!("OK {}", model.counter))?;
                out.leaf(Key(10), NodeKind::Button, bp)?;

                out.end()?;
                out.end()?;
                Ok(())
            }
            FaultMode::DuplicateKey => {
                out.leaf(Key(10), NodeKind::Label, NodeProps::new())?;
                out.leaf(Key(10), NodeKind::Label, NodeProps::new())?;
                out.end()
            }
            FaultMode::UnmatchedEnd => {
                out.leaf(Key(10), NodeKind::Label, NodeProps::new())?;
                // Missing out.end() for Root
                Ok(())
            }
            FaultMode::DepthOverflow => {
                for i in 0..MAX_DEPTH {
                    out.begin(Key(100 + i as u32), NodeKind::Column, NodeProps::new())?;
                }
                Ok(())
            }
            FaultMode::TextOverflow => {
                let mut p = NodeProps::new();
                let s = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
                for i in 0..20 {
                    p.text = out.text_fmt(format_args!("{s}{s}{i}"))?;
                }
                out.leaf(Key(10), NodeKind::Label, p)?;
                out.end()
            }
            FaultMode::NodeOverflow => {
                for i in 0..MAX_NODES {
                    out.leaf(Key(200 + i as u32), NodeKind::Label, NodeProps::new())?;
                }
                out.end()
            }
            FaultMode::KindChangedToLabel => {
                let mut lp = NodeProps::new();
                lp.width = Length::Px(80);
                lp.height = Length::Px(30);
                lp.text = TextRef::Static("Label");
                out.leaf(Key(10), NodeKind::Label, lp)?;
                out.end()
            }
        }
    }

    fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
        None
    }
}

#[test]
fn failed_transactions_keep_active_frame_and_recover() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 120, 80);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; 120 * 8];
    let mut target = VecTarget::new(120, 80);

    let mut engine = Engine::<FaultApp>::new_in_place(
        &mut storage,
        FaultModel {
            mode: FaultMode::Normal,
            counter: 0,
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();

    // Start pointer capture on button Key(10)
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(20, 15),
            },
            InstantMs(10),
            false,
        )
        .unwrap();
    engine.present_sync(&mut target, &mut scratch).unwrap();
    let baseline_fb = target.fb.clone();
    let id_before = engine.active_tree().get_id(Key(10)).unwrap();
    assert_eq!(engine.last_build_error(), None);

    for (mode, expected_err) in [
        (FaultMode::DuplicateKey, BuildError::DuplicateKey),
        (FaultMode::UnmatchedEnd, BuildError::UnclosedContainer),
        (FaultMode::DepthOverflow, BuildError::Depth),
        (FaultMode::TextOverflow, BuildError::TextBytes),
        (FaultMode::NodeOverflow, BuildError::Nodes),
    ] {
        engine
            .message(FaultMsg::SetMode(mode), InstantMs(20), false)
            .unwrap();
        assert!(engine.view_pending());
        assert_eq!(engine.last_build_error(), Some(expected_err));
        assert_eq!(
            engine
                .try_message(FaultMsg::SetMode(mode), InstantMs(20), false)
                .unwrap_err(),
            TransactionError::Build(expected_err)
        );
        assert_eq!(engine.last_build_error(), Some(expected_err));
        // Must NOT spin NextWakeup::Now when damage is empty & view failed!
        assert_eq!(
            engine.next_wakeup(InstantMs(20), false),
            NextWakeup::OnEvent
        );
        assert_eq!(engine.active_tree().get_id(Key(10)), Some(id_before));
        assert!(engine.pointer().captured_slot().is_some());
        assert!(engine.damage().is_empty());

        // Re-rendering active snapshot matches pixel-for-pixel
        let mut check_target = VecTarget::new(120, 80);
        let mut check_dmg = DamageSet::new();
        check_dmg.mark_full_screen(screen);
        liuli::render::present_sync(
            &mut check_target,
            &mut scratch,
            screen,
            8,
            engine.active_commands(),
            &mut check_dmg,
            engine.active_tree(),
            engine.fonts(),
        )
        .unwrap();
        assert_eq!(check_target.fb, baseline_fb);
    }

    // Recover with valid build
    engine
        .try_message(FaultMsg::SetMode(FaultMode::Normal), InstantMs(30), false)
        .unwrap();
    assert!(!engine.view_pending());
    assert_eq!(engine.last_build_error(), None);
    assert_eq!(engine.active_tree().get_id(Key(10)), Some(id_before));

    // Kind change clears pointer capture and issues new generation
    engine
        .try_message(
            FaultMsg::SetMode(FaultMode::KindChangedToLabel),
            InstantMs(40),
            false,
        )
        .unwrap();
    assert!(engine.pointer().captured_slot().is_none());
    let id_after = engine.active_tree().get_id(Key(10)).unwrap();
    assert_ne!(id_before, id_after);
    assert!(!engine.active_tree().is_id_valid(id_before));
    assert_eq!(engine.last_build_error(), None);
}

#[test]
fn try_dispatch_message_and_advance_surface_busy_time_and_build_errors() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 120, 80);
    let mut storage = Box::new(Storage::new());
    let mut engine = Engine::<FaultApp>::new_in_place(
        &mut storage,
        FaultModel {
            mode: FaultMode::Normal,
            counter: 0,
        },
        font,
        screen,
        InstantMs(100),
    )
    .unwrap();

    // DMA busy returns Busy(msg/input) without touching state
    assert!(matches!(
        engine.try_message(
            FaultMsg::SetMode(FaultMode::DuplicateKey),
            InstantMs(105),
            true
        ),
        Err(TransactionError::Busy(FaultMsg::SetMode(
            FaultMode::DuplicateKey
        )))
    ));
    assert_eq!(engine.last_build_error(), None);

    assert!(matches!(
        engine.try_dispatch(RawInput::Activate, InstantMs(105), true),
        Err(TransactionError::Busy(RawInput::Activate))
    ));

    // Backwards time returns WentBackwards
    assert_eq!(
        engine.try_message(FaultMsg::SetMode(FaultMode::Normal), InstantMs(99), false),
        Err(TransactionError::WentBackwards)
    );
    assert_eq!(
        engine.try_dispatch(RawInput::Activate, InstantMs(99), false),
        Err(TransactionError::WentBackwards)
    );
    assert_eq!(
        engine.try_advance(InstantMs(99), false),
        Err(TransactionError::WentBackwards)
    );

    // Clear initial damage then fail via try_message -> next_wakeup stays OnEvent
    let mut target = VecTarget::new(120, 80);
    let mut scratch = [0u16; 120 * 8];
    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert!(engine.damage().is_empty());

    assert_eq!(
        engine.try_message(
            FaultMsg::SetMode(FaultMode::DepthOverflow),
            InstantMs(110),
            false
        ),
        Err(TransactionError::Build(BuildError::Depth))
    );
    assert!(engine.view_pending());
    assert_eq!(engine.last_build_error(), Some(BuildError::Depth));
    assert_eq!(
        engine.next_wakeup(InstantMs(110), false),
        NextWakeup::OnEvent
    );

    // try_advance with valid time succeeds and leaves last_build_error untouched while OnEvent holds
    assert_eq!(engine.try_advance(InstantMs(120), false), Ok(false));
    assert_eq!(engine.last_build_error(), Some(BuildError::Depth));

    // Valid message recovers cleanly
    engine
        .try_message(FaultMsg::SetMode(FaultMode::Normal), InstantMs(130), false)
        .unwrap();
    assert!(!engine.view_pending());
    assert_eq!(engine.last_build_error(), None);
}

struct DeferredDmaSink<'a> {
    in_flight: Option<InFlightBand<'a>>,
    ready_to_finish: bool,
    reject_next: bool,
}

impl<'a> DeferredDmaSink<'a> {
    fn new() -> Self {
        Self {
            in_flight: None,
            ready_to_finish: false,
            reject_next: false,
        }
    }
}

impl<'a> DmaSink<'a> for DeferredDmaSink<'a> {
    type Error = &'static str;

    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error> {
        if self.reject_next {
            self.reject_next = false;
            return StartOutcome::Rejected("bus err", band);
        }
        if self.in_flight.is_some() {
            return StartOutcome::Busy(band);
        }
        self.in_flight = Some(band.into_in_flight());
        self.ready_to_finish = false;
        StartOutcome::Started
    }

    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error> {
        if !self.ready_to_finish {
            return PollOutcome::Pending;
        }
        let Some(band) = self.in_flight.take() else {
            return PollOutcome::Pending;
        };
        let m = band.meta();
        PollOutcome::Completed {
            slot: m.slot,
            frame: m.frame,
            seq: m.seq,
            band: band.confirm_stopped(),
        }
    }
}

#[test]
fn engine_dma_frame_lock_and_metrics_prevent_bypass() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 120, 80);
    let mut storage = Box::new(Storage::new());
    let mut engine = Engine::<FaultApp>::new_in_place(
        &mut storage,
        FaultModel {
            mode: FaultMode::Normal,
            counter: 0,
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();

    let expected_measured = engine.last_metrics().measured_nodes;
    assert!(expected_measured > 0);

    let mut band_a = [0u16; 120 * 8];
    let mut band_b = [0u16; 120 * 8];
    let mut dma = DmaCoordinator::new(&mut band_a, &mut band_b);
    let mut sink = DeferredDmaSink::new();

    // First DMA poll starts frame and leaves engine locked (Pending).
    let st0 = engine.poll_dma(&mut dma, &mut sink).unwrap();
    assert_eq!(st0, PresentStatus::Pending);
    assert!(engine.is_dma_busy());
    assert_eq!(engine.last_metrics().measured_nodes, expected_measured);

    // Every mutating entry MUST refuse or defer while DMA is active — even with dma_busy = false!
    assert_eq!(
        engine.try_dispatch(RawInput::Activate, InstantMs(10), false),
        Err(TransactionError::Busy(RawInput::Activate))
    );
    assert_eq!(
        engine.dispatch(RawInput::Activate, InstantMs(10), false),
        Err(DispatchError::Busy(RawInput::Activate))
    );
    assert_eq!(
        engine.try_message(FaultMsg::SetMode(FaultMode::Normal), InstantMs(10), false),
        Err(TransactionError::Busy(FaultMsg::SetMode(FaultMode::Normal)))
    );
    assert_eq!(
        engine.message(FaultMsg::SetMode(FaultMode::Normal), InstantMs(10), false),
        Err(DispatchError::Busy(FaultMsg::SetMode(FaultMode::Normal)))
    );
    assert_eq!(
        engine.commit_view(InstantMs(10)),
        Err(TransactionError::Busy(()))
    );
    assert_eq!(engine.reemit_commands(), Err(TransactionError::Busy(())));
    assert_eq!(
        engine.try_set_focus_ring_visible(true),
        Err(TransactionError::Busy(()))
    );
    assert!(!engine.focus().ring_visible);
    assert!(matches!(
        engine.try_model_mut(),
        Err(TransactionError::Busy(()))
    ));
    assert!(matches!(
        engine.try_with_model(|m| m.counter += 1),
        Err(TransactionError::Busy(()))
    ));
    assert_eq!(engine.model().counter, 0);

    let mut sync_target = VecTarget::new(120, 80);
    let mut scratch = [0u16; 120 * 8];
    assert_eq!(
        engine.present_sync(&mut sync_target, &mut scratch),
        Err(PresentError::DmaBusy)
    );

    // Foreign DmaCoordinator is rejected while locked.
    let mut foreign_a = [0u16; 120 * 8];
    let mut foreign_b = [0u16; 120 * 8];
    let mut foreign_dma = DmaCoordinator::new(&mut foreign_a, &mut foreign_b);
    assert_eq!(
        engine.poll_dma(&mut foreign_dma, &mut sink),
        Err(PresentError::DmaBusy)
    );

    // try_advance with dma_busy=false still records clock & timer_pending_after_dma without committing!
    assert_eq!(engine.try_advance(InstantMs(25), false), Ok(false));
    assert_eq!(
        engine.next_wakeup(InstantMs(25), false),
        NextWakeup::OnEvent
    );

    // Multiple Pending polls must NOT double-count measured_nodes.
    sink.ready_to_finish = true;
    let st1 = engine.poll_dma(&mut dma, &mut sink).unwrap();
    assert_eq!(st1, PresentStatus::Pending);
    assert_eq!(engine.last_metrics().measured_nodes, expected_measured);

    // Drive DMA frame to completion.
    while engine.poll_dma(&mut dma, &mut sink).unwrap() != PresentStatus::Presented {
        sink.ready_to_finish = true;
    }
    assert!(!engine.is_dma_busy());
    assert_eq!(engine.last_metrics().measured_nodes, expected_measured);
    assert_eq!(engine.next_wakeup(InstantMs(25), false), NextWakeup::Now);

    // Backwards time relative to time recorded during DMA fails!
    assert_eq!(
        engine.try_advance(InstantMs(24), false),
        Err(TransactionError::WentBackwards)
    );

    // Deferred advance runs once, after which Idle poll sees measured_nodes == 0.
    assert_eq!(engine.try_advance(InstantMs(30), false), Ok(false));
    assert_eq!(
        engine.next_wakeup(InstantMs(30), false),
        NextWakeup::OnEvent
    );
    assert_eq!(
        engine.poll_dma(&mut dma, &mut sink).unwrap(),
        PresentStatus::Idle
    );
    assert_eq!(engine.last_metrics().measured_nodes, 0);

    // Now mutations & try_with_model work again; Rejection unlocks cleanly.
    engine.try_with_model(|m| m.counter += 7).unwrap();
    assert_eq!(engine.model().counter, 7);
    engine.try_set_focus_ring_visible(true).unwrap();
    assert!(engine.focus().ring_visible);

    sink.reject_next = true;
    assert_eq!(
        engine.poll_dma(&mut dma, &mut sink),
        Err(PresentError::Target("bus err"))
    );
    assert!(!engine.is_dma_busy());
    assert!(engine.damage().is_full_screen());
}

struct RepeatModel {
    page: u8,
    count: i32,
    slider: i16,
    btn_enabled: bool,
    modal_open: bool,
    fail_page: bool,
    page_mem: [(Option<Key>, i32); 2],
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum RepeatMsg {
    Inc,
    SetSlider(i16),
    SwitchPage(u8),
    OpenModal,
}

struct RepeatApp;

impl App for RepeatApp {
    type Model = RepeatModel;
    type Msg = RepeatMsg;

    fn page_id(model: &Self::Model) -> u8 {
        model.page
    }

    fn is_tab_key(key: Key) -> bool {
        key == Key(200)
    }

    fn page_restore(model: &Self::Model, page: u8) -> PageRestore {
        let (fk, sy) = model.page_mem[(page as usize).min(1)];
        PageRestore {
            scroll_key: Some(Key(2)),
            scroll_y: sy,
            focus_key: fk,
        }
    }

    fn on_page_committed(
        model: &mut Self::Model,
        old_page: u8,
        saved_focus: Option<Key>,
        _saved_scroll_key: Option<Key>,
        saved_scroll_y: i32,
    ) {
        model.page_mem[(old_page as usize).min(1)] = (saved_focus, saved_scroll_y);
    }

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            RepeatMsg::Inc => {
                model.count += 1;
                Change::View
            }
            RepeatMsg::SetSlider(v) => {
                model.slider = v.clamp(0, 100);
                Change::View
            }
            RepeatMsg::SwitchPage(p) => {
                model.page = p;
                Change::View
            }
            RepeatMsg::OpenModal => {
                model.modal_open = true;
                Change::View
            }
        }
    }

    fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let mut root_p = NodeProps::new();
        root_p.width = Length::Fill(1);
        root_p.height = Length::Fill(1);
        out.root(Key(1), root_p, |ui| {
            ui.stack(
                Key(90),
                NodeProps {
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    ..NodeProps::new()
                },
                |ui| {
                    if model.page == 1 && model.fail_page {
                        ui.label(Key(1), NodeProps::new())?; // duplicate key triggers rollback
                    }
                    ui.column(
                        Key(91),
                        NodeProps {
                            width: Length::Fill(1),
                            height: Length::Fill(1),
                            ..NodeProps::new()
                        },
                        |ui| {
                            let mut tab = NodeProps::new();
                            tab.width = Length::Px(80);
                            tab.height = Length::Px(20);
                            tab.focusable = true;
                            tab.text = TextRef::Static("Tab");
                            ui.button(Key(200), tab)?;

                            ui.scroll_y(
                                Key(2),
                                NodeProps {
                                    width: Length::Fill(1),
                                    height: Length::Px(80),
                                    ..NodeProps::new()
                                },
                                |ui| {
                                    // Repeatable button at top of scroll (y=20..50)
                                    let mut rp = NodeProps::new();
                                    rp.width = Length::Px(100);
                                    rp.height = Length::Px(30);
                                    rp.focusable = true;
                                    rp.enabled = model.btn_enabled;
                                    rp.repeatable = true;
                                    rp.text = TextRef::Static("+");
                                    ui.button(Key(10), rp)?;

                                    // Non-repeatable button (y=50..80)
                                    let mut nrp = NodeProps::new();
                                    nrp.width = Length::Px(100);
                                    nrp.height = Length::Px(30);
                                    nrp.focusable = true;
                                    nrp.repeatable = false;
                                    nrp.text = TextRef::Static("Once");
                                    ui.button(Key(11), nrp)?;

                                    // Slider (y=80..104)
                                    let mut sl = NodeProps::new();
                                    sl.width = Length::Px(100);
                                    sl.height = Length::Px(24);
                                    sl.min = 0;
                                    sl.max = 100;
                                    sl.step = 5;
                                    sl.value = model.slider;
                                    sl.focusable = true;
                                    ui.slider(Key(12), sl)?;

                                    // Tall spacer so ScrollY overflows
                                    ui.label(
                                        Key(13),
                                        NodeProps {
                                            width: Length::Px(100),
                                            height: Length::Px(60),
                                            ..NodeProps::new()
                                        },
                                    )
                                },
                            )
                        },
                    )?;

                    if model.modal_open {
                        ui.modal(
                            Key(300),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Fill(1),
                                overlay_alpha: 8,
                                ..NodeProps::new()
                            },
                            |ui| {
                                ui.button(
                                    Key(301),
                                    NodeProps {
                                        width: Length::Px(60),
                                        height: Length::Px(24),
                                        focusable: true,
                                        text: TextRef::Static("OK"),
                                        ..NodeProps::new()
                                    },
                                )
                            },
                        )?;
                    }
                    Ok(())
                },
            )
        })
    }

    fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
        match (event.key, event.kind) {
            (Key(10) | Key(11), ActionKind::Activate) => Some(RepeatMsg::Inc),
            (Key(12), ActionKind::SetValue(v)) => Some(RepeatMsg::SetSlider(v)),
            (Key(200), ActionKind::Activate) => Some(RepeatMsg::SwitchPage(1)),
            _ => None,
        }
    }
}

#[test]
fn long_press_repeat_timing_bounding_and_cancellation() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 120, 120);
    let mut storage = Box::new(Storage::new());
    let mut engine = Engine::<RepeatApp>::new_in_place(
        &mut storage,
        RepeatModel {
            page: 0,
            count: 0,
            slider: 50,
            btn_enabled: true,
            modal_open: false,
            fail_page: false,
            page_mem: [(None, 0); 2],
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();
    let mut scr = VecTarget::new(120, 120);
    let mut scratch = [0u16; 120 * 8];
    engine.present_sync(&mut scr, &mut scratch).unwrap();

    // 1. PointerDown on repeatable button Key(10) at y=30
    engine
        .try_dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(40, 30),
            },
            InstantMs(0),
            false,
        )
        .unwrap();
    engine.present_sync(&mut scr, &mut scratch).unwrap();
    assert!(engine.repeat().is_active());
    assert_eq!(
        engine.next_wakeup(InstantMs(0), false),
        NextWakeup::At(InstantMs(REPEAT_DELAY_MS))
    );

    // At 399 ms: NOT fired yet
    assert!(!engine.try_advance(InstantMs(399), false).unwrap());
    assert_eq!(engine.model().count, 0);

    // At 400 ms: first repeat fires -> 1, next wakeup at 480 ms
    assert!(engine.try_advance(InstantMs(400), false).unwrap());
    engine.present_sync(&mut scr, &mut scratch).unwrap();
    assert_eq!(engine.model().count, 1);
    assert_eq!(
        engine.next_wakeup(InstantMs(400), false),
        NextWakeup::At(InstantMs(480))
    );

    // At 480 ms: second repeat fires -> 2
    assert!(engine.try_advance(InstantMs(480), false).unwrap());
    assert_eq!(engine.model().count, 2);

    // Releasing after repeat fired must cancel repeat WITHOUT firing an extra duplicate click!
    engine
        .try_dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(40, 30),
            },
            InstantMs(490),
            false,
        )
        .unwrap();
    assert!(!engine.repeat().is_active());
    assert_eq!(engine.model().count, 2);

    // 2. KeyDown(Activate) on repeatable button + huge time jump bounded by MAX_REPEAT_STEPS_PER_ADVANCE (8)
    engine
        .try_dispatch(RawInput::KeyDown(HoldKey::Activate), InstantMs(1000), false)
        .unwrap();
    assert_eq!(engine.model().count, 3);
    assert!(engine.try_advance(InstantMs(10_000), false).unwrap());
    assert_eq!(
        engine.model().count,
        3 + (MAX_REPEAT_STEPS_PER_ADVANCE as i32)
    );
    engine
        .try_dispatch(RawInput::KeyUp(HoldKey::Activate), InstantMs(10_010), false)
        .unwrap();
    assert!(!engine.repeat().is_active());

    // 3. Cancellation by drag past DRAG_THRESHOLD_PX
    engine
        .try_dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(40, 30),
            },
            InstantMs(10_100),
            false,
        )
        .unwrap();
    assert!(engine.repeat().is_active());
    engine
        .try_dispatch(
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(40, 15),
            },
            InstantMs(10_110),
            false,
        )
        .unwrap();
    assert!(!engine.repeat().is_active());
    engine
        .try_dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(40, 15),
            },
            InstantMs(10_120),
            false,
        )
        .unwrap();
    // Scroll back to 0 so button stays visible for remaining subtests
    let before_count = engine.model().count;
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(10_130), false)
        .unwrap();

    // 4. Non-repeatable button Key(11): MUST NOT arm repeat!
    assert_eq!(engine.focus().focused_key(), Some(Key(11)));
    engine
        .try_dispatch(
            RawInput::KeyDown(HoldKey::Activate),
            InstantMs(10_200),
            false,
        )
        .unwrap();
    assert!(!engine.repeat().is_active());
    assert!(!engine.try_advance(InstantMs(11_000), false).unwrap());
    assert_eq!(engine.model().count, before_count + 1);
    engine
        .try_dispatch(RawInput::KeyUp(HoldKey::Activate), InstantMs(11_010), false)
        .unwrap();

    // 5. Slider editing repeat + cancel on Back & PointerCancel & Modal open & Target disabled
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(11_100), false)
        .unwrap();
    assert_eq!(engine.focus().focused_key(), Some(Key(12)));
    engine
        .try_dispatch(RawInput::Activate, InstantMs(11_110), false)
        .unwrap();
    assert!(engine.focus().editing);
    engine
        .try_dispatch(
            RawInput::KeyDown(HoldKey::AdjustNext),
            InstantMs(11_200),
            false,
        )
        .unwrap();
    assert_eq!(engine.model().slider, 55);
    assert!(engine.repeat().is_active());
    engine.try_advance(InstantMs(11_600), false).unwrap();
    assert_eq!(engine.model().slider, 60);
    engine
        .try_dispatch(RawInput::Back, InstantMs(11_620), false)
        .unwrap();
    assert!(!engine.focus().editing);
    assert!(!engine.repeat().is_active());

    // Target disabled while held cancels repeat cleanly
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(11_700), false)
        .unwrap(); // Tab 200
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(11_710), false)
        .unwrap(); // Button 10
    assert_eq!(engine.focus().focused_key(), Some(Key(10)));
    engine
        .try_dispatch(
            RawInput::KeyDown(HoldKey::Activate),
            InstantMs(11_800),
            false,
        )
        .unwrap();
    assert!(engine.repeat().is_active());
    engine.try_with_model(|m| m.btn_enabled = false).unwrap();
    engine.commit_view(InstantMs(11_850)).unwrap();
    assert!(!engine.repeat().is_active());

    // Re-enable button, hold, then open Modal -> cancels repeat
    engine.try_with_model(|m| m.btn_enabled = true).unwrap();
    engine.commit_view(InstantMs(11_900)).unwrap();
    engine
        .try_dispatch(
            RawInput::PointerCancel { id: PointerId(1) },
            InstantMs(11_910),
            false,
        )
        .unwrap();
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(11_930), false)
        .unwrap();
    assert_eq!(engine.focus().focused_key(), Some(Key(10)));
    engine
        .try_dispatch(
            RawInput::KeyDown(HoldKey::Activate),
            InstantMs(12_000),
            false,
        )
        .unwrap();
    assert!(engine.repeat().is_active());
    engine
        .try_message(RepeatMsg::OpenModal, InstantMs(12_050), false)
        .unwrap();
    assert!(!engine.repeat().is_active());
}

#[test]
fn failed_page_switch_preserves_old_page_memory_focus_and_repeat() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 120, 120);
    let mut storage = Box::new(Storage::new());
    let mut engine = Engine::<RepeatApp>::new_in_place(
        &mut storage,
        RepeatModel {
            page: 0,
            count: 0,
            slider: 50,
            btn_enabled: true,
            modal_open: false,
            fail_page: true,
            page_mem: [(None, 0); 2],
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();
    let mut scr = VecTarget::new(120, 120);
    let mut scratch = [0u16; 120 * 8];
    engine.present_sync(&mut scr, &mut scratch).unwrap();

    // Move to slider Key(12) and capture baseline right before failed page switch
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(10), false)
        .unwrap();
    engine
        .try_dispatch(RawInput::Nav(NavDir::Next), InstantMs(20), false)
        .unwrap();
    assert_eq!(engine.focus().focused_key(), Some(Key(12)));
    engine.present_sync(&mut scr, &mut scratch).unwrap();
    let baseline = scr.fb.clone();

    // Failed page switch returns BuildError::DuplicateKey and preserves page_mem & active frame!
    let err = engine
        .try_message(RepeatMsg::SwitchPage(1), InstantMs(30), false)
        .unwrap_err();
    assert_eq!(err, TransactionError::Build(BuildError::DuplicateKey));
    assert_eq!(engine.model().page_mem[0], (None, 0));
    assert_eq!(engine.focus().focused_key(), Some(Key(12)));

    let mut check_scr = VecTarget::new(120, 120);
    let mut check_dmg = DamageSet::new();
    check_dmg.mark_full_screen(screen);
    liuli::render::present_sync(
        &mut check_scr,
        &mut scratch,
        screen,
        8,
        engine.active_commands(),
        &mut check_dmg,
        engine.active_tree(),
        engine.fonts(),
    )
    .unwrap();
    assert_eq!(check_scr.fb, baseline);
}
