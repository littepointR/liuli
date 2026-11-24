use core::cell::Cell;

use embedded_graphics_core::Pixel;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use liuli::prelude::*;
use liuli::render::sync::FrameError;
use liuli::text::storage::{ReadAt, ReadError};

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");
const SMALL12_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/small12.lf4");

struct BufferTarget {
    w: u32,
    h: u32,
    pixels: Vec<u16>,
}

impl BufferTarget {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            pixels: vec![0; (w * h) as usize],
        }
    }
}

impl OriginDimensions for BufferTarget {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for BufferTarget {
    type Color = Rgb565;
    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.x < self.w as i32 && p.y >= 0 && p.y < self.h as i32 {
                self.pixels[(p.y as usize) * (self.w as usize) + (p.x as usize)] = c.into_storage();
            }
        }
        Ok(())
    }
}

struct FallibleFlash<'a> {
    data: &'a [u8],
    fail_io: &'a Cell<bool>,
    reads: &'a Cell<u32>,
}

impl ReadAt for FallibleFlash<'_> {
    type Error = ();

    fn byte_len(&self) -> usize {
        self.data.len()
    }

    fn read_at(&mut self, offset: usize, dst: &mut [u8]) -> Result<usize, Self::Error> {
        self.reads.set(self.reads.get().saturating_add(1));
        if self.fail_io.get() {
            return Err(());
        }
        let Some(end) = offset.checked_add(dst.len()) else {
            return Err(());
        };
        if end > self.data.len() {
            return Err(());
        }
        dst.copy_from_slice(&self.data[offset..end]);
        Ok(dst.len())
    }
}

struct StreamModel {
    tick: u32,
}

struct StreamApp;

impl App for StreamApp {
    type Model = StreamModel;
    type Msg = ();

    fn update(model: &mut Self::Model, _msg: Self::Msg, _now: InstantMs) -> Change {
        model.tick += 1;
        Change::View
    }

    fn view(_model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let mut root = NodeProps::new();
        root.bg = Some(Rgb565::new(2, 4, 2));
        root.width = Length::Px(160);
        root.height = Length::Px(96);
        out.begin(Key(1), NodeKind::Root, root)?;

        let mut col = NodeProps::new();
        col.width = Length::Fill(1);
        col.height = Length::Fill(1);
        col.padding = EdgeInsets::all(8);
        col.gap = 8;
        out.begin(Key(2), NodeKind::Column, col)?;

        let mut p1 = NodeProps::new();
        p1.width = Length::Px(144);
        p1.height = Length::Px(24);
        p1.font = FontId::Body16;
        p1.text = TextRef::Static("设备设置 60%");
        p1.fg = Rgb565::new(31, 63, 31);
        out.leaf(Key(3), NodeKind::Label, p1)?;

        let mut p2 = NodeProps::new();
        p2.width = Length::Px(144);
        p2.height = Length::Px(20);
        p2.font = FontId::Small12;
        p2.text = TextRef::Static("屏幕亮度 12:34");
        p2.fg = Rgb565::new(20, 56, 31);
        out.leaf(Key(4), NodeKind::Label, p2)?;

        out.end()?;
        out.end()?;
        Ok(())
    }

    fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
        None
    }
}

