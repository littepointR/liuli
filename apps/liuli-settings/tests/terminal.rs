use core::convert::Infallible;

use liuli::prelude::*;
use liuli_settings::{
    PageId, TOAST_DURATION_MS, TerminalApp, TerminalModel, TerminalMsg, app_fonts,
    terminal_keys as tk,
};

struct MemSpiBus {
    cur: Rect,
}

impl MemSpiBus {
    fn new() -> Self {
        Self { cur: Rect::ZERO }
    }
}

impl SpiWindowBus for MemSpiBus {
    type Error = Infallible;

    fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error> {
        self.cur = rect;
        Ok(())
    }

    fn write_bytes(&mut self, _bytes: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn terminal_three_pages_memory_restore_toast_and_diagnostics_loop_free() {
    let screen = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
    let mut storage = Box::new(Storage::new());
    let mut spi_chunk = [0u8; RECOMMENDED_SPI_CHUNK_BYTES];
    let mut target = SpiWindowTarget::new(
        MemSpiBus::new(),
        screen,
        ByteOrder::BigEndian,
        &mut spi_chunk,
    )
    .unwrap();
    let mut scratch = [0u16; DEFAULT_BAND_PIXELS];

    let mut engine = Engine::<TerminalApp>::new_in_place_with_fonts(
        &mut storage,
        TerminalModel::new(InstantMs(0)),
        app_fonts(),
        screen,
        InstantMs(0),
    )
    .unwrap();
    engine.set_focus_ring_visible(true);
    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert_eq!(engine.model().active_page, PageId::Dashboard);
    // Initial focus skips tab keys and lands on content first focusable!
    assert_eq!(engine.focus().focused_key(), Some(tk::DASH_BTN_PAUSE));

    // Focus DASH_BTN_INC and scroll Dashboard down via drag
    engine
        .dispatch(RawInput::Nav(NavDir::Next), InstantMs(10), false)
        .unwrap();
    engine
        .dispatch(RawInput::Nav(NavDir::Next), InstantMs(20), false)
        .unwrap();
    assert_eq!(engine.focus().focused_key(), Some(tk::DASH_BTN_INC));

    let dash_scroll_rect = engine
        .active_tree()
        .node_by_key(tk::DASH_SCROLL)
        .unwrap()
        .layout_rect;
    let drag_x = dash_scroll_rect.x + dash_scroll_rect.w as i32 / 2;
    let drag_y = dash_scroll_rect.y + 80;
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(drag_x, drag_y),
            },
            InstantMs(30),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerMove {
                id: PointerId(1),
                pos: Point::new(drag_x, drag_y - 25),
            },
            InstantMs(40),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: Point::new(drag_x, drag_y - 25),
            },
            InstantMs(50),
            false,
        )
        .unwrap();
    let dash_scroll_saved = engine
        .active_tree()
        .node_by_key(tk::DASH_SCROLL)
        .unwrap()
        .scroll_y;
    assert!(dash_scroll_saved > 0);

    // Click TAB_SETTINGS directly: content focus saved BEFORE tab click becomes active!
    let tab_set_r = engine
        .active_tree()
        .node_by_key(tk::TAB_SETTINGS)
        .unwrap()
        .layout_rect;
    let tab_set_pt = Point::new(
        tab_set_r.x + tab_set_r.w as i32 / 2,
        tab_set_r.y + tab_set_r.h as i32 / 2,
    );
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: tab_set_pt,
            },
            InstantMs(60),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: tab_set_pt,
            },
            InstantMs(70),
            false,
        )
        .unwrap();
    assert_eq!(engine.model().active_page, PageId::Settings);
    assert_eq!(
        engine.model().page_memory[PageId::Dashboard.as_usize()].focus_key,
        Some(tk::DASH_BTN_INC)
    );
    assert_eq!(
        engine.model().page_memory[PageId::Dashboard.as_usize()].scroll_y,
        dash_scroll_saved
    );
    engine.present_sync(&mut target, &mut scratch).unwrap();

    // On Settings: navigate to SET_BTN_RESET (scrolling Settings to bottom)
    for step in 0..8 {
        if engine.focus().focused_key() == Some(tk::SET_BTN_RESET) {
            break;
        }
        engine
            .dispatch(
                RawInput::Nav(NavDir::Next),
                InstantMs(80 + step * 10),
                false,
            )
            .unwrap();
    }
    assert_eq!(engine.focus().focused_key(), Some(tk::SET_BTN_RESET));
    let set_scroll_saved = engine
        .active_tree()
        .node_by_key(tk::SET_SCROLL)
        .unwrap()
        .scroll_y;
    assert!(set_scroll_saved > 0);

    // Open reset modal -> Cancel closes and restores focus to SET_BTN_RESET
    engine
        .dispatch(RawInput::Activate, InstantMs(160), false)
        .unwrap();
    assert!(engine.model().settings.reset_modal_open);
    assert_eq!(engine.focus().focused_key(), Some(tk::BTN_CANCEL));

    // Confirm Reset -> closes modal and displays 1500ms Toast!
    engine
        .dispatch(RawInput::Nav(NavDir::Next), InstantMs(170), false)
        .unwrap();
    assert_eq!(engine.focus().focused_key(), Some(tk::BTN_CONFIRM));
    engine
        .dispatch(RawInput::Activate, InstantMs(180), false)
        .unwrap();
    assert!(!engine.model().settings.reset_modal_open);
    assert!(engine.model().toast.is_some());
    assert_eq!(engine.focus().focused_key(), Some(tk::SET_BTN_RESET));
    engine.present_sync(&mut target, &mut scratch).unwrap();

    // Toast must be non-focusable and click-through! Clicking directly on TOAST_BOX center passes through.
    let toast_r = engine
        .active_tree()
        .node_by_key(tk::TOAST_BOX)
        .unwrap()
        .layout_rect;
    let toast_pt = Point::new(
        toast_r.x + toast_r.w as i32 / 2,
        toast_r.y + toast_r.h as i32 / 2,
    );
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: toast_pt,
            },
            InstantMs(190),
            false,
        )
        .unwrap();
    engine
        .dispatch(
            RawInput::PointerUp {
                id: PointerId(1),
                pos: toast_pt,
            },
            InstantMs(200),
            false,
        )
        .unwrap();
    assert_ne!(engine.focus().focused_key(), Some(tk::TOAST_BOX));
    assert_ne!(engine.focus().focused_key(), Some(tk::TOAST_LABEL));
    // Clicking Toast center hit SET_ROW_RESET underneath (opening reset modal); dismiss it via Back.
    assert!(engine.model().settings.reset_modal_open);
    engine
        .dispatch(RawInput::Back, InstantMs(205), false)
        .unwrap();
    assert!(!engine.model().settings.reset_modal_open);

    // Switch to Diagnostics via message; Diagnostics records metrics loop-free!
    let m = engine.last_metrics();
    engine
        .try_with_model(|model| model.record_completed_frame(m, InstantMs(205)))
        .unwrap();
    engine
        .message(
            TerminalMsg::SelectPage(PageId::Diagnostics),
            InstantMs(210),
            false,
        )
        .unwrap();
    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert_eq!(engine.model().active_page, PageId::Diagnostics);

    // Turning monitoring off + no pending toast deadline => next_wakeup is OnEvent;
    // recording another completed frame does NOT self-trigger redraws on Diagnostics!
    engine
        .message(TerminalMsg::ToggleMonitoring, InstantMs(220), false)
        .unwrap();
    assert!(
        engine
            .advance(InstantMs(180 + TOAST_DURATION_MS), false)
            .unwrap()
    );
    assert!(engine.model().toast.is_none());
    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert_eq!(
        engine.next_wakeup(InstantMs(2000), false),
        NextWakeup::OnEvent
    );

    let m2 = engine.last_metrics();
    engine
        .try_with_model(|model| model.record_completed_frame(m2, InstantMs(2000)))
        .unwrap();
    assert!(engine.damage().is_empty());
    assert_eq!(
        engine.next_wakeup(InstantMs(2000), false),
        NextWakeup::OnEvent
    );

    // RefreshDiagnostics updates displayed_diag and spawns Toast + exact deadline
    engine
        .message(TerminalMsg::RefreshDiagnostics, InstantMs(2010), false)
        .unwrap();
    engine.present_sync(&mut target, &mut scratch).unwrap();
    assert!(engine.model().toast.is_some());
    assert_eq!(
        engine.next_wakeup(InstantMs(2010), false),
        NextWakeup::At(InstantMs(2010 + TOAST_DURATION_MS))
    );

    // Back from Diagnostics returns to Dashboard and restores saved focus + scroll!
    engine
        .dispatch(RawInput::Back, InstantMs(2020), false)
        .unwrap();
    assert_eq!(engine.model().active_page, PageId::Dashboard);
    assert_eq!(engine.focus().focused_key(), Some(tk::DASH_BTN_INC));
    assert_eq!(
        engine
            .active_tree()
            .node_by_key(tk::DASH_SCROLL)
            .unwrap()
            .scroll_y,
        dash_scroll_saved
    );

    // Large time jump updates Sparkline boundedly (len <= 32) without hang
    engine
        .message(TerminalMsg::ToggleMonitoring, InstantMs(2030), false)
        .unwrap();
    assert!(engine.advance(InstantMs(10_000_000), false).unwrap());
    assert_eq!(engine.model().temp_history.len(), 32);

    // Switch back to Settings: invalid saved focus fallback + clamping works cleanly
    engine
        .try_with_model(|model| {
            model.page_memory[PageId::Settings.as_usize()].focus_key = Some(Key(999_999));
            model.page_memory[PageId::Settings.as_usize()].scroll_y = 9999;
        })
        .unwrap();
    engine
        .message(
            TerminalMsg::SelectPage(PageId::Settings),
            InstantMs(10_000_010),
            false,
        )
        .unwrap();
    let clamped_scroll = engine
        .active_tree()
        .node_by_key(tk::SET_SCROLL)
        .unwrap()
        .scroll_y;
    assert!(clamped_scroll < 9999);
    // Fallback picks content focusable, NOT tab button
    let restored_f = engine.focus().focused_key().unwrap();
    assert!(!TerminalApp::is_tab_key(restored_f));
}
