//! Environment resolution, backend tag axes, host packages, and condition tag
//! vocabulary validation (`FLICK-P059`, `FLICK-P065`, `FLICK-P066`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

mod resolve;
#[cfg(not(target_arch = "wasm32"))]
mod satisfies;
#[cfg(not(target_arch = "wasm32"))]
mod select;

pub(super) use resolve::resolve;

#[cfg(not(target_arch = "wasm32"))]
use super::config::BackendEnvironment;
use crate::diagnostics::{Diagnostic, Severity, codes, suggest};
use crate::resolve::ModuleSyntax;
use crate::source::{SourceFile, Span};
use crate::syntax::foreign_meta::ForeignMeta;
use crate::syntax::when;

/// One selectable tag axis published by a backend descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendAxis {
    pub name: String,
    pub tags: Vec<String>,
    pub default: Option<String>,
    pub exclusive: bool,
}

/// Environment tags, axes, and built-in hosts published by a backend.
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
    /// Backend-declared foreign adapter words and type-form attributes.
    pub foreign_meta: ForeignMeta,
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
    /// True when the caller (`flick build`, `flick test`, `flick inspect --emit output`)
    /// immediately runs the backend component's `check-target` and `generate`
    /// pipeline on the returned compilation, avoiding a duplicate check here.
    pub defer_backend_check: bool,
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

/// Built-in metadata for the official `flick/js` and `flick/go` backends.
/// Must stay in sync with `backends/js/src/component.rs` and `backends/go/src/component.rs`.
#[cfg(not(target_arch = "wasm32"))]
pub fn official_backends() -> Vec<BackendTags> {
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
            foreign_meta: ForeignMeta::js(),
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
            foreign_meta: ForeignMeta::go(),
        },
    ]
}

/// Loaded `[host]` package tags associated with one backend.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub(super) struct BackendHost {
    pub tags: Vec<String>,
    pub requires: Option<String>,
}

