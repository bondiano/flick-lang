//! JS host callbacks and `performance.now()` monotonic wall clock for `wasm32`.

#[cfg(target_arch = "wasm32")]
use std::fmt;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use base64::Engine as _;
#[cfg(target_arch = "wasm32")]
use base64::engine::general_purpose::STANDARD as BASE64;
#[cfg(not(target_arch = "wasm32"))]
use flick_vm::sandbox::SystemWallClock;
use flick_vm::sandbox::WallClock;
#[cfg(target_arch = "wasm32")]
use flick_vm::stdlib::{HostCallbacks, HostFetchRequest, HostFetchResponse};
#[cfg(target_arch = "wasm32")]
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
export function perf_now_ms() {
  if (typeof performance !== "undefined" && typeof performance.now === "function") {
    return performance.now();
  }
  return Date.now();
}

export function call_host_void(host, name, arg) {
  if (!host) {
    return;
  }
  const fn = host[name];
  if (typeof fn === "function") {
    fn.call(host, arg);
  }
}

export function call_host_number(host, name) {
  if (!host) {
    return undefined;
  }
  const fn = host[name];
  if (typeof fn !== "function") {
    return undefined;
  }
  const value = fn.call(host);
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

export function call_host_string(host, name, arg) {
  if (!host) {
    return undefined;
  }
  const fn = host[name];
  if (typeof fn !== "function") {
    return undefined;
  }
  const value = fn.call(host, arg);
  return typeof value === "string" ? value : undefined;
}

export function call_host_env_vars(host) {
  if (!host) {
    return undefined;
  }
  const fn = host.envVars;
  if (typeof fn !== "function") {
    return undefined;
  }
  const value = fn.call(host);
  if (value === undefined || value === null) {
    return undefined;
  }
  if (Array.isArray(value)) {
    return JSON.stringify(value);
  }
  if (typeof value === "object") {
    return JSON.stringify(Object.entries(value));
  }
  return undefined;
}

export function call_host_fetch(host, requestJson) {
  if (!host) {
    return JSON.stringify({ ok: false, error: "host fetch callback not configured" });
  }
  const fn = host.fetch;
  if (typeof fn !== "function") {
    return JSON.stringify({ ok: false, error: "host fetch callback not configured" });
  }
  try {
    const response = fn.call(host, JSON.parse(requestJson));
    return JSON.stringify({ ok: true, response });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return JSON.stringify({ ok: false, error: message });
  }
}
"#)]
extern "C" {
    fn perf_now_ms() -> f64;
    fn call_host_void(host: &JsValue, name: &str, arg: &str);
    fn call_host_number(host: &JsValue, name: &str) -> Option<f64>;
    fn call_host_string(host: &JsValue, name: &str, arg: &str) -> Option<String>;
    fn call_host_env_vars(host: &JsValue) -> Option<String>;
    fn call_host_fetch(host: &JsValue, request_json: &str) -> String;
}

/// Monotonic millisecond [`WallClock`] backed by JS `performance.now()` on
/// `wasm32` (falling back to `Date.now()` if `performance` is absent).
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default)]
pub struct JsPerfWallClock;

#[cfg(target_arch = "wasm32")]
impl WallClock for JsPerfWallClock {
    fn now_ms(&self) -> u64 {
        f64_to_u64_saturating(perf_now_ms())
    }
}

/// Returns the default platform [`WallClock`] for `flick-vm-wasm`:
/// [`JsPerfWallClock`] (`performance.now()`) on `wasm32`, or
/// [`flick_vm::sandbox::SystemWallClock`] on native targets.
pub fn platform_wall_clock() -> Arc<dyn WallClock> {
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(JsPerfWallClock)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Arc::new(SystemWallClock::new())
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_to_u64_saturating(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= 18_446_744_073_709_551_615.0 {
        u64::MAX
    } else {
        value as u64
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_to_u128_saturating(value: f64) -> u128 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value as u128
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsFetchRequestWire<'a> {
    url: &'a str,
    method: &'a str,
    headers: &'a [(String, String)],
    #[serde(skip_serializing_if = "Option::is_none")]
    body_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_text: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsFetchEnvelope {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    response: Option<JsFetchResponseWire>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsFetchResponseWire {
    status: u16,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body_base64: Option<String>,
    #[serde(default)]
    body_text: Option<String>,
}

/// Bridges a JS object of optional host callback methods onto
/// [`flick_vm::stdlib::HostCallbacks`].
#[cfg(target_arch = "wasm32")]
pub struct JsHostCallbacks {
    host: JsValue,
}

#[cfg(target_arch = "wasm32")]
impl fmt::Debug for JsHostCallbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JsHostCallbacks")
    }
}

#[cfg(target_arch = "wasm32")]
impl JsHostCallbacks {
    pub fn new(host: JsValue) -> Option<Self> {
        if host.is_null() || host.is_undefined() {
            None
        } else {
            Some(Self { host })
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl HostCallbacks for JsHostCallbacks {
    fn write_stdout(&mut self, text: &str) {
        call_host_void(&self.host, "writeStdout", text);
    }

    fn write_stderr(&mut self, text: &str) {
        call_host_void(&self.host, "writeStderr", text);
    }

    fn current_time_millis(&mut self) -> Option<f64> {
        call_host_number(&self.host, "currentTimeMillis")
    }

    fn current_time_nanos(&mut self) -> Option<u128> {
        call_host_number(&self.host, "currentTimeNanos").map(f64_to_u128_saturating)
    }

    fn monotonic_time_nanos(&mut self) -> Option<u128> {
        call_host_number(&self.host, "monotonicTimeNanos").map(f64_to_u128_saturating)
    }

    fn random_next(&mut self) -> Option<f64> {
        call_host_number(&self.host, "randomNext")
    }

    fn env_var(&mut self, key: &str) -> Option<String> {
        call_host_string(&self.host, "envVar", key)
    }

    fn env_vars(&mut self) -> Option<Vec<(String, String)>> {
        let json = call_host_env_vars(&self.host)?;
        serde_json::from_str(&json).ok()
    }

    fn fetch(&mut self, req: &HostFetchRequest) -> Result<HostFetchResponse, String> {
        let wire = JsFetchRequestWire {
            url: &req.url,
            method: &req.method,
            headers: &req.headers,
            body_base64: req.body.as_ref().map(|b| BASE64.encode(b)),
            body_text: req
                .body
                .as_ref()
                .and_then(|b| String::from_utf8(b.clone()).ok()),
        };
        let req_json =
            serde_json::to_string(&wire).map_err(|err| format!("encode fetch request: {err}"))?;
        let raw = call_host_fetch(&self.host, &req_json);
        let env: JsFetchEnvelope = serde_json::from_str(&raw)
            .map_err(|err| format!("decode host fetch response: {err}"))?;
        if !env.ok {
            return Err(env
                .error
                .unwrap_or_else(|| "host fetch callback failed".to_owned()));
        }
        let resp = env
            .response
            .ok_or_else(|| "host fetch callback returned empty response".to_owned())?;
        let body = if let Some(b64) = resp.body_base64 {
            BASE64
                .decode(b64)
                .map_err(|err| format!("invalid response bodyBase64: {err}"))?
        } else if let Some(text) = resp.body_text {
            text.into_bytes()
        } else {
            Vec::new()
        };
        Ok(HostFetchResponse {
            status: resp.status,
            headers: resp.headers,
            body,
        })
    }
}
