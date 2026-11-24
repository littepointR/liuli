use liuli::text::{
    Lf4Font,
    storage::{Cached, ReadAt, ReadError, StoredFont},
};
const FONT: &[u8] = include_bytes!("../assets/body.lf4");
#[test]
fn seekable_reads_match_lf4_and_reuse_cached_bitmaps() {
    let mut cache = [0u8; 4096];
    let mut font = StoredFont::open(Cached::new(FONT, &mut cache)).unwrap();
    let mapped = Lf4Font::from_bytes(FONT).unwrap();
    for ch in ['警', '0', 'A'] {
        let mut scratch = [0; 512];
        let g = font.lookup(ch).unwrap().unwrap();
        assert_eq!(
            font.read_bitmap(g, &mut scratch).unwrap(),
            mapped.lookup_exact(ch).unwrap()
        );
        let previous = font.source().metrics().reads;
        font.read_bitmap(g, &mut scratch).unwrap();
        assert_eq!(font.source().metrics().reads, previous);
        assert!(matches!(
            font.read_bitmap(g, &mut []),
            Err(ReadError::Scratch { .. })
        ));
    }
    assert_eq!(font.lookup('🪼').unwrap(), None);
}
struct Broken(bool);
impl ReadAt for Broken {
    type Error = u8;
    fn byte_len(&self) -> usize {
        FONT.len()
    }
    fn read_at(&mut self, _: usize, _: &mut [u8]) -> Result<usize, u8> {
        if self.0 { Err(7) } else { Ok(0) }
    }
}
#[test]
fn distinguishes_format_io_and_truncated_reads() {
    assert!(matches!(
        StoredFont::open(Broken(true)),
        Err(ReadError::Io(7))
    ));
    assert!(matches!(
        StoredFont::open(Broken(false)),
        Err(ReadError::ShortRead)
    ));
    assert!(matches!(
        StoredFont::open(&[0u8; 24][..]),
        Err(ReadError::Format(_))
    ));
}

#[test]
fn font_io_failure_preserves_damage_for_retry() {
    use liuli::prelude::*;
    use liuli::render::sync::FrameError;
    use liuli::text::{GlyphBitmap, storage::GlyphPixels};
    use liuli_alarms::{Alarms, Model};
    use liuli_sim::screen::{MemoryScreen, render_reference_fullscreen};
    struct FailsAfter(usize);
    impl GlyphPixels for FailsAfter {
        type Error = u8;
        fn pixels<'a>(&'a mut self, _: FontId, g: GlyphBitmap<'a>) -> Result<&'a [u8], u8> {
            self.0 = self.0.checked_sub(1).ok_or(9)?;
            Ok(g.data)
        }
    }
    let mut storage = Storage::new();
    let font = Lf4Font::from_bytes(FONT).unwrap();
    let mut app = Engine::<Alarms>::new_in_place(
        &mut storage,
        Model::new(536),
        font,
        Rect::new(0, 0, 240, 536),
        InstantMs(0),
    )
    .unwrap();
    let mut screen = MemoryScreen::new(240, 536);
    let mut band = [0; 240 * 8];
    assert!(matches!(
        app.present_sync_with_pixels(&mut screen, &mut band, |_| Ok(()), &mut FailsAfter(20)),
        Err(FrameError::Font(9))
    ));
    assert!(!app.damage().is_empty());
    app.present_sync(&mut screen, &mut band).unwrap();
    assert!(app.damage().is_empty());
    assert_eq!(
        screen.words(),
        render_reference_fullscreen(app.active_commands(), app.active_tree(), app.fonts()).words()
    );
}
