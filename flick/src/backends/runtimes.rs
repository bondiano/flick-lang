//! The runtime pins of a build, from the report of the backend that makes
//! it: when it is loaded, and again when a launch rechecks its build.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use flick_backend::host::WasmBackend;
use flick_backend::{BackendPin, OptionValue, RuntimeDependency};

/// The components this process loaded, by pin: a launch verifies the
/// runtime of a build with the backend that made it.
static LOADED: Mutex<Vec<(BackendPin, Arc<WasmBackend>)>> = Mutex::new(Vec::new());

pub(super) fn loaded(pin: &BackendPin, backend: &Arc<WasmBackend>) {
    let mut all = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    if !all.iter().any(|(p, _)| p == pin) {
        all.push((pin.clone(), Arc::clone(backend)));
    }
}

/// The backend of `pin`, if this process loaded it.
pub(super) fn find(pin: &BackendPin) -> Option<Arc<WasmBackend>> {
    let all = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    (all.iter().find(|(p, _)| p == pin)).map(|(_, backend)| Arc::clone(backend))
}

/// The pins of the runtime packages `names`, as the backend of `pin`,
/// loaded by this process, resolves them from the directory `dir`.
pub(super) fn runtime_from(
    pin: &BackendPin,
    dir: &Path,
    names: &[String],
) -> Result<Vec<RuntimeDependency>, String> {
    let backend = {
        let all = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
        let (_, backend) = (all.iter().find(|(p, _)| p == pin))
            .ok_or("the backend of this build is not loaded")?;
        Arc::clone(backend)
    };
    let real = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let options = [
        OptionValue {
            key: "packages".into(),
            value: names.join(","),
        },
        OptionValue {
            key: "root".into(),
            value: real.to_string_lossy().replace('\\', "/"),
        },
    ];
    super::packages::report(&*backend, dir, &[], &options).map(|report| report.runtime)
}

/// The pins of the runtime packages `backend` reports for the project at
/// `root`, of the effect runtime or of Flick's own.
pub(super) fn runtime(
    backend: &WasmBackend,
    root: &Path,
    effect: bool,
) -> Result<Vec<RuntimeDependency>, String> {
    let real = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let options = [
        OptionValue {
            key: "runtime".into(),
            value: if effect { "effect" } else { "rt" }.into(),
        },
        OptionValue {
            key: "root".into(),
            value: real.to_string_lossy().replace('\\', "/"),
        },
    ];
    super::packages::report(backend, root, &[], &options).map(|report| report.runtime)
}
