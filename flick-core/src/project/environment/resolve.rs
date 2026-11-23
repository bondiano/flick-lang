//! Project environment resolution, host validation, and backend foreign
//! metadata selection.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(not(target_arch = "wasm32"))]
use super::select::{resolve_selected, select_backend};
#[cfg(not(target_arch = "wasm32"))]
use super::{BackendTags, official_backends, validate_condition_tags};
use super::{EnvironmentContext, TargetOverride};
use crate::diagnostics::Diagnostic;
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::codes;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::config::HostConfig;
#[cfg(feature = "native-project")]
use crate::project::packages::Packages;
#[cfg(feature = "native-project")]
use crate::project::target;
use crate::project::{Host, Project};
use crate::source::SourceMap;
use crate::syntax::foreign_meta::ForeignMeta;

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub(in crate::project) fn resolve(
    _host: &dyn Host,
    _project: Option<&Project>,
    _target_override: Option<&TargetOverride>,
    _require_resolved: bool,
    _sources: &SourceMap,
    _diagnostics: &mut Vec<Diagnostic>,
) -> EnvironmentContext {
    EnvironmentContext {
        foreign_meta: ForeignMeta::js(),
        validate_foreign_meta: true,
    }
}

/// Resolves the project's active environment (when `require_resolved` is true)
/// and validates host declarations, `[host] requires`, and `[package] when` tags.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::project) fn resolve(
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
        return EnvironmentContext {
            vocabulary: BTreeSet::new(),
            validate_tags: false,
            resolved: None,
            foreign_meta: ForeignMeta::default(),
            validate_foreign_meta: false,
            backends,
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

    if !require_resolved {
        let mut ignored = Vec::new();
        let selected = select_backend(project, target_override, &backends, &mut ignored);
        let foreign_meta = selected
            .as_ref()
            .map_or_else(ForeignMeta::js, |b| b.foreign_meta.clone());
        return EnvironmentContext {
            vocabulary,
            validate_tags: true,
            resolved: None,
            foreign_meta,
            validate_foreign_meta: true,
            backends,
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
    let (foreign_meta, validate_foreign_meta) = if let Some(env) = &resolved {
        backends
            .iter()
            .find(|b| b.id == env.backend_id)
            .map_or_else(
                || (ForeignMeta::default(), false),
                |b| (b.foreign_meta.clone(), true),
            )
    } else {
        let mut ignored = Vec::new();
        match select_backend(project, target_override, &backends, &mut ignored) {
            Some(selected) if backends.iter().any(|b| b.id == selected.id) => {
                (selected.foreign_meta, true)
            }
            _ => (ForeignMeta::default(), false),
        }
    };

    EnvironmentContext {
        vocabulary,
        validate_tags: true,
        resolved,
        foreign_meta,
        validate_foreign_meta,
        backends,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn upsert_backend(backends: &mut Vec<BackendTags>, extra: BackendTags) {
    if let Some(existing) = backends.iter_mut().find(|b| b.id == extra.id) {
        *existing = extra;
    } else {
        backends.push(extra);
    }
}
