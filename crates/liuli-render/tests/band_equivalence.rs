use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::damage::DamageSet;
use liuli_core::geometry::{Point, Rect};
use liuli_core::paint::{CommandList, PaintCmd, SliceSceneResources, TextId};
use liuli_core::view::{FontId, IconKind, TextAlign};
use liuli_render::{BandTarget, render_band};
use liuli_text::{FontSet, Lf4Font};

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");
const SMALL12_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/small12.lf4");

fn render_with_band_height(
    cmds: &CommandList,
    resources: &SliceSceneResources<'_>,
    fonts: &FontSet<'_>,
    band_h: u32,
) -> Vec<u16> {
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
        render_band(&mut bt, cmds, resources, fonts);
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
    let fonts = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
    let resources = SliceSceneResources {
        texts: &["设备设置 · 屏幕亮度 60%", "恢复默认设置？\n取消 / 确认"],
        sparklines: &[],
    };
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
    cmds.push(PaintCmd::text(
        Point::new(42, 55),
        Rect::new(42, 55, 200, 18),
        TextId(0),
        Rgb565::new(31, 63, 31),
        240,
        1,
        true,
        FontId::Body16,
        TextAlign::Start,
    ))
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
    cmds.push(PaintCmd::text(
        Point::new(56, 86),
        Rect::new(56, 86, 200, 40),
        TextId(1),
        Rgb565::new(2, 4, 2),
        200,
        2,
        true,
        FontId::Body16,
        TextAlign::Start,
    ))
    .unwrap();

    let ref_240 = render_with_band_height(&cmds, &resources, &fonts, 240);
    for bh in [1u32, 3, 8, 17] {
        let got = render_with_band_height(&cmds, &resources, &fonts, bh);
        assert_eq!(got, ref_240, "mismatch at band height {bh}");
    }
}

