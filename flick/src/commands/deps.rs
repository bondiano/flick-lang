//! `install`, `add`, `remove`, `update` and `deps`. The core does its part
//! on the Flick libraries of `[dependencies]` and `flick.lock` first:
//! `install` and `update` fetch what the lock pins, `remove` removes the
//! libraries it names, `deps` lists them. What is left goes to the plan
//! command of the same name of the project's backend, which owns the
//! packages of its ecosystem, with what the closure's
//! `[target.<name>.dependencies]` ask of it. `add` with a source option
//! adds a Flick library instead, in `libraries`.

#[cfg(feature = "wasm-backends")]
use std::io::{self, Write};

use flick_backend::command::Argument;
#[cfg(feature = "wasm-backends")]
use flick_backend::command::Dependency;
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::project::Project;
#[cfg(feature = "wasm-backends")]
use flick_core::project::{self, target};
use flick_core::source::SourceMap;
use tracing::info;

use super::libraries;
use super::run::find_project;
use crate::args::GlobalArguments;
#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::backends::options;
use crate::command::Outcome;
#[cfg(feature = "wasm-backends")]
use crate::commands::contributed;
#[cfg(feature = "wasm-backends")]
use crate::sources::FsHost;
use crate::{fetch, stdlib};

/// A dependency verb of flick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verb {
    Install { locked: bool, offline: bool },
    Add { packages: Vec<String>, dev: bool },
    Remove { packages: Vec<String> },
    Update { packages: Vec<String> },
    Deps { why: Option<String> },
}

fn failure(code: Code, message: String) -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
    }
}

fn done() -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: Vec::new(),
    }
}

/// Fetches the Flick dependencies for `install` and `update`; the failure.
fn fetch(project: &Project, verb: &Verb) -> Option<Outcome> {
    let (update, options) = match verb {
        Verb::Install { locked, offline } => (
            Vec::new(),
            fetch::Options {
                locked: *locked,
                offline: *offline,
            },
        ),
        Verb::Update { packages } => (packages.clone(), fetch::Options::default()),
        _ => return None,
    };
    let dependencies = stdlib::with_implicit(&project.config.dependencies);
    match fetch::install(&project.root, &dependencies, &update, options) {
        Ok(fetched) => {
            for name in fetched {
                info!("fetched {name}");
            }
        }
        Err(message) => return Some(failure(codes::P034, message)),
    }
    // `update` with no names, or naming the backend, moves its pin too
    let moved = matches!(verb, Verb::Update { packages }
        if packages.is_empty() || packages.iter().any(|p| p.starts_with("flick/")));
    #[cfg(feature = "wasm-backends")]
    if let Err(diagnostic) = backends::pin_official(project, moved, options.locked, options.offline)
    {
        return Some(Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![diagnostic],
        });
    }
    #[cfg(not(feature = "wasm-backends"))]
    let _ = moved;
    None
}

/// The verb's name and arguments as the backend's command takes them, or
/// none when the core did all of it.
fn backend_part(
    project: &Project,
    verb: &Verb,
) -> Result<Option<(&'static str, Vec<Argument>)>, Outcome> {
    let flag = |name: &str, on: bool| {
        on.then(|| Argument {
            name: name.into(),
            values: vec!["true".into()],
        })
    };
    let list = |name: &str, values: Vec<String>| {
        (!values.is_empty()).then(|| Argument {
            name: name.into(),
            values,
        })
    };
    let (name, arguments) = match verb {
        Verb::Install { locked, offline } => (
            "install",
            [flag("locked", *locked), flag("offline", *offline)],
        ),
        Verb::Add { packages, dev } => (
            "add",
            [list("packages", packages.clone()), flag("dev", *dev)],
        ),
        Verb::Remove { packages } => match libraries::remove(project, packages)? {
            rest if rest.is_empty() => return Ok(None),
            rest => ("remove", [list("packages", rest), None]),
        },
        Verb::Update { packages } => {
            // the libraries of `[dependencies]` and official backends were updated with the lock
            let rest: Vec<String> = (packages.iter())
                .filter(|p| {
                    !p.starts_with("flick/")
                        && !project.config.dependencies.iter().any(|d| &d.name == *p)
                })
                .cloned()
                .collect();
            if !packages.is_empty() && rest.is_empty() {
                return Ok(None);
            }
            ("update", [list("packages", rest), None])
        }
        Verb::Deps { why } => {
            if why.is_none() {
                libraries::print_locked(project);
            }
            (
                "deps",
                [
                    why.clone().map(|w| Argument {
                        name: "why".into(),
                        values: vec![w],
                    }),
                    None,
                ],
            )
        }
    };
    Ok(Some((name, arguments.into_iter().flatten().collect())))
}

