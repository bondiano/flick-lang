//! Persistent Node 24 worker bridge for explicitly bridged foreign calls.
//!
//! Spawns one worker process over localhost TCP (`127.0.0.1:0`) and speaks
//! newline-delimited JSON with flat iterative wire payloads (`wire.rs`).
//! Refcount symmetry: every `Data::ForeignHandle` allocated in Rust corresponds
//! 1-to-1 with a host refCount increment in `worker.mjs`; dropping a Rust slot
//! queues a `release_handles` message and `close_exec` asserts 0 live handles.

use std::fmt;
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{self, Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};

use crate::tier0::Error;

pub(crate) mod wire;

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

struct Fd1Capture {
    buf: Vec<u8>,
    syncs: usize,
}

/// Persistent Node 24 host bridge shared across counted executions.
pub struct HostBridge {
    child: Child,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    fd1: Arc<(Mutex<Fd1Capture>, Condvar)>,
    fd1_seen: usize,
}

impl fmt::Debug for HostBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostBridge")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

fn accept_ready_worker(
    listener: &TcpListener,
    child: &mut Child,
    expected_token: &str,
) -> Result<(BufReader<TcpStream>, TcpStream), Error> {
    let _ = listener.set_nonblocking(true);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_nodelay(true);
                let rem = deadline.saturating_duration_since(Instant::now());
                let _ = stream.set_read_timeout(Some(rem.min(Duration::from_secs(5))));
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                let is_worker = reader.read_line(&mut line).is_ok_and(|n| {
                    n > 0
                        && serde_json::from_str::<Json>(&line).is_ok_and(|msg| {
                            msg.get("type").and_then(Json::as_str) == Some("ready")
                                && msg.get("token").and_then(Json::as_str) == Some(expected_token)
                        })
                });
                if is_worker {
                    let writer = reader
                        .get_ref()
                        .try_clone()
                        .map_err(|e| Error::HostBridge(format!("clone bridge socket: {e}")))?;
                    let _ = reader
                        .get_ref()
                        .set_read_timeout(Some(Duration::from_mins(1)));
                    return Ok((reader, writer));
                }
                // Drop foreign localhost probes (such as IDE/WSL port forwarders)
                // and keep waiting for our spawned worker process.
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(Error::HostBridge(format!(
                        "host bridge worker exited before connecting ({status})"
                    )));
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::HostBridge(
                        "timed out waiting for host bridge worker connection".into(),
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::HostBridge(format!("accept bridge worker: {err}")));
            }
        }
    }
}

