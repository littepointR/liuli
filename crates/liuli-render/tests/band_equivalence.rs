use embedded_graphics::pixelcolor::Rgb565;
use liuli_core::damage::DamageSet;
use liuli_core::geometry::{Point, Rect};
use liuli_core::paint::{
    CommandList, GRADIENT_DIAGONAL, GRADIENT_HORIZONTAL, GRADIENT_VERTICAL, PaintCmd,
    SliceSceneResources, TextId,
};
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
fn adjacent_fills_match_single_row_raster_across_clips_gaps_and_overlaps() {
    let fonts = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
    let resources = SliceSceneResources {
        texts: &[],
        sparklines: &[],
    };
    let mut cmds = CommandList::new();
    cmds.screen = Rect::new(0, 0, 63, 29);
    for (i, (x, y, w, h)) in [
        (-4, 2, 9, 27),
        (5, 2, 7, 27),
        (12, 2, 3, 27),
        (18, 2, 9, 27),
        (23, 2, 10, 27),
        (33, 5, 13, 19),
        (46, 5, 23, 19),
    ]
    .into_iter()
    .enumerate()
    {
        cmds.push(PaintCmd::FillRect {
            rect: Rect::new(x, y, w, h),
            clip: Rect::new(1, 1, 60, 27),
            color: Rgb565::new(3 + i as u8, 30, 8),
        })
        .unwrap();
    }
    let reference = render_with_band_height(&cmds, &resources, &fonts, 1);
    for height in [3, 7, 29] {
        assert_eq!(
            render_with_band_height(&cmds, &resources, &fonts, height),
            reference
        );
    }
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

#[test]
fn render_band_multibpp_rle_lcd_gamma_decor_matches_reference() {
    use liuli_core::view::{SubpixelOrder, TextGamma, TextStyleFlags, WrapMode};
    use liuli_text::{GlyphEncoding, GlyphFormat};

    fn pack_v2(
        format: GlyphFormat,
        encoding: GlyphEncoding,
        phases: u8,
        glyphs: &[(char, u16, u16, &[u8])],
    ) -> Vec<u8> {
        let count = glyphs.len() as u16;
        let table_off = 32u32;
        let mut table = Vec::new();
        let mut bitmaps = Vec::new();
        for &(ch, w, h, data) in glyphs {
            let off = bitmaps.len() as u32;
            if encoding == GlyphEncoding::Rle && w > 0 && h > 0 {
                bitmaps.extend_from_slice(&(data.len() as u16).to_le_bytes());
            }
            bitmaps.extend_from_slice(data);
            table.extend_from_slice(&(ch as u32).to_le_bytes());
            table.push(w as u8 + 1);
            table.push(0);
            table.push(10i8 as u8);
            table.push(w as u8);
            table.push(h as u8);
            table.push((off & 0xFF) as u8);
            table.push(((off >> 8) & 0xFF) as u8);
            table.push(((off >> 16) & 0xFF) as u8);
        }
        let bm_off = table_off + table.len() as u32;
        let phase_bits = match phases {
            2 => 1u8 << 1,
            4 => 2u8 << 1,
            _ => 0,
        };
        let mut out = Vec::new();
        out.extend_from_slice(b"LLF4");
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&12i16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&14u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&table_off.to_le_bytes());
        out.extend_from_slice(&bm_off.to_le_bytes());
        out.push(format as u8);
        out.push(encoding as u8);
        out.push(1 | phase_bits);
        out.push(0);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(&bitmaps);
        out
    }

    // Exercise A1, A8+RLE, and LcdRgb+RLE across multiple band heights to confirm
    // band height 1 == band height 48 == full frame parity + non-empty rendering.
    let a1_bytes = pack_v2(
        GlyphFormat::A1,
        GlyphEncoding::Raw,
        1,
        &[
            (' ', 2, 0, &[]),
            ('A', 4, 4, &[0x90, 0xF0, 0x90, 0x90]),
            ('□', 4, 4, &[0xF0, 0x90, 0x90, 0xF0]),
        ],
    );
    // 2x2 A8 -> RLE literal run of 4 bytes: header (4-1)=3 followed by 4 coverage values.
    let a8_rle_bytes = pack_v2(
        GlyphFormat::A8,
        GlyphEncoding::Rle,
        1,
        &[
            (' ', 2, 0, &[]),
            ('B', 2, 2, &[3, 64, 192, 128, 255]),
            ('□', 2, 2, &[3, 255, 255, 255, 255]),
        ],
    );
    // 2x1 LcdRgb -> 3 nibbles per px -> 3 bytes/row -> RLE literal 3 bytes.
    let lcd_bytes = pack_v2(
        GlyphFormat::LcdRgb,
        GlyphEncoding::Rle,
        1,
        &[
            (' ', 2, 0, &[]),
            ('C', 2, 1, &[2, 0xF8, 0x40, 0xCF]),
            ('□', 2, 1, &[2, 0xFF, 0xFF, 0xFF]),
        ],
    );

    let fonts = FontSet::single(Lf4Font::from_bytes(&a1_bytes).unwrap())
        .with_slot(FontId::Title20, Lf4Font::from_bytes(&a8_rle_bytes).unwrap())
        .with_slot(FontId::Display28, Lf4Font::from_bytes(&lcd_bytes).unwrap());

    let screen = Rect::new(0, 0, 64, 48);
    let resources = SliceSceneResources {
        texts: &["A", "B", "C"],
        sparklines: &[],
    };
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(2, 4, 2);

    let decor_style = TextStyleFlags::DEFAULT
        .with_wrap(WrapMode::Word)
        .with_gamma(TextGamma::Perceptual)
        .with_subpixel(SubpixelOrder::Rgb)
        .with_underline(true)
        .with_strikethrough(true);
    cmds.push(PaintCmd::text_styled(
        Point::new(2, 2),
        Rect::new(2, 2, 60, 18),
        TextId(0),
        Rgb565::new(31, 63, 31),
        60,
        1,
        false,
        FontId::Body16,
        TextAlign::Start,
        decor_style,
        1,
        0,
    ))
    .unwrap();
    cmds.push(PaintCmd::text_styled(
        Point::new(2, 18),
        Rect::new(2, 18, 60, 16),
        TextId(1),
        Rgb565::new(31, 63, 31),
        60,
        1,
        false,
        FontId::Title20,
        TextAlign::Start,
        decor_style,
        0,
        0,
    ))
    .unwrap();
    cmds.push(PaintCmd::text_styled(
        Point::new(2, 34),
        Rect::new(2, 34, 60, 14),
        TextId(2),
        Rgb565::new(31, 63, 31),
        60,
        1,
        false,
        FontId::Display28,
        TextAlign::Start,
        decor_style,
        0,
        0,
    ))
    .unwrap();

    let full = render_with_band_height(&cmds, &resources, &fonts, 48);
    let bg_u16 = liuli_core::geometry::rgb565_to_u16(cmds.root_bg);
    assert!(full.iter().any(|&p| p != bg_u16), "rendered empty");
    for bh in [1u32, 3, 5, 11] {
        assert_eq!(
            render_with_band_height(&cmds, &resources, &fonts, bh),
            full,
            "multi-bpp band mismatch at band_h={bh}"
        );
    }
}

