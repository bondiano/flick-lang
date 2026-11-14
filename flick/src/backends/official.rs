//! Compiler-distributed packages have the same pins and loader as project packages.

use super::{component::Package, failure};
use flick_backend::BackendPin;
use flick_core::diagnostics::Diagnostic;
use std::path::Path;

const JS: &[u8] = include_bytes!("../../../backends/js/package/component.wasm");
const PIN: &str = include_str!("../../../backends/js/package/backend.json");

pub(super) fn javascript(root: &Path, rt: bool) -> Result<Package, Diagnostic> {
    let pin: BackendPin = serde_json::from_str(PIN)
        .map_err(|e| failure(format!("invalid bundled JS backend pin: {e}")))?;
    Package::from_bytes(root, pin, JS, rt)
}
