use liuli::prelude::*;
use liuli::render::InFlightBand;
use liuli_settings::{
    SettingsApp, SettingsModel, TerminalApp, TerminalModel, app_fonts, keys, settings_font,
    terminal_keys as tk,
};
use liuli_sim::screen::{MemoryScreen, render_reference_fullscreen};

fn assert_matches_reference<A: App>(engine: &Engine<'_, '_, A>, screen: &MemoryScreen, step: &str) {
    let ref_scr = render_reference_fullscreen(
        engine.active_commands(),
        engine.active_tree(),
        engine.fonts(),
    );
    if let Some((x, y, got, exp)) = screen.diff_first_mismatch(&ref_scr) {
        panic!(
            "step '{step}': incremental output diverged from fullscreen reference at ({x},{y}): got 0x{got:04X}, expected 0x{exp:04X}"
        );
    }
}

#[test]
fn incremental_damage_transfers_and_1000_step_fuzz_match_reference() {
    let font = settings_font();
    let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    let mut engine = Engine::<SettingsApp>::new_in_place(
        &mut storage,
        SettingsModel::new(InstantMs(0)),
        font,
        screen_rect,
        InstantMs(0),
    )
    .unwrap();
    engine.set_focus_ring_visible(true);

    // 1. Initial frame must send exact full screen (76,800 pixels, 30 bands of height 8)
    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert_eq!(engine.last_metrics().pixels_sent, 76_800);
    assert_eq!(engine.last_metrics().bands, 30);
    assert_matches_reference(&engine, &screen, "initial");

    // 2. Idle re-present does nothing
    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Idle);
    assert_eq!(engine.last_metrics().bands, 0);
    assert_eq!(engine.last_metrics().pixels_sent, 0);

    // 3. Temperature update while scrolled at 0 (temp row 218..258 is clipped below viewport 32..216):
    //    must not relayout (measured_nodes == 0) and must emit 0 damage / 0 bands / 0 pixels.
    let changed = engine.advance(InstantMs(1000), false).unwrap();
    assert!(changed);
    assert_eq!(engine.model().temp_deci_c, 231);
    assert_eq!(engine.last_metrics().measured_nodes, 0);
    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Idle);
    assert_eq!(engine.last_metrics().bands, 0);
    assert_eq!(engine.last_metrics().pixels_sent, 0);
    assert_matches_reference(&engine, &screen, "offscreen-temp");

    // 4. Scroll down to BTN_RESET via 4 Nav(Next) steps: header (<32) and footer (>=216) must stay outside damage
    let mut now_ms = 1010u64;
    for _ in 0..4 {
        engine
            .dispatch(RawInput::Nav(NavDir::Next), InstantMs(now_ms), false)
            .unwrap();
        now_ms += 10;
    }
    assert_eq!(engine.focus().focused_key(), Some(keys::BTN_RESET));
    let scroll_y = engine
        .active_tree()
        .node_by_key(keys::SCROLL)
        .unwrap()
        .scroll_y;
    assert_eq!(scroll_y, 91);
    for &d in engine.damage().as_slice() {
        assert!(d.y >= 32, "scroll dirtied fixed header: {d:?}");
        assert!(d.bottom64() <= 216, "scroll dirtied fixed footer: {d:?}");
    }
    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert_matches_reference(&engine, &screen, "scrolled-to-reset");

    // 5. Temperature update while temp row IS visible: must keep measured_nodes == 0 and pixels_sent <= 320 * 40
    let changed = engine.advance(InstantMs(2000), false).unwrap();
    assert!(changed);
    assert_eq!(engine.model().temp_deci_c, 232);
    assert_eq!(engine.last_metrics().measured_nodes, 0);
    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert!(engine.last_metrics().pixels_sent > 0);
    assert!(engine.last_metrics().pixels_sent <= 320 * 40);
    assert_matches_reference(&engine, &screen, "visible-temp");

    // 6. Focus-only Nav while both controls are visible: partial damage only
    engine
        .dispatch(RawInput::Nav(NavDir::Prev), InstantMs(2010), false)
        .unwrap();
    assert_eq!(engine.focus().focused_key(), Some(keys::TOGGLE_MONITOR));
    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert!(
        engine.last_metrics().pixels_sent < 320 * 120,
        "focus-only move sent too many pixels: {}",
        engine.last_metrics().pixels_sent
    );
    assert_matches_reference(&engine, &screen, "focus-only");

    // 7. 1,000-step deterministic fixed-seed mixed input + timer fuzz sequence
    let mut seed: u64 = 0x4C49_554C_495F_3031;
    let mut next_u64 = || -> u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    now_ms = 2020;
    let mut down = false;
    let mut cur_x = 160i32;
    let mut cur_y = 120i32;
    let mut fuzz_presented = 0usize;

    for step in 0..1000usize {
        let dt = 5 + (next_u64() % 245);
        now_ms += dt;
        let now = InstantMs(now_ms);
        let _ = engine.advance(now, false).unwrap();

        let choice = next_u64() % 10;
        let input = match choice {
            0..=1 => RawInput::Nav(if (next_u64() & 1) == 0 {
                NavDir::Next
            } else {
                NavDir::Prev
            }),
            2 => RawInput::Activate,
            3 => RawInput::Adjust(((next_u64() % 5) as i16) - 2),
            4 => RawInput::Back,
            5 if !down => {
                down = true;
                cur_x = 20 + (next_u64() % 280) as i32;
                cur_y = 20 + (next_u64() % 200) as i32;
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(cur_x, cur_y),
                }
            }
            6..=7 if down => {
                let dx = ((next_u64() % 41) as i32) - 20;
                let dy = ((next_u64() % 61) as i32) - 30;
                cur_x = (cur_x + dx).clamp(0, 319);
                cur_y = (cur_y + dy).clamp(0, 239);
                RawInput::PointerMove {
                    id: PointerId(1),
                    pos: Point::new(cur_x, cur_y),
                }
            }
            8 if down => {
                down = false;
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(cur_x, cur_y),
                }
            }
            9 if down => {
                down = false;
                RawInput::PointerCancel { id: PointerId(1) }
            }
            _ => {
                // Idle / timer-only tick already ran advance() above
                if engine.present_sync(&mut screen, &mut scratch).unwrap()
                    == PresentStatus::Presented
                {
                    fuzz_presented += 1;
                    assert_matches_reference(&engine, &screen, &format!("fuzz-idle-{step}"));
                }
                continue;
            }
        };

        let _ = engine.dispatch(input, now, false).unwrap();
        if engine.present_sync(&mut screen, &mut scratch).unwrap() == PresentStatus::Presented {
            fuzz_presented += 1;
            assert_matches_reference(&engine, &screen, &format!("fuzz-step-{step}"));
        }
    }

    assert!(
        fuzz_presented > 100,
        "expected plenty of presented frames during 1000-step fuzz"
    );
}

