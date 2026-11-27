//! `wasm-bindgen` runtime interface to `flick-vm` for browsers and Node.js.
//!
//! Exposes a single-threaded cooperative [`VmSession`] that loads and saves
//! `.fvm` images from byte buffers (`Uint8Array`), enforces `#992` capability
//! and resource limits (with wall-clock deadlines backed by JS
//! `performance.now()` on `wasm32`), bridges console/clock/random/env/fetch
//! callbacks to the JS host, and steps long-running or infinite Flick
//! computations in bounded fuel slices (`start_eval` / `step` / `abandon`).

mod config;
mod host;

use flick_core::ir::memory::Name;
use flick_vm::image::{compiler_hash, format_hash, memory_ir_hash};
use flick_vm::sandbox::verify_image_bytes;
use flick_vm::session::VmSession as InnerSession;
use serde_json::json;
use wasm_bindgen::prelude::*;

use config::{
    encode_eval_outcome, encode_fetch_response, encode_load_report, encode_step_outcome,
    encode_test_results, encode_unload_report, parse_fetch_request, parse_session_config,
};
#[cfg(target_arch = "wasm32")]
use flick_vm::stdlib::HostCallbacks;
#[cfg(target_arch = "wasm32")]
use host::JsHostCallbacks;

/// Returns the Flick compiler/runtime version string baked into this VM build.
#[wasm_bindgen]
pub fn vm_version() -> String {
    flick_core::VERSION.to_owned()
}

/// Returns the 64-character hex compiler compatibility hash expected by `.fvm` images.
#[wasm_bindgen]
pub fn image_compiler_hash() -> String {
    format_hash(&compiler_hash())
}

/// Returns the 64-character hex memory-IR schema hash expected by `.fvm` images.
#[wasm_bindgen]
pub fn image_memory_ir_hash() -> String {
    format_hash(&memory_ir_hash())
}

/// Single-threaded cooperative Flick VM session exposed to JavaScript hosts.
#[wasm_bindgen]
pub struct VmSession {
    inner: InnerSession,
}