/// Result of resolving a project's environment and building its tag vocabulary.
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct EnvironmentContext {
    pub vocabulary: BTreeSet<String>,
    /// False when a frontend-only host (`!host.loads_backends()`) cannot load
    /// an uncached custom `[build.backend]` descriptor, so tag vocabulary is
    /// incomplete and must not emit false `FLICK-P066` errors.
    pub validate_tags: bool,
    pub resolved: Option<ResolvedEnvironment>,
    /// Active backend's declared foreign adapter words and type-form attributes.
    pub foreign_meta: ForeignMeta,
    /// False when the selected backend descriptor could not be loaded or is
    /// unknown, so S011/S014 validation must not emit cascading errors.
    pub validate_foreign_meta: bool,
    /// All known backend descriptors for resolving per-package or `@when`-gated
    /// module foreign metadata.
    pub backends: Vec<BackendTags>,
    /// Loaded `[host]` packages keyed by canonical backend id.
    pub hosts: BTreeMap<String, Vec<BackendHost>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl EnvironmentContext {
    pub(super) fn standalone_js() -> Self {
        let backends = official_backends();
        let mut vocabulary = BTreeSet::new();
        for backend in &backends {
            vocabulary.extend(backend.full_vocabulary());
        }
        Self {
            vocabulary,
            validate_tags: true,
            resolved: None,
            foreign_meta: ForeignMeta::js(),
            validate_foreign_meta: true,
            backends,
            hosts: BTreeMap::new(),
        }
    }

    fn backend_can_satisfy(&self, backend: &BackendTags, expr: &when::Expr) -> bool {
        let extra_hosts = self.hosts.get(&backend.id).map_or(&[][..], Vec::as_slice);
        satisfies::backend_can_satisfy(backend, extra_hosts, expr)
    }

    /// Selects the foreign metadata descriptor for one module: its package's
    /// declared backend if any, else a uniquely matching backend for an
    /// inactive `@when` module, else the project's active backend.
    pub(super) fn foreign_meta_for(
        &self,
        package_backend: Option<&str>,
        module_when: Option<&str>,
    ) -> &ForeignMeta {
        if let Some(name) = package_backend
            && let Some(backend) = self.backends.iter().find(|b| b.matches_name(name))
        {
            return &backend.foreign_meta;
        }
        if let Some(cond) = module_when
            && let Ok(expr) = when::parse(cond)
        {
            let active_matches = match &self.resolved {
                Some(env) => expr.eval(&|tag| env.tags.contains(tag)),
                None => self
                    .backends
                    .iter()
                    .find(|b| b.id == self.foreign_meta.backend_id)
                    .is_some_and(|b| self.backend_can_satisfy(b, &expr)),
            };
            if !active_matches {
                let mut matching = self
                    .backends
                    .iter()
                    .filter(|b| self.backend_can_satisfy(b, &expr));
                if let (Some(only), None) = (matching.next(), matching.next()) {
                    return &only.foreign_meta;
                }
            }
        }
        &self.foreign_meta
    }
}

/// Validates `@when "<cond>"` on a standalone file checked outside any project.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn validate_standalone_when(
    syntax: &ModuleSyntax,
    file: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((cond, span)) = syntax.header.as_ref().and_then(|h| h.when.as_ref()) else {
        return;
    };
    let mut vocabulary = BTreeSet::new();
    for backend in official_backends() {
        vocabulary.extend(backend.full_vocabulary());
    }
    validate_condition_tags(
        cond,
        Some(*span),
        Some(&file.text),
        &vocabulary,
        diagnostics,
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn validate_module_when(
    cond: &str,
    literal_span: Option<Span>,
    source_text: Option<&str>,
    ctx: &EnvironmentContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if ctx.validate_tags {
        validate_condition_tags(
            cond,
            literal_span,
            source_text,
            &ctx.vocabulary,
            diagnostics,
        );
    }
}

/// Validates every tag referenced in `cond` against `vocabulary`, emitting
/// `FLICK-P066` at each unknown tag's subspan.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn validate_condition_tags(
    cond: &str,
    literal_span: Option<Span>,
    source_text: Option<&str>,
    vocabulary: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let known: Vec<&str> = vocabulary.iter().map(String::as_str).collect();
    validate_tags_against(cond, literal_span, source_text, &known, diagnostics);
}

fn validate_tags_against(
    cond: &str,
    literal_span: Option<Span>,
    source_text: Option<&str>,
    known: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Ok(expr) = when::parse(cond) else {
        return;
    };
    for (tag, range) in expr.tags() {
        if known.contains(&tag) {
            continue;
        }
        let at = match (literal_span, source_text) {
            (Some(span), Some(text)) => Some(when::subspan(text, span, range)),
            (Some(span), None) => Some(span),
            (None, _) => None,
        };
        let message = format!(
            "unknown environment tag `{tag}` in condition `{cond}`; known tags: {}",
            known
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut diag = match at {
            Some(span) => Diagnostic::error(codes::P066, message, span),
            None => Diagnostic::new(codes::P066, Severity::Error, message),
        };
        if let Some(closest) = suggest::closest(tag, known.iter().copied()) {
            diag = diag.with_fix(|| (format!("did you mean `{closest}`?"), Vec::new()));
        }
        diagnostics.push(diag);
    }
}

#[cfg(target_arch = "wasm32")]
const OFFICIAL_TAGS: &[&str] = &[
    "browser",
    "bun",
    "cgo",
    "cloudflare",
    "effect",
    "effect-adapter",
    "go",
    "js",
    "native",
    "node",
    "rt",
];

#[cfg(target_arch = "wasm32")]
pub(super) struct EnvironmentContext {
    pub foreign_meta: ForeignMeta,
    pub validate_foreign_meta: bool,
}

#[cfg(target_arch = "wasm32")]
impl EnvironmentContext {
    pub(super) fn standalone_js() -> Self {
        Self {
            foreign_meta: ForeignMeta::js(),
            validate_foreign_meta: true,
        }
    }

    pub(super) fn foreign_meta_for(
        &self,
        _package_backend: Option<&str>,
        _module_when: Option<&str>,
    ) -> &ForeignMeta {
        &self.foreign_meta
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn validate_standalone_when(
    syntax: &ModuleSyntax,
    file: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((cond, span)) = syntax.header.as_ref().and_then(|h| h.when.as_ref()) else {
        return;
    };
    validate_tags_against(
        cond,
        Some(*span),
        Some(&file.text),
        OFFICIAL_TAGS,
        diagnostics,
    );
}

#[cfg(target_arch = "wasm32")]
pub(super) fn validate_module_when(
    cond: &str,
    literal_span: Option<Span>,
    source_text: Option<&str>,
    _ctx: &EnvironmentContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_tags_against(cond, literal_span, source_text, OFFICIAL_TAGS, diagnostics);
}
