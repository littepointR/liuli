use core::convert::Infallible;

use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Dimensions, OriginDimensions, Point, Size};
use embedded_graphics::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics::primitives::Rectangle;
use liuli_core::capacity::{PresentError, PresentStatus};
use liuli_core::color::PixelFormat;
use liuli_core::damage::DamageSet;
use liuli_core::dither::DitherMode;
use liuli_core::geometry::{Rect, rgb565_to_u16, u16_to_rgb565};
use liuli_core::paint::{CommandList, PaintCmd, SliceSceneResources};
use liuli_render::{
    BandTarget, BufferTooSmall, ByteOrder, ByteWindowBus, ByteWindowSink, DisplaySink,
    EgDisplaySink, FrameError, RECOMMENDED_SPI_CHUNK_BYTES, RECOMMENDED_WIRE_CHUNK_BYTES,
    SpiWindowBus, SpiWindowTarget, encode_rgb565, encode_rgb565_in_place, encode_rgb565_row,
    encode_rgb565_words, present_sink, present_sync, present_sync_with,
};
use liuli_text::{FontSet, Lf4Font};

fn dummy_lf4() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"LLF4");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&14i16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&24u32.to_le_bytes());
    out.extend_from_slice(&44u32.to_le_bytes());
    out.extend_from_slice(&(0x25A1u32).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&0i16.to_le_bytes());
    out.extend_from_slice(&12i16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(0xFF);
    out
}

#[test]
fn band_target_left_clip_stride_short_long_empty_and_sentinels() {
    let mut words = [0xBEEFu16; 40]; // only 8x4 = 32 pixels active, 8 trailing sentinels
    let mut target = BandTarget::new::<()>(&mut words, Rect::new(24, 53, 8, 4)).unwrap();

    // Key case: 4 colors per row, left-clipped by 1 column (x = -1, y = 0, w = 4, h = 2)
    let input_colors = [
        u16_to_rgb565(0x0001),
        u16_to_rgb565(0x0002),
        u16_to_rgb565(0x0003),
        u16_to_rgb565(0x0004),
        u16_to_rgb565(0x0005),
        u16_to_rgb565(0x0006),
        u16_to_rgb565(0x0007),
        u16_to_rgb565(0x0008),
        u16_to_rgb565(0x9999), // extra long iterator items must be ignored
        u16_to_rgb565(0xAAAA),
    ];

    target.clear(u16_to_rgb565(0x1111)).unwrap();
    target
        .fill_contiguous(
            &Rectangle::new(Point::new(-1, 0), Size::new(4, 2)),
            input_colors,
        )
        .unwrap();

    // Row 0 cols 0..3 => input[1..4] = 2, 3, 4; col 3 untouched = 0x1111
    assert_eq!(target.words()[0], 0x0002);
    assert_eq!(target.words()[1], 0x0003);
    assert_eq!(target.words()[2], 0x0004);
    assert_eq!(target.words()[3], 0x1111);
    // Row 1 cols 0..3 => input[5..8] = 6, 7, 8
    assert_eq!(target.words()[8], 0x0006);
    assert_eq!(target.words()[9], 0x0007);
    assert_eq!(target.words()[10], 0x0008);
    assert_eq!(target.words()[11], 0x1111);

    // Short iterator preserves trailing pixels
    target
        .fill_contiguous(
            &Rectangle::new(Point::new(4, 2), Size::new(3, 1)),
            [u16_to_rgb565(0x2222), u16_to_rgb565(0x3333)],
        )
        .unwrap();
    assert_eq!(target.words()[2 * 8 + 4], 0x2222);
    assert_eq!(target.words()[2 * 8 + 5], 0x3333);
    assert_eq!(target.words()[2 * 8 + 6], 0x1111);

    // Empty rectangle and completely OOB fill_solid do nothing
    target
        .fill_contiguous(
            &Rectangle::new(Point::new(0, 0), Size::new(0, 2)),
            input_colors,
        )
        .unwrap();
    target
        .fill_solid(
            &Rectangle::new(Point::new(-5, -5), Size::new(3, 3)),
            u16_to_rgb565(0x4444),
        )
        .unwrap();

    // Trailing sentinels past 32 words are untouched
    assert!(words[32..40].iter().all(|&w| w == 0xBEEF));
}

struct DrawIterOnlyScreen {
    origin: Point,
    w: u32,
    h: u32,
    fb: Vec<u16>,
    draw_calls: usize,
}

