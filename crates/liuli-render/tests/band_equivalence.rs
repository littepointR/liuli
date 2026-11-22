use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::damage::DamageSet;
use liuli_core::geometry::{Point, Rect};
use liuli_core::paint::{CommandList, PaintCmd, StaticTextLookup};
use liuli_core::view::{IconKind, TextRef};
use liuli_render::{BandTarget, render_band};
use liuli_text::Lf4Font;

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

fn render_with_band_height(cmds: &CommandList, font: &Lf4Font<'_>, band_h: u32) -> Vec<u16> {
    let w = cmds.screen.w as usize;
    let h = cmds.screen.h as usize;
    let mut full = vec![0u16; w * h];
    let mut scratch = vec![0u16; w * (band_h as usize)];

    let mut dmg = DamageSet::new();
    dmg.mark_full_screen(cmds.screen);

    for band in
        liuli_render::BandIter::new::<()>(dmg.as_slice(), cmds.screen, band_h, scratch.len())
            .unwrap()
    {
        let mut bt = BandTarget::new::<()>(&mut scratch, band).unwrap();
        render_band(&mut bt, cmds, &StaticTextLookup, font);
        let words = bt.words();
        for ry in 0..band.h as usize {
            let gy = band.y as usize + ry;
            let dst = gy * w + band.x as usize;
            let src = ry * band.w as usize;
            full[dst..dst + band.w as usize].copy_from_slice(&words[src..src + band.w as usize]);
        }
    }
    full
}

#[test]
fn band_heights_1_3_8_17_240_produce_identical_pixels_including_chinese_text() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 320, 240);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(2, 4, 3);

    cmds.push(PaintCmd::FillRect {
        rect: Rect::new(0, 0, 320, 32),
        clip: screen,
        color: Rgb565::new(4, 8, 6),
    })
    .unwrap();
    cmds.push(PaintCmd::FillRoundedRect {
        rect: Rect::new(10, 40, 300, 48),
        clip: screen,
        color: Rgb565::new(6, 12, 8),
        radius: 8,
        border_color: Some(Rgb565::new(18, 36, 22)),
        border_width: 2,
    })
    .unwrap();
    cmds.push(PaintCmd::Icon {
        rect: Rect::new(20, 56, 14, 14),
        clip: screen,
        kind: IconKind::Sliders,
        color: Rgb565::new(31, 63, 31),
    })
    .unwrap();
    // Chinese text intentionally crossing band boundaries (y=55 crosses y=56, 64, 68, etc.)
    cmds.push(PaintCmd::Text {
        pos: Point::new(42, 55),
        clip: Rect::new(40, 42, 260, 44),
        ink: Rect::new(42, 55, 200, 18),
        text: TextRef::Static("设备设置 · 屏幕亮度 60%"),
        color: Rgb565::new(31, 63, 31),
        max_width: 240,
        max_lines: 1,
        ellipsis: true,
    })
    .unwrap();
    cmds.push(PaintCmd::StrokeRect {
        rect: Rect::new(8, 38, 304, 52),
        clip: screen,
        color: Rgb565::new(31, 48, 0),
        width: 2,
    })
    .unwrap();
    cmds.push(PaintCmd::Overlay {
        rect: screen,
        clip: screen,
        color: Rgb565::new(0, 0, 0),
        alpha: 9,
    })
    .unwrap();
    cmds.push(PaintCmd::FillRoundedRect {
        rect: Rect::new(40, 70, 240, 100),
        clip: screen,
        color: Rgb565::new(30, 60, 30),
        radius: 10,
        border_color: Some(Rgb565::new(10, 20, 10)),
        border_width: 1,
    })
    .unwrap();
    cmds.push(PaintCmd::Text {
        pos: Point::new(56, 86),
        clip: Rect::new(42, 72, 236, 96),
        ink: Rect::new(56, 86, 200, 40),
        text: TextRef::Static("恢复默认设置？\n取消 / 确认"),
        color: Rgb565::new(2, 4, 2),
        max_width: 200,
        max_lines: 2,
        ellipsis: true,
    })
    .unwrap();

    let ref_240 = render_with_band_height(&cmds, &font, 240);
    for bh in [1u32, 3, 8, 17] {
        let got = render_with_band_height(&cmds, &font, bh);
        assert_eq!(got, ref_240, "mismatch at band height {bh}");
    }
}
