//! The official JavaScript backend built from this compiler's own sources,
//! run in process instead of as its component when the selected component is
//! the one built from those sources: what the editor asks of a project's
//! packages on every analysis then needs no WebAssembly engine, nor the
//! component's compiled code, in the language server. `check-component`
//! holds the pin committed beside the sources to them. The Go backend stays
//! a component: its crate ships files its component does, and the compiler
//! holds no backend component.

use std::sync::{Arc, OnceLock};

use flick_backend::{Backend, BackendPin};

/// The pins committed beside the sources of the backends that run in process.
const PINS: [&str; 1] = [include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../backends/js/package/backend.json"
))];

/// The official backend `pin` names, in process, when it is the component
/// built from this compiler's sources.
pub(super) fn backend(pin: &BackendPin) -> Option<Arc<dyn Backend>> {
    if !own(pin) {
        return None;
    }
    match pin.identity.id.as_str() {
        super::official::JS => Some(Arc::new(flick_backend_js::Javascript)),
        _ => None,
    }
}

/// Whether `pin` is the committed pin of a backend's own sources.
fn own(pin: &BackendPin) -> bool {
    static OWN: OnceLock<Vec<BackendPin>> = OnceLock::new();
    let own = OWN.get_or_init(|| {
        (PINS.iter())
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect()
    });
    own.contains(pin)
}

#[cfg(test)]
mod tests {
    use super::{PINS, backend};
    use flick_backend::BackendPin;

    fn pins() -> Vec<BackendPin> {
        (PINS.iter())
            .map(|json| serde_json::from_str(json).expect("a committed pin"))
            .collect()
    }

    /// Only the very components built from these sources run in process: an
    /// official backend of another version or digest stays a component.
    #[test]
    fn only_the_components_of_these_sources_run_in_process() {
        for pin in pins() {
            assert!(backend(&pin).is_some(), "{}", pin.identity.id);
            let mut other = pin.clone();
            other.sha256 = "0".repeat(64);
            assert!(backend(&other).is_none());
            let mut other = pin.clone();
            other.identity.version.push_str("-next");
            assert!(backend(&other).is_none());
            let mut other = pin;
            other.identity.id = "acme/js".into();
            assert!(backend(&other).is_none());
        }
    }
}