#[test]
fn wave3_widgets_dual_fonts_icons_and_offscreen_sparkline_match_reference() {
    use embedded_graphics::pixelcolor::Rgb565;
    use liuli_settings::app_fonts;

    #[derive(Clone)]
    struct ShowcaseModel {
        load: i16,
        ring: SampleRing<32>,
        offscreen: SampleRing<8>,
    }

    #[derive(Debug)]
    enum ShowcaseMsg {
        UpdateVisible(i16, i16),
        UpdateOffscreen(i16),
    }

    struct ShowcaseApp;

    impl App for ShowcaseApp {
        type Model = ShowcaseModel;
        type Msg = ShowcaseMsg;

        fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
            match msg {
                ShowcaseMsg::UpdateVisible(load, sample) => {
                    model.load = load;
                    model.ring.push(sample);
                    Change::View
                }
                ShowcaseMsg::UpdateOffscreen(sample) => {
                    model.offscreen.push(sample);
                    Change::View
                }
            }
        }

        fn view(model: &Self::Model) -> impl Scene {
            let mut root_p = NodeProps::new();
            root_p.width = Length::Fill(1);
            root_p.height = Length::Fill(1);
            root_p.bg = Some(Rgb565::new(2, 4, 3));

            layout(Key(1), root_p, move |ui| {
                ui.column(
                    Key(2),
                    NodeProps {
                        width: Length::Fill(1),
                        height: Length::Fill(1),
                        padding: EdgeInsets::all(6),
                        gap: 6,
                        ..NodeProps::new()
                    },
                    |ui| {
                        // Dual fonts + TextAlign Center & End
                        ui.label(
                            Key(10),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(18),
                                fg: Rgb565::new(31, 63, 31),
                                font: FontId::Small12,
                                text_align: TextAlign::Center,
                                text: TextRef::Static("监测仪表盘 · Small12"),
                                max_lines: 1,
                                ..NodeProps::new()
                            },
                        )?;
                        let load_text = ui.text_fmt(format_args!("负载 {}%", model.load))?;
                        ui.label(
                            Key(11),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(20),
                                fg: Rgb565::new(18, 56, 31),
                                font: FontId::Body16,
                                text_align: TextAlign::End,
                                text: load_text,
                                max_lines: 1,
                                ..NodeProps::new()
                            },
                        )?;
                        // ProgressBar
                        ui.progress_bar(
                            Key(12),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(12),
                                bg: Some(Rgb565::new(6, 12, 8)),
                                accent: Rgb565::new(0, 63, 0),
                                radius: 4,
                                min: 0,
                                max: 100,
                                value: model.load,
                                ..NodeProps::new()
                            },
                        )?;
                        // Visible Sparkline crossing band lines
                        ui.sparkline_ring(
                            Key(13),
                            &model.ring,
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(56),
                                padding: EdgeInsets::all(4),
                                bg: Some(Rgb565::new(4, 8, 6)),
                                border_color: Some(Rgb565::new(12, 24, 16)),
                                border_width: 1,
                                accent: Rgb565::new(0, 63, 31),
                                min: 0,
                                max: 100,
                                ..NodeProps::new()
                            },
                        )?;
                        // Icon showcase buttons (distinct icons)
                        ui.row(
                            Key(14),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(26),
                                gap: 4,
                                ..NodeProps::new()
                            },
                            |ui| {
                                for (idx, icon) in [
                                    IconKind::Gauge,
                                    IconKind::Waveform,
                                    IconKind::Diagnostics,
                                    IconKind::Bell,
                                    IconKind::Moon,
                                    IconKind::Thermometer,
                                    IconKind::Reset,
                                ]
                                .iter()
                                .enumerate()
                                {
                                    ui.button(
                                        Key(30 + idx as u32),
                                        NodeProps {
                                            width: Length::Px(36),
                                            height: Length::Px(24),
                                            bg: Some(Rgb565::new(8, 16, 12)),
                                            fg: Rgb565::new(31, 63, 31),
                                            icon: *icon,
                                            text: TextRef::Static(""),
                                            focusable: true,
                                            ..NodeProps::new()
                                        },
                                    )?;
                                }
                                Ok(())
                            },
                        )?;
                        // ScrollY containing an off-screen Sparkline below the viewport
                        ui.scroll_y(
                            Key(15),
                            NodeProps {
                                width: Length::Fill(1),
                                height: Length::Px(64),
                                ..NodeProps::new()
                            },
                            |ui| {
                                ui.label(
                                    Key(40),
                                    NodeProps {
                                        width: Length::Fill(1),
                                        height: Length::Px(120),
                                        fg: Rgb565::new(20, 40, 20),
                                        font: FontId::Small12,
                                        text: TextRef::Static("系统诊断"),
                                        max_lines: 1,
                                        ..NodeProps::new()
                                    },
                                )?;
                                ui.sparkline_ring(
                                    Key(41),
                                    &model.offscreen,
                                    NodeProps {
                                        width: Length::Fill(1),
                                        height: Length::Px(40),
                                        accent: Rgb565::new(31, 31, 0),
                                        min: 0,
                                        max: 100,
                                        ..NodeProps::new()
                                    },
                                )
                            },
                        )
                    },
                )
            })
        }

        fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
            None
        }
    }

    let mut ring = SampleRing::new();
    for i in 0..16 {
        ring.push((i * 6) as i16);
    }
    let mut offscreen = SampleRing::new();
    offscreen.push(10);
    offscreen.push(20);

    let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    let mut engine = Engine::<ShowcaseApp>::new_in_place_with_fonts(
        &mut storage,
        ShowcaseModel {
            load: 65,
            ring,
            offscreen,
        },
        app_fonts(),
        screen_rect,
        InstantMs(0),
    )
    .unwrap();

    assert_eq!(
        engine.present_sync(&mut screen, &mut scratch).unwrap(),
        PresentStatus::Presented
    );
    let ref0 = render_reference_fullscreen(
        engine.active_commands(),
        engine.active_tree(),
        engine.fonts(),
    );
    assert_eq!(screen.diff_first_mismatch(&ref0), None);

    // Update visible ProgressBar + Sparkline + Label: 0 measured nodes, bounded transfer, 0-pixel mismatch
    engine
        .try_message(ShowcaseMsg::UpdateVisible(80, 95), InstantMs(10), false)
        .unwrap();
    assert_eq!(engine.last_metrics().measured_nodes, 0);
    assert_eq!(
        engine.present_sync(&mut screen, &mut scratch).unwrap(),
        PresentStatus::Presented
    );
    assert!(engine.last_metrics().pixels_sent < 320 * 240);
    let ref1 = render_reference_fullscreen(
        engine.active_commands(),
        engine.active_tree(),
        engine.fonts(),
    );
    assert_eq!(screen.diff_first_mismatch(&ref1), None);

    // Update off-screen Sparkline only: zero transfer, PresentStatus::Idle!
    engine
        .try_message(ShowcaseMsg::UpdateOffscreen(90), InstantMs(20), false)
        .unwrap();
    assert_eq!(engine.last_metrics().measured_nodes, 0);
    assert!(engine.damage().is_empty());
    assert_eq!(
        engine.present_sync(&mut screen, &mut scratch).unwrap(),
        PresentStatus::Idle
    );
}