impl OriginDimensions for DrawIterOnlyScreen {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for DrawIterOnlyScreen {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.draw_calls += 1;
        for Pixel(p, c) in pixels {
            let lx = p.x - self.origin.x;
            let ly = p.y - self.origin.y;
            if lx >= 0 && lx < self.w as i32 && ly >= 0 && ly < self.h as i32 {
                self.fb[(ly as usize) * (self.w as usize) + (lx as usize)] = c.into_storage();
            }
        }
        Ok(())
    }
}

struct SubOriginRecordingTarget {
    origin: Point,
    w: u32,
    h: u32,
    fb: Vec<u16>,
    contiguous_calls: Vec<Rect>,
    fail_on_call: Option<usize>,
}

impl Dimensions for SubOriginRecordingTarget {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(self.origin, Size::new(self.w, self.h))
    }
}

impl DrawTarget for SubOriginRecordingTarget {
    type Color = Rgb565;
    type Error = &'static str;

    fn draw_iter<I>(&mut self, _pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        panic!("present_sync should upload via fill_contiguous");
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let call_idx = self.contiguous_calls.len();
        if self.fail_on_call == Some(call_idx) {
            return Err("write failed");
        }
        self.contiguous_calls.push(Rect::from_eg(*area));
        let mut iter = colors.into_iter();
        for py in area.top_left.y..(area.top_left.y + area.size.height as i32) {
            for px in area.top_left.x..(area.top_left.x + area.size.width as i32) {
                let Some(c) = iter.next() else {
                    return Ok(());
                };
                let lx = px - self.origin.x;
                let ly = py - self.origin.y;
                if lx >= 0 && lx < self.w as i32 && ly >= 0 && ly < self.h as i32 {
                    self.fb[(ly as usize) * (self.w as usize) + (lx as usize)] = c.into_storage();
                }
            }
        }
        Ok(())
    }
}

