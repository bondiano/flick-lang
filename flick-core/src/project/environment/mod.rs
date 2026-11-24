//! Environment resolution, backend tag axes, host packages, and condition tag
//! vocabulary validation (`FLICK-P059`, `FLICK-P065`, `FLICK-P066`, `FLICK-F014`).

#[cfg(not(target_arch = "wasm32"))]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "native-project")]
mod abi;
#[cfg(not(target_arch = "wasm32"))]
mod backends;
#[cfg(not(target_arch = "wasm32"))]
mod select;
mod types;

#[cfg(not(target_arch = "wasm32"))]
pub use backends::official_backends;
#[cfg(not(target_arch = "wasm32"))]
use backends::{select_foreign_backend, upsert_backend};
pub use types::{BackendAxis, BackendTags, ResolvedEnvironment, TargetOverride};

#[cfg(not(target_arch = "wasm32"))]
use super::config::HostConfig;
#[cfg(feature = "native-project")]
use super::packages::Packages;
#[cfg(feature = "native-project")]
use super::target;
use super::{Host, Project};
use crate::diagnostics::{Diagnostic, Severity, codes, suggest};
use crate::resolve::ModuleSyntax;
use crate::source::{SourceFile, SourceMap, Span};
use crate::syntax::when;
use crate::types::{ForeignBackendSpec, NativeOverrideEnv};
#[cfg(not(target_arch = "wasm32"))]
use select::resolve_selected;

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
    pub foreign_backend: ForeignBackendSpec,
    /// Whether foreign metadata words and attributes should be validated against
    /// `foreign_backend` (false when an uncached custom backend could not be loaded).
    pub validate_foreign_metadata: bool,
    /// Official and project-loaded backend descriptors, used to pick the right
    /// foreign metadata vocabulary for inactive `@when` modules.
    pub backends: Vec<BackendTags>,
    /// Tag world model and active selection state for `@native` overrides.
    pub native_overrides: NativeOverrideEnv,
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

#[cfg(not(target_arch = "wasm32"))]
impl EnvironmentContext {
    pub(crate) fn native_overrides(&self) -> &NativeOverrideEnv {
        &self.native_overrides
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) struct EnvironmentContext {
    pub foreign_backend: ForeignBackendSpec,
    pub validate_foreign_metadata: bool,
}

#[cfg(target_arch = "wasm32")]
impl EnvironmentContext {
    pub(super) fn standalone() -> Self {
        Self {
            foreign_backend: ForeignBackendSpec::js(),
            validate_foreign_metadata: true,
        }
    }