#[test]
fn terminal_dashboard_single_sample_damage_is_bounded_and_matches_reference() {
    let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    let mut engine = Engine::<TerminalApp>::new_in_place_with_fonts(
        &mut storage,
        TerminalModel::new(InstantMs(0)),
        app_fonts(),
        screen_rect,
        InstantMs(0),
    )
    .unwrap();
    engine.set_focus_ring_visible(true);

    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert_eq!(engine.last_metrics().bands, 30);
    assert_eq!(engine.last_metrics().pixels_sent, 76_800);
    assert_matches_reference(&engine, &screen, "terminal-initial");

    // Advance to the first 1,000 ms telemetry sample tick on Dashboard.
    let changed = engine.try_advance(InstantMs(1000), false).unwrap();
    assert!(changed, "1,000 ms sample tick must update Dashboard");

    // Compute exact damage-merged upper bound from the declared readout & waveform nodes.
    let mut expected_damage = DamageSet::new();
    for k in [
        tk::DASH_TEMP_VAL,
        tk::DASH_LOAD_VAL,
        tk::DASH_LOAD_BAR,
        tk::DASH_WAVE_META,
        tk::DASH_SPARKLINE,
    ] {
        let node = engine
            .active_tree()
            .node_by_key(k)
            .unwrap_or_else(|| panic!("missing dashboard node {k:?}"));
        expected_damage.add(node.visible_rect, screen_rect);
    }
    let upper_bound_pixels: u32 = liuli::render::BandIter::new::<()>(
        expected_damage.as_slice(),
        screen_rect,
        DEFAULT_BAND_HEIGHT as u32,
        DEFAULT_BAND_PIXELS,
    )
    .unwrap()
    .map(|b| b.w * b.h)
    .sum();

    for &d in engine.damage().as_slice() {
        assert!(
            d.y >= 32,
            "single telemetry sample dirtied top tab bar: {d:?}"
        );
        assert!(
            d.bottom64() <= 176,
            "single telemetry sample dirtied below waveform card: {d:?}"
        );
    }

    let st = engine.present_sync(&mut screen, &mut scratch).unwrap();
    assert_eq!(st, PresentStatus::Presented);
    let sent = engine.last_metrics().pixels_sent;
    assert!(sent > 0, "expected non-zero pixel transfer after sample");
    assert!(
        sent <= upper_bound_pixels,
        "sample pixels_sent ({sent}) exceeded damage-merged upper bound ({upper_bound_pixels})"
    );
    assert!(
        sent < DEFAULT_SCREEN_WIDTH * DEFAULT_SCREEN_HEIGHT,
        "single sample unexpectedly triggered fullscreen transfer ({sent})"
    );
    assert_matches_reference(&engine, &screen, "terminal-single-sample");
}