#[test]
fn engine_streaming_matches_xip_and_recovers_from_io_fault() {
    let screen = Rect::new(0, 0, 160, 96);
    let mut scratch = [0u16; 160 * 16];

    let body_xip = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let small_xip = Lf4Font::from_bytes(SMALL12_LF4).unwrap();
    let mut xip_storage = Storage::new();
    let mut xip_engine = Engine::<StreamApp>::new_in_place_with_fonts(
        &mut xip_storage,
        StreamModel { tick: 0 },
        FontSet::new(body_xip, small_xip),
        screen,
        InstantMs(0),
    )
    .unwrap();
    let mut xip_target = BufferTarget::new(160, 96);
    assert_eq!(
        xip_engine
            .present_sync(&mut xip_target, &mut scratch)
            .unwrap(),
        PresentStatus::Presented
    );

    let fail_io = Cell::new(false);
    let reads = Cell::new(0);
    let mut b_blk = [0u8; 2048];
    let mut b_pool = [0u8; 4096];
    let mut b_scr = [0u8; 512];
    let stream_body = StreamingFont::<_, 32, 64, 32, 4>::open(
        FallibleFlash {
            data: SETTINGS_LF4,
            fail_io: &fail_io,
            reads: &reads,
        },
        &mut b_blk,
        &mut b_pool,
        &mut b_scr,
    )
    .unwrap();

    let mut s_blk = [0u8; 2048];
    let mut s_pool = [0u8; 4096];
    let mut s_scr = [0u8; 512];
    let stream_small = StreamingFont::<_, 32, 64, 32, 4>::open(
        FallibleFlash {
            data: SMALL12_LF4,
            fail_io: &fail_io,
            reads: &reads,
        },
        &mut s_blk,
        &mut s_pool,
        &mut s_scr,
    )
    .unwrap();

    let hybrid =
        HybridFontSet::streaming(&stream_body).with_stream_slot(FontId::Small12, &stream_small);
    let mut storage = Storage::new();
    let mut engine = Engine::<StreamApp, _>::new_in_place_with_provider(
        &mut storage,
        StreamModel { tick: 0 },
        hybrid,
        screen,
        InstantMs(0),
    )
    .unwrap();

    // Clear L1/L2/L3 caches and inject mid-frame I/O error: Engine must preserve damage.
    FontProvider::clear_caches(engine.font_provider());
    fail_io.set(true);
    let mut stream_target = BufferTarget::new(160, 96);
    let err = engine
        .present_sync_streaming(&mut stream_target, &mut scratch)
        .unwrap_err();
    assert!(matches!(err, FrameError::Font(ReadError::Io(()))));
    assert!(!engine.damage().is_empty());

    // Recover once Flash I/O succeeds: full frame matches XIP pixel-exact.
    fail_io.set(false);
    assert_eq!(
        engine
            .present_sync_streaming(&mut stream_target, &mut scratch)
            .unwrap(),
        PresentStatus::Presented
    );
    assert!(engine.damage().is_empty());
    assert_eq!(stream_target.pixels, xip_target.pixels);

    // Cached redraw hits L1 metrics + L2 glyph pool with 0 extra external Flash reads.
    let reads_before_redraw = reads.get();
    engine.invalidate_screen().unwrap();
    assert_eq!(
        engine
            .present_sync_streaming(&mut stream_target, &mut scratch)
            .unwrap(),
        PresentStatus::Presented
    );
    assert_eq!(reads.get(), reads_before_redraw);
    let stats = engine.font_cache_stats();
    assert!(stats.metrics_hits > 0 && stats.bitmap_hits > 0);
}

#[test]
fn engine_reload_fonts_remeasures_and_redraws() {
    let screen = Rect::new(0, 0, 160, 96);
    let mut scratch = [0u16; 160 * 16];

    let small_xip = Lf4Font::from_bytes(SMALL12_LF4).unwrap();
    let settings_xip = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();

    let mut expected_storage = Storage::new();
    let mut expected_engine = Engine::<StreamApp>::new_in_place_with_fonts(
        &mut expected_storage,
        StreamModel { tick: 0 },
        FontSet::new(settings_xip, small_xip),
        screen,
        InstantMs(0),
    )
    .unwrap();
    let mut expected_target = BufferTarget::new(160, 96);
    expected_engine
        .present_sync(&mut expected_target, &mut scratch)
        .unwrap();

    let fail_io = Cell::new(false);
    let reads = Cell::new(0);
    let mut s_blk = [0u8; 2048];
    let mut s_pool = [0u8; 4096];
    let mut s_scr = [0u8; 512];
    let stream_small = StreamingFont::<_, 32, 64, 32, 4>::open(
        FallibleFlash {
            data: SMALL12_LF4,
            fail_io: &fail_io,
            reads: &reads,
        },
        &mut s_blk,
        &mut s_pool,
        &mut s_scr,
    )
    .unwrap();

    let mut b_blk = [0u8; 2048];
    let mut b_pool = [0u8; 4096];
    let mut b_scr = [0u8; 512];
    let stream_settings = StreamingFont::<_, 32, 64, 32, 4>::open(
        FallibleFlash {
            data: SETTINGS_LF4,
            fail_io: &fail_io,
            reads: &reads,
        },
        &mut b_blk,
        &mut b_pool,
        &mut b_scr,
    )
    .unwrap();

    // Start with small12 everywhere, then hot-reload Body16 to settings.lf4.
    let initial = HybridFontSet::streaming(&stream_small);
    let mut storage = Storage::new();
    let mut engine = Engine::<StreamApp, _>::new_in_place_with_provider(
        &mut storage,
        StreamModel { tick: 0 },
        initial,
        screen,
        InstantMs(0),
    )
    .unwrap();
    let mut target = BufferTarget::new(160, 96);
    engine
        .present_sync_streaming(&mut target, &mut scratch)
        .unwrap();
    let before = target.pixels.clone();

    let updated =
        HybridFontSet::streaming(&stream_settings).with_stream_slot(FontId::Small12, &stream_small);
    engine.reload_fonts(updated, InstantMs(10)).unwrap();
    assert!(!engine.damage().is_empty());
    engine
        .present_sync_streaming(&mut target, &mut scratch)
        .unwrap();
    assert_ne!(target.pixels, before);
    assert_eq!(target.pixels, expected_target.pixels);
}
