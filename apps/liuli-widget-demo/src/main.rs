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
    let mut clock = 0;
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
            Page::Home => unreachable!(),
        }
        tap(app, &mut clock, 3)?;
        assert_eq!(app.engine().model().page, Page::Home);
        println!("{name}: passed");
    }
    Ok(())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut headless = false;
    let mut directory = None;
    let mut replay = None;
    let mut port = 7878u16;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => headless = true,
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
                    "liuli-widget-demo [--port PORT] [--headless] [--screenshots DIR] [--replay TRACE]"
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
        Model::new(536),
        fonts,
        Size::new(240, 536),
        InstantMs(0),
        true,
    )?;
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
