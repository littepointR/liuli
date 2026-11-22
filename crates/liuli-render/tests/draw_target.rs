use core::convert::Infallible;

use embedded_graphics::Pixel;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Dimensions, OriginDimensions, Point, Size};
use embedded_graphics::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics::primitives::Rectangle;
use liuli_core::capacity::{PresentError, PresentStatus};
use liuli_core::damage::DamageSet;
use liuli_core::geometry::{Rect, rgb565_to_u16, u16_to_rgb565};
use liuli_core::paint::{CommandList, PaintCmd, StaticTextLookup};
use liuli_render::{BandTarget, present_sync, present_sync_with};
use liuli_text::Lf4Font;

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
    let font = Lf4Font::from_bytes(&lf4).unwrap();
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
        &StaticTextLookup,
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
        &StaticTextLookup,
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
        &StaticTextLookup,
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
        &StaticTextLookup,
        &font,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(st2, PresentStatus::Presented);
    assert!(dmg.is_empty());
    assert_eq!(rec_target.fb, iter_target.fb);
    assert_eq!(rgb565_to_u16(u16_to_rgb565(0xF800)), 0xF800);
}
