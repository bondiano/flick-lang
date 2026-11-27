//! Pluggable host callback hooks for non-native embedders (such as `wasm32` JS hosts).
//!
//! Embedders may install a [`HostCallbacks`] implementation on
//! [`crate::session::VmSession`] to supply console sinks, real wall/monotonic
//! clocks, host entropy, environment variables, and (when [`crate::sandbox::Capability::Net`]
//! is granted) host-provided `fetch` without requiring OS APIs in `flick-vm`.

use std::fmt;

/// Host HTTP fetch request forwarded to an embedder's `fetch` callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFetchRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Host HTTP fetch response returned by an embedder's `fetch` callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFetchResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Embedder-supplied host service callbacks (console, clock, random, env, fetch).
///
/// Every method has a safe default returning `None` (or an unsupported error for
/// `fetch`), so embedders only implement the host services they wish to bridge.
pub trait HostCallbacks: fmt::Debug {
    /// Invoked whenever guest code writes to standard output.
    fn write_stdout(&mut self, _text: &str) {}

    /// Invoked whenever guest code writes to standard error.
    fn write_stderr(&mut self, _text: &str) {}

    /// Returns wall-clock milliseconds since the Unix epoch when running in
    /// [`crate::stdlib::HostMode::LiveOs`].
    fn current_time_millis(&mut self) -> Option<f64> {
        None
    }

    /// Returns wall-clock nanoseconds since the Unix epoch when running in
    /// [`crate::stdlib::HostMode::LiveOs`].
    fn current_time_nanos(&mut self) -> Option<u128> {
        None
    }

    /// Returns a monotonically non-decreasing timestamp in nanoseconds when
    /// running in [`crate::stdlib::HostMode::LiveOs`].
    fn monotonic_time_nanos(&mut self) -> Option<u128> {
        None
    }

    /// Returns a host-provided random draw in `[0, 1)` when running in
    /// [`crate::stdlib::HostMode::LiveOs`].
    fn random_next(&mut self) -> Option<f64> {
        None
    }

    /// Looks up an environment variable in [`crate::stdlib::HostMode::LiveOs`].
    fn env_var(&mut self, _key: &str) -> Option<String> {
        None
    }

    /// Lists all environment variables in [`crate::stdlib::HostMode::LiveOs`].
    fn env_vars(&mut self) -> Option<Vec<(String, String)>> {
        None
    }

    /// Performs a host-provided HTTP fetch when [`crate::sandbox::Capability::Net`]
    /// is granted.
    fn fetch(&mut self, _req: &HostFetchRequest) -> Result<HostFetchResponse, String> {
        Err("host fetch callback not configured".to_owned())
    }
}
