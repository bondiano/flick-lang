//! Built-in official backend metadata (`flick/js`, `flick/go`) and active
//! foreign backend selection for checking and IDE analysis.

use std::collections::BTreeSet;

use super::{BackendAxis, BackendTags, EnvironmentContext, ResolvedEnvironment, TargetOverride};
use crate::project::Project;
use crate::project::config::HostConfig;
use crate::syntax::when;
use crate::types::{
    AxisTagModel, BackendTagModel, ForeignBackendSpec, HostTagModel, NativeOverrideEnv,
};

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
            #[cfg(not(target_arch = "wasm32"))]
            value_abi: js.value_abi,
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
            #[cfg(not(target_arch = "wasm32"))]
            value_abi: go.value_abi,
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
                    #[cfg(not(target_arch = "wasm32"))]
                    value_abi: None,
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

impl EnvironmentContext {
    /// Context for checking a standalone file outside a project.
    pub(in crate::project) fn standalone() -> Self {
        let backends = official_backends();
        let mut vocabulary = BTreeSet::new();
        for b in &backends {
            vocabulary.extend(b.full_vocabulary());
        }
        Self {
            vocabulary,
            validate_tags: true,
            resolved: None,
            foreign_backend: ForeignBackendSpec::js(),
            validate_foreign_metadata: true,
            backends,
            native_overrides: NativeOverrideEnv::official_js(),
        }
    }

    /// Selects the foreign metadata specification for one module: a uniquely
    /// matching backend for an inactive `@when` condition, else the project's
    /// active backend.
    pub(in crate::project) fn foreign_backend_for(
        &self,
        module_when: Option<&str>,
    ) -> ForeignBackendSpec {
        if let Some(cond) = module_when
            && let Ok(expr) = when::parse(cond)
        {
            let active_matches = match &self.resolved {
                Some(env) => expr.eval(&|tag| env.tags.contains(tag)),
                None => self
                    .backends
                    .iter()
                    .find(|b| b.id == self.foreign_backend.backend_id)
                    .is_some_and(|b| {
                        let vocab = b.full_vocabulary();
                        expr.eval(&|tag| vocab.contains(tag))
                    }),
            };
            if !active_matches {
                let mut matching = self.backends.iter().filter(|b| {
                    let vocab = b.full_vocabulary();
                    expr.eval(&|tag| vocab.contains(tag))
                });
                if let (Some(only), None) = (matching.next(), matching.next()) {
                    return only.foreign_spec();
                }
            }
        }
        self.foreign_backend.clone()
    }
}

/// Builds the Layer-2 [`NativeOverrideEnv`] from resolved backend/host metadata.
pub(super) fn build_native_override_env(
    vocabulary: &BTreeSet<String>,
    validate_tags: bool,
    backends: &[BackendTags],
    hosts: &[(Option<String>, &HostConfig)],
    resolved: Option<&ResolvedEnvironment>,
    foreign_backend: &ForeignBackendSpec,
    target_override: Option<&TargetOverride>,
) -> NativeOverrideEnv {
    let backend_models: Vec<BackendTagModel> = backends
        .iter()
        .map(|b| BackendTagModel {
            id: b.id.clone(),
            short: b.short.clone(),
            tags: b.tags.clone(),
            axes: b
                .axes
                .iter()
                .map(|a| AxisTagModel {
                    tags: a.tags.clone(),
                    default: a.default.clone(),
                    exclusive: a.exclusive,
                })
                .collect(),
            builtin_hosts: b.environments.iter().map(|(_, p)| p.clone()).collect(),
            foreign_spec: b.foreign_spec(),
        })
        .collect();
    let host_models: Vec<HostTagModel> = hosts
        .iter()
        .map(|(_, h)| HostTagModel {
            name: h.name.clone(),
            backend: h.backend.clone(),
            requires: h.requires.as_ref().map(|(c, _)| c.clone()),
            tags: h.tags.iter().map(|(t, _)| t.clone()).collect(),
        })
        .collect();
    let (active_tags, active_backend_id) = if let Some(env) = resolved {
        (Some(env.tags.clone()), Some(env.backend_id.clone()))
    } else if !validate_tags {
        (None, None)
    } else if let Some(backend) = backends.iter().find(|b| b.id == foreign_backend.backend_id) {
        let mut tags: BTreeSet<String> = backend.tags.iter().cloned().collect();
        for axis in &backend.axes {
            if backend.id == "flick/js" && axis.name == "runtime" {
                let chosen = match target_override.and_then(|o| o.rt) {
                    Some(true) => "rt",
                    Some(false) => "effect",
                    None => axis.default.as_deref().unwrap_or("rt"),
                };
                tags.insert(chosen.to_owned());
            } else if backend.id == "flick/js" && axis.name == "adapter" {
                if target_override.and_then(|o| o.effect_adapter) == Some(true) {
                    tags.insert("effect-adapter".to_owned());
                }
            } else if let Some(def) = &axis.default {
                tags.insert(def.clone());
            }
        }
        if let Some((_, platform)) = backend.environments.first() {
            tags.insert(platform.clone());
        }
        (Some(tags), Some(backend.id.clone()))
    } else {
        (None, None)
    };
    NativeOverrideEnv {
        vocabulary: vocabulary.clone(),
        validate_tags,
        backends: backend_models,
        hosts: host_models,
        active_tags,
        active_backend_id,
        disable_native_overrides: target_override.is_some_and(|o| o.disable_native_overrides),
        every_clause: false,
    }
}
