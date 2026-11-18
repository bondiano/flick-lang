//! Compiler-distributed packages have the same pins and loader as project packages.

use super::{component::Package, failure};
use flick_backend::BackendPin;
use flick_core::diagnostics::Diagnostic;
use flick_core::project::config::BackendPackage;
use std::collections::BTreeMap;
use std::path::Path;

const JS: &[u8] = include_bytes!("../../../backends/js/package/component.wasm");
const PIN: &str = include_str!("../../../backends/js/package/backend.json");

pub(super) fn javascript(
    root: &Path,
    rt: bool,
    tools: &BTreeMap<String, Vec<String>>,
) -> Result<Package, Diagnostic> {
    Package::from_bytes(root, pin()?, JS, rt, tools)
}

pub(super) fn pin() -> Result<BackendPin, Diagnostic> {
    serde_json::from_str(PIN).map_err(|e| failure(format!("invalid bundled JS backend pin: {e}")))
}

pub(super) fn matches(package: &BackendPackage) -> bool {
    pin().is_ok_and(|pin| {
        package.id == pin.identity.id
            && package.version == pin.identity.version
            && package.sha256 == pin.sha256
    })
}