impl HostBridge {
    /// Spawns a fresh Node 24 worker process and completes the TCP handshake.
    pub fn new() -> Result<Self, Error> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let worker_path = manifest_dir.join("src/stdlib/host_bridge/worker.mjs");
        let repo_root = manifest_dir
            .parent()
            .ok_or_else(|| Error::HostBridge("cannot resolve repo root".into()))?;
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| Error::HostBridge(format!("bind bridge listener: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::HostBridge(format!("bridge local addr: {e}")))?
            .port();
        let token = format!(
            "{}-{}",
            process::id(),
            NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
        );
        let mut child = Command::new("node")
            .arg(&worker_path)
            .arg(port.to_string())
            .arg(repo_root)
            .arg(&token)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::HostBridge(format!("spawn node host worker: {e}")))?;
        let fd1 = Arc::new((
            Mutex::new(Fd1Capture {
                buf: Vec::new(),
                syncs: 0,
            }),
            Condvar::new(),
        ));
        if let Some(mut child_out) = child.stdout.take() {
            let fd1_bg = Arc::clone(&fd1);
            thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                while let Ok(n) = child_out.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let (lock, cvar) = &*fd1_bg;
                    if let Ok(mut guard) = lock.lock() {
                        for &b in &chunk[..n] {
                            if b == 0 {
                                guard.syncs += 1;
                                cvar.notify_all();
                            } else {
                                guard.buf.push(b);
                            }
                        }
                    }
                }
            });
        }
        if let Some(mut child_err) = child.stderr.take() {
            thread::spawn(move || {
                let mut sink = io::stderr();
                let _ = io::copy(&mut child_err, &mut sink);
            });
        }
        let (reader, writer) = accept_ready_worker(&listener, &mut child, &token)?;
        Ok(Self {
            child,
            reader,
            writer,
            fd1,
            fd1_seen: 0,
        })
    }

    pub(crate) fn drain_fd1_stdout(&mut self, stdout: &mut String, wait_sync: bool) {
        let (lock, cvar) = &*self.fd1;
        let Ok(mut guard) = lock.lock() else {
            return;
        };
        if wait_sync {
            let target = self.fd1_seen + 1;
            let deadline = Instant::now() + Duration::from_millis(500);
            while guard.syncs < target {
                let rem = deadline.saturating_duration_since(Instant::now());
                if rem.is_zero() {
                    break;
                }
                match cvar.wait_timeout(guard, rem) {
                    Ok((g, _)) => guard = g,
                    Err(_) => return,
                }
            }
            self.fd1_seen = guard.syncs;
        }
        if !guard.buf.is_empty() {
            let text = String::from_utf8_lossy(&guard.buf);
            stdout.push_str(&text);
            guard.buf.clear();
        }
    }

    pub(crate) fn send(&mut self, msg: &Json) -> Result<(), Error> {
        let line = serde_json::to_string(msg)
            .map_err(|e| Error::HostBridge(format!("serialize bridge message: {e}")))?;
        self.writer
            .write_all(line.as_bytes())
            .and_then(|()| self.writer.write_all(b"\n"))
            .and_then(|()| self.writer.flush())
            .map_err(|e| Error::HostBridge(format!("write bridge message: {e}")))
    }

    pub(crate) fn recv(&mut self) -> Result<Json, Error> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .map_err(|e| Error::HostBridge(format!("read bridge message: {e}")))?;
        if n == 0 {
            return Err(Error::HostBridge("host bridge worker disconnected".into()));
        }
        serde_json::from_str(&line)
            .map_err(|e| Error::HostBridge(format!("parse bridge message: {e}")))
    }

    /// Non-blocking poll for any already-buffered or immediately available message.
    pub(crate) fn try_recv(&mut self) -> Result<Option<Json>, Error> {
        if !self.reader.buffer().is_empty() {
            return self.recv().map(Some);
        }
        let _ = self.reader.get_ref().set_nonblocking(true);
        let mut line = String::new();
        let res = self.reader.read_line(&mut line);
        let _ = self.reader.get_ref().set_nonblocking(false);
        let _ = self
            .reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_mins(1)));
        match res {
            Ok(0) => Err(Error::HostBridge("host bridge worker disconnected".into())),
            Ok(_) => serde_json::from_str(&line)
                .map(Some)
                .map_err(|e| Error::HostBridge(format!("parse bridge message: {e}"))),
            Err(err) if err.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(Error::HostBridge(format!("poll bridge message: {err}"))),
        }
    }

    pub(crate) fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<Json>, Error> {
        if !self.reader.buffer().is_empty() {
            return self.recv().map(Some);
        }
        let _ = self.reader.get_ref().set_read_timeout(Some(timeout));
        let mut line = String::new();
        let res = self.reader.read_line(&mut line);
        let _ = self
            .reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_mins(1)));
        match res {
            Ok(0) => Err(Error::HostBridge("host bridge worker disconnected".into())),
            Ok(_) => serde_json::from_str(&line)
                .map(Some)
                .map_err(|e| Error::HostBridge(format!("parse bridge message: {e}"))),
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Ok(None)
            }
            Err(err) => Err(Error::HostBridge(format!("poll bridge timeout: {err}"))),
        }
    }

    /// Temporarily sets `process.argv[1]` in the worker so self-spawning fixtures
    /// (`stdlib/rpc/{stdio,worker}`) resolve their compiled JS server entry script.
    pub fn set_entry_script(&mut self, script: Option<&Path>) -> Result<(), Error> {
        let value = script.map(|p| p.to_string_lossy().into_owned());
        self.send(&json!({ "op": "set_entry_script", "path": value }))?;
        loop {
            let msg = self.recv()?;
            match msg.get("type").and_then(Json::as_str) {
                Some("ok") => return Ok(()),
                Some("fatal") => {
                    let text = msg
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("worker fatal");
                    return Err(Error::HostBridge(text.to_owned()));
                }
                _ => {}
            }
        }
    }

    /// Installs or clears a mock `globalThis.fetch` map for the next execution
    /// (used for offline playground examples with sibling `.fetch.json` fixtures).
    pub fn set_fetch_mock(&mut self, replies: Option<&Json>) -> Result<(), Error> {
        self.send(&json!({ "op": "set_fetch_mock", "replies": replies }))?;
        loop {
            let msg = self.recv()?;
            match msg.get("type").and_then(Json::as_str) {
                Some("ok") => return Ok(()),
                Some("fatal") => {
                    let text = msg
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("worker fatal");
                    return Err(Error::HostBridge(text.to_owned()));
                }
                _ => {}
            }
        }
    }

    pub(crate) fn open_exec(
        &mut self,
        exec: usize,
        tags: &[String],
        imports: &[String],
    ) -> Result<(), Error> {
        self.send(&json!({
            "op": "open_exec",
            "exec": exec,
            "tags": tags,
            "imports": imports,
        }))?;
        loop {
            let msg = self.recv()?;
            match msg.get("type").and_then(Json::as_str) {
                Some("ok") => return Ok(()),
                Some("fatal") => {
                    let text = msg
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("worker fatal");
                    return Err(Error::HostBridge(text.to_owned()));
                }
                _ => {}
            }
        }
    }

    pub(crate) fn release_handles(&mut self, exec: usize, ids: &[u64]) -> Result<(), Error> {
        if ids.is_empty() {
            return Ok(());
        }
        self.send(&json!({ "op": "release_handles", "exec": exec, "ids": ids }))
    }

    pub(crate) fn equals_handles(
        &mut self,
        exec: usize,
        a: u64,
        b: u64,
        stdout: &mut String,
    ) -> Result<bool, Error> {
        self.send(&json!({ "op": "equals", "exec": exec, "a": a, "b": b }))?;
        loop {
            let Some(msg) = self.recv_timeout(Duration::from_secs(10))? else {
                return Err(Error::HostBridge(
                    "bridge timed out waiting for equals".into(),
                ));
            };
            match msg.get("type").and_then(Json::as_str) {
                Some("stdout") => {
                    if let Some(text) = msg.get("text").and_then(Json::as_str) {
                        stdout.push_str(text);
                    }
                }
                Some("equals_done") => {
                    return Ok(msg.get("equal").and_then(Json::as_bool).unwrap_or(false));
                }
                Some("fatal") => {
                    let text = msg
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("worker fatal");
                    return Err(Error::HostBridge(text.to_owned()));
                }
                _ => {}
            }
        }
    }

    pub(crate) fn close_exec(&mut self, exec: usize, stdout: &mut String) -> Result<usize, Error> {
        self.send(&json!({ "op": "close_exec", "exec": exec }))?;
        loop {
            let Some(msg) = self.recv_timeout(Duration::from_secs(10))? else {
                return Err(Error::HostBridge(
                    "bridge timed out waiting for close_exec".into(),
                ));
            };
            match msg.get("type").and_then(Json::as_str) {
                Some("stdout") => {
                    if let Some(text) = msg.get("text").and_then(Json::as_str) {
                        stdout.push_str(text);
                    }
                }
                Some("cb_invoke" | "cell_get" | "cell_set") => {
                    if let Some(req_id) = msg.get("req_id").and_then(Json::as_u64) {
                        let _ = self.send(&json!({
                            "op": "cb_return",
                            "exec": exec,
                            "req_id": req_id,
                            "ok": false,
                            "closing": true,
                            "message": "execution closing",
                        }));
                    }
                }
                Some("closed") => {
                    let live = msg.get("live_handles").and_then(Json::as_u64).unwrap_or(0);
                    return usize::try_from(live)
                        .map_err(|_| Error::HostBridge("live_handles overflow".into()));
                }
                Some("fatal") => {
                    let text = msg
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("worker fatal");
                    return Err(Error::HostBridge(text.to_owned()));
                }
                _ => {}
            }
        }
    }
}

impl Drop for HostBridge {
    fn drop(&mut self) {
        let _ = self.send(&json!({ "op": "shutdown" }));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
