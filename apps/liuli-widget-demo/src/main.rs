use liuli::prelude::*;
use liuli_sim::{
    preview::{PreviewServer, replay_trace},
    session::AppSession,
};
use liuli_widget_demo::{Catalog, FONT, Model, PAGES, Page};
use std::path::{Path, PathBuf};

type Session<'s> = AppSession<'s, 'static, Catalog>;
fn input(app: &mut Session<'_>, clock: &mut u64, event: RawInput) -> Result<(), String> {
    *clock += 16;
    app.step(InstantMs(*clock), Some(event), true)?;
    Ok(())
}
fn center(app: &Session<'_>, key: u32) -> Result<Point, String> {
    let node = app
        .engine()
        .active_tree()
        .node_by_key(Key(key))
        .ok_or_else(|| format!("missing key {key}"))?;
    let r = node.visible_rect;
    if r.w == 0 || r.h == 0 {
        return Err(format!("key {key} is clipped"));
    }
    Ok(Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32))
}
fn tap(app: &mut Session<'_>, clock: &mut u64, key: u32) -> Result<(), String> {
    let pos = center(app, key)?;
    let id = PointerId(1);
    input(app, clock, RawInput::PointerDown { id, pos })?;
    input(app, clock, RawInput::PointerUp { id, pos })
}
fn label<'a>(app: &'a Session<'_>, key: u32) -> &'a str {
    let tree = app.engine().active_tree();
    tree.text_str(tree.node_by_key(Key(key)).unwrap().props.text())
}
fn shot(app: &Session<'_>, dir: Option<&Path>, name: &str) -> Result<(), String> {
    if let Some(dir) = dir {
        std::fs::write(dir.join(format!("{name}.png")), app.screen().to_png())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn check_focus_outline(app: &Session<'_>) {
    let tree = app.engine().active_tree();
    let key = app.engine().focus().focused_key().unwrap();
    let node = tree.node_by_key(key).unwrap();
    let clip = node.clip_rect.intersect(app.engine().screen());
    let (bounds, radius) =
        node.props
            .focus_ring()
            .outline_geometry(node.layout_rect, clip, node.props.radius);
    assert_eq!(node.layout_rect, node.layout_rect.intersect(node.clip_rect));
    let width = app.engine().screen().w as usize;
    let color = liuli::core::geometry::rgb565_to_u16(node.props.focus_color);
    let at = |x: i32, y: i32| app.screen().words()[y as usize * width + x as usize];
    let right = bounds.x + bounds.w as i32 - 1;
    let bottom = bounds.y + bounds.h as i32 - 1;
    let radius = i32::from(radius);
    for y in bounds.y + radius..=bottom - radius {
        for x in [bounds.x, right] {
            assert_eq!(at(x, y), color, "focus {key:?} at ({x},{y})");
        }
    }
    for x in bounds.x + radius..=right - radius {
        for y in [bounds.y, bottom] {
            assert_eq!(at(x, y), color, "focus {key:?} at ({x},{y})");
        }
    }
}
fn exercise(app: &mut Session<'_>, dir: Option<&Path>) -> Result<(), String> {
    exercise_from(app, dir, 0)
}
fn exercise_from(app: &mut Session<'_>, dir: Option<&Path>, start_ms: u64) -> Result<(), String> {
    let mut clock = start_ms;
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    shot(app, dir, "00-home")?;
    check_focus_outline(app);
    input(app, &mut clock, RawInput::Nav(NavDir::Next))?;
    check_focus_outline(app);
    input(app, &mut clock, RawInput::Nav(NavDir::Prev))?;
    check_focus_outline(app);
    let before_theme = app.screen().words().to_vec();
    tap(app, &mut clock, 5)?;
    assert_ne!(before_theme, app.screen().words());
    shot(app, dir, "00-home-alternate")?;
    tap(app, &mut clock, 5)?;
    for (i, &(page, name, _)) in PAGES.iter().enumerate() {
        if page.parent() != Page::Home {
            tap(app, &mut clock, page.parent().route_key().0)?;
        }
        tap(app, &mut clock, page.route_key().0)?;
        assert_eq!(app.engine().model().page, page);
        shot(app, dir, &format!("{:02}-{page:?}", i + 1))?;
        match page {
            Page::Inputs => {
                tap(app, &mut clock, 30)?;
                tap(app, &mut clock, 31)?;
                assert!(
                    !app.engine()
                        .active_tree()
                        .node_by_key(Key(30))
                        .unwrap()
                        .props
                        .enabled()
                );
                tap(app, &mut clock, 32)?;
            }
            Page::Data => {
                let before = app.screen().words().to_vec();
                tap(app, &mut clock, 44)?;
                assert_ne!(app.screen().words(), before);
            }
            Page::Choices => {
                tap(app, &mut clock, 54)?;
                assert!(
                    app.engine()
                        .active_tree()
                        .node_by_key(Key(54))
                        .unwrap()
                        .props
                        .checked()
                );
                tap(app, &mut clock, 51)?;
                tap(app, &mut clock, 57)?;
                shot(app, dir, "03-value")?;
            }
            Page::Feedback => {
                tap(app, &mut clock, 60)?;
                clock += 3100;
                app.step(InstantMs(clock), None, true)?;
                tap(app, &mut clock, 60)?;
                shot(app, dir, "04-dialog")?;
                clock += 4000;
                app.step(InstantMs(clock), None, true)?;
                input(app, &mut clock, RawInput::Back)?;
                assert_eq!(app.engine().model().page, Page::Feedback);
                tap(app, &mut clock, 60)?;
                tap(app, &mut clock, 63)?;
                assert!(app.engine().active_tree().node_by_key(Key(66)).is_some());
                shot(app, dir, "04-toast")?;
                clock += 2100;
                app.step(InstantMs(clock), None, true)?;
                assert!(app.engine().active_tree().node_by_key(Key(66)).is_none());
            }
            Page::List => {
                // The recording made in the preceding workflow is selectable and has its actual duration.
                tap(app, &mut clock, 2000)?;
                assert_eq!(label(app, 451), "Field take 01");
                assert!(label(app, 452).contains("0:03"));
                tap(app, &mut clock, 90)?;
                clock += 600;
                app.step(InstantMs(clock), None, true)?;
                assert_eq!(
                    app.engine()
                        .active_tree()
                        .node_by_key(Key(456))
                        .unwrap()
                        .props
                        .max,
                    875
                );
                clock += 500;
                app.step(InstantMs(clock), None, true)?;
                tap(app, &mut clock, 90)?;
                assert!(label(app, 452).starts_with("0:01"));
                assert!(label(app, 455).starts_with("Line input"));
                tap(app, &mut clock, Page::List.route_key().0)?;
                input(app, &mut clock, RawInput::Nav(NavDir::Next))?;
                check_focus_outline(app);
                let id = PointerId(1);
                input(
                    app,
                    &mut clock,
                    RawInput::PointerDown {
                        id,
                        pos: Point::new(120, 430),
                    },
                )?;
                input(
                    app,
                    &mut clock,
                    RawInput::PointerMove {
                        id,
                        pos: Point::new(120, 110),
                    },
                )?;
                input(
                    app,
                    &mut clock,
                    RawInput::PointerUp {
                        id,
                        pos: Point::new(120, 110),
                    },
                )?;
                assert!(app.engine().model().offset > 0);
                let tree = app.engine().active_tree();
                let row_count = tree
                    .nodes
                    .iter()
                    .filter(|n| n.occupied && (1000..=2007).contains(&n.key.0))
                    .count();
                let viewport = tree.node_by_key(Key(12)).unwrap().layout_rect.h;
                assert!(row_count <= viewport.div_ceil(38) as usize + 3);
                let offset = app.engine().model().offset;
                let row = tree
                    .nodes
                    .iter()
                    .find(|n| {
                        n.occupied
                            && (1000..2000).contains(&n.key.0)
                            && n.layout_rect.intersect(n.clip_rect) == n.layout_rect
                    })
                    .unwrap()
                    .key
                    .0;
                tap(app, &mut clock, row)?;
                assert_eq!(app.engine().model().page, Page::Home);
                tap(app, &mut clock, Page::List.route_key().0)?;
                assert_eq!(app.engine().model().offset, offset);
                assert_eq!(app.engine().focus().focused_key(), Some(Key(row)));
            }
            Page::Icons => {
                for key in 200..211 {
                    tap(app, &mut clock, key)?;
                }
            }
            Page::Image => {
                let before = app.screen().words().to_vec();
                tap(app, &mut clock, 72)?;
                assert_ne!(before, app.screen().words());
                tap(app, &mut clock, 73)?;
                tap(app, &mut clock, 74)?;
            }
            #[cfg(feature = "enhanced")]
            Page::Text => {
                tap(app, &mut clock, 80)?;
                let before = app.screen().words().to_vec();
                clock += 16;
                app.text(InstantMs(clock), TextInput::Compose("告警"), true)?;
                assert_ne!(before, app.screen().words());
                clock += 16;
                app.text(InstantMs(clock), TextInput::Cancel, true)?;
                assert_eq!(before, app.screen().words());
                for key in 81..=84 {
                    tap(app, &mut clock, key)?;
                }
                tap(app, &mut clock, 863)?;
                assert!(
                    app.engine()
                        .active_tree()
                        .node_by_key(Key(863))
                        .unwrap()
                        .props
                        .checked()
                );
                let before_gamma = app.screen().words().to_vec();
                tap(app, &mut clock, 85)?;
                assert_ne!(before_gamma, app.screen().words());
                tap(app, &mut clock, 85)?;
            }
            Page::Depth => exercise_depth(app, &mut clock, dir)?,
            Page::Motion => exercise_motion(app, &mut clock, dir)?,
            Page::Home => unreachable!(),
        }
        tap(app, &mut clock, 3)?;
        assert_eq!(app.engine().model().page, page.parent());
        if page.parent() != Page::Home {
            tap(app, &mut clock, Page::Home.route_key().0)?;
        }
        assert_eq!(app.engine().model().page, Page::Home);
        if page == Page::Motion {
            assert_eq!(
                Catalog::next_deadline(app.engine().model(), InstantMs(clock)),
                None
            );
        }
        println!("{name}: passed");
    }
    Ok(())
}
fn exercise_motion(
    app: &mut Session<'_>,
    clock: &mut u64,
    dir: Option<&Path>,
) -> Result<(), String> {
    let positions = |app: &Session<'_>| {
        [402, 404].map(|key| {
            app.engine()
                .active_tree()
                .node_by_key(Key(key))
                .unwrap()
                .props
                .value
        })
    };
    assert_eq!(positions(app), [0, 0]);
    tap(app, clock, 406)?;
    *clock += 600;
    app.step(InstantMs(*clock), None, true)?;
    assert_eq!(positions(app), [500, 875]);
    for (key, fraction) in [(402, 500), (404, 875)] {
        let track = app.engine().active_tree().node_by_key(Key(key)).unwrap();
        let r = track.layout_rect;
        let x = r.x as u32 + 8 + (r.w - 16) * fraction / 1000;
        let y = r.y as u32 + r.h / 2 - 4;
        assert_eq!(
            app.screen().pixel(x, y),
            liuli::core::geometry::rgb565_to_u16(track.props.fg)
        );
    }
    shot(app, dir, "animations-halfway")?;
    tap(app, clock, 406)?;
    let paused_positions = positions(app);
    *clock += 200;
    app.step(InstantMs(*clock), None, true)?;
    assert_eq!(positions(app), paused_positions);
    let paused = app.screen().words().to_vec();
    *clock += 1500;
    app.step(InstantMs(*clock), None, true)?;
    assert!(
        paused == app.screen().words(),
        "paused frame changed after release settled"
    );
    assert_eq!(
        Catalog::next_deadline(app.engine().model(), InstantMs(*clock)),
        None
    );
    tap(app, clock, 410)?;
    tap(app, clock, 406)?;
    let resumed = positions(app)[0];
    *clock += 60;
    app.step(InstantMs(*clock), None, true)?;
    assert!((98..=101).contains(&(positions(app)[0] - resumed)));
    tap(app, clock, 412)?;
    let retimed = positions(app)[0];
    *clock += 240;
    app.step(InstantMs(*clock), None, true)?;
    assert!((98..=101).contains(&(positions(app)[0] - retimed)));
    *clock += 2400;
    app.step(InstantMs(*clock), None, true)?;
    assert_eq!(positions(app), [1000, 1000]);
    assert_eq!(
        Catalog::next_deadline(app.engine().model(), InstantMs(*clock)),
        None
    );
    tap(app, clock, 406)?;
    assert_eq!(positions(app), [0, 0]);
    tap(app, clock, 407)?;
    assert_eq!(positions(app), [0, 0]);
    assert_eq!(
        Catalog::next_deadline(app.engine().model(), InstantMs(*clock)),
        None
    );
    tap(app, clock, 406)?; // The caller leaves while playback is active.
    Ok(())
}
fn exercise_depth(
    app: &mut Session<'_>,
    clock: &mut u64,
    dir: Option<&Path>,
) -> Result<(), String> {
    let id = PointerId(3);
    let pos = center(app, 305)?;
    input(app, clock, RawInput::PointerDown { id, pos })?;
    let down = app.screen().words().to_vec();
    *clock += 60;
    app.step(InstantMs(*clock), None, true)?;
    assert_ne!(
        down,
        app.screen().words(),
        "pressed card shadow must animate"
    );
    shot(app, dir, "depth-press")?;
    input(app, clock, RawInput::PointerUp { id, pos })?;
    assert_eq!(app.engine().model().depth.presses, 1);
    tap(app, clock, 306)?;
    #[cfg(feature = "enhanced")]
    {
        // Isolate projection-only cache reuse after the stamp button's release transition.
        *clock += 200;
        app.step(InstantMs(*clock), None, true)?;
        tap(app, clock, 307)?;
        let start = *clock;
        *clock = start + 160;
        app.step(InstantMs(*clock), None, true)?;
        assert_eq!(app.engine().model().depth.turn, 32);
        assert_eq!(app.engine().active_commands().reused_groups(), 1);
        shot(app, dir, "depth-tilt")?;
        // The old right edge has moved. The left half of the projected button still accepts a tap.
        let rect = app
            .engine()
            .active_tree()
            .node_by_key(Key(305))
            .unwrap()
            .layout_rect;
        let sample = |n| Point::new(rect.x + rect.w as i32 * n / 16, rect.y + rect.h as i32 / 2);
        for (pos, expected) in [(sample(15), 1), (sample(6), 2)] {
            for event in [
                RawInput::PointerDown { id, pos },
                RawInput::PointerUp { id, pos },
            ] {
                app.step(InstantMs(*clock), Some(event), true)?;
            }
            assert_eq!(app.engine().model().depth.presses, expected);
        }
        assert_eq!(app.engine().model().depth.presses, 2);
        *clock = start + 320;
        app.step(InstantMs(*clock), None, true)?;
        assert_eq!(app.engine().model().depth.turn, 64);
        let tree = app.engine().active_tree();
        let slot = tree.find_slot_by_key(Key(305)).unwrap();
        assert!(tree.local_point(slot, center(app, 305)?).is_none());
        shot(app, dir, "depth-edge")?;
        *clock = start + 640;
        app.step(InstantMs(*clock), None, true)?;
        assert_eq!(app.engine().model().depth.turn, 128);
        let button = app
            .engine()
            .active_tree()
            .node_by_key(Key(305))
            .unwrap()
            .layout_rect;
        let sample = Point::new(button.x + 12, button.y + button.h as i32 / 2);
        let pixel = |app: &Session<'_>| app.screen().pixel(sample.x as u32, sample.y as u32);
        let opaque = pixel(app);
        let slider = app
            .engine()
            .active_tree()
            .node_by_key(Key(308))
            .unwrap()
            .layout_rect;
        let min = Point::new(slider.x + 1, slider.y + slider.h as i32 / 2);
        for ev in [
            RawInput::PointerDown { id, pos: min },
            RawInput::PointerUp { id, pos: min },
        ] {
            input(app, clock, ev)?;
        }
        assert_eq!(app.engine().model().depth.opacity, 0);
        let transparent = pixel(app);
        tap(app, clock, 308)?;
        let alpha = app.engine().model().depth.opacity;
        assert!((1..15).contains(&alpha));
        let rgb = liuli::core::geometry::u16_to_rgb565;
        let expected = liuli::text::blend_rgb565_a4(rgb(opaque), rgb(transparent), alpha);
        assert_eq!(
            pixel(app),
            liuli::core::geometry::rgb565_to_u16(expected),
            "composite the whole button once"
        );
        let before = app.screen().words().to_vec();
        tap(app, clock, 310)?;
        assert_ne!(before, app.screen().words());
        shot(app, dir, "depth-glass")?;
        tap(app, clock, 309)?;
    }
    Ok(())
}
fn save_scanout<A: App>(app: &AppSession<'_, '_, A>, path: &Path) -> Result<(), String> {
    let bytes: Vec<u8> = app
        .screen()
        .words()
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect();
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// The board harness injects these raw taps and reads the scanned SDRAM buffer after each release.
fn panel_checkpoints(app: &mut Session<'_>, dir: &Path) -> Result<(), String> {
    let mut clock = 0;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut trace = String::new();
    let mut save = |app: &Session<'_>, seq: usize, point: Point| -> Result<(), String> {
        use std::fmt::Write;
        let name = format!("frame-{seq:02}.rgb565");
        save_scanout(app, &dir.join(&name))?;
        writeln!(
            trace,
            "{} {} {} {}",
            point.x,
            point.y,
            app.engine().model().page as u8,
            name
        )
        .unwrap();
        Ok(())
    };
    save(app, 0, Point::new(-1, -1))?;
    let mut seq = 0;
    for &(page, _, _) in PAGES {
        let mut steps = Vec::new();
        if page.parent() != Page::Home {
            steps.push((page.parent().route_key().0, page.parent()));
        }
        steps.push((page.route_key().0, page));
        steps.push((3, page.parent()));
        if page.parent() != Page::Home {
            steps.push((Page::Home.route_key().0, Page::Home));
        }
        for (key, expected) in steps {
            let point = center(app, key)?;
            tap(app, &mut clock, key)?;
            if app.engine().model().page != expected {
                return Err(format!("tap {key} missed {expected:?}"));
            }
            seq += 1;
            save(app, seq, point)?;
        }
    }
    std::fs::write(dir.join("frames.txt"), trace).map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut headless = false;
    let mut showcase = true;
    let mut directory = None;
    let mut replay = None;
    let mut scanout = None;
    let mut port = 7878u16;
    let mut panel_frames = None;
    let mut size = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => headless = true,
            "--showcase" => showcase = true,
            "--studio" => showcase = false,
            "--size" => {
                size = Some(match args.next().as_deref() {
                    Some("800x480") => Size::new(800, 480),
                    Some("240x536") => Size::new(240, 536),
                    _ => return Err("expected --size 800x480 or 240x536".into()),
                });
            }
            "--panel-frames" => {
                panel_frames = Some(PathBuf::from(
                    args.next().ok_or("missing reference frame directory")?,
                ))
            }
            "--port" => {
                port = args
                    .next()
                    .ok_or("missing port")?
                    .parse()
                    .map_err(|_| "invalid port")?;
            }
            "--screenshots" => {
                directory = Some(PathBuf::from(
                    args.next().ok_or("missing screenshot directory")?,
                ))
            }
            "--replay" => replay = Some(args.next().ok_or("missing trace file")?),
            "--scanout" => {
                scanout = Some(PathBuf::from(
                    args.next().ok_or("missing RGB565 output path")?,
                ))
            }
            "-h" | "--help" => {
                println!(
                    "liuli-widget-demo [--showcase|--studio] [--port PORT] [--headless] [--size 240x536|800x480] [--screenshots DIR] [--panel-frames DIR] [--replay TRACE [--scanout FILE]]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let mut storage = Storage::new();
    if scanout.is_some() && (replay.is_none() || panel_frames.is_some()) {
        return Err("--scanout requires --replay without --panel-frames".into());
    }
    if showcase && panel_frames.is_none() {
        use liuli_widget_demo::showcase::{ShowcaseApp, ShowcaseModel, showcase_fonts};
        let size = size.unwrap_or(Size::new(800, 480));
        let mut app = AppSession::<ShowcaseApp>::new(
            &mut storage,
            ShowcaseModel::for_size(size),
            showcase_fonts().map_err(|e| format!("{e:?}"))?,
            size,
            InstantMs(0),
            true,
        )?;
        #[cfg(feature = "enhanced")]
        app.engine_mut()
            .set_layer_budget(1024 * 1024)
            .map_err(|e| format!("{e:?}"))?;
        if headless || replay.is_some() || directory.is_some() {
            let trace = match replay {
                Some(path) => std::fs::read_to_string(path).map_err(|e| e.to_string())?,
                None if size.width < 600 => {
                    include_str!("../tests/fixtures/portrait-motion.trace").into()
                }
                None => include_str!("../tests/fixtures/h743-style.trace").into(),
            };
            replay_trace(&mut app, &trace)?;
            if let Some(path) = scanout {
                save_scanout(&app, &path)?;
            }
            if let Some(dir) = directory {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(dir.join("showcase.png"), app.screen().to_png())
                    .map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        let server = PreviewServer::bind(port)?;
        println!("Widget Demo   http://{}", server.addr());
        return server.serve_app(&mut app);
    }
    let size = size.unwrap_or(Size::new(240, 536));
    let fonts =
        liuli_widget_demo::studio_fonts(Lf4Font::from_bytes(FONT).map_err(|e| format!("{e:?}"))?)
            .map_err(|e| format!("{e:?}"))?;
    let mut app = AppSession::<Catalog>::new(
        &mut storage,
        Model::for_size(size),
        fonts,
        size,
        InstantMs(0),
        true,
    )?;
    #[cfg(feature = "enhanced")]
    if size.width == 800 {
        app.engine_mut()
            .set_layer_budget(1024 * 1024)
            .map_err(|e| format!("{e:?}"))?;
    }
    if let Some(dir) = panel_frames {
        return panel_checkpoints(&mut app, &dir);
    }
    if let Some(path) = replay {
        replay_trace(
            &mut app,
            &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        )?;
        if let Some(path) = scanout {
            save_scanout(&app, &path)?;
        }
        return Ok(());
    }
    if headless || directory.is_some() {
        return exercise(&mut app, directory.as_deref());
    }
    let server = PreviewServer::bind(port)?;
    println!("Spool   http://{}", server.addr());
    server.serve_app(&mut app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_modern_themes_pass_all_pages_and_focus_outlines() {
        let fonts = liuli_widget_demo::studio_fonts(Lf4Font::from_bytes(FONT).unwrap()).unwrap();

        let mut dark_storage = Storage::new();
        let mut dark_app = AppSession::<Catalog>::new(
            &mut dark_storage,
            Model::new(536),
            fonts,
            Size::new(240, 536),
            InstantMs(0),
            true,
        )
        .unwrap();
        let dark_home = dark_app.screen().words().to_vec();
        exercise(&mut dark_app, None).unwrap();

        let mut light_storage = Storage::new();
        let mut light_app = AppSession::<Catalog>::new(
            &mut light_storage,
            Model::new(536),
            fonts,
            Size::new(240, 536),
            InstantMs(0),
            true,
        )
        .unwrap();
        let mut clock = 0u64;
        tap(&mut light_app, &mut clock, 5).unwrap();
        let light_home = light_app.screen().words().to_vec();
        assert_ne!(dark_home, light_home);

        // Touch hides the focus ring; re-enable rotary focus before running exercise_from.
        input(&mut light_app, &mut clock, RawInput::Nav(NavDir::Next)).unwrap();
        input(&mut light_app, &mut clock, RawInput::Nav(NavDir::Prev)).unwrap();
        exercise_from(&mut light_app, None, clock).unwrap();
    }
}