/// What the closure asks of the backend's ecosystem, checked by it first;
/// the failure.
#[cfg(feature = "wasm-backends")]
fn asked(project: &Project, global: &GlobalArguments) -> Result<Vec<Dependency>, Outcome> {
    if let Err(message) = stdlib::prepare(global.project.as_deref()) {
        return Err(failure(codes::P034, message));
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let discovered =
        match project::discover(&host, &mut sources, &project.root, Some(&project.root)) {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(Vec::new()),
            Err(diagnostic) => {
                return Err(Outcome {
                    sources,
                    diagnostics: vec![diagnostic],
                });
            }
        };
    let mut diagnostics = Vec::new();
    let wanted = target::wanted(&host, &mut sources, &discovered, &mut diagnostics);
    diagnostics.extend(target::conflicts(&wanted));
    diagnostics.extend(backends::verify(&discovered, &wanted, false));
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return Err(Outcome {
            sources,
            diagnostics,
        });
    }
    Ok(backends::asked(&discovered, &wanted))
}

pub fn deps(verb: &Verb, global: &GlobalArguments) -> Outcome {
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    if let Some(failed) = fetch(&project, verb) {
        return failed;
    }
    match backend_part(&project, verb) {
        Ok(Some((name, arguments))) => delegated(&project, verb, name, arguments, global),
        Ok(None) => done(),
        Err(failed) => failed,
    }
}

/// Runs the backend's command `name` for `verb`.
#[cfg(feature = "wasm-backends")]
fn delegated(
    project: &Project,
    verb: &Verb,
    name: &str,
    arguments: Vec<Argument>,
    global: &GlobalArguments,
) -> Outcome {
    let dependencies = if matches!(verb, Verb::Install { .. } | Verb::Update { .. }) {
        match asked(project, global) {
            Ok(asked) => asked,
            Err(failed) => return failed,
        }
    } else {
        Vec::new()
    };
    let has_target_deps = !dependencies.is_empty();
    let Some(mut ran) = contributed::delegate(project, name, arguments, dependencies, global)
    else {
        return unsupported(project, verb, has_target_deps);
    };
    let _ = io::stdout().write_all(&ran.stdout);
    let writes = !matches!(verb, Verb::Deps { .. });
    if writes
        && ran.executed
        && !ran
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        && let Some(problem) = backends::check_runtime_project(project, false)
    {
        ran.diagnostics.push(problem);
    }
    Outcome {
        sources: ran.sources,
        diagnostics: ran.diagnostics,
    }
}

/// A build without backends has none to run the command.
#[cfg(not(feature = "wasm-backends"))]
fn delegated(
    project: &Project,
    verb: &Verb,
    _: &str,
    _: Vec<Argument>,
    _: &GlobalArguments,
) -> Outcome {
    unsupported(project, verb, false)
}

/// A backend without the command: packages it would add or remove have
/// nowhere to go, and there is nothing else to do.
fn unsupported(project: &Project, verb: &Verb, has_target_deps: bool) -> Outcome {
    let needed = match verb {
        Verb::Add { packages, .. } | Verb::Remove { packages } | Verb::Update { packages } => {
            !packages.is_empty() || has_target_deps
        }
        Verb::Deps { why } => why.is_some(),
        Verb::Install { .. } => has_target_deps,
    };
    if !needed {
        return done();
    }
    let backend = options::resolve_target(None, Some(project)).map_or_else(
        |_| project.config.default_target().to_owned(),
        |(choice, _)| choice.id().to_owned(),
    );
    let diagnostic = Diagnostic::new(
        codes::P063,
        Severity::Error,
        format!("the backend `{backend}` manages no packages of its own"),
    )
    .with_fix(|| {
        (
            "name a Flick library with `--path`, `--git` or `--url`".to_owned(),
            Vec::new(),
        )
    });
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![diagnostic],
    }
}
