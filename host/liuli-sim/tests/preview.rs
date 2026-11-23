use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use liuli_sim::image::decode_png_rgb8;
use liuli_sim::preview::PreviewServer;
use liuli_sim::sim::SimScenario;

fn http_request_chunked(addr: SocketAddr, chunks: &[&[u8]]) -> (String, Vec<u8>) {
    let mut last_err = None;
    let mut stream = None;
    for _ in 0..20 {
        match TcpStream::connect(addr) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    let mut stream = stream.unwrap_or_else(|| panic!("failed to connect {addr}: {last_err:?}"));
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    for (idx, chunk) in chunks.iter().enumerate() {
        stream.write_all(chunk).unwrap();
        stream.flush().unwrap();
        if idx + 1 < chunks.len() {
            thread::sleep(Duration::from_millis(5));
        }
    }

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();

    let split_pos = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("missing HTTP header terminator");
    let header = String::from_utf8_lossy(&raw[..split_pos]).into_owned();
    let body = raw[split_pos + 4..].to_vec();
    (header, body)
}

fn http_request(addr: SocketAddr, req: &str) -> (String, Vec<u8>) {
    http_request_chunked(addr, &[req.as_bytes()])
}

fn post_form(addr: SocketAddr, path: &str, body: &str) -> (String, String) {
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (hdr, raw) = http_request(addr, &req);
    (hdr, String::from_utf8(raw).expect("valid utf8 body"))
}

fn extract_json_u64(json: &str, key: &str) -> u64 {
    let start = json
        .find(key)
        .unwrap_or_else(|| panic!("missing key '{key}' in {json}"))
        + key.len();
    let digits: String = json[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits
        .parse()
        .unwrap_or_else(|e| panic!("invalid u64 for '{key}' in {json}: {e}"))
}

#[test]
fn preview_server_serves_terminal_by_default_clock_overlays_and_ring() {
    let server = PreviewServer::bind(0).expect("bind localhost ephemeral port");
    let addr = server.addr();
    let stop = server.stop_handle();

    let handle = thread::spawn(move || {
        server.serve().expect("preview server loop failed");
    });

    thread::sleep(Duration::from_millis(25));

    let (hdr_index, body_index) = http_request(
        addr,
        &format!("GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(
        hdr_index.starts_with("HTTP/1.1 200 OK"),
        "bad index header: {hdr_index}"
    );
    let html = String::from_utf8(body_index).expect("valid utf8 html");
    assert!(html.contains("<html"), "missing html tag");
    assert!(html.contains("/frame.png"), "missing /frame.png reference");
    assert!(html.contains("id=\"overlay\""), "missing overlay canvas");
    assert!(html.contains("/clock"), "missing /clock controls");

    let (hdr_st1, st1) = http_request(
        addr,
        &format!("GET /state HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_st1.starts_with("HTTP/1.1 200 OK"));
    let st1_str = String::from_utf8(st1).unwrap();
    assert!(
        st1_str.contains("\"scenario\":\"terminal\""),
        "expected terminal scenario, got {st1_str}"
    );
    assert!(
        st1_str.contains("\"frame_id\":1"),
        "expected initial frame_id 1, got {st1_str}"
    );
    assert!(
        st1_str.contains("\"focus_key\":1016"),
        "expected initial DASH_BTN_PAUSE key 1016, got {st1_str}"
    );

    // GET /state must be read-only and must not mutate frame_id or advance time.
    let (hdr_st1b, st1b) = http_request(
        addr,
        &format!("GET /state HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_st1b.starts_with("HTTP/1.1 200 OK"));
    let st1b_str = String::from_utf8(st1b).unwrap();
    assert!(
        st1b_str.contains("\"frame_id\":1"),
        "GET /state should remain read-only, got {st1b_str}"
    );

    let (hdr_png1, png1) = http_request(
        addr,
        &format!("GET /frame.png HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_png1.starts_with("HTTP/1.1 200 OK"));
    assert!(
        hdr_png1.contains("X-Liuli-Frame-Id: 1"),
        "missing X-Liuli-Frame-Id header: {hdr_png1}"
    );
    let (w1, h1, rgb1) = decode_png_rgb8(&png1).expect("decode initial png");
    assert_eq!((w1, h1), (320, 240));

    let (hdr_404, _) = http_request(
        addr,
        &format!("GET /frame.png?frame=999 HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(
        hdr_404.starts_with("HTTP/1.1 404 Not Found"),
        "expected 404 for unknown frame, got {hdr_404}"
    );

    let (hdr_400_q, _) = http_request(
        addr,
        &format!("GET /frame.png?frame=abc HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(
        hdr_400_q.starts_with("HTTP/1.1 400 Bad Request"),
        "expected 400 for bad frame query, got {hdr_400_q}"
    );

    // Segmented TCP write: split header across chunks and send body separately.
    let seg_body = b"kind=next";
    let seg_head = format!(
        "POST /input HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        seg_body.len()
    );
    let split = seg_head.len() / 2;
    let (hdr_post, body_post) = http_request_chunked(
        addr,
        &[
            &seg_head.as_bytes()[..split],
            &seg_head.as_bytes()[split..],
            seg_body,
        ],
    );
    assert!(hdr_post.starts_with("HTTP/1.1 200 OK"));
    let post_str = String::from_utf8(body_post).unwrap();
    assert!(
        post_str.contains("\"focus_key\":1017"),
        "expected DASH_BTN_DEC key 1017 after next, got {post_str}"
    );

    let (hdr_png2, png2) = http_request(
        addr,
        &format!("GET /frame.png HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_png2.starts_with("HTTP/1.1 200 OK"));
    let (w2, h2, rgb2) = decode_png_rgb8(&png2).expect("decode post-input png");
    assert_eq!((w2, h2), (320, 240));
    assert_ne!(rgb1, rgb2, "rendered frame should reflect moved focus ring");

    // Clock controls: pause, rate, step, next_deadline, resume + invalid op check.
    let (hdr_pause, st_pause) = post_form(addr, "/clock", "op=pause");
    assert!(hdr_pause.starts_with("HTTP/1.1 200 OK"));
    assert!(st_pause.contains("\"paused\":true"), "{st_pause}");
    let paused_now = extract_json_u64(&st_pause, "\"now_ms\":");

    let (hdr_rate, st_rate) = post_form(addr, "/clock", "op=rate&v=4");
    assert!(hdr_rate.starts_with("HTTP/1.1 200 OK"));
    assert!(st_rate.contains("\"rate_x\":4"), "{st_rate}");
    assert_eq!(extract_json_u64(&st_rate, "\"now_ms\":"), paused_now);

    let (hdr_step, st_step) = post_form(addr, "/clock", "op=step&ms=500");
    assert!(hdr_step.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(
        extract_json_u64(&st_step, "\"now_ms\":"),
        paused_now + 500,
        "{st_step}"
    );
    let next_dl = extract_json_u64(&st_step, "\"wakeup\":\"at:");

    let (hdr_next_dl, st_next_dl) = post_form(addr, "/clock", "op=next_deadline");
    assert!(hdr_next_dl.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(
        extract_json_u64(&st_next_dl, "\"now_ms\":"),
        next_dl,
        "{st_next_dl}"
    );

    let (hdr_resume, st_resume) = post_form(addr, "/clock", "op=resume");
    assert!(hdr_resume.starts_with("HTTP/1.1 200 OK"));
    assert!(st_resume.contains("\"paused\":false"), "{st_resume}");

    // Strict 400 validation on invalid /input or /clock params.
    let (hdr_bad_inp, _) = post_form(addr, "/input", "kind=bogus");
    assert!(hdr_bad_inp.starts_with("HTTP/1.1 400 Bad Request"));
    let (hdr_bad_clk, _) = post_form(addr, "/clock", "op=rate&v=9");
    assert!(hdr_bad_clk.starts_with("HTTP/1.1 400 Bad Request"));

    // Push past the 16-frame PNG history ring via telemetry sample ticks -> 410 Gone.
    let (hdr_p2, _) = post_form(addr, "/clock", "op=pause");
    assert!(hdr_p2.starts_with("HTTP/1.1 200 OK"));
    for _ in 0..20 {
        let (h, _) = post_form(addr, "/clock", "op=step&ms=1000");
        assert!(h.starts_with("HTTP/1.1 200 OK"));
    }
    let (hdr_410, _) = http_request(
        addr,
        &format!("GET /frame.png?frame=1 HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(
        hdr_410.starts_with("HTTP/1.1 410 Gone"),
        "expected 410 Gone once frame 1 evicted from ring, got {hdr_410}"
    );

    stop.store(true, Ordering::Relaxed);
    handle
        .join()
        .expect("preview server thread should exit cleanly");
}

#[test]
fn preview_server_supports_legacy_settings_scenario() {
    let server = PreviewServer::bind_scenario(0, SimScenario::Settings)
        .expect("bind legacy settings preview server");
    let addr = server.addr();
    let stop = server.stop_handle();

    let handle = thread::spawn(move || {
        server.serve().expect("preview server loop failed");
    });
    thread::sleep(Duration::from_millis(25));

    let (hdr_st, st) = http_request(
        addr,
        &format!("GET /state HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_st.starts_with("HTTP/1.1 200 OK"));
    let st_str = String::from_utf8(st).unwrap();
    assert!(st_str.contains("\"scenario\":\"settings\""), "{st_str}");
    assert!(st_str.contains("\"focus_key\":33"), "{st_str}");

    let (hdr_next, next_st) = post_form(addr, "/input", "kind=next");
    assert!(hdr_next.starts_with("HTTP/1.1 200 OK"));
    assert!(next_st.contains("\"focus_key\":43"), "{next_st}");

    stop.store(true, Ordering::Relaxed);
    handle.join().expect("legacy preview server exits cleanly");
}
