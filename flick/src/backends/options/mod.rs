//! Backend-declared options, target selection, CLI flag parsing, and
//! `[target.<name>]` merging.

#[cfg(feature = "wasm-backends")]
mod cache;
mod cli;
mod errors;
mod resolve;
mod scan;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[cfg(feature = "wasm-backends")]
use flick_backend::OptionValue;
use flick_backend::{Descriptor, OptionDescriptor, Verb};
use flick_core::diagnostics::{Diagnostic, Severity, codes, suggest};
use flick_core::project::Project;
use flick_core::project::config::BackendPackage;
#[cfg(feature = "wasm-backends")]
use flick_core::project::config::TargetValue;

#[cfg(feature = "wasm-backends")]
pub(crate) use cache::{
    cached_descriptor_for_project, descriptor_for_project, descriptor_for_target, save_descriptor,
};
pub(crate) use cli::{ParseOutcome, parse_cli};
pub(crate) use resolve::resolve;

pub(crate) const OFFICIAL_JS: &str = "flick/js";
pub(crate) const OFFICIAL_GO: &str = "flick/go";

/// The short name of a backend id (`flick/js` -> `js`, `test/literal` -> `literal`).
pub(crate) fn short_name(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// Which backend a command invocation selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetChoice {
    Official {
        id: &'static str,
        short: &'static str,
    },
    Pinned {
        package: BackendPackage,
        short: String,
    },
}

impl TargetChoice {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Official { id, .. } => id,
            Self::Pinned { package, .. } => &package.id,
        }
    }

    pub(crate) fn short(&self) -> &str {
        match self {
            Self::Official { short, .. } => short,
            Self::Pinned { short, .. } => short,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_official_js(&self) -> bool {
        self.id() == OFFICIAL_JS
    }
}

/// Resolve the selected target backend from CLI `--target` and `project`.
/// Returns `(choice, target_overridden)` where `target_overridden` is true when
/// `--target` selects a different backend than the project's default.
pub(crate) fn resolve_target(
    cli_target: Option<&str>,
    project: Option<&Project>,
) -> Result<(TargetChoice, bool), Diagnostic> {
    let project_choice = project_default_target(project)?;
    let Some(requested) = cli_target else {
        return Ok((project_choice, false));
    };
    let choice = resolve_named_target(requested, project)?;
    let overridden = choice.id() != project_choice.id();
    Ok((choice, overridden))
}

fn project_default_target(project: Option<&Project>) -> Result<TargetChoice, Diagnostic> {
    let Some(project) = project else {
        return Ok(TargetChoice::Official {
            id: OFFICIAL_JS,
            short: "js",
        });
    };
    if let Some(package) = &project.config.build.backend {
        return Ok(TargetChoice::Pinned {
            short: short_name(&package.id).to_owned(),
            package: package.clone(),
        });
    }
    if let Some(target) = &project.config.build.target {
        return resolve_named_target(target, Some(project));
    }
    if let Some(env) = &project.config.build.environment
        && let Ok(choice) = resolve_named_target(&env.backend, Some(project))
    {
        return Ok(choice);
    }
    Ok(TargetChoice::Official {
        id: OFFICIAL_JS,
        short: "js",
    })
}

fn resolve_named_target(name: &str, project: Option<&Project>) -> Result<TargetChoice, Diagnostic> {
    if let Some(package) = project.and_then(|p| p.config.build.backend.as_ref()) {
        let short = short_name(&package.id);
        if name == short || name == package.id {
            return Ok(TargetChoice::Pinned {
                package: package.clone(),
                short: short.to_owned(),
            });
        }
    }
    match name {
        "js" | OFFICIAL_JS => Ok(TargetChoice::Official {
            id: OFFICIAL_JS,
            short: "js",
        }),
        "go" | OFFICIAL_GO => Ok(TargetChoice::Official {
            id: OFFICIAL_GO,
            short: "go",
        }),
        other => Err(unknown_target_diagnostic(other, project)),
    }
}

fn unknown_target_diagnostic(name: &str, project: Option<&Project>) -> Diagnostic {
    let mut known = vec!["js".to_owned(), "go".to_owned()];
    if let Some(package) = project.and_then(|p| p.config.build.backend.as_ref()) {
        let short = short_name(&package.id).to_owned();
        if !known.contains(&short) {
            known.push(short);
        }
    }
    let listed = known
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut diag = Diagnostic::new(
        codes::P059,
        Severity::Error,
        format!("unknown target backend `{name}`; available targets: {listed}"),
    );
    if let Some(suggestion) = suggest::closest(name, known.iter().map(String::as_str)) {
        diag = diag.with_fix(|| (format!("did you mean `--target {suggestion}`?"), Vec::new()));
    }
    diag
}

/// Active option values resolved for one backend invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResolvedOptions {
    pub values: BTreeMap<String, Vec<String>>,
}

impl ResolvedOptions {
    pub(crate) fn flag(&self, name: &str) -> bool {
        self.values
            .get(name)
            .and_then(|v| v.last())
            .is_some_and(|v| v == "true")
    }

    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    #[cfg(feature = "wasm-backends")]
    pub(crate) fn to_wire(&self) -> Vec<OptionValue> {
        let mut out = Vec::new();
        for (name, values) in &self.values {
            let key = format!("option.{name}");
            for value in values {
                out.push(OptionValue {
                    key: key.clone(),
                    value: value.clone(),
                });
            }
        }
        out
    }

