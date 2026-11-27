#![cfg(feature = "desktop")]
use liuli::prelude::*;
use liuli_sim::session::AppSession;
use liuli_widget_demo::showcase::showcase_fonts;
const WHITE: Rgb565 = Rgb565::new(31, 63, 31);
const BLUE: Rgb565 = Rgb565::new(0, 0, 31);
const GREEN: Rgb565 = Rgb565::new(0, 63, 0);
const RED: Rgb565 = Rgb565::new(31, 0, 0);

#[test]
fn bitmap_mask_composites_overlapping_paths_once_at_every_band_height() {
    use liuli::core::paint::{CommandList, PaintCmd};
    use liuli::render::{BandTarget, render_band};
    const ALPHA: Bitmap =
        Bitmap::from_asset(b"L565\x02\x08\x03\x00\x01\x00\x00\x00\xB2\x0F").unwrap();
    const IMAGE_MASK: SurfaceComposition = SurfaceComposition {
        mask: Mask::Alpha(&ALPHA),
        ..SurfaceComposition::DEFAULT
    };
    let mut cmds = CommandList::new();
    let screen = Rect::new(0, 0, 49, 39);
    let rect = Rect::new(1, 2, 45, 35);
    cmds.screen = screen;
    cmds.root_bg = WHITE;
    cmds.push(PaintCmd::MaskGroup {
        rect,
        clip: screen,
        end: 3,
        radius: 0,
        composition: &IMAGE_MASK,
    })
    .unwrap();
    cmds.push(PaintCmd::FillRect {
        rect,
        clip: screen,
        color: RED,
    })
    .unwrap();
    cmds.push(PaintCmd::StaticPath {
        rect,
        clip: screen,
        path: &PATH,
        paint: PathPaint::Fill(FillRule::EvenOdd),
        color: BLUE,
    })
    .unwrap();
    let fonts = showcase_fonts().unwrap();
    let resources = SliceSceneResources {
        texts: &[],
        sparklines: &[],
    };
    let expected = liuli_sim::screen::render_reference_fullscreen(&cmds, &resources, &fonts);
    for (x, alpha) in [(5, 2), (21, 11), (43, 15)] {
        assert_eq!(
            expected.pixel(x, 5),
            liuli::text::blend_rgb565_a4_u16(rgb565_to_u16(BLUE), rgb565_to_u16(WHITE), alpha)
        );
    }
    for band_h in [1, 3, 8, 17, screen.h] {
        let mut scratch = vec![0; (screen.w * band_h) as usize];
        for y in (0..screen.h).step_by(band_h as usize) {
            let band = Rect::new(0, y as i32, screen.w, band_h.min(screen.h - y));
            let mut target = BandTarget::new::<()>(&mut scratch, band).unwrap();
            render_band(&mut target, &cmds, &resources, &fonts);
            let start = (y * screen.w) as usize;
            assert_eq!(
                target.words(),
                &expected.words()[start..start + target.words().len()]
            );
        }
    }
}