#[test]
fn sparkline_dual_font_align_and_all_icons_band_equivalent() {
    let fonts = FontSet::new(
        Lf4Font::from_bytes(SETTINGS_LF4).unwrap(),
        Lf4Font::from_bytes(SMALL12_LF4).unwrap(),
    );
    let zig: Vec<i16> = (0..32).map(|i| if i % 2 == 0 { 0 } else { 100 }).collect();
    let flat = [42i16; 8];
    let one = [75i16];
    let empty: [i16; 0] = [];
    let clamped = [-50i16, 50, 200];

    let resources = SliceSceneResources {
        texts: &[
            "监测仪表盘 · Body16 Center",
            "负载 65% · Small12 End",
            "系统诊断 Start",
        ],
        sparklines: &[&empty, &one, &zig, &flat, &clamped],
    };

    let screen = Rect::new(0, 0, 320, 240);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(1, 2, 1);

    // 0-point empty sparkline
    cmds.push(PaintCmd::Sparkline {
        rect: Rect::new(8, 8, 40, 16),
        clip: screen,
        color: Rgb565::new(0, 63, 0),
        series: 0,
        min: 0,
        max: 100,
        stroke_width: 1,
    })
    .unwrap();
    // 1-point sparkline
    cmds.push(PaintCmd::Sparkline {
        rect: Rect::new(56, 8, 40, 16),
        clip: screen,
        color: Rgb565::new(31, 63, 0),
        series: 1,
        min: 0,
        max: 100,
        stroke_width: 1,
    })
    .unwrap();
    // 32-point zigzag crossing y = 16, 17, 24, 32, 34, 48 band lines
    cmds.push(PaintCmd::Sparkline {
        rect: Rect::new(8, 15, 300, 35),
        clip: screen,
        color: Rgb565::new(0, 63, 31),
        series: 2,
        min: 0,
        max: 100,
        stroke_width: 1,
    })
    .unwrap();
    // Degenerate min == max flat sparkline and 1px height clamped sparkline
    cmds.push(PaintCmd::Sparkline {
        rect: Rect::new(8, 54, 100, 10),
        clip: screen,
        color: Rgb565::new(31, 31, 31),
        series: 3,
        min: 42,
        max: 42,
        stroke_width: 1,
    })
    .unwrap();
    cmds.push(PaintCmd::Sparkline {
        rect: Rect::new(120, 54, 80, 1),
        clip: screen,
        color: Rgb565::new(31, 0, 0),
        series: 4,
        min: 0,
        max: 100,
        stroke_width: 1,
    })
    .unwrap();

    // Dual fonts + TextAlign::{Center, End, Start}
    cmds.push(PaintCmd::text(
        Point::new(10, 72),
        Rect::new(10, 72, 300, 20),
        TextId(0),
        Rgb565::new(31, 63, 31),
        300,
        1,
        true,
        FontId::Body16,
        TextAlign::Center,
    ))
    .unwrap();
    cmds.push(PaintCmd::text(
        Point::new(10, 96),
        Rect::new(10, 96, 300, 16),
        TextId(1),
        Rgb565::new(18, 50, 31),
        300,
        1,
        true,
        FontId::Small12,
        TextAlign::End,
    ))
    .unwrap();
    cmds.push(PaintCmd::text(
        Point::new(10, 114),
        Rect::new(10, 114, 300, 16),
        TextId(2),
        Rgb565::new(31, 63, 0),
        300,
        1,
        true,
        FontId::Small12,
        TextAlign::Start,
    ))
    .unwrap();

    // All 11 non-None icons rendered side by side across band lines
    let icons = [
        IconKind::ChevronRight,
        IconKind::Check,
        IconKind::Warning,
        IconKind::Sliders,
        IconKind::Bell,
        IconKind::Moon,
        IconKind::Thermometer,
        IconKind::Reset,
        IconKind::Gauge,
        IconKind::Waveform,
        IconKind::Diagnostics,
    ];
    for (idx, &kind) in icons.iter().enumerate() {
        let ix = 10 + (idx as i32) * 20;
        cmds.push(PaintCmd::Icon {
            rect: Rect::new(ix, 140, 14, 14),
            clip: screen,
            kind,
            color: Rgb565::new(31, 63, 31),
        })
        .unwrap();
        // Also 6x6 shrunk clipped variant
        cmds.push(PaintCmd::Icon {
            rect: Rect::new(ix, 165, 6, 6),
            clip: Rect::new(ix, 165, 6, 6),
            kind,
            color: Rgb565::new(15, 63, 15),
        })
        .unwrap();
    }

    let ref_240 = render_with_band_height(&cmds, &resources, &fonts, 240);
    for bh in [1u32, 3, 8, 17] {
        let got = render_with_band_height(&cmds, &resources, &fonts, bh);
        assert_eq!(got, ref_240, "Wave 3 scene mismatch at band height {bh}");
    }

    // Verify 1pt sparkline actually drew a non-bg pixel and all 11 14x14 icons are pairwise distinct
    // and never paint outside their declared bounds.
    let small_screen = Rect::new(0, 0, 20, 20);
    let mut icon_buffers: Vec<Vec<u16>> = Vec::new();
    for &kind in &icons {
        let mut c = CommandList::new();
        c.screen = small_screen;
        c.root_bg = Rgb565::new(0, 0, 0);
        let box_rect = Rect::new(3, 3, 14, 14);
        c.push(PaintCmd::Icon {
            rect: box_rect,
            clip: box_rect,
            kind,
            color: Rgb565::new(31, 63, 31),
        })
        .unwrap();
        let buf = render_with_band_height(&c, &SliceSceneResources::EMPTY, &fonts, 8);
        let mut lit = 0usize;
        for y in 0..20i32 {
            for x in 0..20i32 {
                let px = buf[(y as usize) * 20 + (x as usize)];
                if px != 0 {
                    lit += 1;
                    assert!(
                        box_rect.contains_point(Point::new(x, y)),
                        "icon {kind:?} spilled outside bounds at ({x},{y})"
                    );
                }
            }
        }
        assert!(lit > 0, "icon {kind:?} rendered empty");
        assert!(
            !icon_buffers.contains(&buf),
            "icon {kind:?} collided visually with another icon"
        );
        icon_buffers.push(buf);
    }
}