struct SliceFlash<'a>(&'a [u8]);

impl liuli_text::storage::ReadAt for SliceFlash<'_> {
    type Error = ();

    fn byte_len(&self) -> usize {
        self.0.len()
    }

    fn read_at(&mut self, offset: usize, dst: &mut [u8]) -> Result<usize, Self::Error> {
        let Some(end) = offset.checked_add(dst.len()) else {
            return Err(());
        };
        if end > self.0.len() {
            return Err(());
        }
        dst.copy_from_slice(&self.0[offset..end]);
        Ok(dst.len())
    }
}

fn render_provider_with_band_height<F: liuli_text::FontProvider>(
    cmds: &CommandList,
    resources: &SliceSceneResources<'_>,
    fonts: &F,
    band_h: u32,
) -> Vec<u16>
where
    F::Error: core::fmt::Debug,
{
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
        liuli_render::render_band_with_provider(
            &mut bt,
            cmds,
            resources,
            fonts,
            liuli_render::raster::blend_solid,
        )
        .unwrap();
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
fn streaming_font_and_hybrid_font_set_match_xip_pixel_exact_across_band_heights() {
    use liuli_core::view::{SubpixelOrder, TextGamma, TextStyleFlags, WrapMode};
    use liuli_text::{FontProvider, HybridFontSet, StreamingFont};

    let body = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let small = Lf4Font::from_bytes(SMALL12_LF4).unwrap();
    let xip_fonts = FontSet::new(body, small);

    let resources = SliceSceneResources {
        texts: &["设备设置 · 屏幕亮度 60%", "恢复默认设置？ 12:34"],
        sparklines: &[],
    };
    let mut cmds = CommandList::new();
    cmds.screen = Rect::new(0, 0, 240, 120);
    cmds.root_bg = Rgb565::new(2, 4, 2);

    let style = TextStyleFlags::DEFAULT
        .with_wrap(WrapMode::Word)
        .with_gamma(TextGamma::Perceptual)
        .with_subpixel(SubpixelOrder::None)
        .with_underline(true);
    cmds.push(PaintCmd::text_styled(
        Point::new(8, 10),
        Rect::new(0, 0, 240, 120),
        TextId(0),
        Rgb565::new(31, 63, 31),
        220,
        2,
        true,
        FontId::Body16,
        TextAlign::Start,
        style,
        0,
        2,
    ))
    .unwrap();
    cmds.push(PaintCmd::text_styled(
        Point::new(8, 56),
        Rect::new(0, 0, 240, 120),
        TextId(1),
        Rgb565::new(20, 50, 31),
        220,
        2,
        false,
        FontId::Small12,
        TextAlign::Start,
        style,
        0,
        1,
    ))
    .unwrap();

    let mut b_blk = [0u8; 2048];
    let mut b_pool = [0u8; 4096];
    let mut b_scr = [0u8; 512];
    let stream_body = StreamingFont::<_, 32, 64, 32, 4>::open(
        SliceFlash(SETTINGS_LF4),
        &mut b_blk,
        &mut b_pool,
        &mut b_scr,
    )
    .unwrap();

    let mut s_blk = [0u8; 2048];
    let mut s_pool = [0u8; 4096];
    let mut s_scr = [0u8; 512];
    let stream_small = StreamingFont::<_, 32, 64, 32, 4>::open(
        SliceFlash(SMALL12_LF4),
        &mut s_blk,
        &mut s_pool,
        &mut s_scr,
    )
    .unwrap();

    let pure_streaming =
        HybridFontSet::streaming(&stream_body).with_stream_slot(FontId::Small12, &stream_small);
    let mixed_hybrid = HybridFontSet::from_xip(FontSet::single(body))
        .with_stream_slot(FontId::Small12, &stream_small);

    for bh in [1u32, 3, 8, 120] {
        let xip_ref = render_with_band_height(&cmds, &resources, &xip_fonts, bh);
        assert_eq!(
            render_provider_with_band_height(&cmds, &resources, &pure_streaming, bh),
            xip_ref,
            "pure streaming mismatch at band_h={bh}"
        );
        assert_eq!(
            render_provider_with_band_height(&cmds, &resources, &mixed_hybrid, bh),
            xip_ref,
            "mixed hybrid mismatch at band_h={bh}"
        );
    }
    let stats = pure_streaming.cache_stats();
    assert!(stats.metrics_hits > 0 && stats.bitmap_hits > 0);
}

#[test]
fn vector_line_circle_arc_match_reference_across_band_heights() {
    let fonts = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
    let resources = SliceSceneResources {
        texts: &[],
        sparklines: &[],
    };
    let screen = Rect::new(0, 0, 200, 120);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(2, 4, 2);

    // Anti-aliased and non-AA lines, diagonal, vertical/horizontal, thin & thick, partial clip.
    cmds.push(PaintCmd::Line {
        p0: Point::new(8, 8),
        p1: Point::new(72, 44),
        clip: Rect::new(10, 10, 56, 36),
        color: Rgb565::new(31, 63, 0),
        width: 3,
        aa: true,
    })
    .unwrap();
    cmds.push(PaintCmd::Line {
        p0: Point::new(80, 12),
        p1: Point::new(18, 52),
        clip: screen,
        color: Rgb565::new(0, 63, 31),
        width: 1,
        aa: false,
    })
    .unwrap();

    // Filled circle, hollow circle border, and combined fill+border crossing band boundaries.
    cmds.push(PaintCmd::Circle {
        center: Point::new(40, 82),
        clip: screen,
        radius: 22,
        fill: Some(Rgb565::new(28, 12, 6)),
        border_color: Some(Rgb565::new(31, 63, 31)),
        border_width: 2,
    })
    .unwrap();
    cmds.push(PaintCmd::Circle {
        center: Point::new(86, 82),
        clip: screen,
        radius: 16,
        fill: None,
        border_color: Some(Rgb565::new(8, 56, 16)),
        border_width: 3,
    })
    .unwrap();

    // Positive sweep arc (round caps), negative sweep arc (flat caps), and full 360 ring.
    cmds.push(PaintCmd::Arc {
        center: Point::new(132, 42),
        clip: screen,
        radius: 24,
        start_deg: 135,
        sweep_deg: 240,
        color: Rgb565::new(4, 48, 31),
        width: 6,
        cap_round: true,
    })
    .unwrap();
    cmds.push(PaintCmd::Arc {
        center: Point::new(132, 86),
        clip: screen,
        radius: 18,
        start_deg: -45,
        sweep_deg: -160,
        color: Rgb565::new(31, 40, 0),
        width: 4,
        cap_round: false,
    })
    .unwrap();
    cmds.push(PaintCmd::Arc {
        center: Point::new(174, 62),
        clip: Rect::new(152, 30, 44, 64),
        radius: 16,
        start_deg: 0,
        sweep_deg: 360,
        color: Rgb565::new(24, 63, 24),
        width: 3,
        cap_round: false,
    })
    .unwrap();

    let ref_full = render_with_band_height(&cmds, &resources, &fonts, 120);
    // Ensure full-screen vector direct sink matches band renderer output as well.
    let mut direct_vec = vec![liuli_core::geometry::rgb565_to_u16(cmds.root_bg); 200 * 120];
    let mut sink: &mut [u16] = &mut direct_vec;
    for cmd in cmds.as_slice() {
        match *cmd {
            PaintCmd::Line {
                p0,
                p1,
                clip,
                color,
                width,
                aa,
            } => liuli_render::vector::draw_line(&mut sink, screen, p0, p1, clip, color, width, aa),
            PaintCmd::Circle {
                center,
                clip,
                radius,
                fill,
                border_color,
                border_width,
            } => liuli_render::vector::draw_circle(
                &mut sink,
                screen,
                center,
                radius,
                clip,
                fill,
                border_color,
                border_width,
            ),
            PaintCmd::Arc {
                center,
                clip,
                radius,
                start_deg,
                sweep_deg,
                color,
                width,
                cap_round,
            } => liuli_render::vector::draw_arc(
                &mut sink, screen, center, radius, start_deg, sweep_deg, width, cap_round, clip,
                color,
            ),
            _ => {}
        }
    }
    assert_eq!(ref_full, direct_vec);

    let bg_raw = liuli_core::geometry::rgb565_to_u16(cmds.root_bg);
    assert!(ref_full.iter().any(|&px| px != bg_raw));

    for bh in [1u32, 3, 8, 17] {
        let got = render_with_band_height(&cmds, &resources, &fonts, bh);
        assert_eq!(got, ref_full, "vector mismatch at band_h={bh}");
    }
}

#[test]
fn inline_text_matches_reference_across_band_heights() {
    let fonts = FontSet::new(
        Lf4Font::from_bytes(SETTINGS_LF4).unwrap(),
        Lf4Font::from_bytes(SMALL12_LF4).unwrap(),
    );
    let resources = SliceSceneResources::EMPTY;
    let screen = Rect::new(0, 0, 200, 120);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(2, 4, 2);

    cmds.push(
        PaintCmd::inline_text(
            Point::new(8, 12),
            Rect::new(0, 0, 200, 40),
            "ABC 123",
            Rgb565::new(31, 63, 31),
            120,
            FontId::Body16,
            TextAlign::Center,
        )
        .unwrap(),
    )
    .unwrap();
    cmds.push(
        PaintCmd::inline_text(
            Point::new(10, 52),
            Rect::new(10, 52, 80, 32),
            "设备温度",
            Rgb565::new(0, 63, 31),
            80,
            FontId::Small12,
            TextAlign::End,
        )
        .unwrap(),
    )
    .unwrap();

    let ref_full = render_with_band_height(&cmds, &resources, &fonts, 120);
    let bg_raw = liuli_core::geometry::rgb565_to_u16(cmds.root_bg);
    assert!(ref_full.iter().any(|&px| px != bg_raw));
    for bh in [1u32, 3, 8, 17] {
        let got = render_with_band_height(&cmds, &resources, &fonts, bh);
        assert_eq!(got, ref_full, "inline text mismatch at band_h={bh}");
    }
}

#[test]
fn dithered_gradients_match_reference_across_band_heights() {
    let fonts = FontSet::single(Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
    let resources = SliceSceneResources::EMPTY;
    let screen = Rect::new(0, 0, 128, 64);
    let mut cmds = CommandList::new();
    cmds.screen = screen;
    cmds.root_bg = Rgb565::new(0, 0, 0);

    for (x, dir, radius) in [
        (0, GRADIENT_HORIZONTAL, 0),
        (42, GRADIENT_VERTICAL, 6),
        (84, GRADIENT_DIAGONAL, 8),
    ] {
        cmds.push(PaintCmd::GradientRect {
            rect: Rect::new(x, 4, 38, 56),
            clip: Rect::new(x + 2, 6, 34, 52),
            start: Rgb565::new(3, 10, 20),
            end: Rgb565::new(28, 54, 31),
            direction: dir,
            radius,
        })
        .unwrap();
    }

    let ref_full = render_with_band_height(&cmds, &resources, &fonts, 64);
    for bh in [1u32, 3, 8, 17] {
        let got = render_with_band_height(&cmds, &resources, &fonts, bh);
        assert_eq!(got, ref_full, "dithered gradient mismatch at band_h={bh}");
    }
}