#[wasm_bindgen]
impl VmSession {
    /// Creates a new VM session from an optional JSON configuration object.
    ///
    /// Defaults to the untrusted sandbox preset (zero external capabilities,
    /// conservative fuel/heap/wall-time/stack limits, live host mode).
    #[wasm_bindgen(constructor)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(config_json: Option<String>) -> Result<Self, JsValue> {
        let resolved =
            parse_session_config(config_json.as_deref()).map_err(|err| JsValue::from_str(&err))?;
        let mut inner = InnerSession::with_sandbox(resolved.host_mode, resolved.sandbox);
        inner.set_program_args(resolved.program_args);
        Ok(Self { inner })
    }

    /// Replaces the session's sandbox limits, capabilities, and program args.
    pub fn set_config(&mut self, config_json: &str) -> Result<(), JsValue> {
        let resolved =
            parse_session_config(Some(config_json)).map_err(|err| JsValue::from_str(&err))?;
        self.inner.set_sandbox(resolved.sandbox);
        self.inner.set_program_args(resolved.program_args);
        Ok(())
    }

    /// Installs or clears the JS host callback object (`writeStdout`,
    /// `writeStderr`, `currentTimeMillis`, `currentTimeNanos`,
    /// `monotonicTimeNanos`, `randomNext`, `envVar`, `envVars`, `fetch`).
    #[allow(clippy::needless_pass_by_value)]
    pub fn set_host_callbacks(&mut self, callbacks: JsValue) {
        #[cfg(target_arch = "wasm32")]
        {
            let boxed =
                JsHostCallbacks::new(callbacks).map(|cb| Box::new(cb) as Box<dyn HostCallbacks>);
            self.inner.set_host_callbacks(boxed);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = callbacks;
            self.inner.set_host_callbacks(None);
        }
    }

    /// Verifies, decodes, and links `.fvm` image bytes (`Uint8Array`) into this session.
    ///
    /// Returns a JSON object describing the loaded modules, globals, restored
    /// values, optional entry point, and optional root layer.
    pub fn load_image(&mut self, bytes: &[u8]) -> Result<String, JsValue> {
        let image = verify_image_bytes(bytes).map_err(|err| JsValue::from_str(&err.to_string()))?;
        let entry = image.entry().map(|n| (n.module.clone(), n.name.clone()));
        let root = image
            .root_layer()
            .map(|n| (n.module.clone(), n.name.clone()));
        let report = self
            .inner
            .load_image(&image)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        let payload = encode_load_report(
            &report,
            entry.as_ref().map(|(m, n)| (m.as_str(), n.as_str())),
            root.as_ref().map(|(m, n)| (m.as_str(), n.as_str())),
        );
        Ok(payload.to_string())
    }

    /// Snapshots the live session into self-framing `.fvm` bytes (`Uint8Array`).
    #[allow(clippy::needless_pass_by_value)]
    pub fn save_image(
        &self,
        entry_module: Option<String>,
        entry_name: Option<String>,
    ) -> Result<Vec<u8>, JsValue> {
        let entry = match (entry_module, entry_name) {
            (Some(module), Some(name)) => Some(Name::new(&module, &name)),
            (None, None) => None,
            _ => {
                return Err(JsValue::from_str(
                    "entry_module and entry_name must both be provided or both omitted",
                ));
            }
        };
        self.inner
            .save_image_bytes(entry)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Unloads a module from the live session and returns a JSON report.
    pub fn unload_module(&mut self, module: &str) -> Result<String, JsValue> {
        let report = self
            .inner
            .unload_module(module)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        Ok(encode_unload_report(&report).to_string())
    }

    /// Returns a JSON array of all module names currently loaded in the session.
    pub fn loaded_modules(&self) -> String {
        let modules: Vec<String> = self.inner.loaded_modules().into_iter().collect();
        json!(modules).to_string()
    }

    /// Returns live accounted heap bytes in this session.
    pub fn heap_live_bytes(&self) -> usize {
        self.inner.heap_live_bytes()
    }

    /// Starts a cooperative evaluation of an already-loaded global (`module`,
    /// `name`). Returns JSON `{ "ok": true }` or `{ "ok": false, "outcome": ... }`.
    pub fn start_eval(&mut self, module: &str, name: &str, main_protocol: bool) -> String {
        let entry = Name::new(module, name);
        match self.inner.start_eval_loaded(&entry, main_protocol) {
            Ok(()) => json!({ "ok": true }).to_string(),
            Err(outcome) => json!({
                "ok": false,
                "outcome": encode_eval_outcome(&outcome),
            })
            .to_string(),
        }
    }

    /// Runs the active cooperative evaluation for up to `fuel` VM steps and
    /// returns a JSON `StepOutcome` (`yielded`, `completed`, `limitExceeded`,
    /// or `faulted`).
    pub fn step(&mut self, fuel: u32) -> String {
        let outcome = self.inner.step(u64::from(fuel));
        encode_step_outcome(&outcome).to_string()
    }

    /// Abandons the active cooperative evaluation, unwinding open scopes and
    /// running LIFO finalizers cleanly without corrupting the session.
    pub fn abandon(&mut self) -> String {
        let outcome = self.inner.abandon();
        encode_eval_outcome(&outcome).to_string()
    }

    /// Returns `true` if a cooperative evaluation is currently suspended.
    pub fn has_active_eval(&self) -> bool {
        self.inner.has_active_eval()
    }

    /// Returns buffered stdout for the active cooperative evaluation, if any.
    pub fn active_stdout(&self) -> Option<String> {
        self.inner.active_stdout().map(str::to_owned)
    }

    /// Returns buffered stderr for the active cooperative evaluation, if any.
    pub fn active_stderr(&self) -> Option<String> {
        self.inner.active_stderr().map(str::to_owned)
    }

    /// Executes a host-provided fetch through the installed JS `fetch` callback
    /// after enforcing `Capability::Net`.
    pub fn host_fetch(&mut self, request_json: &str) -> Result<String, JsValue> {
        let req = parse_fetch_request(request_json).map_err(|err| JsValue::from_str(&err))?;
        let resp = self
            .inner
            .host_fetch(&req)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        Ok(encode_fetch_response(&resp).to_string())
    }

    /// Runs a loaded `Test` suite global (`module`, `name`) on an isolated
    /// counted heap and returns a JSON array of test case results.
    #[allow(clippy::needless_pass_by_value)]
    pub fn run_loaded_test_suite(
        &self,
        module: &str,
        name: &str,
        seed: i32,
        fuel_per_test: u32,
        skip_groups_json: Option<String>,
    ) -> Result<String, JsValue> {
        let skip_groups: Vec<String> = match skip_groups_json.as_deref() {
            Some(raw) if !raw.trim().is_empty() => serde_json::from_str(raw)
                .map_err(|err| JsValue::from_str(&format!("invalid skip_groups JSON: {err}")))?,
            _ => Vec::new(),
        };
        let fuel = if fuel_per_test == 0 {
            u64::MAX
        } else {
            u64::from(fuel_per_test)
        };
        let entry = Name::new(module, name);
        let results = self
            .inner
            .run_loaded_test_suite(&entry, seed, fuel, &skip_groups)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        Ok(encode_test_results(&results).to_string())
    }

    /// Releases all persistent session roots and verifies zero leaked heap slots.
    pub fn shutdown(&mut self) -> Result<(), JsValue> {
        self.inner.shutdown().map_err(|err| JsValue::from_str(&err))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use flick_vm::sandbox::{Capability, CapabilitySet};

    #[test]
    fn default_config_is_untrusted_with_no_capabilities() {
        let resolved = parse_session_config(None).expect("default config parses");
        assert_eq!(resolved.sandbox.capabilities, CapabilitySet::pure());
        assert!(resolved.sandbox.limits.fuel.is_some());
        assert!(resolved.sandbox.limits.heap_bytes.is_some());
        assert!(resolved.sandbox.limits.wall_time_ms.is_some());
        assert!(resolved.sandbox.limits.max_stack_depth.is_some());
    }

    #[test]
    fn custom_config_parses_capabilities_and_limits() {
        let raw = r#"{
            "preset": "untrusted",
            "capabilities": ["clock", "random", "net"],
            "hostMode": "deterministic",
            "fuel": 5000,
            "heapBytes": 1048576,
            "wallTimeMs": 250,
            "maxStackDepth": 128,
            "programArgs": ["alpha", "beta"]
        }"#;
        let resolved = parse_session_config(Some(raw)).expect("custom config parses");
        assert!(resolved.sandbox.capabilities.contains(Capability::Clock));
        assert!(resolved.sandbox.capabilities.contains(Capability::Random));
        assert!(resolved.sandbox.capabilities.contains(Capability::Net));
        assert!(!resolved.sandbox.capabilities.contains(Capability::Fs));
        assert_eq!(resolved.sandbox.limits.fuel, Some(5000));
        assert_eq!(resolved.sandbox.limits.heap_bytes, Some(1_048_576));
        assert_eq!(resolved.sandbox.limits.wall_time_ms, Some(250));
        assert_eq!(resolved.sandbox.limits.max_stack_depth, Some(128));
        assert_eq!(resolved.program_args, vec!["alpha", "beta"]);
    }

    #[test]
    fn unknown_capability_and_preset_are_rejected() {
        assert!(parse_session_config(Some(r#"{"preset":"magic"}"#)).is_err());
        assert!(parse_session_config(Some(r#"{"capabilities":["teleport"]}"#)).is_err());
    }

    #[test]
    fn version_and_hashes_are_well_formed() {
        assert!(!vm_version().is_empty());
        assert_eq!(image_compiler_hash().len(), 64);
        assert_eq!(image_memory_ir_hash().len(), 64);
    }
}