struct ScreenDmaSink<'a, 's> {
    screen: &'s mut MemoryScreen,
    in_flight: Option<InFlightBand<'a>>,
}

impl<'a, 's> ScreenDmaSink<'a, 's> {
    fn new(screen: &'s mut MemoryScreen) -> Self {
        Self {
            screen,
            in_flight: None,
        }
    }
}

impl<'a> DmaSink<'a> for ScreenDmaSink<'a, '_> {
    type Error = ();

    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error> {
        if self.in_flight.is_some() {
            return StartOutcome::Busy(band);
        }
        self.in_flight = Some(band.into_in_flight());
        StartOutcome::Started
    }

    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error> {
        let Some(band) = self.in_flight.take() else {
            return PollOutcome::Pending;
        };
        let meta = band.meta();
        let src = band.pixels();
        let width = self.screen.width() as usize;
        let words = self.screen.words_mut();
        for ry in 0..meta.rect.h as usize {
            let dst_off = (meta.rect.y as usize + ry) * width + (meta.rect.x as usize);
            let src_off = ry * (meta.rect.w as usize);
            words[dst_off..dst_off + meta.rect.w as usize]
                .copy_from_slice(&src[src_off..src_off + meta.rect.w as usize]);
        }
        PollOutcome::Completed {
            slot: meta.slot,
            frame: meta.frame,
            seq: meta.seq,
            band: band.confirm_stopped(),
        }
    }
}