    pub(crate) fn cache_key(&self) -> String {
        let mut out = String::new();
        for (name, values) in &self.values {
            let _ = write!(out, "{name}={values:?};");
        }
        out
    }

    #[cfg(test)]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "test helper for the five boolean JS backend options"
    )]
    pub(crate) fn js(
        typescript: bool,
        declarations: bool,
        promise_facade: bool,
        schema_aot: bool,
        rt: bool,
    ) -> Self {
        let b = |v: bool| vec![if v { "true" } else { "false" }.to_owned()];
        let mut values = BTreeMap::new();
        values.insert("typescript".into(), b(typescript));
        values.insert("declarations".into(), b(declarations));
        values.insert("promise-facade".into(), b(promise_facade));
        values.insert("schema-aot".into(), b(schema_aot));
        values.insert(
            "runtime".into(),
            vec![if rt { "rt" } else { "effect" }.into()],
        );
        Self { values }
    }
}

pub(super) fn active_for_verb(option: &OptionDescriptor, verb: Verb) -> bool {
    option.verbs.is_empty() || option.verbs.contains(&verb)
}

/// Whether the selected backend descriptor and resolved options use
/// class-derived built-in stages.
pub(crate) fn stages_by_class(descriptor: Option<&Descriptor>, resolved: &ResolvedOptions) -> bool {
    let Some(required) = descriptor.and_then(|d| d.stages_by_class_option.as_ref()) else {
        return false;
    };
    resolved.get(&required.key) == Some(required.value.as_str())
}

/// Whether `project`'s configured target options select class-derived built-in
/// stages for `descriptor`.
#[cfg(feature = "wasm-backends")]
pub(crate) fn project_stages_by_class(descriptor: &Descriptor, project: &Project) -> bool {
    let Some(required) = descriptor.stages_by_class_option.as_ref() else {
        return false;
    };
    let short = short_name(&descriptor.identity.id);
    if let Some(table) = project.config.targets.get(short)
        && let Some(option) = table.get(&required.key)
    {
        return match &option.value {
            TargetValue::String(s) => s == &required.value,
            TargetValue::Bool(b) => (if *b { "true" } else { "false" }) == required.value,
            TargetValue::List(_) | TargetValue::Unsupported(_) => false,
        };
    }
    descriptor
        .options
        .iter()
        .find(|o| o.parameter.name == required.key)
        .and_then(|o| o.default.as_deref())
        == Some(required.value.as_str())
}

/// Stage-mode fallback when no backend component is loaded: reads the disk
/// descriptor cache first (without instantiating WASM), then falls back to
/// known official target defaults from `project.config`.
#[cfg(feature = "wasm-backends")]
pub(crate) fn project_stages_by_class_cached(project: &Project) -> bool {
    if let Some(descriptor) = cached_descriptor_for_project(project) {
        return project_stages_by_class(&descriptor, project);
    }
    let Ok((choice, _)) = resolve_target(None, Some(project)) else {
        return true;
    };
    match choice {
        TargetChoice::Official {
            id: OFFICIAL_JS, ..
        } => !matches!(
            project
                .config
                .targets
                .get("js")
                .and_then(|t| t.get("runtime"))
                .map(|o| &o.value),
            Some(TargetValue::String(v)) if v == "effect"
        ),
        TargetChoice::Official {
            id: OFFICIAL_GO, ..
        } => false,
        TargetChoice::Official { .. } | TargetChoice::Pinned { .. } => true,
    }
}

/// Configured `[target.<name>]` values of `project`'s selected target encoded
/// as `option.<key>` wire entries for callers without resolved CLI flags.
#[cfg(feature = "wasm-backends")]
pub(crate) fn project_wire_options(
    project: &Project,
    target_override: Option<&str>,
) -> Vec<OptionValue> {
    let Ok((choice, _)) = resolve_target(target_override, Some(project)) else {
        return Vec::new();
    };
    let Some(table) = project.config.targets.get(choice.short()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, opt) in table {
        match &opt.value {
            TargetValue::String(s) => out.push(OptionValue {
                key: format!("option.{key}"),
                value: s.clone(),
            }),
            TargetValue::Bool(b) => out.push(OptionValue {
                key: format!("option.{key}"),
                value: if *b { "true" } else { "false" }.to_owned(),
            }),
            TargetValue::List(items) => {
                for (s, _) in items {
                    out.push(OptionValue {
                        key: format!("option.{key}"),
                        value: s.clone(),
                    });
                }
            }
            TargetValue::Unsupported(_) => {}
        }
    }
    out
}

pub(super) fn verb_name(verb: Verb) -> &'static str {
    match verb {
        Verb::Build => "build",
        Verb::Run => "run",
        Verb::Dev => "dev",
        Verb::Test => "test",
        Verb::Inspect => "inspect",
    }
}

pub(super) fn format_verbs(verbs: &[Verb]) -> String {
    let names: Vec<String> = verbs
        .iter()
        .map(|v| format!("`flick {}`", verb_name(*v)))
        .collect();
    match names.as_slice() {
        [] => "all commands".into(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        many => {
            let last = many.last().expect("non-empty");
            format!("{}, and {last}", many[..many.len() - 1].join(", "))
        }
    }
}