#[test]
fn present_sync_honors_draw_iter_only_non_zero_origin_and_flush_failure() {
    let lf4 = dummy_lf4();
    let font = FontSet::single(Lf4Font::from_bytes(&lf4).unwrap());
    let screen = Rect::new(0, 0, 16, 12);
    let mut cmds = CommandList::new();
    cmds.root_bg = u16_to_rgb565(0x001F);
    cmds.screen = screen;
    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(2, 3, 6, 4),
        clip: screen,
        color: u16_to_rgb565(0xF800),
    })
    .unwrap();

    // 1. Buffer smaller than screen width returns BufferTooSmall
    let mut tiny_scratch = [0u16; 15];
    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(screen);
    let mut iter_target = DrawIterOnlyScreen {
        origin: Point::new(0, 0),
        w: 16,
        h: 12,
        fb: vec![0; 16 * 12],
        draw_calls: 0,
    };
    let err = present_sync(
        &mut iter_target,
        &mut tiny_scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    );
    assert!(matches!(err, Err(PresentError::BufferTooSmall)));
    assert!(!dmg.is_empty());

    // 2. Minimal draw_iter-only target succeeds
    let mut scratch = [0u16; 16 * 4];
    let (st, metrics) = present_sync(
        &mut iter_target,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert_eq!(metrics.bands, 3);
    assert_eq!(metrics.pixels_sent, 16 * 12);
    assert!(dmg.is_empty());
    assert_eq!(iter_target.fb[3 * 16 + 2], 0xF800);
    assert_eq!(iter_target.fb[0], 0x001F);

    // 3. Non-zero target origin + flush failure preserves full-frame damage and retries cleanly
    let mut rec_target = SubOriginRecordingTarget {
        origin: Point::new(15, 25),
        w: 16,
        h: 12,
        fb: vec![0; 16 * 12],
        contiguous_calls: Vec::new(),
        fail_on_call: None,
    };
    dmg.mark_full_screen(screen);
    let fail_res = present_sync_with(
        &mut rec_target,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
        |_| Err("flush failed"),
    );
    assert_eq!(fail_res, Err(PresentError::Target("flush failed")));
    assert!(dmg.is_full_screen());

    // Retry succeeds and clears damage
    let (st2, _) = present_sync_with(
        &mut rec_target,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(st2, PresentStatus::Presented);
    assert!(dmg.is_empty());
    assert_eq!(rec_target.fb, iter_target.fb);
    assert_eq!(rgb565_to_u16(u16_to_rgb565(0xF800)), 0xF800);
}

#[test]
fn encode_rgb565_vectors_empty_odd_short_and_sentinel() {
    let vec_words = [0xF800u16, 0x07E0, 0x001F, 0x1234];

    let mut be_out = [0xAAu8; 10];
    assert_eq!(
        encode_rgb565(&vec_words, &mut be_out, ByteOrder::BigEndian),
        Ok(8)
    );
    assert_eq!(
        &be_out[..8],
        &[0xF8, 0x00, 0x07, 0xE0, 0x00, 0x1F, 0x12, 0x34]
    );
    assert_eq!(&be_out[8..], &[0xAA, 0xAA]);

    let mut le_out = [0xAAu8; 10];
    assert_eq!(
        encode_rgb565(&vec_words, &mut le_out, ByteOrder::LittleEndian),
        Ok(8)
    );
    assert_eq!(
        &le_out[..8],
        &[0x00, 0xF8, 0xE0, 0x07, 0x1F, 0x00, 0x34, 0x12]
    );
    assert_eq!(&le_out[8..], &[0xAA, 0xAA]);

    // Empty input works even into empty slice; non-empty buffer sentinels stay intact.
    assert_eq!(encode_rgb565(&[], &mut [], ByteOrder::BigEndian), Ok(0));
    let mut empty_sentinel = [0x77u8; 4];
    assert_eq!(
        encode_rgb565(&[], &mut empty_sentinel, ByteOrder::LittleEndian),
        Ok(0)
    );
    assert_eq!(empty_sentinel, [0x77; 4]);

    // Odd pixel counts (1 and 3 words)
    let mut odd_out = [0xCCu8; 8];
    assert_eq!(
        encode_rgb565(&vec_words[..3], &mut odd_out, ByteOrder::BigEndian),
        Ok(6)
    );
    assert_eq!(&odd_out[..6], &[0xF8, 0x00, 0x07, 0xE0, 0x00, 0x1F]);
    assert_eq!(&odd_out[6..], &[0xCC, 0xCC]);

    // Short output fails without touching any byte of the output slice.
    let mut short_out = [0x55u8; 7];
    assert_eq!(
        encode_rgb565(&vec_words, &mut short_out, ByteOrder::BigEndian),
        Err(BufferTooSmall)
    );
    assert_eq!(short_out, [0x55; 7]);
}

struct MockSpiBus {
    origin: Point,
    w: u32,
    h: u32,
    order: ByteOrder,
    active_win: Option<Rect>,
    byte_buf: Vec<u8>,
    fb: Vec<u16>,
    window_log: Vec<Rect>,
    write_chunks: Vec<usize>,
    flush_count: usize,
    fail_window_call: Option<usize>,
    fail_write_call: Option<usize>,
    fail_flush: bool,
}

impl MockSpiBus {
    fn new(origin: Point, w: u32, h: u32, order: ByteOrder) -> Self {
        Self {
            origin,
            w,
            h,
            order,
            active_win: None,
            byte_buf: Vec::new(),
            fb: vec![0; (w * h) as usize],
            window_log: Vec::new(),
            write_chunks: Vec::new(),
            flush_count: 0,
            fail_window_call: None,
            fail_write_call: None,
            fail_flush: false,
        }
    }

    fn drain_words_to_fb(&mut self) {
        let Some(win) = self.active_win else {
            return;
        };
        assert_eq!(self.byte_buf.len() % 2, 0, "odd SPI byte count");
        let words: Vec<u16> = self
            .byte_buf
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| match self.order {
                ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
                ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
            })
            .collect();
        let expected = (win.w as usize) * (win.h as usize);
        assert_eq!(
            words.len(),
            expected,
            "window {win:?} expected {expected} pixels, got {}",
            words.len()
        );
        for ry in 0..win.h as usize {
            for rx in 0..win.w as usize {
                let lx = (win.x + rx as i32) - self.origin.x;
                let ly = (win.y + ry as i32) - self.origin.y;
                if lx >= 0 && lx < self.w as i32 && ly >= 0 && ly < self.h as i32 {
                    self.fb[(ly as usize) * (self.w as usize) + (lx as usize)] =
                        words[ry * (win.w as usize) + rx];
                }
            }
        }
        self.byte_buf.clear();
    }
}

impl SpiWindowBus for MockSpiBus {
    type Error = &'static str;

    fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error> {
        let idx = self.window_log.len();
        if self.fail_window_call == Some(idx) {
            self.active_win = None;
            self.byte_buf.clear();
            return Err("window fail");
        }
        self.drain_words_to_fb();
        self.window_log.push(rect);
        self.active_win = Some(rect);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let idx = self.write_chunks.len();
        if self.fail_write_call == Some(idx) {
            self.active_win = None;
            self.byte_buf.clear();
            return Err("write fail");
        }
        self.write_chunks.push(bytes.len());
        self.byte_buf.extend_from_slice(bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        if self.fail_flush {
            self.active_win = None;
            self.byte_buf.clear();
            return Err("flush fail");
        }
        self.drain_words_to_fb();
        self.active_win = None;
        self.flush_count += 1;
        Ok(())
    }
}

#[test]
fn spi_window_target_chunks_stride_flush_idle_and_error_recovery() {
    let lf4 = dummy_lf4();
    let font = FontSet::single(Lf4Font::from_bytes(&lf4).unwrap());
    let screen = Rect::new(0, 0, 20, 14);
    let mut cmds = CommandList::new();
    cmds.root_bg = u16_to_rgb565(0x001F);
    cmds.screen = screen;
    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(3, 2, 11, 9),
        clip: screen,
        color: u16_to_rgb565(0xF800),
    })
    .unwrap();
    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(6, 5, 5, 4),
        clip: screen,
        color: u16_to_rgb565(0x07E0),
    })
    .unwrap();

    // Scratch too small (< 2 B) rejected
    let mut too_small = [0u8; 1];
    assert!(
        SpiWindowTarget::new(
            MockSpiBus::new(Point::new(0, 0), 20, 14, ByteOrder::BigEndian),
            screen,
            ByteOrder::BigEndian,
            &mut too_small
        )
        .is_err()
    );

    // Reference DrawTarget framebuffer
    let mut ref_target = SubOriginRecordingTarget {
        origin: Point::new(0, 0),
        w: 20,
        h: 14,
        fb: vec![0; 20 * 14],
        contiguous_calls: Vec::new(),
        fail_on_call: None,
    };
    let mut scratch = [0u16; 20 * 4];
    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(screen);
    present_sync(
        &mut ref_target,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();

    // Test both BE (64 B chunk) and LE (tight 6 B chunk crossing pixel rows)
    for (order, chunk_len) in [
        (ByteOrder::BigEndian, RECOMMENDED_SPI_CHUNK_BYTES),
        (ByteOrder::LittleEndian, 6usize),
    ] {
        let mut chunk = vec![0u8; chunk_len];
        let mut spi = SpiWindowTarget::new(
            MockSpiBus::new(Point::new(0, 0), 20, 14, order),
            screen,
            order,
            &mut chunk,
        )
        .unwrap();
        dmg.mark_full_screen(screen);
        let (st, metrics) = present_sync_with(
            &mut spi,
            &mut scratch,
            screen,
            4,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
            |t| t.flush(),
        )
        .unwrap();
        assert_eq!(st, PresentStatus::Presented);
        assert_eq!(metrics.bands, 4); // 14 rows split by band_h=4 -> 4+4+4+2
        assert_eq!(spi.bus().window_log.len(), 4);
        assert_eq!(spi.bus().flush_count, 1);
        assert!(spi.bus().write_chunks.iter().all(|&c| c <= chunk_len));
        assert_eq!(spi.bus().fb, ref_target.fb);

        // Idle second frame with empty damage must not touch SPI bus at all!
        let wins_before = spi.bus().window_log.len();
        let writes_before = spi.bus().write_chunks.len();
        let flushes_before = spi.bus().flush_count;
        let (st_idle, _) = present_sync_with(
            &mut spi,
            &mut scratch,
            screen,
            4,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
            |t| t.flush(),
        )
        .unwrap();
        assert_eq!(st_idle, PresentStatus::Idle);
        assert_eq!(spi.bus().window_log.len(), wins_before);
        assert_eq!(spi.bus().write_chunks.len(), writes_before);
        assert_eq!(spi.bus().flush_count, flushes_before);
    }

    // Clipped non-zero origin window stride preservation: area (-1, -1, 10, 6) clipped to (0, 0, 5, 4)
    {
        let bounds = Rect::new(10, 20, 5, 4);
        let mut chunk = [0u8; 6];
        let mut spi = SpiWindowTarget::new(
            MockSpiBus::new(Point::new(10, 20), 5, 4, ByteOrder::BigEndian),
            bounds,
            ByteOrder::BigEndian,
            &mut chunk,
        )
        .unwrap();
        // Source grid: 7x6 centered at (9, 19), i.e., top-left overlaps left by 1 col and top by 1 row
        let mut src_colors = Vec::new();
        for sy in 0..6u16 {
            for sx in 0..7u16 {
                let raw = ((sy + 1) << 11) | (sx + 1);
                src_colors.push(u16_to_rgb565(raw));
            }
        }
        spi.fill_contiguous(
            &Rectangle::new(Point::new(9, 19), Size::new(7, 6)),
            src_colors,
        )
        .unwrap();
        spi.flush().unwrap();
        assert_eq!(spi.bus().window_log, vec![bounds]);
        for ry in 0..4u16 {
            for rx in 0..5u16 {
                let expected = ((ry + 2) << 11) | (rx + 2);
                assert_eq!(spi.bus().fb[(ry as usize) * 5 + (rx as usize)], expected);
            }
        }
    }

    // Error recovery: fail set_window, write_bytes, and flush each keep full-screen damage
    for fail_mode in 0..3 {
        let mut chunk = [0u8; 16];
        let mut bus = MockSpiBus::new(Point::new(0, 0), 20, 14, ByteOrder::BigEndian);
        match fail_mode {
            0 => bus.fail_window_call = Some(1),
            1 => bus.fail_write_call = Some(2),
            _ => bus.fail_flush = true,
        }
        let mut spi = SpiWindowTarget::new(bus, screen, ByteOrder::BigEndian, &mut chunk).unwrap();
        dmg.clear();
        dmg.add(Rect::new(0, 0, 10, 8), screen);
        let res = present_sync_with(
            &mut spi,
            &mut scratch,
            screen,
            4,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
            |t| t.flush(),
        );
        assert!(matches!(res, Err(PresentError::Target(_))));
        assert!(dmg.is_full_screen());

        // Clear fault and retry full screen
        spi.bus_mut().fail_window_call = None;
        spi.bus_mut().fail_write_call = None;
        spi.bus_mut().fail_flush = false;
        let (st, _) = present_sync_with(
            &mut spi,
            &mut scratch,
            screen,
            4,
            &cmds,
            &mut dmg,
            &SliceSceneResources::EMPTY,
            &font,
            |t| t.flush(),
        )
        .unwrap();
        assert_eq!(st, PresentStatus::Presented);
        assert!(dmg.is_empty());
        assert_eq!(spi.bus().fb, ref_target.fb);
    }
}

