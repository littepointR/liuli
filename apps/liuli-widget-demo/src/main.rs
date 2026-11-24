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
    let bounds = node
        .layout_rect
        .expand(2)
        .intersect(node.clip_rect)
        .intersect(app.engine().screen());
    assert_eq!(node.layout_rect, node.layout_rect.intersect(node.clip_rect));
    let width = app.engine().screen().w as usize;
    let color = liuli::core::geometry::rgb565_to_u16(node.props.focus_color);
    let at = |x: i32, y: i32| app.screen().words()[y as usize * width + x as usize];
    let right = bounds.x + bounds.w as i32 - 1;
    let bottom = bounds.y + bounds.h as i32 - 1;
    for y in bounds.y..=bottom {
        for x in [bounds.x, right] {
            assert_eq!(at(x, y), color, "focus {key:?} at ({x},{y})");
        }
    }
    for x in bounds.x..=right {
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
    tap(app, &mut clock, 5)?;
    for (i, &(page, name, _)) in PAGES.iter().enumerate() {
        tap(app, &mut clock, 100 + i as u32)?;
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
                let before = app.engine().model().value;
                tap(app, &mut clock, 44)?;
                assert_ne!(app.engine().model().value, before);
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
                shot(app, dir, "04-dialog")?;
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
                assert!(app.engine().active_tree().active_count < 26);
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
            }
            Page::Depth => exercise_depth(app, &mut clock, dir)?,
            Page::Home => unreachable!(),
        }
        tap(app, &mut clock, 3)?;
        assert_eq!(app.engine().model().page, Page::Home);
        println!("{name}: passed");
    }
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
/// The board harness injects these raw taps and reads the scanned SDRAM buffer after each release.
fn panel_checkpoints(app: &mut Session<'_>, dir: &Path) -> Result<(), String> {
    let mut clock = 0;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut trace = String::new();
    let mut save = |app: &Session<'_>, seq: usize, point: Point| -> Result<(), String> {
        use std::fmt::Write;
        let name = format!("frame-{seq:02}.rgb565");
        let bytes: Vec<u8> = app
            .screen()
            .words()
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect();
        std::fs::write(dir.join(&name), bytes).map_err(|e| e.to_string())?;
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
    for (i, &(page, _, _)) in PAGES.iter().enumerate() {
        for (j, key) in [100 + i as u32, 3].into_iter().enumerate() {
            let point = center(app, key)?;
            tap(app, &mut clock, key)?;
            let expected = if j == 0 { page } else { Page::Home };
            if app.engine().model().page != expected {
                return Err(format!("tap {key} missed {expected:?}"));
            }
            save(app, 1 + 2 * i + j, point)?;
        }
    }
    std::fs::write(dir.join("frames.txt"), trace).map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut headless = false;
    let mut directory = None;
    let mut replay = None;
    let mut port = 7878u16;
    let mut panel_frames = None;
    let mut size = Size::new(240, 536);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => headless = true,
            "--size" => {
                size = match args.next().as_deref() {
                    Some("800x480") => Size::new(800, 480),
                    Some("240x536") => Size::new(240, 536),
                    _ => return Err("expected --size 800x480 or 240x536".into()),
                };
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
            "-h" | "--help" => {
                println!(
                    "liuli-widget-demo [--port PORT] [--headless] [--size 240x536|800x480] [--screenshots DIR] [--panel-frames DIR] [--replay TRACE]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let mut storage = Storage::new();
    let fonts = FontSet::single(Lf4Font::from_bytes(FONT).map_err(|e| format!("{e:?}"))?);
    let mut app = AppSession::<Catalog>::new(
        &mut storage,
        Model::new(size.height as u16),
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
        return replay_trace(
            &mut app,
            &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        );
    }
    if headless || directory.is_some() {
        return exercise(&mut app, directory.as_deref());
    }
    let server = PreviewServer::bind(port)?;
    println!("Widget Demo   http://{}", server.addr());
    server.serve_app(&mut app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_and_light_modern_themes_pass_all_pages_and_focus_outlines() {
        let fonts = FontSet::single(Lf4Font::from_bytes(FONT).unwrap());

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
