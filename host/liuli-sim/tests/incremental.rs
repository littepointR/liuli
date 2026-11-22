use liuli::prelude::*;
use liuli_settings::{SettingsApp, SettingsModel, keys, settings_font};
use liuli_sim::screen::{MemoryScreen, render_reference_fullscreen};

fn assert_matches_reference(
    engine: &Engine<'_, '_, SettingsApp>,
    screen: &MemoryScreen,
    step: &str,
) {
    let ref_scr = render_reference_fullscreen(
        engine.active_commands(),
        engine.active_tree(),
        engine.font(),
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
    assert_eq!(engine.focus().focused_key, Some(keys::BTN_RESET));
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
    assert_eq!(engine.focus().focused_key, Some(keys::TOGGLE_MONITOR));
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
