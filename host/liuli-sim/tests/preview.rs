use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use liuli_sim::image::decode_png_rgb8;
use liuli_sim::preview::PreviewServer;

fn http_request(addr: SocketAddr, req: &str) -> (String, Vec<u8>) {
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
    stream.write_all(req.as_bytes()).unwrap();
    stream.flush().unwrap();

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

#[test]
fn preview_server_serves_html_png_and_updates_on_input() {
    let server = PreviewServer::bind(0).expect("bind localhost ephemeral port");
    let addr = server.addr();
    let stop = server.stop_handle();

    let handle = thread::spawn(move || {
        server.serve().expect("preview server loop failed");
    });

    // Give server thread a moment to settle
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

    let (hdr_png1, png1) = http_request(
        addr,
        &format!("GET /frame.png HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_png1.starts_with("HTTP/1.1 200 OK"));
    let (w1, h1, rgb1) = decode_png_rgb8(&png1).expect("decode initial png");
    assert_eq!((w1, h1), (320, 240));

    let (hdr_st1, st1) = http_request(
        addr,
        &format!("GET /state HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_st1.starts_with("HTTP/1.1 200 OK"));
    let st1_str = String::from_utf8(st1).unwrap();
    assert!(
        st1_str.contains("\"focus_key\":33"),
        "expected initial brightness key 33, got {st1_str}"
    );

    let post_body = "kind=next";
    let post_req = format!(
        "POST /input HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{post_body}",
        post_body.len()
    );
    let (hdr_post, body_post) = http_request(addr, &post_req);
    assert!(hdr_post.starts_with("HTTP/1.1 200 OK"));
    let post_str = String::from_utf8(body_post).unwrap();
    assert!(
        post_str.contains("\"focus_key\":43"),
        "expected sound toggle key 43 after next, got {post_str}"
    );

    let (hdr_png2, png2) = http_request(
        addr,
        &format!("GET /frame.png HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    );
    assert!(hdr_png2.starts_with("HTTP/1.1 200 OK"));
    let (w2, h2, rgb2) = decode_png_rgb8(&png2).expect("decode post-input png");
    assert_eq!((w2, h2), (320, 240));
    assert_ne!(rgb1, rgb2, "rendered frame should reflect moved focus ring");

    stop.store(true, Ordering::Relaxed);
    handle
        .join()
        .expect("preview server thread should exit cleanly");
}
