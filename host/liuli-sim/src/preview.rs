use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use liuli::prelude::*;

use crate::clock::ClockController;
use crate::sim::{FrameLookupError, SimScenario, SimSession};

const PREVIEW_HTML: &str = include_str!("preview.html");
const MAX_HTTP_HEADER_BYTES: usize = 8192;
const MAX_HTTP_BODY_BYTES: usize = 4096;
const MAX_CLOCK_STEP_MS: u64 = 600_000;

pub struct PreviewServer {
    listener: TcpListener,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    scenario: SimScenario,
}

impl PreviewServer {
    pub fn bind(port: u16) -> Result<Self, String> {
        Self::bind_scenario(port, SimScenario::Terminal)
    }

    pub fn bind_scenario(port: u16, scenario: SimScenario) -> Result<Self, String> {
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
            scenario,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn scenario(&self) -> SimScenario {
        self.scenario
    }

    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub fn serve(&self) -> Result<(), String> {
        let mut storage = Box::new(Storage::new());
        let mut session = SimSession::new(self.scenario, &mut storage, InstantMs(0), false)?;
        let mut clock = ClockController::new(InstantMs(0), Instant::now());

        while !self.stop.load(Ordering::Relaxed) {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = handle_connection(&mut stream, &mut session, &mut clock);
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

struct ParsedRequest {
    method: String,
    path: String,
    query: String,
    body: String,
}

enum ReadRequestOutcome {
    Empty,
    Request(ParsedRequest),
    BadRequest(String),
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<ReadRequestOutcome> {
    let mut raw = Vec::with_capacity(512);
    let mut chunk = [0u8; 1024];
    let mut header_end: Option<usize> = None;

    while header_end.is_none() {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            if raw.is_empty() {
                return Ok(ReadRequestOutcome::Empty);
            }
            return Ok(ReadRequestOutcome::BadRequest(
                "incomplete HTTP headers".into(),
            ));
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = Some(pos);
            break;
        }
        if raw.len() > MAX_HTTP_HEADER_BYTES {
            return Ok(ReadRequestOutcome::BadRequest(
                "HTTP header section exceeds limit".into(),
            ));
        }
    }

    let hdr_end = header_end.unwrap();
    if hdr_end > MAX_HTTP_HEADER_BYTES {
        return Ok(ReadRequestOutcome::BadRequest(
            "HTTP header section exceeds limit".into(),
        ));
    }

    let header_str = match std::str::from_utf8(&raw[..hdr_end]) {
        Ok(s) => s,
        Err(_) => {
            return Ok(ReadRequestOutcome::BadRequest(
                "non-UTF8 HTTP headers".into(),
            ));
        }
    };

    let mut lines = header_str.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return Ok(ReadRequestOutcome::BadRequest("missing HTTP method".into()));
    };
    let Some(raw_target) = parts.next() else {
        return Ok(ReadRequestOutcome::BadRequest("missing HTTP target".into()));
    };

    let (path, query) = match raw_target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (raw_target, ""),
    };

    let mut content_length: Option<usize> = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Ok(ReadRequestOutcome::BadRequest(
                "malformed HTTP header line".into(),
            ));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let parsed: usize = match value.trim().parse() {
                Ok(n) => n,
                Err(_) => {
                    return Ok(ReadRequestOutcome::BadRequest(
                        "invalid Content-Length header".into(),
                    ));
                }
            };
            if parsed > MAX_HTTP_BODY_BYTES {
                return Ok(ReadRequestOutcome::BadRequest(
                    "Content-Length exceeds 4096 bytes".into(),
                ));
            }
            if let Some(prev) = content_length
                && prev != parsed
            {
                return Ok(ReadRequestOutcome::BadRequest(
                    "conflicting Content-Length headers".into(),
                ));
            }
            content_length = Some(parsed);
        }
    }

    let expected_body_len = content_length.unwrap_or(0);
    let body_start = hdr_end + 4;
    let total_needed = body_start + expected_body_len;
    let method_owned = method.to_string();
    let path_owned = path.to_string();
    let query_owned = query.to_string();

    while raw.len() < total_needed {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(ReadRequestOutcome::BadRequest(
                "unexpected EOF reading HTTP body".into(),
            ));
        }
        raw.extend_from_slice(&chunk[..n]);
    }

    let body_slice = &raw[body_start..body_start + expected_body_len];
    let body = match std::str::from_utf8(body_slice) {
        Ok(s) => s.to_string(),
        Err(_) => return Ok(ReadRequestOutcome::BadRequest("non-UTF8 HTTP body".into())),
    };

    Ok(ReadRequestOutcome::Request(ParsedRequest {
        method: method_owned,
        path: path_owned,
        query: query_owned,
        body,
    }))
}

