#![no_std]
#![forbid(unsafe_code)]

#[cfg(all(test, feature = "layers"))]
extern crate alloc;

pub mod accel;
pub mod band;
pub mod bitmap_store;
pub mod display;
pub mod dma;
pub mod drivers;
mod effects;
pub mod raster;
pub mod spi;
pub mod sync;
pub mod vector;
pub mod wire;

pub use accel::{DrawAccel, SoftAccel};
pub use band::{BandIter, BandTarget};
pub use bitmap_store::{StoredBitmap, StoredBitmapError};
pub use display::{
    ByteWindowBus, ByteWindowSink, DisplaySink, EgDisplaySink, RECOMMENDED_WIRE_CHUNK_BYTES,
    RotatedDisplaySink,
};
pub use dma::{
    AlignedBand, BandMeta, BandSlot, CACHE_LINE_BYTES, DmaCacheOps, DmaCoordinator, DmaSink,
    FreeBand, InFlightBand, NoCacheOps, PollOutcome, ReadyBand, SealError, StartOutcome, TeError,
    TeGate, TeMode, TeWait,
};
pub use drivers::{
    Cst816Decoder, DisplayCmdBus, Ft3168Decoder, Gc9a01, Gt911Decoder, Ili9341, PanelConfig,
    Rm67162, St7789, Xpt2046Calibrator,
};
pub use raster::{draw_eg, render_band, render_band_with_accel, render_band_with_provider};
pub use spi::{RECOMMENDED_SPI_CHUNK_BYTES, SpiWindowBus, SpiWindowTarget};
pub use sync::{
    FrameError, present_sink, present_sink_with_blend, present_sink_with_pixels, present_sync,
    present_sync_with, present_sync_with_provider,
};
pub use wire::{
    BufferTooSmall, ByteOrder, encode_rgb565, encode_rgb565_in_place, encode_rgb565_words,
};

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec::Vec;

    use liuli_core::bitmap::{L565_FLAG_ALPHA4, L565_FLAG_CHROMA_KEY, L565_FLAG_RLE};
    use liuli_core::geometry::{Rect, Rgb565, Rotation, Size, rgb565_to_u16};
    use liuli_core::paint::{CommandList, GRADIENT_VERTICAL, PaintCmd};

    use crate::{
        BandTarget, DisplaySink, DrawAccel, RotatedDisplaySink, StoredBitmap, render_band,
        render_band_with_accel,
    };

    const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-alarms/assets/body.lf4");

    #[derive(Default)]
    struct MockSink {
        flushes: Vec<(Rect, Vec<u16>)>,
    }
    impl DisplaySink for MockSink {
        type Error = ();
        fn flush_rect(&mut self, rect: Rect, pixels: &[u16]) -> Result<(), Self::Error> {
            self.flushes.push((rect, pixels.to_vec()));
            Ok(())
        }
    }

    #[test]
    fn stored_bitmap_streams_v1_v2_chroma_a4_and_rle_into_band() {
        let mut rle_a4 = b"L565\x02".to_vec();
        rle_a4.push(L565_FLAG_CHROMA_KEY | L565_FLAG_ALPHA4 | L565_FLAG_RLE);
        rle_a4.extend_from_slice(&2u16.to_le_bytes());
        rle_a4.extend_from_slice(&2u16.to_le_bytes());
        rle_a4.extend_from_slice(&0xF81Fu16.to_le_bytes());
        rle_a4.extend_from_slice(&0u32.to_le_bytes());
        rle_a4.extend_from_slice(&4u32.to_le_bytes());
        // Row 0: repeat 2x red (#F800, alpha=15)
        rle_a4.push(0x81);
        rle_a4.extend_from_slice(&0xF800u16.to_le_bytes());
        rle_a4.push(15);
        // Row 1: literal chroma key (0xF81F, alpha=15 -> becomes 0), blue (0x001F, alpha=15)
        rle_a4.push(0x01);
        rle_a4.extend_from_slice(&0xF81Fu16.to_le_bytes());
        rle_a4.push(15);
        rle_a4.extend_from_slice(&0x001Fu16.to_le_bytes());
        rle_a4.push(15);

        let mut reader: &[u8] = &rle_a4;
        let bmp = StoredBitmap::open(&mut reader).unwrap();
        assert_eq!((bmp.width(), bmp.height()), (2, 2));
        assert!(bmp.has_alpha() && bmp.is_rle());

        let mut band = [0xFFFFu16; 4];
        let mut target = BandTarget::new::<()>(&mut band, Rect::new(0, 0, 2, 2)).unwrap();
        bmp.blit_to_band(
            &mut reader,
            &mut target,
            Rect::new(0, 0, 2, 2),
            Rect::new(0, 0, 2, 2),
            15,
        )
        .unwrap();
        assert_eq!(band[0], 0xF800);
        assert_eq!(band[1], 0xF800);
        assert_eq!(band[2], 0xFFFF); // transparent chroma-key kept background white!
        assert_eq!(band[3], 0x001F);
    }

    #[test]
    fn gradient_rect_and_draw_accel_render_correctly() {
        let mut cmds = CommandList::new();
        cmds.screen = Rect::new(0, 0, 16, 16);
        cmds.root_bg = Rgb565::new(0, 0, 0);
        let start = Rgb565::new(31, 0, 0);
        let end = Rgb565::new(0, 0, 31);
        cmds.push(PaintCmd::FillRect {
            rect: Rect::new(0, 0, 4, 4),
            clip: cmds.screen,
            color: start,
        })
        .unwrap();
        cmds.push(PaintCmd::GradientRect {
            rect: Rect::new(4, 0, 8, 8),
            clip: cmds.screen,
            start,
            end,
            direction: GRADIENT_VERTICAL,
            radius: 0,
        })
        .unwrap();

        struct CountAccel(u32);
        impl DrawAccel for CountAccel {
            fn fill_rect(&mut self, target: &mut BandTarget<'_>, rect: Rect, color: Rgb565) {
                self.0 += 1;
                let local = rect
                    .intersect(target.band_rect())
                    .translate(-target.band_rect().x, -target.band_rect().y);
                let _ = embedded_graphics::draw_target::DrawTarget::fill_solid(
                    target,
                    &local.to_eg(),
                    color,
                );
            }
        }

        let mut words = [0u16; 16 * 16];
        let mut target = BandTarget::new::<()>(&mut words, cmds.screen).unwrap();
        let fonts =
            liuli_text::FontSet::single(liuli_text::Lf4Font::from_bytes(SETTINGS_LF4).unwrap());
        let mut accel = CountAccel(0);
        render_band_with_accel(
            &mut target,
            &cmds,
            &liuli_core::paint::SliceSceneResources::EMPTY,
            &fonts,
            &mut accel,
        );
        assert_eq!(accel.0, 1);
        assert_eq!(words[0], rgb565_to_u16(start));
        // Top of vertical gradient matches start, bottom matches end, middle blends cleanly.
        assert_eq!(words[4], rgb565_to_u16(start));
        assert_eq!(words[7 * 16 + 4], rgb565_to_u16(end));
        assert!(words[3 * 16 + 4] != rgb565_to_u16(start));

        // Verify reference render_band produces identical pixels for GradientRect.
        let mut ref_words = [0u16; 16 * 16];
        let mut ref_target = BandTarget::new::<()>(&mut ref_words, cmds.screen).unwrap();
        render_band(
            &mut ref_target,
            &cmds,
            &liuli_core::paint::SliceSceneResources::EMPTY,
            &fonts,
        );
        assert_eq!(words, ref_words);
    }

    #[test]
    fn rotated_display_sink_tiles_and_orients_pixels() {
        let phys = Size::new(4, 6); // Logical Deg90 size = 6x4
        // Intentionally tiny MAX_PIXELS = 4 to exercise multi-tile streaming!
        let mut sink =
            RotatedDisplaySink::<MockSink, 4>::new(MockSink::default(), phys, Rotation::Deg90);
        // Write 2x2 logical band at (0, 0) with unique corner colors [1, 2, 3, 4]
        sink.flush_rect(Rect::new(0, 0, 2, 2), &[1, 2, 3, 4])
            .unwrap();
        let mut fb = [0u16; 24];
        for (rect, px) in &sink.inner().flushes {
            for ry in 0..rect.h as usize {
                for rx in 0..rect.w as usize {
                    fb[(rect.y as usize + ry) * 4 + (rect.x as usize + rx)] =
                        px[ry * rect.w as usize + rx];
                }
            }
        }
        let check = |lx: i32, ly: i32, expected: u16| {
            let p = Rotation::Deg90
                .map_point_to_physical(liuli_core::geometry::Point::new(lx, ly), phys);
            assert_eq!(fb[p.y as usize * 4 + p.x as usize], expected);
        };
        check(0, 0, 1);
        check(1, 0, 2);
        check(0, 1, 3);
        check(1, 1, 4);
    }
}
