use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::{env, fs, process, thread};

use flick_core::plugin::reach::Outside;
use flick_plugin::{ReachError, Request, Response};

use super::Live;

/// A server at a port of 127.0.0.1 that answers each request with
/// `answer`, and the URL of `path` on it: a probe of the new port, as WSL
/// makes, is answered too.
fn serve(answer: &'static [u8], path: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("bound").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let mut reader = BufReader::new(stream.try_clone().expect("stream"));
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let _ = stream.write_all(answer);
        }
    });
    format!("http://127.0.0.1:{port}{path}")
}

fn dir(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("flick-outside-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn get(url: String) -> Request {
    Request {
        method: "GET".into(),
        url,
        ..Request::default()
    }
}

#[test]
fn an_answer_is_kept_and_replayed_offline() {
    let at = dir("text").join("a.json");
    let url = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", "/t");
    let online = Live { offline: false };
    let answered = online.fetch(&at, &get(url.clone())).expect("answered");
    assert_eq!(
        answered,
        Response {
            status: 200,
            body: b"hello".to_vec()
        }
    );
    let kept = fs::read_to_string(&at).expect("kept");
    assert!(kept.contains("\"text\": \"hello\""), "{kept}");
    let offline = Live { offline: true };
    assert_eq!(offline.fetch(&at, &get(url)), Ok(answered));
}

#[test]
fn a_binary_body_is_kept_as_hex() {
    let at = dir("hex").join("a.json");
    let url = serve(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n\xff\x00",
        "/b",
    );
    let answered = (Live { offline: false })
        .fetch(&at, &get(url))
        .expect("answered");
    assert!(
        fs::read_to_string(&at)
            .expect("kept")
            .contains("\"hex\": \"ff00\"")
    );
    assert_eq!((Live { offline: true }).replay(&at), Some(answered));
}

#[test]
fn a_server_failure_is_answered_but_not_kept() {
    let at = dir("fail").join("a.json");
    let url = serve(b"HTTP/1.1 503 Busy\r\nContent-Length: 0\r\n\r\n", "/");
    let answered = (Live { offline: false })
        .fetch(&at, &get(url))
        .expect("answered");
    assert_eq!(answered.status, 503);
    assert!(!at.exists());
}

#[test]
fn a_redirect_is_the_answer_not_followed() {
    let at = dir("redirect").join("a.json");
    let url = serve(
        b"HTTP/1.1 302 Found\r\nLocation: http://example.com/\r\nContent-Length: 0\r\n\r\n",
        "/",
    );
    let answered = (Live { offline: false })
        .fetch(&at, &get(url))
        .expect("answered");
    assert_eq!(answered.status, 302);
}

#[test]
fn offline_without_a_snapshot_is_no_answer() {
    let at = dir("offline").join("a.json");
    let asked = (Live { offline: true }).fetch(&at, &get("http://127.0.0.1:9/".into()));
    assert!(matches!(asked, Err(ReachError::Offline(_))), "{asked:?}");
}

#[test]
fn a_header_with_a_line_break_is_refused() {
    let at = dir("header").join("a.json");
    let mut request = get("http://127.0.0.1:9/".into());
    request.headers.push(("X".into(), "a\r\nHost: evil".into()));
    let asked = (Live { offline: false }).fetch(&at, &request);
    assert!(matches!(asked, Err(ReachError::Failed(m)) if m.contains("breaks a line")));
}
