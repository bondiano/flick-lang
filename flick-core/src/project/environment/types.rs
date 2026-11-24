//! Core environment, backend-tag, and target-override data structures.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use crate::project::config::BackendEnvironment;
#[cfg(not(target_arch = "wasm32"))]
use crate::syntax::when;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::ValueAbiSpec;
use crate::types::{AdapterWordSpec, ForeignBackendSpec, TypeFormAttrSpec};

/// One selectable tag axis published by a backend descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendAxis {
    pub name: String,
    pub tags: Vec<String>,
    pub default: Option<String>,
    pub exclusive: bool,
}

/// Environment tags, axes, built-in hosts, and foreign metadata vocabulary
/// published by a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendTags {
    /// Full backend id, e.g. `"flick/js"`, `"flick/go"`, `"test/literal"`.
    pub id: String,
    /// Short target name, e.g. `"js"`, `"go"`, `"literal"`.
    pub short: String,
    /// Always-on backend tags, e.g. `["js"]`, `["go"]`, `["literal"]`.
    pub tags: Vec<String>,
    /// Selectable tag axes.
    pub axes: Vec<BackendAxis>,
    /// Built-in `(backend, host)` environment pairs published by the backend.
    pub environments: Vec<(String, String)>,
    /// Backend-declared foreign adapter words.
    pub foreign_adapters: Vec<AdapterWordSpec>,
    /// Backend-declared foreign type-form attributes (such as `@ts`, `@go`).
    pub type_form_attributes: Vec<TypeFormAttrSpec>,
    /// Versioned value-ABI table published by this backend, if any.
    #[cfg(not(target_arch = "wasm32"))]
    pub value_abi: Option<ValueAbiSpec>,
}

impl BackendTags {
    /// Converts this backend's foreign adapter words, type-form attributes, and
    /// value-ABI table into a [`ForeignBackendSpec`] for type checking and IDE queries.
    pub fn foreign_spec(&self) -> ForeignBackendSpec {
        ForeignBackendSpec {
            backend_id: self.id.clone(),
            adapters: self.foreign_adapters.clone(),
            type_form_attributes: self.type_form_attributes.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            value_abi: self.value_abi.clone(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl BackendTags {
    /// Whether `name` refers to this backend by short name, full id, or base tag.
    pub fn matches_name(&self, name: &str) -> bool {
        self.short == name || self.id == name || self.tags.iter().any(|t| t == name)
    }

    /// Tags owned directly by this backend (base tags + axis tags, excluding host tags).
    pub fn backend_vocabulary(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        out.extend(self.tags.iter().cloned());
        for axis in &self.axes {
            out.extend(axis.tags.iter().cloned());
        }
        out
    }

    /// All tags this backend contributes to the combined vocabulary (including built-in host names).
    pub fn full_vocabulary(&self) -> BTreeSet<String> {
        let mut out = self.backend_vocabulary();
        for (_, platform) in &self.environments {
            out.insert(platform.clone());
        }
        out
    }
}

/// CLI or build-time override of the target backend and its resolved options.
#[allow(
    clippy::struct_excessive_bools,
    reason = "one per target override flag"
)]
#[derive(Debug, Clone, Default)]
pub struct TargetOverride {
    /// Selected backend short name or id (e.g. `"js"`, `"go"`, `"literal"`).
    pub target: Option<String>,
    /// True when `--target` selected a different backend than the project default.
    pub target_overridden: bool,
    /// Descriptor tags of the loaded backend component, when already loaded by the host.
    pub backend: Option<BackendTags>,
    /// Resolved backend options (`name -> values`).
    pub options: BTreeMap<String, Vec<String>>,
    /// Explicit JS runtime override (`Some(true)` = `rt`, `Some(false)` = `effect`).
    pub rt: Option<bool>,
    /// Explicit UI renderer override (`--ui` or `[target.js] ui`).
    pub ui: Option<String>,
    /// Explicit Effect-adapter availability override (when the caller already computed it).
    pub effect_adapter: Option<bool>,
    /// True during `flick test` so reachability checks test suites rather than app entries.
    pub for_tests: bool,
    /// Specific test file paths selected for `flick test`, if any.
    pub test_paths: Vec<PathBuf>,
    /// Skip invoking `Host::check_target` during `check_environment` when the
    /// caller (`flick build`, `flick test`, `flick inspect --emit output`) runs
    /// backend `check-target` itself during artifact generation.
    pub skip_backend_check_target: bool,
    /// Disable `@native` override lowering (`--no-native-overrides`) so functions run
    /// their Flick fallback body for conformance testing while still validating overrides.
    pub disable_native_overrides: bool,
}

/// Concrete environment resolved for checking, IDE analysis, or building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvironment {
    pub backend_id: String,
    pub backend: String,
    pub platform: String,
    pub host_package: Option<String>,
    /// Active environment tags (backend tags + active axis tags + active host tags).
    pub tags: BTreeSet<String>,
    /// True when `[build] environment` was explicitly set (enables `FLICK-E020` gated-service checks).
    pub explicit_environment: bool,
    /// Custom UI renderer specifier (`[target.js] ui` or `--ui`), if configured.
    pub ui: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ResolvedEnvironment {
    /// Pair representation `(backend, platform)` for legacy APIs and diagnostics.
    pub fn pair(&self) -> BackendEnvironment {
        BackendEnvironment {
            backend: self.backend.clone(),
            platform: self.platform.clone(),
        }
    }

    /// Active tags formatted for diagnostic messages.
    pub fn format_tags(&self) -> String {
        if self.tags.is_empty() {
            "none".to_owned()
        } else {
            self.tags
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    /// Evaluates a parsed `when` condition string against this environment's active tags.
    pub fn matches(&self, condition: &str) -> bool {
        when::parse(condition).is_ok_and(|expr| expr.eval(&|tag| self.tags.contains(tag)))
    }
}