#[test]
fn basic_profile_reports_content_shadow_as_unsupported() {
    if SurfaceStyle::has_layers_support() {
        return;
    }
    let mut storage = Storage::new();
    let result = Engine::<Consumer>::new_in_place_with_fonts(
        &mut storage,
        Model {
            content: true,
            offset: 0,
            nested: false,
        },
        showcase_fonts().unwrap(),
        Rect::new(0, 0, 52, 44),
        InstantMs(0),
    );
    assert!(matches!(result, Err(BuildError::UnsupportedEffect)));
}
const RING: PreparedPath<8> = match PreparedPath::prepare(
    &[
        PathVerb::Move(0, 0),
        PathVerb::Line(1024, 0),
        PathVerb::Line(1024, 1024),
        PathVerb::Line(0, 1024),
        PathVerb::Close,
        PathVerb::Move(256, 256),
        PathVerb::Line(768, 256),
        PathVerb::Line(768, 768),
        PathVerb::Line(256, 768),
        PathVerb::Close,
    ],
    1,
) {
    Ok(v) => v,
    Err(_) => panic!("ring"),
};
const PATH: Path<'static> = RING.path();
const CUTOUT: SurfaceComposition = SurfaceComposition {
    outline: Outline::Path(&PATH, FillRule::EvenOdd),
    mask: Mask::FadeBottom(8),
    hit_outline: true,
    clip_content: true,
    ..SurfaceComposition::DEFAULT
};
const CONTENT: SurfaceComposition = SurfaceComposition {
    shadow_source: ShadowSource::Content,
    ..SurfaceComposition::DEFAULT
};
const OUTER: SurfaceComposition = SurfaceComposition {
    mask: Mask::FadeBottom(16),
    ..SurfaceComposition::DEFAULT
};
struct Consumer;
#[derive(Clone, Copy, Debug)]
struct Model {
    offset: i16,
    content: bool,
    nested: bool,
}
impl App for Consumer {
    type Model = Model;
    type Msg = Model;
    fn update(m: &mut Model, next: Model, _: InstantMs) -> Change {
        *m = next;
        Change::View
    }
    fn on_action(_: &Model, _: UiAction) -> Option<Model> {
        None
    }
    fn view(m: &Model, v: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        v.root(Key(0), NodeProps::new().bg(WHITE), |root| {
            root.stack(
                Key(1),
                NodeProps::new().size(Length::Px(52), Length::Px(44)),
                |stage| {
                    stage.custom(
                        Key(2),
                        NodeProps::new()
                            .size(Length::Px(24), Length::Px(24))
                            .clickable(true)
                            .value(i16::from(m.content)),
                    )?;
                    stage.set_offset(Key(2), m.offset, 6)?;
                    stage.decorate(
                        Key(2),
                        SurfaceStyle::FLAT
                            .with_composition(if m.content { &CONTENT } else { &CUTOUT })
                            .with_shadow(Shadow {
                                x: 3,
                                y: 4,
                                blur: 1,
                                color: BLUE,
                                opacity: 15,
                            }),
                    )
                },
            )?;
            if m.nested {
                root.decorate(Key(1), SurfaceStyle::FLAT.with_composition(&OUTER))?
            }
            Ok(())
        })
    }
    fn paint_custom(_: Key, p: &StoredProps, c: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
        let rect = Rect::new(0, 0, 24, 24);
        if p.value == 1 {
            // A stack-local path has gone out of scope before the first band is drawn.
            let ring = PreparedPath::<8>::prepare(
                &[
                    PathVerb::Move(0, 0),
                    PathVerb::Line(1024, 0),
                    PathVerb::Line(1024, 1024),
                    PathVerb::Line(0, 1024),
                    PathVerb::Close,
                    PathVerb::Move(256, 256),
                    PathVerb::Line(768, 256),
                    PathVerb::Line(768, 768),
                    PathVerb::Line(256, 768),
                    PathVerb::Close,
                ],
                1,
            )?;
            c.path(rect, ring.path(), PathPaint::Fill(FillRule::EvenOdd), GREEN)
        } else {
            c.fill(rect, GREEN)?;
            c.overlay(rect, RED, 8)
        }
    }
}
#[test]
fn masked_subtree_shares_its_outline_with_hits_and_cleans_up_its_old_shadow() {
    let mut s = Storage::new();
    let mut model = Model {
        offset: 4,
        content: false,
        nested: true,
    };
    let mut app = AppSession::<Consumer>::new(
        &mut s,
        model,
        showcase_fonts().unwrap(),
        Size::new(56, 48),
        InstantMs(0),
        true,
    )
    .unwrap();
    let tree = app.engine().active_tree();
    let rect = tree.node_by_key(Key(2)).unwrap().layout_rect;
    let hit = |x, y| {
        liuli::core::focus::hit_test_in_scope(
            tree,
            tree.root.get_u16().unwrap(),
            Point::new(rect.x + x, rect.y + y),
        )
    };
    assert_eq!(hit(2, 2), tree.find_slot_by_key(Key(2)));
    assert_ne!(hit(12, 12), tree.find_slot_by_key(Key(2)));
    let px = |app: &AppSession<'_, '_, Consumer>, x, y| {
        app.screen().pixel((rect.x + x) as u32, (rect.y + y) as u32)
    };
    assert_eq!(px(&app, 12, 12), rgb565_to_u16(WHITE));
    assert_ne!(px(&app, 2, 2), rgb565_to_u16(WHITE));
    let a = OUTER.coverage(
        tree.node_by_key(Key(1)).unwrap().layout_rect,
        0,
        Point::new(rect.x + 12, rect.y + 25),
    );
    assert!((1..15).contains(&a));
    assert_eq!(
        px(&app, 12, 25),
        liuli::text::blend_rgb565_a4_u16(rgb565_to_u16(BLUE), rgb565_to_u16(WHITE), a)
    );
    model.offset = 26;
    app.message(InstantMs(12), model, true).unwrap();
    assert_eq!(px(&app, 12, 25), rgb565_to_u16(WHITE));
}
#[cfg(feature = "enhanced")]
#[test]
fn content_shadow_follows_runtime_path_alpha_including_its_hole() {
    let mut s = Storage::new();
    let m = Model {
        offset: 4,
        content: true,
        nested: false,
    };
    let app = AppSession::<Consumer>::new(
        &mut s,
        m,
        showcase_fonts().unwrap(),
        Size::new(56, 48),
        InstantMs(0),
        true,
    )
    .unwrap();
    let r = app
        .engine()
        .active_tree()
        .node_by_key(Key(2))
        .unwrap()
        .layout_rect;
    let px = |x, y| app.screen().pixel((r.x + x) as u32, (r.y + y) as u32);
    assert_eq!(px(2, 2), rgb565_to_u16(GREEN));
    assert_eq!(px(14, 15), rgb565_to_u16(WHITE));
    assert_eq!(px(10, 25), rgb565_to_u16(BLUE));
    assert_eq!(px(1, 27), rgb565_to_u16(WHITE));
}
#[cfg(feature = "enhanced")]
#[test]
fn shadow_survives_an_offscreen_caster_then_clears_when_it_moves_away() {
    let mut s = Storage::new();
    let mut m = Model {
        offset: -25,
        content: true,
        nested: false,
    };
    let mut app = AppSession::<Consumer>::new(
        &mut s,
        m,
        showcase_fonts().unwrap(),
        Size::new(56, 48),
        InstantMs(0),
        true,
    )
    .unwrap();
    assert!(
        app.engine()
            .active_tree()
            .node_by_key(Key(2))
            .unwrap()
            .visible_rect
            .is_empty()
    );
    assert_eq!(app.screen().pixel(0, 16), rgb565_to_u16(BLUE));
    m.offset = -38;
    app.message(InstantMs(10), m, true).unwrap();
    assert_eq!(app.screen().pixel(0, 16), rgb565_to_u16(WHITE));
}
#[test]
fn graphics_card_scrolls_through_the_viewport_with_per_frame_pixel_checks() {
    use liuli_widget_demo::showcase::{ShowcaseApp, ShowcaseModel};
    let size = Size::new(800, 480);
    let mut s = Storage::new();
    let mut app = AppSession::<ShowcaseApp>::new(
        &mut s,
        ShowcaseModel::for_size(size),
        showcase_fonts().unwrap(),
        size,
        InstantMs(0),
        true,
    )
    .unwrap();
    let trace = include_str!("fixtures/h743-composition.trace");
    let (before, after) = trace.split_once("920 kind=down").unwrap();
    liuli_sim::preview::replay_trace(&mut app, before).unwrap();
    let card = |app: &AppSession<'_, '_, ShowcaseApp>| {
        app.engine()
            .active_tree()
            .node_by_key(Key(8802))
            .unwrap()
            .layout_rect
    };
    let first = card(&app);
    liuli_sim::preview::replay_trace(&mut app, &format!("liuli-trace 1\n920 kind=down{after}"))
        .unwrap();
    let last = card(&app);
    let viewport = app.engine().active_tree().node_by_key(Key(8800)).unwrap();
    assert!(viewport.scroll_y > 0 && last.y < first.y);
    assert_eq!(last.intersect(viewport.clip_rect), last);
    let x = (last.x + last.w as i32 / 2) as u32;
    assert_ne!(
        app.screen().pixel(x, (last.y + last.h as i32 / 2) as u32),
        app.screen().pixel(x, (last.bottom64() - 1) as u32)
    );
}

#[test]
fn open_paths_close_for_fill_and_preserve_their_stroke() {
    let verbs = [
        PathVerb::Move(0, 0),
        PathVerb::Line(1024, 0),
        PathVerb::Quad((1024, 512), (1024, 1024)),
    ];
    let path = PreparedPath::<16>::prepare(&verbs, 8).unwrap();
    let r = Rect::new(0, 0, 64, 64);
    assert!(
        path.path()
            .contains(r, Point::new(48, 8), FillRule::NonZero)
    );
    assert!(
        !path
            .path()
            .contains(r, Point::new(8, 48), FillRule::EvenOdd)
    );
    assert_eq!(
        liuli::core::path::coverage(
            path.path().edges.iter(),
            r,
            Point::new(32, 32),
            PathPaint::Stroke(50)
        ),
        0
    );
    assert!(PreparedPath::<1>::prepare(&verbs, 8).is_err());
}
