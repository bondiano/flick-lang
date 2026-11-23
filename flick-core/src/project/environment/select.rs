//! Backend selection, explicit `[build] environment` resolution, and active
//! backend tag evaluation.

use std::collections::{BTreeMap, BTreeSet};

use super::{BackendTags, ResolvedEnvironment, TargetOverride};
use crate::diagnostics::{Diagnostic, Severity, codes, suggest};
#[cfg(feature = "native-project")]
use crate::effects::bindings;
use crate::project::config::{BackendEnvironment, HostConfig};
#[cfg(feature = "native-project")]
use crate::project::config::{TargetOption, TargetValue};
#[cfg(feature = "native-project")]
use crate::project::target;
use crate::project::{Host, Project};
use crate::source::Span;
#[cfg(feature = "native-project")]
use crate::syntax::foreign_meta::ForeignMeta;
use crate::syntax::when;

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_selected(
    host: &dyn Host,
    project: Option<&Project>,
    #[cfg(feature = "native-project")] target_dependencies: &[target::Wanted],
    target_override: Option<&TargetOverride>,
    backends: &[BackendTags],
    hosts: &[(Option<String>, &HostConfig)],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ResolvedEnvironment> {
    let selected_backend = select_backend(project, target_override, backends, diagnostics)?;
    let backend_tags = active_backend_tags(
        host,
        project,
        #[cfg(feature = "native-project")]
        target_dependencies,
        target_override,
        &selected_backend,
    );

    #[cfg_attr(not(feature = "native-project"), allow(unused_mut))]
    let mut ui = target_override
        .and_then(|o| o.ui.clone())
        .or_else(|| target_override.and_then(|o| o.options.get("ui")?.last().cloned()));
    #[cfg(feature = "native-project")]
    if ui.is_none() {
        ui = project.and_then(|p| match &p.config.targets.get("js")?.get("ui")?.value {
            TargetValue::String(s) => Some(s.clone()),
            _ => None,
        });
    }

    let target_overridden = target_override.is_some_and(|o| o.target_overridden);
    let explicit_env = project
        .and_then(|p| {
            let env = p.config.build.environment.as_ref()?;
            Some((env, p.config.build.environment_span))
        })
        .filter(|(env, _)| {
            !target_overridden
                || selected_backend.matches_name(&env.backend)
                || selected_backend
                    .environments
                    .iter()
                    .any(|(b, _)| b == &env.backend)
        });

    if let Some((env, env_span)) = explicit_env {
        return resolve_explicit_environment(
            project,
            &selected_backend,
            backend_tags,
            hosts,
            env,
            env_span,
            ui,
            diagnostics,
        );
    }

    // No explicit `[build] environment`: use the backend's default environment.
    let (backend, platform) = selected_backend
        .environments
        .first()
        .cloned()
        .unwrap_or_else(|| (selected_backend.short.clone(), "default".to_owned()));
    let mut tags = backend_tags;
    if !selected_backend.environments.is_empty() {
        tags.insert(platform.clone());
    }
    Some(ResolvedEnvironment {
        backend_id: selected_backend.id.clone(),
        backend,
        platform,
        host_package: None,
        tags,
        explicit_environment: false,
        ui,
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_explicit_environment(
    project: Option<&Project>,
    selected_backend: &BackendTags,
    backend_tags: BTreeSet<String>,
    hosts: &[(Option<String>, &HostConfig)],
    env: &BackendEnvironment,
    env_span: Option<Span>,
    ui: Option<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ResolvedEnvironment> {
    // 1. Check loaded `[host]` packages first.
    if let Some((pkg_opt, host_cfg)) = hosts.iter().find(|(_, h)| {
        h.name == env.platform
            && selected_backend.matches_name(&h.backend)
            && (selected_backend.matches_name(&env.backend) || h.backend == env.backend)
    }) {
        if let Some((req, _)) = &host_cfg.requires
            && let Ok(expr) = when::parse(req)
            && !expr.eval(&|t| backend_tags.contains(t))
        {
            let active = backend_tags
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!(
                "host `{}` requires backend condition `{req}`, which is not satisfied by backend `{}` (active backend tags: {active})",
                host_cfg.name, selected_backend.short
            );
            diagnostics.push(match env_span {
                Some(span) => Diagnostic::error(codes::P065, message, span),
                None => Diagnostic::new(codes::P065, Severity::Error, message),
            });
            return None;
        }
        let mut tags = backend_tags;
        tags.insert(host_cfg.name.clone());
        for (tag, _) in &host_cfg.tags {
            tags.insert(tag.clone());
        }
        let host_package = Some(
            pkg_opt
                .clone()
                .or_else(|| {
                    project
                        .and_then(|p| p.config.package.as_ref())
                        .map(|pk| pk.name.clone())
                })
                .unwrap_or_else(|| host_cfg.name.clone()),
        );
        return Some(ResolvedEnvironment {
            backend_id: selected_backend.id.clone(),
            backend: env.backend.clone(),
            platform: env.platform.clone(),
            host_package,
            tags,
            explicit_environment: true,
            ui,
        });
    }

    // 2. Check built-in environments declared by the selected backend.
    if selected_backend
        .environments
        .iter()
        .any(|(b, p)| b == &env.backend && p == &env.platform)
    {
        let mut tags = backend_tags;
        tags.insert(env.platform.clone());
        return Some(ResolvedEnvironment {
            backend_id: selected_backend.id.clone(),
            backend: env.backend.clone(),
            platform: env.platform.clone(),
            host_package: None,
            tags,
            explicit_environment: true,
            ui,
        });
    }

    // 3. Unknown host or backend mismatch -> FLICK-P065.
    emit_unsupported_environment(selected_backend, hosts, env, env_span, diagnostics);
    None
}

fn emit_unsupported_environment(
    selected_backend: &BackendTags,
    hosts: &[(Option<String>, &HostConfig)],
    env: &BackendEnvironment,
    env_span: Option<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let backend_matches = selected_backend.matches_name(&env.backend)
        || selected_backend
            .environments
            .iter()
            .any(|(b, _)| b == &env.backend);
    let diag = if backend_matches {
        let mut available: Vec<String> = selected_backend
            .environments
            .iter()
            .filter(|(b, _)| b == &env.backend)
            .map(|(_, p)| p.clone())
            .collect();
        for (_, h) in hosts {
            if selected_backend.matches_name(&h.backend) && !available.contains(&h.name) {
                available.push(h.name.clone());
            }
        }
        let listed = if available.is_empty() {
            "none".to_owned()
        } else {
            available
                .iter()
                .map(|h| format!("`{h}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let message = format!(
            "unsupported environment `{}/{}`: unknown host `{}` for backend `{}`; available hosts: {listed}",
            env.backend, env.platform, env.platform, selected_backend.id
        );
        let mut d = match env_span {
            Some(span) => Diagnostic::error(codes::P065, message, span),
            None => Diagnostic::new(codes::P065, Severity::Error, message),
        };
        if let Some(closest) = suggest::closest(&env.platform, available.iter().map(String::as_str))
        {
            d = d.with_fix(|| {
                (
                    format!("did you mean `{}/{closest}`?", env.backend),
                    Vec::new(),
                )
            });
        }
        d
    } else {
        let message = format!(
            "environment `{}/{}` names backend `{}`, which does not match selected backend `{}`",
            env.backend, env.platform, env.backend, selected_backend.short
        );
        let mut d = match env_span {
            Some(span) => Diagnostic::error(codes::P065, message, span),
            None => Diagnostic::new(codes::P065, Severity::Error, message),
        };
        if env_span.is_some() {
            d = d.with_fix(|| {
                (
                    format!(
                        "use an environment for `{}` or change `[build] target`",
                        selected_backend.short
                    ),
                    Vec::new(),
                )
            });
        }
        d
    };
    diagnostics.push(diag);
}

pub(super) fn select_backend(
    project: Option<&Project>,
    target_override: Option<&TargetOverride>,
    backends: &[BackendTags],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BackendTags> {
    if let Some(name) = target_override.and_then(|o| o.target.as_deref()) {
        if let Some(found) = backends
            .iter()
            .find(|b| b.short == name || b.id == name)
            .cloned()
        {
            return Some(found);
        }
        emit_unknown_backend(name, None, backends, diagnostics);
        return None;
    }
    #[cfg(feature = "native-project")]
    if let Some(pkg) = project.and_then(|p| p.config.build.backend.as_ref()) {
        let short = pkg.id.rsplit('/').next().unwrap_or(&pkg.id);
        if let Some(found) = backends
            .iter()
            .find(|b| b.id == pkg.id || b.short == short)
            .cloned()
        {
            return Some(found);
        }
        // Fallback for unit-test hosts that do not load WASM component descriptors.
        return Some(BackendTags {
            id: pkg.id.clone(),
            short: short.to_owned(),
            tags: vec![short.to_owned()],
            axes: Vec::new(),
            environments: Vec::new(),
            foreign_meta: ForeignMeta {
                backend_id: pkg.id.clone(),
                ..ForeignMeta::default()
            },
        });
    }
    #[cfg(feature = "native-project")]
    if let Some(project) = project
        && let Some(target) = &project.config.build.target
    {
        if let Some(found) = backends
            .iter()
            .find(|b| b.short == *target || b.id == *target)
            .cloned()
        {
            return Some(found);
        }
        emit_unknown_backend(
            target,
            project.config.build.target_span,
            backends,
            diagnostics,
        );
        return None;
    }
    if let Some(project) = project
        && let Some(env) = &project.config.build.environment
    {
        if let Some(found) = backends
            .iter()
            .find(|b| b.short == env.backend || b.id == env.backend)
            .cloned()
        {
            return Some(found);
        }
        emit_unknown_backend(
            &env.backend,
            project.config.build.environment_span,
            backends,
            diagnostics,
        );
        return None;
    }
    backends
        .iter()
        .find(|b| b.short == "js")
        .or_else(|| backends.first())
        .cloned()
}

fn emit_unknown_backend(
    name: &str,
    span: Option<Span>,
    backends: &[BackendTags],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut known: Vec<&str> = backends.iter().map(|b| b.short.as_str()).collect();
    known.sort_unstable();
    known.dedup();
    let listed = known
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let message = format!("unknown target backend `{name}`; available targets: {listed}");
    let mut diag = match span {
        Some(span) => Diagnostic::error(codes::P059, message, span),
        None => Diagnostic::new(codes::P059, Severity::Error, message),
    };
    if let Some(closest) = suggest::closest(name, known.iter().copied()) {
        diag = diag.with_fix(|| (format!("did you mean `{closest}`?"), Vec::new()));
    }
    diagnostics.push(diag);
}

struct BackendOptions<'a> {
    overrides: Option<&'a BTreeMap<String, Vec<String>>>,
    #[cfg(feature = "native-project")]
    table: Option<&'a BTreeMap<String, TargetOption>>,
}

impl BackendOptions<'_> {
    fn str_val(&self, key: &str) -> Option<String> {
        if let Some(val) = self
            .overrides
            .and_then(|m| m.get(key))
            .and_then(|v| v.last())
        {
            return Some(val.clone());
        }
        #[cfg(feature = "native-project")]
        if let Some(opt) = self.table.and_then(|t| t.get(key))
            && let TargetValue::String(s) = &opt.value
        {
            return Some(s.clone());
        }
        None
    }

    fn bool_val(&self, key: &str) -> Option<bool> {
        if let Some(val) = self
            .overrides
            .and_then(|m| m.get(key))
            .and_then(|v| v.last())
        {
            return match val.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
        #[cfg(feature = "native-project")]
        if let Some(opt) = self.table.and_then(|t| t.get(key))
            && let TargetValue::Bool(b) = opt.value
        {
            return Some(b);
        }
        None
    }

    fn list_val(&self, key: &str) -> Vec<String> {
        if let Some(vals) = self.overrides.and_then(|m| m.get(key)) {
            return vals.clone();
        }
        #[cfg(feature = "native-project")]
        if let Some(opt) = self.table.and_then(|t| t.get(key)) {
            return match &opt.value {
                TargetValue::List(items) => items.iter().map(|(s, _)| s.clone()).collect(),
                TargetValue::String(s) => vec![s.clone()],
                _ => Vec::new(),
            };
        }
        Vec::new()
    }
}

fn active_backend_tags(
    host: &dyn Host,
    project: Option<&Project>,
    #[cfg(feature = "native-project")] target_dependencies: &[target::Wanted],
    target_override: Option<&TargetOverride>,
    backend: &BackendTags,
) -> BTreeSet<String> {
    let mut tags: BTreeSet<String> = backend.tags.iter().cloned().collect();
    let opts = BackendOptions {
        overrides: target_override.map(|o| &o.options),
        #[cfg(feature = "native-project")]
        table: project.and_then(|p| p.config.targets.get(&backend.short)),
    };

    for axis in &backend.axes {
        if backend.id == "flick/js" && axis.name == "runtime" {
            let chosen = match target_override.and_then(|o| o.rt) {
                Some(true) => "rt".to_owned(),
                Some(false) => "effect".to_owned(),
                None => opts
                    .str_val("runtime")
                    .filter(|v| axis.tags.contains(v))
                    .or_else(|| axis.default.clone())
                    .unwrap_or_else(|| "rt".to_owned()),
            };
            tags.insert(chosen);
            continue;
        }
        if backend.id == "flick/js" && axis.name == "adapter" {
            let available = target_override
                .and_then(|o| o.effect_adapter)
                .unwrap_or_else(|| {
                    #[cfg(feature = "native-project")]
                    {
                        has_effect_adapter(host, project, target_dependencies)
                    }
                    #[cfg(not(feature = "native-project"))]
                    {
                        let _ = (host, project);
                        false
                    }
                });
            if available {
                tags.insert("effect-adapter".to_owned());
            }
            continue;
        }
        if axis.exclusive {
            let chosen = opts
                .str_val(&axis.name)
                .filter(|v| axis.tags.contains(v))
                .or_else(|| axis.default.clone());
            if let Some(tag) = chosen {
                tags.insert(tag);
            }
        } else {
            let mut matched_any = false;
            for tag in &axis.tags {
                if opts.bool_val(tag) == Some(true)
                    || (axis.tags.len() == 1 && opts.bool_val(&axis.name) == Some(true))
                {
                    tags.insert(tag.clone());
                    matched_any = true;
                }
            }
            for val in opts.list_val(&axis.name) {
                if axis.tags.contains(&val) {
                    tags.insert(val);
                    matched_any = true;
                }
            }
            if !matched_any && let Some(default) = &axis.default {
                tags.insert(default.clone());
            }
        }
    }

    tags
}

/// Whether compiled JS RT code may use the Effect adapter package: via
/// `[target.js.dependencies]` or the backend's declared foreign packages.
#[cfg(feature = "native-project")]
fn has_effect_adapter(
    host: &dyn Host,
    project: Option<&Project>,
    target_dependencies: &[target::Wanted],
) -> bool {
    project.is_none_or(|p| {
        target_dependencies.iter().any(|dependency| {
            dependency.target == "js" && dependency.name == bindings::ADAPTER_PREFIX
        }) || host
            .foreign(p)
            .declared
            .iter()
            .any(|dependency| dependency == bindings::ADAPTER_PREFIX)
    })
}