#[test]
fn terminal_three_fixed_seeds_2048_step_fuzz_with_dma_and_repeats_match_reference() {
    let seeds = [
        0x5630_325F_5345_4544u64,
        0x4C49_554C_495F_5630u64,
        0xCAFE_BABE_1234_5678u64,
    ];
    let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
    let mut band_a = Box::new(AlignedBand::<DEFAULT_BAND_PIXELS>::zeroed());
    let mut band_b = Box::new(AlignedBand::<DEFAULT_BAND_PIXELS>::zeroed());
    let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);

    for &initial_seed in &seeds {
        let mut seed = initial_seed;
        let mut next_u64 = || -> u64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        screen.words_mut().fill(0);
        let mut engine = Engine::<TerminalApp>::new_in_place_with_fonts(
            &mut storage,
            TerminalModel::new(InstantMs(0)),
            app_fonts(),
            screen_rect,
            InstantMs(0),
        )
        .unwrap();
        engine.set_focus_ring_visible(true);
        assert_eq!(
            engine.present_sync(&mut screen, &mut scratch).unwrap(),
            PresentStatus::Presented
        );

        let mut frame_id = 1u64;
        let mut now_ms = 0u64;
        let mut down = false;
        let mut cur_x = 160i32;
        let mut cur_y = 120i32;
        let mut dma_frames = 0usize;
        let mut sync_frames = 0usize;

        for step in 0..2048usize {
            // Occasional backward-time probe (relative to already-committed now_ms)
            // must fail cleanly without corrupting state.
            if now_ms > 0 && (step % 97) == 5 {
                let back = InstantMs(now_ms - 1);
                assert_eq!(
                    engine.try_advance(back, false),
                    Err(TransactionError::WentBackwards)
                );
                assert_eq!(
                    engine.try_dispatch(RawInput::Activate, back, false),
                    Err(TransactionError::WentBackwards)
                );
            }

            let dt = if (next_u64() % 29) == 0 {
                // Occasional large jump across telemetry/toast/repeat boundaries
                1500 + (next_u64() % 3501)
            } else {
                next_u64() % 601
            };
            now_ms += dt;
            let now = InstantMs(now_ms);

            let _ = engine.try_advance(now, false).unwrap();

            let hold_keys = [HoldKey::Activate, HoldKey::AdjustPrev, HoldKey::AdjustNext];
            let choice = next_u64() % 14;
            let mut event_desc = String::new();
            let input_opt = match choice {
                0..=1 => Some(RawInput::Nav(if (next_u64() & 1) == 0 {
                    NavDir::Next
                } else {
                    NavDir::Prev
                })),
                2 => Some(RawInput::Activate),
                3 => Some(RawInput::Adjust(((next_u64() % 5) as i16) - 2)),
                4 => Some(RawInput::Back),
                5 if !down => {
                    // Explicit tab-bar taps on page buttons to force multi-page switching
                    let tab_x = match next_u64() % 3 {
                        0 => 40,
                        1 => 160,
                        _ => 280,
                    };
                    down = true;
                    cur_x = tab_x;
                    cur_y = 14;
                    Some(RawInput::PointerDown {
                        id: PointerId(1),
                        pos: Point::new(cur_x, cur_y),
                    })
                }
                6 if !down => {
                    down = true;
                    cur_x = 8 + (next_u64() % 304) as i32;
                    cur_y = 8 + (next_u64() % 224) as i32;
                    Some(RawInput::PointerDown {
                        id: PointerId(1),
                        pos: Point::new(cur_x, cur_y),
                    })
                }
                7..=8 if down => {
                    let dx = ((next_u64() % 41) as i32) - 20;
                    let dy = ((next_u64() % 61) as i32) - 30;
                    cur_x = (cur_x + dx).clamp(0, 319);
                    cur_y = (cur_y + dy).clamp(0, 239);
                    Some(RawInput::PointerMove {
                        id: PointerId(1),
                        pos: Point::new(cur_x, cur_y),
                    })
                }
                9 if down => {
                    down = false;
                    Some(RawInput::PointerUp {
                        id: PointerId(1),
                        pos: Point::new(cur_x, cur_y),
                    })
                }
                10 if down => {
                    down = false;
                    Some(RawInput::PointerCancel { id: PointerId(1) })
                }
                11 => {
                    let hk = hold_keys[(next_u64() as usize) % hold_keys.len()];
                    Some(RawInput::KeyDown(hk))
                }
                12 => {
                    let hk = hold_keys[(next_u64() as usize) % hold_keys.len()];
                    Some(RawInput::KeyUp(hk))
                }
                _ => None,
            };

            if let Some(inp) = input_opt {
                event_desc = format!("{inp:?}");
                let _ = engine.try_dispatch(inp, now, false).unwrap();
            } else {
                event_desc.push_str("Idle");
            }

            if (step % 17) == 0 && !engine.damage().is_empty() {
                let mut dma = DmaCoordinator::new(band_a.as_mut_slice(), band_b.as_mut_slice());
                let mut sink = ScreenDmaSink::new(&mut screen);
                let mut checked_busy = false;
                for _ in 0..256 {
                    let status = engine.poll_dma(&mut dma, &mut sink).unwrap();
                    if status == PresentStatus::Pending {
                        assert!(engine.is_dma_busy());
                        if !checked_busy {
                            checked_busy = true;
                            assert!(matches!(
                                engine.try_dispatch(RawInput::Nav(NavDir::Next), now, false),
                                Err(TransactionError::Busy(_))
                            ));
                            assert_eq!(engine.try_advance(now, false), Ok(false));
                        }
                    } else {
                        if status == PresentStatus::Presented {
                            frame_id += 1;
                            dma_frames += 1;
                        }
                        break;
                    }
                }
                assert!(
                    !engine.is_dma_busy(),
                    "DMA frame did not finish in 256 polls"
                );
            } else if engine.present_sync(&mut screen, &mut scratch).unwrap()
                == PresentStatus::Presented
            {
                frame_id += 1;
                sync_frames += 1;
            }

            let ref_scr = render_reference_fullscreen(
                engine.active_commands(),
                engine.active_tree(),
                engine.fonts(),
            );
            if let Some((x, y, got, exp)) = screen.diff_first_mismatch(&ref_scr) {
                panic!(
                    "fuzz divergence: seed=0x{initial_seed:016X}, step={step}, t={now_ms}ms, event={event_desc}, frame_id={frame_id}, first_mismatch=({x},{y}) got=0x{got:04X} expected=0x{exp:04X}, damage={:?}",
                    engine.damage().as_slice()
                );
            }
        }

        assert!(
            dma_frames > 0,
            "expected DMA frames for seed 0x{initial_seed:016X}"
        );
        assert!(
            sync_frames > 0,
            "expected sync frames for seed 0x{initial_seed:016X}"
        );
    }
}
