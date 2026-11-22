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
use flick_core::project::config::BackendPackage;
use flick_core::project::{BackendAxis, BackendTags, Project};

#[cfg(feature = "wasm-backends")]
pub(crate) use cache::{project_backend_tags, save_descriptor};
pub(crate) use cli::{ParseOutcome, parse_cli};
pub(crate) use resolve::resolve;

#[cfg(not(feature = "wasm-backends"))]
pub(crate) fn project_backend_tags(_project: &Project, _allow_load: bool) -> Vec<BackendTags> {
    Vec::new()
}

/// Converts a backend component descriptor into the core environment metadata.
pub(crate) fn descriptor_tags(descriptor: &Descriptor) -> BackendTags {
    BackendTags {
        id: descriptor.identity.id.clone(),
        short: short_name(&descriptor.identity.id).to_owned(),
        tags: descriptor.tags.clone(),
        axes: descriptor
            .axes
            .iter()
            .map(|a| BackendAxis {
                name: a.name.clone(),
                tags: a.tags.clone(),
                default: a.default.clone(),
                exclusive: a.exclusive,
            })
            .collect(),
        environments: descriptor
            .environments
            .iter()
            .map(|e| (e.backend.clone(), e.platform.clone()))
            .collect(),
    }
}

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

    pub(crate) fn is_go(&self) -> bool {
        self.id() == OFFICIAL_GO
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
    if let Some(env) = &project.config.build.environment {
        return resolve_named_target(&env.backend, Some(project));
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

    pub(crate) fn is_rt(&self) -> bool {
        self.get("runtime") == Some("rt")
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