#[test]
fn display_sink_byte_window_and_eg_adapter_work_uniformly_without_losing_compat() {
    // 1. Verify encode_rgb565_words and encode_rgb565_in_place match BE wire bytes.
    let logical = [0xF800u16, 0x07E0, 0x001F, 0x1234];
    let mut words_be = [0u16; 4];
    assert_eq!(
        encode_rgb565_words(&logical, &mut words_be, ByteOrder::BigEndian),
        Ok(4)
    );
    let mut in_place_be = logical;
    encode_rgb565_in_place(&mut in_place_be, ByteOrder::BigEndian);
    assert_eq!(words_be, in_place_be);
    let mut bytes_be = [0u8; 8];
    encode_rgb565(&logical, &mut bytes_be, ByteOrder::BigEndian).unwrap();
    for (w, &[b0, b1]) in words_be.iter().zip(bytes_be.as_chunks::<2>().0) {
        assert_eq!(w.to_ne_bytes(), [b0, b1]);
    }
    let mut tiny = [0u16; 2];
    assert_eq!(
        encode_rgb565_words(&logical, &mut tiny, ByteOrder::LittleEndian),
        Err(BufferTooSmall)
    );

    // 2. Verify SpiWindowBus/SpiWindowTarget are exact 1.x aliases of ByteWindowBus/ByteWindowSink.
    fn assert_same_bus_trait<B: SpiWindowBus + ByteWindowBus>() {}
    fn assert_is_display_sink<S: DisplaySink>() {}
    assert_same_bus_trait::<MockSpiBus>();
    assert_is_display_sink::<ByteWindowSink<'_, MockSpiBus>>();

    let lf4 = dummy_lf4();
    let font = FontSet::single(Lf4Font::from_bytes(&lf4).unwrap());
    let screen = Rect::new(0, 0, 20, 14);
    let mut cmds = CommandList::new();
    cmds.root_bg = u16_to_rgb565(0x001F);
    cmds.screen = screen;
    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(3, 2, 11, 9),
        clip: screen,
        color: u16_to_rgb565(0xF800),
    })
    .unwrap();

    let mut scratch = [0u16; 20 * 4];
    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(screen);

    let mut iter_sink = EgDisplaySink::new(DrawIterOnlyScreen {
        origin: Point::new(0, 0),
        w: 20,
        h: 14,
        fb: vec![0; 20 * 14],
        draw_calls: 0,
    });
    let (st_iter, _) = present_sink(
        &mut iter_sink,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();
    assert_eq!(st_iter, PresentStatus::Presented);
    assert!(iter_sink.inner().draw_calls > 0);

    dmg.mark_full_screen(screen);
    let mut eg_sink = EgDisplaySink::new(SubOriginRecordingTarget {
        origin: Point::new(4, 6),
        w: 20,
        h: 14,
        fb: vec![0; 20 * 14],
        contiguous_calls: Vec::new(),
        fail_on_call: None,
    });
    let (st, metrics) = present_sink(
        &mut eg_sink,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert_eq!(metrics.bands, 4);
    assert!(dmg.is_empty());
    assert_eq!(iter_sink.inner().fb, eg_sink.inner().fb);

    // 3. ByteWindowSink implements DisplaySink directly (no DrawTarget/iterator round-trip).
    let mut chunk = [0u8; RECOMMENDED_WIRE_CHUNK_BYTES];
    let mut wire_sink = ByteWindowSink::new(
        MockSpiBus::new(Point::new(0, 0), 20, 14, ByteOrder::BigEndian),
        screen,
        ByteOrder::BigEndian,
        &mut chunk,
    )
    .unwrap();
    dmg.mark_full_screen(screen);
    let (st, _) = present_sink(
        &mut wire_sink,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap();
    assert_eq!(st, PresentStatus::Presented);
    assert_eq!(wire_sink.bus().flush_count, 1);
    assert_eq!(wire_sink.bus().fb, eg_sink.inner().fb);

    // 4. Error recovery through DisplaySink restores full-screen damage.
    wire_sink.bus_mut().fail_flush = true;
    dmg.add(Rect::new(0, 0, 8, 4), screen);
    let err = present_sink(
        &mut wire_sink,
        &mut scratch,
        screen,
        4,
        &cmds,
        &mut dmg,
        &SliceSceneResources::EMPTY,
        &font,
    )
    .unwrap_err();
    assert!(matches!(err, FrameError::Display(PresentError::Target(_))));
    assert!(dmg.is_full_screen());
}

#[derive(Default)]
struct RecordingByteBus {
    windows: Vec<Rect>,
    bytes: Vec<u8>,
}

impl ByteWindowBus for RecordingByteBus {
    type Error = ();
    fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error> {
        self.windows.push(rect);
        Ok(())
    }
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

#[test]
fn encode_rgb565_row_supports_all_pixel_formats_and_byte_window_sink_modes() {
    // Pixels: Pure Red (0xF800), Mid Gray (~0x8410), Pure White (0xFFFF), Pure Black (0x0000), Pure Blue (0x001F)
    let row = [0xF800u16, 0x8410, 0xFFFF, 0x0000, 0x001F];
    let mut buf = [0xAAu8; 32];

    assert_eq!(
        encode_rgb565_row(&row[..2], 0, 0, PixelFormat::Rgb565Le, DitherMode::None, &mut buf[..3]),
        Err(BufferTooSmall)
    );
    assert_eq!(
        encode_rgb565_row(&row[..2], 0, 0, PixelFormat::Rgb565Be, DitherMode::None, &mut buf),
        Ok(4)
    );
    assert_eq!(&buf[..4], &[0xF8, 0x00, 0x84, 0x10]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Bgr565Be, DitherMode::None, &mut buf),
        Ok(2)
    );
    assert_eq!(&buf[..2], &[0x00, 0x1F]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Bgr565Le, DitherMode::None, &mut buf),
        Ok(2)
    );
    assert_eq!(&buf[..2], &[0x1F, 0x00]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Rgb666, DitherMode::None, &mut buf),
        Ok(3)
    );
    assert_eq!(&buf[..3], &[0xFC, 0x00, 0x00]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Rgb888, DitherMode::None, &mut buf),
        Ok(3)
    );
    assert_eq!(&buf[..3], &[0xFF, 0x00, 0x00]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Bgr888, DitherMode::None, &mut buf),
        Ok(3)
    );
    assert_eq!(&buf[..3], &[0x00, 0x00, 0xFF]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Argb8888, DitherMode::None, &mut buf),
        Ok(4)
    );
    assert_eq!(&buf[..4], &[0x00, 0x00, 0xFF, 0xFF]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Xrgb8888, DitherMode::None, &mut buf),
        Ok(4)
    );
    assert_eq!(&buf[..4], &[0x00, 0x00, 0xFF, 0xFF]);

    assert_eq!(
        encode_rgb565_row(&row[..1], 0, 0, PixelFormat::Abgr8888, DitherMode::None, &mut buf),
        Ok(4)
    );
    assert_eq!(&buf[..4], &[0xFF, 0x00, 0x00, 0xFF]);

    assert_eq!(
        encode_rgb565_row(&row[2..4], 0, 0, PixelFormat::Gray8, DitherMode::None, &mut buf),
        Ok(2)
    );
    assert_eq!(&buf[..2], &[255, 0]);

    assert_eq!(
        encode_rgb565_row(&row[2..5], 0, 0, PixelFormat::Gray4, DitherMode::None, &mut buf),
        Ok(2)
    );
    // px0=white(15<<4), px1=black(0) => 0xF0; px2=blue(luma<16 => 1<<4 = 0x10)
    assert_eq!(buf[0], 0xF0);
    assert_eq!(buf[1] & 0x0F, 0x00);

    let bw_row = [0xFFFFu16, 0x0000, 0xFFFF, 0x0000, 0xFFFF, 0xFFFF, 0x0000, 0x0000, 0xFFFF];
    assert_eq!(
        encode_rgb565_row(&bw_row, 0, 0, PixelFormat::Mono1Msb, DitherMode::None, &mut buf),
        Ok(2)
    );
    assert_eq!(&buf[..2], &[0b1010_1100, 0b1000_0000]);

    assert_eq!(
        encode_rgb565_row(&bw_row, 0, 0, PixelFormat::Mono1Lsb, DitherMode::None, &mut buf),
        Ok(2)
    );
    assert_eq!(&buf[..2], &[0b0011_0101, 0b0000_0001]);

    // Bayer dithered Mono1 produces deterministic non-trivial pattern for mid gray (0x8410).
    let grays = [0x8410u16; 8];
    assert_eq!(
        encode_rgb565_row(&grays, 0, 0, PixelFormat::Mono1Msb, DitherMode::Bayer4x4, &mut buf),
        Ok(1)
    );
    assert!(buf[0] != 0x00 && buf[0] != 0xFF);

    // ByteWindowSink::with_format handles RGB666, RGB888, ARGB8888, Gray4, Mono1Msb.
    for (fmt, dither, expected_bytes) in [
        (PixelFormat::Rgb666, DitherMode::None, vec![0xFC, 0x00, 0x00, 0xFC, 0xFC, 0xFC]),
        (PixelFormat::Rgb888, DitherMode::None, vec![0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF]),
        (
            PixelFormat::Argb8888,
            DitherMode::None,
            vec![0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        ),
        (PixelFormat::Mono1Msb, DitherMode::None, vec![0b0100_0000]),
    ] {
        let mut chunk = [0u8; 64];
        let mut sink = ByteWindowSink::with_format(
            RecordingByteBus::default(),
            Rect::new(0, 0, 4, 2),
            fmt,
            dither,
            &mut chunk,
        )
        .unwrap();
        assert_eq!(sink.format(), fmt);
        assert_eq!(sink.dither(), dither);
        sink.flush_rect(Rect::new(0, 0, 2, 1), &[0xF800, 0xFFFF])
            .unwrap();
        assert_eq!(sink.bus().bytes, expected_bytes);
    }

    // Sub-format chunk size checks (Rgb888 requires at least 3 bytes).
    let mut two_bytes = [0u8; 2];
    assert!(
        ByteWindowSink::with_format(
            RecordingByteBus::default(),
            Rect::new(0, 0, 4, 2),
            PixelFormat::Rgb888,
            DitherMode::None,
            &mut two_bytes,
        )
        .is_err()
    );
}