    pub(super) fn foreign_backend_for(&self, _module_when: Option<&str>) -> ForeignBackendSpec {
        self.foreign_backend.clone()
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn native_overrides(&self) -> &NativeOverrideEnv {
        NativeOverrideEnv::default_js()
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

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve(
    _host: &dyn Host,
    _project: Option<&Project>,
    _target_override: Option<&TargetOverride>,
    _require_resolved: bool,
    _sources: &SourceMap,
    _diagnostics: &mut Vec<Diagnostic>,
) -> EnvironmentContext {
    EnvironmentContext {
        foreign_backend: ForeignBackendSpec::js(),
        validate_foreign_metadata: true,
    }
}

/// Resolves the project's active environment (when `require_resolved` is true)
/// and validates host declarations, `[host] requires`, and `[package] when` tags.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn resolve(
    host: &dyn Host,
    project: Option<&Project>,
    #[cfg(feature = "native-project")] packages: &Packages,
    #[cfg(feature = "native-project")] target_dependencies: &[target::Wanted],
    target_override: Option<&TargetOverride>,
    require_resolved: bool,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> EnvironmentContext {
    let mut backends = official_backends();
    if let Some(project) = project {
        for extra in host.backend_tags(project) {
            if !backends.iter().any(|b| b.id == extra.id) {
                backends.push(extra);
            }
        }
    }
    if let Some(extra) = target_override.and_then(|o| o.backend.clone()) {
        upsert_backend(&mut backends, extra);
    }

    // Frontend-only commands (`flick inspect --emit iface`) intentionally avoid
    // instantiating `[build.backend]` components. When a pinned third-party
    // backend has no cached descriptor yet, its tags, axes and built-in hosts
    // are unknown, so skip vocabulary validation and environment resolution
    // rather than fabricating an empty-axes fallback that emits false errors.
    #[cfg(feature = "native-project")]
    let uncached_custom_backend = !host.loads_backends()
        && project
            .and_then(|p| p.config.build.backend.as_ref())
            .is_some_and(|pkg| !backends.iter().any(|b| b.id == pkg.id));
    #[cfg(not(feature = "native-project"))]
    let uncached_custom_backend = false;
    if uncached_custom_backend {
        let native_overrides = backends::build_native_override_env(
            &BTreeSet::new(),
            false,
            &backends,
            &[],
            None,
            &ForeignBackendSpec::js(),
            target_override,
        );
        return EnvironmentContext {
            vocabulary: BTreeSet::new(),
            validate_tags: false,
            resolved: None,
            foreign_backend: ForeignBackendSpec::js(),
            validate_foreign_metadata: false,
            backends,
            native_overrides,
        };
    }

    // Collect every declared `[host]` (from dependency packages and, if this project
    // itself is a host library, from the project manifest).
    let mut hosts: Vec<(Option<String>, &HostConfig)> = Vec::new();
    if let Some(own_host) = project.and_then(|p| p.config.host.as_ref()) {
        hosts.push((None, own_host));
    }
    #[cfg(feature = "native-project")]
    for (pkg_name, host_cfg) in packages.hosts() {
        hosts.push((Some(pkg_name.to_owned()), host_cfg));
    }

    // Detect duplicate `(backend, host name)` declarations across packages.
    let mut seen_hosts: BTreeMap<(String, String), String> = BTreeMap::new();
    for (pkg_opt, host_cfg) in &hosts {
        let canonical_backend = backends
            .iter()
            .find(|b| b.matches_name(&host_cfg.backend))
            .map_or_else(|| host_cfg.backend.clone(), |b| b.id.clone());
        let owner = pkg_opt.clone().unwrap_or_else(|| {
            project
                .and_then(|p| p.config.package.as_ref().map(|pk| pk.name.clone()))
                .unwrap_or_else(|| "this project".to_owned())
        });
        let key = (canonical_backend, host_cfg.name.clone());
        if let Some(first) = seen_hosts.get(&key) {
            diagnostics.push(Diagnostic::error(
                codes::P065,
                format!(
                    "duplicate host `{}` for backend `{}`: already declared by `{first}`",
                    host_cfg.name, host_cfg.backend
                ),
                host_cfg.span,
            ));
        } else {
            seen_hosts.insert(key, owner);
        }
    }

    // Build combined tag vocabulary (backends + axes + built-in hosts + loaded host packages).
    let mut vocabulary = BTreeSet::new();
    let mut all_backend_tags = BTreeSet::new();
    for backend in &backends {
        vocabulary.extend(backend.full_vocabulary());
        all_backend_tags.extend(backend.backend_vocabulary());
    }
    for (_, host_cfg) in &hosts {
        vocabulary.insert(host_cfg.name.clone());
        for (tag, _) in &host_cfg.tags {
            vocabulary.insert(tag.clone());
        }
    }

    // Validate `[host] requires` against backend tags (not host tags).
    for (_, host_cfg) in &hosts {
        if let Some((cond, span)) = &host_cfg.requires {
            let backend_vocab = backends
                .iter()
                .find(|b| b.matches_name(&host_cfg.backend))
                .map_or_else(|| all_backend_tags.clone(), BackendTags::backend_vocabulary);
            let text = &sources.get(span.file).text;
            validate_condition_tags(cond, Some(*span), Some(text), &backend_vocab, diagnostics);
        }
    }

    // Validate `[package] when` on the project and all dependency packages.
    if let Some((cond, span)) = project
        .and_then(|p| p.config.package.as_ref())
        .and_then(|p| p.when.as_ref())
    {
        let text = &sources.get(span.file).text;
        validate_condition_tags(cond, Some(*span), Some(text), &vocabulary, diagnostics);
    }
    #[cfg(feature = "native-project")]
    for (_, cond, span) in packages.package_when() {
        let text = &sources.get(span.file).text;
        validate_condition_tags(cond, Some(span), Some(text), &vocabulary, diagnostics);
    }

    let (foreign_backend, validate_foreign_metadata) =
        select_foreign_backend(project, target_override, &backends);

    if !require_resolved {
        let native_overrides = backends::build_native_override_env(
            &vocabulary,
            true,
            &backends,
            &hosts,
            None,
            &foreign_backend,
            target_override,
        );
        #[cfg(feature = "native-project")]
        if validate_foreign_metadata {
            abi::check_package_abis(
                project,
                packages,
                native_overrides.active_tags.as_ref(),
                &foreign_backend,
                diagnostics,
            );
        }
        return EnvironmentContext {
            vocabulary,
            validate_tags: true,
            resolved: None,
            foreign_backend,
            validate_foreign_metadata,
            backends,
            native_overrides,
        };
    }

    let resolved = resolve_selected(
        host,
        project,
        #[cfg(feature = "native-project")]
        target_dependencies,
        target_override,
        &backends,
        &hosts,
        diagnostics,
    );
    let native_overrides = backends::build_native_override_env(
        &vocabulary,
        true,
        &backends,
        &hosts,
        resolved.as_ref(),
        &foreign_backend,
        target_override,
    );
    #[cfg(feature = "native-project")]
    if validate_foreign_metadata && resolved.is_some() {
        abi::check_package_abis(
            project,
            packages,
            native_overrides.active_tags.as_ref(),
            &foreign_backend,
            diagnostics,
        );
    }

    EnvironmentContext {
        vocabulary,
        validate_tags: true,
        resolved,
        foreign_backend,
        validate_foreign_metadata,
        backends,
        native_overrides,
    }
}