fn handle_connection(
    stream: &mut TcpStream,
    session: &mut SimSession<'_>,
    clock: &mut ClockController,
) -> std::io::Result<()> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));

    let req = match read_http_request(stream)? {
        ReadRequestOutcome::Empty => return Ok(()),
        ReadRequestOutcome::BadRequest(msg) => {
            return write_http(stream, "400 Bad Request", "text/plain", &[], msg.as_bytes());
        }
        ReadRequestOutcome::Request(r) => r,
    };

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => write_http(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            &[],
            PREVIEW_HTML.as_bytes(),
        ),
        ("GET", "/state") => {
            let json = session.state_json(clock.is_paused(), clock.rate());
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                &[],
                json.as_bytes(),
            )
        }
        ("GET", "/frame.png") => {
            let frame_req = match parse_frame_query(&req.query) {
                Ok(f) => f,
                Err(msg) => {
                    return write_http(
                        stream,
                        "400 Bad Request",
                        "text/plain",
                        &[],
                        msg.as_bytes(),
                    );
                }
            };
            match session.png_for_frame(frame_req) {
                Ok((fid, png)) => {
                    let hdr = format!("X-Liuli-Frame-Id: {fid}\r\n");
                    write_http(stream, "200 OK", "image/png", hdr.as_bytes(), png)
                }
                Err(FrameLookupError::Expired) => write_http(
                    stream,
                    "410 Gone",
                    "text/plain",
                    &[],
                    b"requested frame snapshot expired from ring buffer",
                ),
                Err(FrameLookupError::NotFound) => write_http(
                    stream,
                    "404 Not Found",
                    "text/plain",
                    &[],
                    b"requested frame not found",
                ),
            }
        }
        ("POST", "/tick") => {
            let now = clock.sample_wall(Instant::now());
            if let Err(err) = session.step(now, None, false) {
                return write_http(stream, "400 Bad Request", "text/plain", &[], err.as_bytes());
            }
            let json = session.state_json(clock.is_paused(), clock.rate());
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                &[],
                json.as_bytes(),
            )
        }
        ("POST", "/input") => {
            let input = match parse_input_strict(&req.body) {
                Ok(inp) => inp,
                Err(msg) => {
                    return write_http(
                        stream,
                        "400 Bad Request",
                        "text/plain",
                        &[],
                        msg.as_bytes(),
                    );
                }
            };
            let now = clock.sample_wall(Instant::now());
            if let Err(err) = session.step(now, Some(input), false) {
                return write_http(stream, "400 Bad Request", "text/plain", &[], err.as_bytes());
            }
            let json = session.state_json(clock.is_paused(), clock.rate());
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                &[],
                json.as_bytes(),
            )
        }
        ("POST", "/clock") => {
            let wall = Instant::now();
            match handle_clock_op(&req.body, wall, clock, session) {
                Ok(()) => {
                    let json = session.state_json(clock.is_paused(), clock.rate());
                    write_http(
                        stream,
                        "200 OK",
                        "application/json; charset=utf-8",
                        &[],
                        json.as_bytes(),
                    )
                }
                Err(msg) => {
                    write_http(stream, "400 Bad Request", "text/plain", &[], msg.as_bytes())
                }
            }
        }
        _ => write_http(stream, "404 Not Found", "text/plain", &[], b"not found"),
    }
}

fn parse_frame_query(query: &str) -> Result<Option<u64>, String> {
    if query.is_empty() {
        return Ok(None);
    }
    let mut frame_id: Option<u64> = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let Some((k, v)) = pair.split_once('=') else {
            return Err("malformed query parameter".into());
        };
        if k == "frame" {
            let parsed: u64 = v
                .parse()
                .map_err(|_| format!("invalid frame query parameter '{v}'"))?;
            frame_id = Some(parsed);
        }
    }
    Ok(frame_id)
}

