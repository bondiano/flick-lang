use flick_core::codegen::EffectBridge;
use flick_core::project::{Compilation, Host, Project};

use crate::sources::FsHost;

/// Whether compiled RT code may reach the separately installed Effect adapter:
/// declared in `[target.js.dependencies]` or in the backend's declared dependencies.
pub(super) fn effect_bridge(
    host: &FsHost,
    project: Option<&Project>,
    compilation: &Compilation,
) -> EffectBridge {
    let available = project.is_none_or(|p| {
        compilation.target_dependencies.iter().any(|dependency| {
            dependency.target == "js" && dependency.name == "@flick-lang/effect-ts"
        }) || host
            .foreign(p)
            .declared
            .iter()
            .any(|dependency| dependency == "@flick-lang/effect-ts")
    });
    if available {
        EffectBridge::Available
    } else {
        EffectBridge::Absent
    }
}
