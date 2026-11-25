use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use liuli::prelude::*;

use crate::clock::ClockController;
use crate::session::{AppSession, FrameLookupError, FrameSnapshot, rects_to_json};
#[cfg(feature = "demos")]
use crate::sim::{SimScenario, SimSession};
use liuli::core::node::RetainedTree;

const PREVIEW_HTML: &str = include_str!("preview.html");
const MAX_HTTP_HEADER_BYTES: usize = 8192;
const MAX_HTTP_BODY_BYTES: usize = 4096;
const MAX_CLOCK_STEP_MS: u64 = 600_000;

#[derive(PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

pub struct FileWatcher {
    paths: Vec<(PathBuf, Option<FileStamp>)>,
    revision: u64,
}

impl FileWatcher {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let entries = paths
            .into_iter()
            .map(|p| {
                let sig = file_signature(&p);
                (p, sig)
            })
            .collect();
        Self {
            paths: entries,
            revision: 0,
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for (path, last) in &mut self.paths {
            let cur = file_signature(path);
            if cur != *last {
                *last = cur;
                changed = true;
            }
        }
        if changed {
            self.revision = self.revision.saturating_add(1);
        }
        changed
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn watched_count(&self) -> usize {
        self.paths.len()
    }

    pub fn status_json(&self) -> String {
        format!(
            "{{\"revision\":{},\"watched\":{}}}",
            self.revision,
            self.paths.len()
        )
    }
}

fn file_signature(path: &std::path::Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

pub struct PreviewServer {
    listener: TcpListener,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    watch_paths: Vec<PathBuf>,
    #[cfg(feature = "demos")]
    scenario: SimScenario,
}

impl PreviewServer {
    pub fn bind(port: u16) -> Result<Self, String> {
        Self::bind_socket(port)
    }

    #[cfg(feature = "demos")]
    pub fn bind_scenario(port: u16, scenario: SimScenario) -> Result<Self, String> {
        let mut server = Self::bind_socket(port)?;
        server.scenario = scenario;
        Ok(server)
    }

    #[must_use]
    pub fn watch_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.watch_paths.push(path.into());
        self
    }

    fn bind_socket(port: u16) -> Result<Self, String> {
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
            watch_paths: Vec::new(),
            #[cfg(feature = "demos")]
            scenario: SimScenario::Terminal,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[cfg(feature = "demos")]
    pub fn scenario(&self) -> SimScenario {
        self.scenario
    }

    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    #[cfg(feature = "demos")]
    pub fn serve(&self) -> Result<(), String> {
        let mut storage = Box::new(Storage::new());
        let mut session = SimSession::new(self.scenario, &mut storage, InstantMs(0), false)?;
        self.serve_session(&mut session)
    }

    pub fn serve_app<A: App>(&self, session: &mut AppSession<'_, '_, A>) -> Result<(), String> {
        self.serve_session(session)
    }

    fn serve_session(&self, session: &mut impl PreviewSession) -> Result<(), String> {
        let mut clock = ClockController::new(InstantMs(session.snapshot().now_ms), Instant::now());
        let mut trace = Trace::new();
        let mut watcher = FileWatcher::new(self.watch_paths.clone());
        let mut last_poll = Instant::now();
        while !self.stop.load(Ordering::Relaxed) {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = handle_connection(
                        &mut stream,
                        session,
                        &mut clock,
                        &mut trace,
                        &mut watcher,
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if !self.watch_paths.is_empty()
                        && last_poll.elapsed() >= Duration::from_millis(200)
                    {
                        watcher.poll();
                        last_poll = Instant::now();
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(format!("accept failed: {e}")),
            }
        }
        Ok(())
    }
}

trait PreviewSession {
    fn source(&self, key: Key) -> Option<&liuli::core::style_source::StyleSource>;
    fn text(&mut self, _now: InstantMs, _input: TextInput<'_>) -> Result<(), String> {
        Err("no focused text field".into())
    }
    fn frame_report(&self, id: u64) -> Option<&str>;
    fn step(
        &mut self,
        now: InstantMs,
        input: Option<RawInput>,
        verify: bool,
    ) -> Result<&FrameSnapshot, String>;
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup;
    fn png_for_frame(&self, id: Option<u64>) -> Result<(u64, &[u8]), FrameLookupError>;
    fn state_json(&self, paused: bool, rate: u32) -> String;
    fn profile_json(&self) -> String;
    fn tree(&self) -> &RetainedTree;
    fn snapshot(&self) -> &FrameSnapshot;
    fn size(&self) -> (u32, u32);
}
#[cfg(feature = "demos")]
impl PreviewSession for SimSession<'_> {
    fn source(&self, key: Key) -> Option<&liuli::core::style_source::StyleSource> {
        match self {
            SimSession::Settings(s) => s.engine().style_source(key),
            SimSession::Terminal(s) => s.engine().style_source(key),
        }
    }
    fn frame_report(&self, id: u64) -> Option<&str> {
        match self {
            SimSession::Settings(s) => s.frame_report(id),
            SimSession::Terminal(s) => s.frame_report(id),
        }
    }
    fn step(
        &mut self,
        now: InstantMs,
        input: Option<RawInput>,
        verify: bool,
    ) -> Result<&FrameSnapshot, String> {
        self.step(now, input, verify)
    }
    fn png_for_frame(&self, id: Option<u64>) -> Result<(u64, &[u8]), FrameLookupError> {
        self.png_for_frame(id)
    }
    fn snapshot(&self) -> &FrameSnapshot {
        self.snapshot()
    }
    fn size(&self) -> (u32, u32) {
        (self.screen().width(), self.screen().height())
    }
    fn tree(&self) -> &RetainedTree {
        self.active_tree()
    }
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        self.next_wakeup(now)
    }
    fn state_json(&self, paused: bool, rate: u32) -> String {
        self.state_json(paused, rate)
    }
    fn profile_json(&self) -> String {
        self.profile_json()
    }
}
impl<A: App> PreviewSession for AppSession<'_, '_, A> {
    fn text(&mut self, now: InstantMs, input: TextInput<'_>) -> Result<(), String> {
        AppSession::text(self, now, input, false).map(|_| ())
    }
    fn source(&self, key: Key) -> Option<&liuli::core::style_source::StyleSource> {
        self.engine().style_source(key)
    }
    fn frame_report(&self, id: u64) -> Option<&str> {
        self.frame_report(id)
    }
    fn step(
        &mut self,
        now: InstantMs,
        input: Option<RawInput>,
        verify: bool,
    ) -> Result<&FrameSnapshot, String> {
        self.step(now, input, verify)
    }
    fn png_for_frame(&self, id: Option<u64>) -> Result<(u64, &[u8]), FrameLookupError> {
        self.png_for_frame(id)
    }
    fn snapshot(&self) -> &FrameSnapshot {
        self.snapshot()
    }
    fn size(&self) -> (u32, u32) {
        (self.screen().width(), self.screen().height())
    }
    fn tree(&self) -> &RetainedTree {
        self.engine().active_tree()
    }
    fn next_wakeup(&self, now: InstantMs) -> NextWakeup {
        self.engine().next_wakeup(now, false)
    }
    fn state_json(&self, paused: bool, rate: u32) -> String {
        let frame = self.snapshot();
        let engine = self.engine();
        let tree = engine.active_tree();
        let focus = engine
            .focus()
            .focused_key()
            .map_or("null".into(), |k| k.0.to_string());
        let capture = engine
            .pointer()
            .captured_id()
            .filter(|id| tree.is_id_valid(*id))
            .map_or("null".into(), |id| {
                tree.nodes[id.slot() as usize].key.0.to_string()
            });
        format!(
            r#"{{"frame_id":{},"now_ms":{},"paused":{},"rate_x":{},"measured_nodes":{},"pixels_sent":{},"damage":{},"band_rects":{},"focus":{focus},"capture":{capture},"text_input":{},"active_nodes":{},"commands":{},"text_bytes":{}}}"#,
            frame.frame_id,
            frame.now_ms,
            paused,
            rate,
            frame.measured_nodes,
            frame.pixels_sent,
            rects_to_json(&frame.damage),
            rects_to_json(&frame.band_rects),
            engine.text_target().is_some(),
            tree.active_count,
            engine.active_commands().count,
            tree.text_len
        )
    }
    fn profile_json(&self) -> String {
        self.profile_json()
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

// Pick the last painted visible descendant, including non-interactive labels and custom paint.
fn pick_visible(tree: &RetainedTree, slot: u16, point: Point) -> Option<u16> {
    let node = &tree.nodes[usize::from(slot)];
    if !node.occupied || !node.props.visible() {
        return None;
    }
    let mut hit = tree
        .local_point(slot, point)
        .is_some_and(|p| node.visible_rect.contains_point(p))
        .then_some(slot);
    let mut child = node.first_child.get_u16();
    while let Some(slot) = child {
        hit = pick_visible(tree, slot, point).or(hit);
        child = tree.nodes[usize::from(slot)].next_sibling.get_u16();
    }
    hit
}
fn handle_connection(
    stream: &mut TcpStream,
    session: &mut impl PreviewSession,
    clock: &mut ClockController,
    trace: &mut Trace,
    watcher: &mut FileWatcher,
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
        ("GET", "/") => {
            let (w, h) = session.size();
            let html = PREVIEW_HTML
                .replace("SCREEN_WIDTH", &w.to_string())
                .replace("SCREEN_HEIGHT", &h.to_string());
            write_http(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                &[],
                html.as_bytes(),
            )
        }
        ("GET", "/trace") => {
            if trace.overflow {
                return write_http(
                    stream,
                    "409 Conflict",
                    "text/plain",
                    &[],
                    b"trace capacity exceeded; restart preview for a new recording",
                );
            }
            write_http(
                stream,
                "200 OK",
                "text/plain; charset=utf-8",
                b"Content-Disposition: attachment; filename=liuli.trace\r\n",
                trace.data.as_bytes(),
            )
        }
        ("GET", "/inspect") => {
            let Some((x, y)) = req
                .query
                .strip_prefix("x=")
                .and_then(|q| q.split_once("&y="))
                .and_then(|(x, y)| Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?)))
            else {
                return write_http(
                    stream,
                    "400 Bad Request",
                    "text/plain",
                    &[],
                    b"expected x and y",
                );
            };
            let tree = session.tree();
            let point = Point::new(x, y);
            let scope = liuli::core::focus::active_scope_root(tree);
            let hit =
                scope.and_then(|root| liuli::core::focus::hit_test_in_scope(tree, root, point));
            let mut chain = vec![format!(
                "input_target={:?}",
                hit.map(|slot| tree.nodes[usize::from(slot)].key)
            )];
            let mut current = scope.and_then(|root| pick_visible(tree, root, point));
            while let Some(slot) = current {
                let node = &tree.nodes[slot as usize];
                chain.push(format!("key={} {:?} width={:?} height={:?} measured={:?} layout={:?} clip={:?} visible={:?} font={:?} fg={:?} bg={:?}",node.key.0,node.kind,node.props.width,node.props.height,node.measured_size,node.layout_rect,node.clip_rect,node.visible_rect,node.props.font(),node.props.fg,node.props.bg()));
                let effect = tree.effects.get(node.key);
                if effect != liuli::prelude::SurfaceStyle::FLAT {
                    chain.push(format!(
                        "  surface={effect:?} local_pointer={:?}",
                        tree.local_point(slot, point)
                    ));
                }
                if let Some(source) = session.source(node.key) {
                    let mut line = format!(
                        "  style={} declaration={}",
                        source.theme,
                        source
                            .declaration
                            .map_or_else(|| "unknown".into(), |l| l.to_string())
                    );
                    if let Some(v) = source.variant {
                        line.push_str(&format!(" variant={v}"));
                    }
                    if let Some(m) = source.material {
                        line.push_str(&format!(" material={m}"));
                    }
                    chain.push(line);
                    for (token, location) in liuli::core::style_source::StyleSource::TOKENS
                        .iter()
                        .zip(source.overrides)
                    {
                        if let Some(l) = location {
                            chain.push(format!("  {token} override={l}"));
                        }
                    }
                }
                current = node.parent.get_u16();
            }
            write_http(
                stream,
                "200 OK",
                "text/plain; charset=utf-8",
                &[],
                chain.join("\n").as_bytes(),
            )
        }
        ("GET", "/diagnostics") => {
            let id = match parse_frame_query(&req.query) {
                Ok(id) => id.unwrap_or(session.snapshot().frame_id),
                Err(e) => {
                    return write_http(stream, "400 Bad Request", "text/plain", &[], e.as_bytes());
                }
            };
            match session.frame_report(id) {
                Some(text) => write_http(
                    stream,
                    "200 OK",
                    "text/plain; charset=utf-8",
                    &[],
                    text.as_bytes(),
                ),
                None => write_http(
                    stream,
                    "404 Not Found",
                    "text/plain",
                    &[],
                    b"frame diagnostics unavailable",
                ),
            }
        }
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
        ("GET", "/api/profile") => {
            let json = session.profile_json();
            write_http(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                &[],
                json.as_bytes(),
            )
        }
        ("GET", "/api/watch") => {
            watcher.poll();
            let json = watcher.status_json();
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
            trace.append(now, "tick");
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
        ("POST", "/text") => {
            let op = req.query.strip_prefix("op=").unwrap_or("");
            let now = clock.sample_wall(Instant::now());
            let result = parse_text(op, &req.body).and_then(|input| session.text(now, input));
            if let Err(err) = result {
                return write_http(stream, "400 Bad Request", "text/plain", &[], err.as_bytes());
            }
            let hex: String = req.body.bytes().map(|b| format!("{b:02x}")).collect();
            trace.append(now, &format!("text {op} {hex}"));
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
            trace.append(now, req.body.trim());
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
            match handle_clock_op(&req.body, wall, clock, session, trace) {
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

struct Trace {
    data: String,
    overflow: bool,
}
impl Trace {
    fn new() -> Self {
        Self {
            data: "liuli-trace 1\n".into(),
            overflow: false,
        }
    }
    fn append(&mut self, now: InstantMs, entry: &str) {
        use std::fmt::Write;
        if self.data.len() + entry.len() + 32 > 8 * 1024 * 1024 {
            self.overflow = true;
        }
        if !self.overflow {
            writeln!(self.data, "{} {}", now.0, entry).expect("String write");
        }
    }
}

/// Replay a preview trace on a freshly initialized instance of the same application.
/// Each rendered frame is compared with a full redraw.
pub fn replay_trace<A: App>(
    session: &mut AppSession<'_, '_, A>,
    trace: &str,
) -> Result<(), String> {
    let mut lines = trace.lines();
    if lines.next() != Some("liuli-trace 1") {
        return Err("unsupported trace header".into());
    }
    for (i, line) in lines.enumerate() {
        let result = (|| -> Result<(), String> {
            let (time, op) = line.split_once(' ').ok_or("missing event")?;
            let time = InstantMs(time.parse().map_err(|_| "invalid time")?);
            if let Some(event) = op.strip_prefix("text ") {
                let (kind, hex) = event.split_once(' ').unwrap_or((event, ""));
                if hex.len() > MAX_HTTP_BODY_BYTES * 2
                    || hex.len() % 2 != 0
                    || !hex.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("invalid text bytes".into());
                }
                let mut bytes = Vec::with_capacity(hex.len() / 2);
                for i in (0..hex.len()).step_by(2) {
                    bytes.push(u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| "invalid hex")?);
                }
                let body = std::str::from_utf8(&bytes).map_err(|_| "invalid UTF-8")?;
                session.text(time, parse_text(kind, body)?, true)?;
            } else {
                let input = if op == "tick" {
                    None
                } else {
                    Some(parse_input_strict(op)?)
                };
                session.step(time, input, true)?;
            }
            Ok(())
        })();
        result.map_err(|e| format!("trace line {}: {e}", i + 2))?;
    }
    Ok(())
}
fn parse_text<'a>(op: &str, body: &'a str) -> Result<TextInput<'a>, String> {
    match op {
        "compose" => Ok(TextInput::Compose(body)),
        "commit" => Ok(TextInput::Commit(body)),
        "cancel" if body.is_empty() => Ok(TextInput::Cancel),
        "backspace" if body.is_empty() => Ok(TextInput::Backspace),
        "left" if body.is_empty() => Ok(TextInput::Left),
        "right" if body.is_empty() => Ok(TextInput::Right),
        _ => Err("invalid text operation or unexpected body".into()),
    }
}
fn parse_input_strict(body: &str) -> Result<RawInput, String> {
    if body.trim().bytes().any(|b| b.is_ascii_control()) {
        return Err("control character in input".into());
    }
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
    session: &mut impl PreviewSession,
    trace: &mut Trace,
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
            trace.append(clock.now(), "tick");
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
            trace.append(target, "tick");
            session.step(target, None, false)?;
            Ok(())
        }
        Some("next_deadline") => {
            let wakeup = session.next_wakeup(clock.now());
            let target = clock.jump_next_deadline(wakeup, wall);
            trace.append(target, "tick");
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
