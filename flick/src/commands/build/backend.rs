use flick_core::codegen::EffectBridge;
use flick_core::project::{Compilation, Host, Project};

use crate::sources::FsHost;

/// whether compiled RT code may reach the separately installed Effect
/// adapter: a JS dependency or a package.json entry on the adapter.
pub(super) fn effect_bridge(
    host: &FsHost,
    project: Option<&Project>,
    compilation: &Compilation,
) -> EffectBridge {
    let available = project.is_none_or(|p| {
        compilation
            .js_dependencies
            .iter()
            .any(|dependency| dependency.name == "@flick-lang/effect-ts")
            || host
                .read(&p.root.join("package.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|package| {
                    package["dependencies"]
                        .get("@flick-lang/effect-ts")
                        .is_some()
                })
    });
    if available {
        EffectBridge::Available
    } else {
        EffectBridge::Absent
    }
}
