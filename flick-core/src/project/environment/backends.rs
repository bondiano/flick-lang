//! Built-in official backend metadata (`flick/js`, `flick/go`) and active
//! foreign backend selection for checking and IDE analysis.

use super::{BackendAxis, BackendTags, TargetOverride};
use crate::project::Project;
use crate::types::ForeignBackendSpec;

/// Built-in metadata for the official `flick/js` and `flick/go` backends.
/// Must stay in sync with `backends/js/src/component.rs` and `backends/go/src/component.rs`.
pub fn official_backends() -> Vec<BackendTags> {
    let js = ForeignBackendSpec::js();
    let go = ForeignBackendSpec::go();
    vec![
        BackendTags {
            id: "flick/js".into(),
            short: "js".into(),
            tags: vec!["js".into()],
            axes: vec![
                BackendAxis {
                    name: "runtime".into(),
                    tags: vec!["rt".into(), "effect".into()],
                    default: Some("rt".into()),
                    exclusive: true,
                },
                BackendAxis {
                    name: "adapter".into(),
                    tags: vec!["effect-adapter".into()],
                    default: None,
                    exclusive: false,
                },
            ],
            environments: vec![
                ("js".into(), "node".into()),
                ("js".into(), "browser".into()),
                ("js".into(), "bun".into()),
                ("js".into(), "cloudflare".into()),
            ],
            foreign_adapters: js.adapters,
            type_form_attributes: js.type_form_attributes,
        },
        BackendTags {
            id: "flick/go".into(),
            short: "go".into(),
            tags: vec!["go".into()],
            axes: vec![BackendAxis {
                name: "cgo".into(),
                tags: vec!["cgo".into()],
                default: None,
                exclusive: false,
            }],
            environments: vec![("go".into(), "native".into())],
            foreign_adapters: go.adapters,
            type_form_attributes: go.type_form_attributes,
        },
    ]
}

pub(super) fn select_foreign_backend(
    project: Option<&Project>,
    target_override: Option<&TargetOverride>,
    backends: &[BackendTags],
) -> (ForeignBackendSpec, bool) {
    if let Some(name) = target_override.and_then(|o| o.target.as_deref())
        && let Some(found) = backends.iter().find(|b| b.short == name || b.id == name)
    {
        return (found.foreign_spec(), true);
    }
    #[cfg(feature = "native-project")]
    if let Some(pkg) = project.and_then(|p| p.config.build.backend.as_ref()) {
        let short = pkg.id.rsplit('/').next().unwrap_or(&pkg.id);
        return match backends.iter().find(|b| b.id == pkg.id || b.short == short) {
            Some(found) => (found.foreign_spec(), true),
            None => (
                ForeignBackendSpec {
                    backend_id: pkg.id.clone(),
                    adapters: Vec::new(),
                    type_form_attributes: Vec::new(),
                },
                false,
            ),
        };
    }
    #[cfg(feature = "native-project")]
    if let Some(target) = project.and_then(|p| p.config.build.target.as_deref())
        && let Some(found) = backends
            .iter()
            .find(|b| b.short == target || b.id == target)
    {
        return (found.foreign_spec(), true);
    }
    if let Some(env) = project.and_then(|p| p.config.build.environment.as_ref())
        && let Some(found) = backends
            .iter()
            .find(|b| b.short == env.backend || b.id == env.backend)
    {
        return (found.foreign_spec(), true);
    }
    if let Some(host_cfg) = project.and_then(|p| p.config.host.as_ref())
        && let Some(found) = backends.iter().find(|b| b.matches_name(&host_cfg.backend))
    {
        return (found.foreign_spec(), true);
    }
    let default = backends
        .iter()
        .find(|b| b.short == "js")
        .map_or_else(ForeignBackendSpec::js, BackendTags::foreign_spec);
    (default, true)
}

pub(super) fn upsert_backend(backends: &mut Vec<BackendTags>, extra: BackendTags) {
    if let Some(existing) = backends.iter_mut().find(|b| b.id == extra.id) {
        *existing = extra;
    } else {
        backends.push(extra);
    }
}
