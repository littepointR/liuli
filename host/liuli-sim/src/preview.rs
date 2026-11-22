use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use liuli::prelude::*;
use liuli_settings::{SettingsApp, SettingsModel, settings_font};

use crate::screen::MemoryScreen;

const PREVIEW_HTML: &str = include_str!("preview.html");

pub struct PreviewServer {
    listener: TcpListener,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl PreviewServer {
    pub fn bind(port: u16) -> Result<Self, String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener =
            TcpListener::bind(addr).map_err(|e| format!("failed to bind {addr}: {e}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| format!("local_addr failed: {e}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("set_nonblocking failed: {e}"))?;
        Ok(Self {
            listener,
            addr: local_addr,
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub fn serve(&self) -> Result<(), String> {
        let font = settings_font();
        let screen_rect = Rect::new(0, 0, DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
        let mut storage = Box::new(Storage::new());
        let mut scratch = [0u16; DEFAULT_BAND_PIXELS];
        let mut screen = MemoryScreen::new(DEFAULT_SCREEN_WIDTH, DEFAULT_SCREEN_HEIGHT);
        let start = Instant::now();

        let mut engine = Engine::<SettingsApp>::new_in_place(
            &mut storage,
            SettingsModel::new(InstantMs(0)),
            font,
            screen_rect,
            InstantMs(0),
        )
        .map_err(|e| format!("engine init failed: {e:?}"))?;
        engine.set_focus_ring_visible(true);
        let _ = engine.present_sync(&mut screen, &mut scratch);

        while !self.stop.load(Ordering::Relaxed) {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let now = InstantMs(start.elapsed().as_millis() as u64);
                    let _ =
                        handle_connection(&mut stream, &mut engine, &mut screen, &mut scratch, now);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(format!("accept failed: {e}")),
            }
        }
        Ok(())
    }
}

fn handle_connection(
    stream: &mut TcpStream,
    engine: &mut Engine<'_, '_, SettingsApp>,
    screen: &mut MemoryScreen,
    scratch: &mut [u16],
    now: InstantMs,
) -> std::io::Result<()> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));

    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req_str = String::from_utf8_lossy(&buf[..n]);
    let mut lines = req_str.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_target = parts.next().unwrap_or("/");
    let path = raw_target.split('?').next().unwrap_or("/");

    let body = req_str.split("\r\n\r\n").nth(1).unwrap_or("");

    let _ = engine.advance(now, false);

    match (method, path) {
        ("GET", "/") => write_http(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            PREVIEW_HTML.as_bytes(),
        ),
        ("GET", "/frame.png") => {
            let _ = engine.present_sync(screen, scratch);
            let png = screen.to_png();
            write_http(stream, "200 OK", "image/png", &png)
        }
        ("GET", "/state") => {
            let _ = engine.present_sync(screen, scratch);
            let json = format_state_json(engine, now);
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                json.as_bytes(),
            )
        }
        ("POST", "/tick") => {
            let _ = engine.present_sync(screen, scratch);
            let json = format_state_json(engine, now);
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                json.as_bytes(),
            )
        }
        ("POST", "/input") => {
            if let Some(input) = parse_input_params(body) {
                let _ = engine.dispatch(input, now, false);
            }
            let _ = engine.present_sync(screen, scratch);
            let json = format_state_json(engine, now);
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                json.as_bytes(),
            )
        }
        _ => write_http(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn parse_input_params(body: &str) -> Option<RawInput> {
    let mut kind = "";
    let mut v: i32 = 0;
    let mut x: i32 = 0;
    let mut y: i32 = 0;

    for pair in body.trim().split('&') {
        if let Some((k, val)) = pair.split_once('=') {
            match k {
                "kind" => kind = val,
                "v" => v = val.parse().unwrap_or(0),
                "x" => x = val.parse().unwrap_or(0),
                "y" => y = val.parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    match kind {
        "next" => Some(RawInput::Nav(NavDir::Next)),
        "prev" => Some(RawInput::Nav(NavDir::Prev)),
        "activate" => Some(RawInput::Activate),
        "back" => Some(RawInput::Back),
        "adjust" => Some(RawInput::Adjust(v as i16)),
        "down" => Some(RawInput::PointerDown {
            id: PointerId(v as u8),
            pos: Point::new(x, y),
        }),
        "move" => Some(RawInput::PointerMove {
            id: PointerId(v as u8),
            pos: Point::new(x, y),
        }),
        "up" => Some(RawInput::PointerUp {
            id: PointerId(v as u8),
            pos: Point::new(x, y),
        }),
        "cancel" => Some(RawInput::PointerCancel {
            id: PointerId(v as u8),
        }),
        _ => None,
    }
}

fn format_state_json(engine: &Engine<'_, '_, SettingsApp>, now: InstantMs) -> String {
    let m = engine.model();
    let metrics = engine.last_metrics();
    let scroll_y = engine
        .active_tree()
        .node_by_key(liuli_settings::keys::SCROLL)
        .map(|n| n.scroll_y)
        .unwrap_or(0);
    let focus_key = engine.focus().focused_key.map(|k| k.0).unwrap_or(0);
    let wakeup = match engine.next_wakeup(now, false) {
        NextWakeup::Now => "now".to_string(),
        NextWakeup::OnEvent => "on_event".to_string(),
        NextWakeup::At(t) => format!("at:{}", t.0),
    };
    format!(
        "{{\"brightness\":{},\"sound\":{},\"dark_theme\":{},\"live_monitoring\":{},\"temp_deci_c\":{},\"modal_open\":{},\"focus_key\":{},\"editing\":{},\"scroll_y\":{},\"wakeup\":\"{}\",\"measured_nodes\":{},\"bands\":{},\"pixels_sent\":{}}}",
        m.brightness,
        m.sound_enabled,
        m.dark_theme,
        m.live_monitoring,
        m.temp_deci_c,
        m.reset_modal_open,
        focus_key,
        engine.focus().editing,
        scroll_y,
        wakeup,
        metrics.measured_nodes,
        metrics.bands,
        metrics.pixels_sent
    )
}

fn write_http(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
