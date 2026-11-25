use std::path::Path;
use std::{fs, process};

use flick_backend::host::runtime::Cancellation;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{BackendPin, sha256};

use super::{cached, package_patterns, patterns_path};
use crate::component;

fn js_pin() -> (BackendPin, Vec<u8>) {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../backends/js/package");
    let pin = serde_json::from_slice(&fs::read(package.join("backend.json")).expect("pin"))
        .expect("JSON");
    (
        pin,
        fs::read(package.join("component.wasm")).expect("component"),
    )
}

fn js(cancel: Cancellation) -> WasmBackend {
    let (pin, bytes) = js_pin();
    let engine = component::engine().expect("an engine");
    WasmBackend::load_with_cancellation(engine, pin, &bytes, Budget::default(), cancel)
        .expect("the JS component loads")
}

/// A `package-files` call that fails, here by cancellation, leaves no cached
/// answer behind, so the next run asks the backend again and sees its packages.
#[test]
fn a_failed_package_files_call_is_not_kept() {
    let engine = component::engine().expect("an engine");
    let mut pin = js_pin().0;
    // a digest of its own keeps this test's entry apart from the real store's
    pin.sha256 = sha256(format!("failed package files {}", process::id()).as_bytes());
    let path = patterns_path(&pin.sha256).expect("a store");
    let _ = fs::remove_file(&path);

    let cancel = Cancellation::new(engine);
    let cancelled = js(cancel.clone());
    cancel.cancel();
    assert!(cancelled.try_package_files().is_err());
    assert_eq!(package_patterns(&pin, &cancelled), None);
    assert!(
        !path.exists(),
        "a failed call was cached as the backend's answer"
    );

    let live = js(Cancellation::new(engine));
    let patterns = package_patterns(&pin, &live).expect("the JS backend answers");
    assert!(!patterns.is_empty(), "the JS backend reads package.json");
    assert_eq!(cached(&pin.sha256), Some(patterns));
    fs::remove_file(path).expect("cleanup");
}
