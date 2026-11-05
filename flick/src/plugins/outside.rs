//! The world outside the compiler as a plugin reaches it from the command
//! line and the language server: files and variables as they are now, and
//! answers over HTTP from the project's snapshots, fetched and kept there
//! when the run is online. A snapshot is a JSON file with the request's
//! method and URL, the status and the body: as text when it is UTF-8, else
//! as hex. Deleting it fetches the answer again.

use std::io::Read;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{env, fs, io, str};

use flick_core::plugin::reach::Outside;
use flick_plugin::{ReachError, Request, Response};
use serde_json::{Value, json};

use crate::fetch;

/// The largest body an answer over HTTP may have.
const LIMIT: u64 = 16 << 20;
const TIMEOUT: Duration = Duration::from_secs(30);

static OFFLINE: AtomicBool = AtomicBool::new(false);

/// Whether the plugins of this run answer from snapshots only.
pub fn set_offline(offline: bool) {
    OFFLINE.store(offline, Ordering::Relaxed);
}

/// The outside of this run; the language server's is always offline.
pub fn outside(offline: bool) -> Arc<dyn Outside> {
    Arc::new(Live {
        offline: offline || OFFLINE.load(Ordering::Relaxed),
    })
}

struct Live {
    offline: bool,
}

impl Outside for Live {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn env(&self, name: &str) -> Option<String> {
        env::var(name).ok()
    }

    fn fetch(&self, snapshot: &Path, request: &Request) -> Result<Response, ReachError> {
        if let Some(kept) = self.replay(snapshot) {
            return Ok(kept);
        }
        if self.offline {
            return Err(ReachError::Offline(format!(
                "no snapshot answers `{} {}` in an offline run: run once without `--offline` and `CI` to keep one at `{}`",
                request.method,
                request.url,
                snapshot.display()
            )));
        }
        let response = get(request).map_err(ReachError::Failed)?;
        // a server's failure is no answer to keep
        if response.status < 500 {
            keep(snapshot, request, &response).map_err(|e| {
                ReachError::Failed(format!("cannot keep `{}`: {e}", snapshot.display()))
            })?;
        }
        Ok(response)
    }

    fn replay(&self, snapshot: &Path) -> Option<Response> {
        let kept: Value = serde_json::from_slice(&fs::read(snapshot).ok()?).ok()?;
        let status = u16::try_from(kept["status"].as_u64()?).ok()?;
        let body = match (kept["text"].as_str(), kept["hex"].as_str()) {
            (Some(text), _) => text.as_bytes().to_vec(),
            (None, Some(hex)) => unhex(hex)?,
            (None, None) => return None,
        };
        Some(Response { status, body })
    }
}

/// The answer to `request`, without following a redirect: a plugin is
/// granted hosts, and a redirect may lead to another.
fn get(request: &Request) -> Result<Response, String> {
    let what = format!("`{} {}`", request.method, request.url);
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(TIMEOUT)
        .build();
    let mut call = agent.request(&request.method, &request.url);
    for (name, value) in &request.headers {
        if [name, value].iter().any(|s| s.contains(['\r', '\n'])) {
            return Err(format!("{what}: the header `{name}` breaks a line"));
        }
        call = call.set(name, value);
    }
    let response = match call.send_bytes(&request.body) {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(e) => return Err(format!("{what}: {e}")),
    };
    let status = response.status();
    let mut body = Vec::new();
    (response.into_reader().take(LIMIT + 1))
        .read_to_end(&mut body)
        .map_err(|e| format!("{what}: {e}"))?;
    if body.len() as u64 > LIMIT {
        return Err(format!("{what}: the answer is over {} MiB", LIMIT >> 20));
    }
    Ok(Response { status, body })
}

/// Writes the snapshot whole or not at all, so a run that stops halfway
/// leaves none.
fn keep(snapshot: &Path, request: &Request, response: &Response) -> io::Result<()> {
    let mut kept =
        json!({ "method": request.method, "url": request.url, "status": response.status });
    match str::from_utf8(&response.body) {
        Ok(text) => kept["text"] = Value::String(text.to_owned()),
        Err(_) => kept["hex"] = Value::String(fetch::hex(&response.body)),
    }
    if let Some(dir) = snapshot.parent() {
        fs::create_dir_all(dir)?;
    }
    let partial = snapshot.with_extension(format!("{}.tmp", process::id()));
    let text = serde_json::to_string_pretty(&kept).map_err(io::Error::other)?;
    fs::write(&partial, text + "\n")?;
    fs::rename(partial, snapshot)
}

fn unhex(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
#[path = "outside_tests.rs"]
mod tests;