fn parse_input_strict(body: &str) -> Result<RawInput, String> {
    let mut kind: Option<&str> = None;
    let mut key_name: Option<&str> = None;
    let mut v: Option<i64> = None;
    let mut x: Option<i32> = None;
    let mut y: Option<i32> = None;

    for pair in body.trim().split('&') {
        if pair.is_empty() {
            continue;
        }
        let Some((k, val)) = pair.split_once('=') else {
            return Err("malformed form pair in /input body".into());
        };
        match k {
            "kind" => kind = Some(val),
            "key" => key_name = Some(val),
            "v" => {
                v = Some(
                    val.parse::<i64>()
                        .map_err(|_| format!("invalid numeric field v='{val}'"))?,
                );
            }
            "x" => {
                x = Some(
                    val.parse::<i32>()
                        .map_err(|_| format!("invalid numeric field x='{val}'"))?,
                );
            }
            "y" => {
                y = Some(
                    val.parse::<i32>()
                        .map_err(|_| format!("invalid numeric field y='{val}'"))?,
                );
            }
            other => return Err(format!("unknown /input field '{other}'")),
        }
    }

    let Some(k) = kind else {
        return Err("missing 'kind' field in /input body".into());
    };

    let parse_pointer_id = |val: Option<i64>| -> Result<PointerId, String> {
        let raw = val.ok_or_else(|| "missing pointer id 'v'".to_string())?;
        if !(0..=u8::MAX as i64).contains(&raw) {
            return Err(format!("pointer id {raw} out of u8 range"));
        }
        Ok(PointerId(raw as u8))
    };

    let parse_hold_key = |name: Option<&str>| -> Result<HoldKey, String> {
        match name.unwrap_or("activate") {
            "activate" => Ok(HoldKey::Activate),
            "adjust_prev" => Ok(HoldKey::AdjustPrev),
            "adjust_next" => Ok(HoldKey::AdjustNext),
            other => Err(format!("unknown hold key '{other}'")),
        }
    };

    match k {
        "next" => Ok(RawInput::Nav(NavDir::Next)),
        "prev" => Ok(RawInput::Nav(NavDir::Prev)),
        "activate" => Ok(RawInput::Activate),
        "back" => Ok(RawInput::Back),
        "adjust" => {
            let raw = v.ok_or_else(|| "missing 'v' for adjust".to_string())?;
            if !(i16::MIN as i64..=i16::MAX as i64).contains(&raw) {
                return Err(format!("adjust value {raw} out of i16 range"));
            }
            Ok(RawInput::Adjust(raw as i16))
        }
        "key_down" => Ok(RawInput::KeyDown(parse_hold_key(key_name)?)),
        "key_up" => Ok(RawInput::KeyUp(parse_hold_key(key_name)?)),
        "down" => Ok(RawInput::PointerDown {
            id: parse_pointer_id(v)?,
            pos: Point::new(
                x.ok_or_else(|| "missing 'x' coordinate".to_string())?,
                y.ok_or_else(|| "missing 'y' coordinate".to_string())?,
            ),
        }),
        "move" => Ok(RawInput::PointerMove {
            id: parse_pointer_id(v)?,
            pos: Point::new(
                x.ok_or_else(|| "missing 'x' coordinate".to_string())?,
                y.ok_or_else(|| "missing 'y' coordinate".to_string())?,
            ),
        }),
        "up" => Ok(RawInput::PointerUp {
            id: parse_pointer_id(v)?,
            pos: Point::new(
                x.ok_or_else(|| "missing 'x' coordinate".to_string())?,
                y.ok_or_else(|| "missing 'y' coordinate".to_string())?,
            ),
        }),
        "cancel" => Ok(RawInput::PointerCancel {
            id: parse_pointer_id(v)?,
        }),
        other => Err(format!("unknown /input kind '{other}'")),
    }
}

fn handle_clock_op(
    body: &str,
    wall: Instant,
    clock: &mut ClockController,
    session: &mut SimSession<'_>,
) -> Result<(), String> {
    let mut op: Option<&str> = None;
    let mut ms: Option<u64> = None;
    let mut v: Option<u32> = None;

    for pair in body.trim().split('&') {
        if pair.is_empty() {
            continue;
        }
        let Some((k, val)) = pair.split_once('=') else {
            return Err("malformed form pair in /clock body".into());
        };
        match k {
            "op" => op = Some(val),
            "ms" => {
                ms = Some(
                    val.parse::<u64>()
                        .map_err(|_| format!("invalid ms='{val}'"))?,
                );
            }
            "v" => {
                v = Some(
                    val.parse::<u32>()
                        .map_err(|_| format!("invalid v='{val}'"))?,
                );
            }
            other => return Err(format!("unknown /clock field '{other}'")),
        }
    }

    match op {
        Some("pause") => {
            clock.pause(wall);
            session.step(clock.now(), None, false)?;
            Ok(())
        }
        Some("resume") => {
            clock.resume(wall);
            Ok(())
        }
        Some("step") => {
            let delta = ms.ok_or_else(|| "missing 'ms' for /clock step".to_string())?;
            if delta > MAX_CLOCK_STEP_MS {
                return Err(format!("step ms={delta} exceeds {MAX_CLOCK_STEP_MS}"));
            }
            let target = clock.step_ms(delta, wall);
            session.step(target, None, false)?;
            Ok(())
        }
        Some("next_deadline") => {
            let wakeup = session.next_wakeup(clock.now());
            let target = clock.jump_next_deadline(wakeup, wall);
            session.step(target, None, false)?;
            Ok(())
        }
        Some("rate") => {
            let rate = v.ok_or_else(|| "missing 'v' for /clock rate".to_string())?;
            clock.set_rate(rate, wall)?;
            Ok(())
        }
        Some(other) => Err(format!("unknown /clock op '{other}'")),
        None => Err("missing 'op' in /clock body".into()),
    }
}

fn write_http(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    extra_headers: &[u8],
    body: &[u8],
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(extra_headers)?;
    stream.write_all(b"\r\n")?;
    stream.write_all(body)?;
    stream.flush()
}
