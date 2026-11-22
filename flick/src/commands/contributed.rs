//! Commands that backends and library plugins add to `flick`: found in the
//! project, described from a cache in the store, and run by the host from
//! the plan or the session they declare.

mod delegate;
mod inputs;
pub(crate) mod plan;
mod preview;
mod projection;
mod registry;
mod session;
mod validate;

pub(crate) use delegate::delegate;
pub(crate) use preview::preview;
pub(crate) use registry::Contributor;

use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, CommandFactory};
use flick_backend::command::{self, Argument, Descriptor, Invocation, Style, ValueKind};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::plugin::first_party;
use flick_core::project::Project;
use flick_core::source::SourceMap;

#[cfg(test)]
use registry::builtin_std_source;
use registry::{
    NativeContributor, Source, SourceKind, cached_everywhere, descriptors, known, load, read_cache,
    sources,
};

use super::run::{find_project, find_project_at};
use crate::args::{CommandArguments, GlobalArguments};
use crate::backends::Tool;
use crate::{backends, report};

/// The names of the core's commands, which no contributor may take.
fn reserved() -> Vec<String> {
    let mut names: Vec<String> = (CommandArguments::command().get_subcommands())
        .map(|c| c.get_name().to_owned())
        .collect();
    names.push("help".into());
    names
}

fn first_word(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// One contributor's commands, with the warnings of the ones it may not add.
struct Contributed {
    short: String,
    commands: Vec<Descriptor>,
}

fn contributed(project: &Project) -> (Vec<(Source, Contributed)>, Vec<Diagnostic>) {
    let reserved = reserved();
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    for source in sources(project) {
        let all = match descriptors(project, &source) {
            Ok(all) => all,
            Err(why) => {
                warnings.push(Diagnostic::new(
                    codes::P013,
                    Severity::Warning,
                    format!("the commands of `{}` are unavailable: {why}", source.id),
                ));
                continue;
            }
        };
        let (taken, commands): (Vec<_>, Vec<_>) =
            (all.into_iter()).partition(|d| reserved.iter().any(|r| r == first_word(&d.name)));
        let backend = matches!(source.kind, SourceKind::Backend { .. });
        for d in taken {
            if backend && delegate::VERBS.contains(&d.name.as_str()) {
                continue;
            }
            warnings.push(Diagnostic::new(
                codes::P054,
                Severity::Warning,
                format!(
                    "`{}` adds the command `{}`, a command of flick itself: it is ignored",
                    source.id, d.name
                ),
            ));
        }
        let contributed = Contributed {
            short: source.short.clone(),
            commands,
        };
        found.push((source, contributed));
    }
    (found, warnings)
}

/// The help text of contributed commands, by contributor: built-in `std` and
/// those of the project's backend and plugins, or outside a project every
/// cached one. Reads only in-memory `std` and the cache: no component is
/// loaded and nothing is fetched.
pub(crate) fn help() -> String {
    let mut groups: Vec<(String, Vec<Descriptor>)> = Vec::new();
    if let Ok(std_ds) = NativeContributor::std().describe() {
        groups.push((first_party::PACKAGE.into(), std_ds));
    }
    match find_project_at(None) {
        Ok(project) => groups.extend(
            sources(&project)
                .into_iter()
                .filter(|s| !matches!(s.kind, SourceKind::BuiltinStd))
                .filter_map(|s| Some((s.id, read_cache(&s.digest)?.descriptors))),
        ),
        Err(_) => groups.extend(cached_everywhere()),
    }
    let reserved = reserved();
    let mut text = String::new();
    for (id, commands) in groups {
        let commands: Vec<_> = (commands.iter())
            .filter(|d| !reserved.iter().any(|r| r == first_word(&d.name)))
            .collect();
        if commands.is_empty() {
            continue;
        }
        let _ = writeln!(text, "Commands of `{id}`:");
        for d in commands {
            let _ = writeln!(text, "  {:<22} {}", d.name, d.summary);
        }
        text.push('\n');
    }
    text
}

/// Runs the contributed command `words` names, flat or as `short:name`.
pub(crate) fn run(words: &[OsString], global: &GlobalArguments) -> ExitCode {
    let words: Vec<String> = words
        .iter()
        .map(|w| w.to_string_lossy().into_owned())
        .collect();
    let fail = |diagnostic: Diagnostic| {
        report::render(global, &SourceMap::default(), &[diagnostic]);
        ExitCode::from(2)
    };
    // outside a project, the official backends' commands run in the working directory
    let inside = find_project(global).ok();
    let outside = inside.is_none();
    let found = inside.or_else(|| {
        let cwd = env::current_dir().ok()?;
        backends::outside_project(&cwd).ok()
    });
    let Some(mut project) = found else {
        return fail(unknown(&words, &[], &cached_everywhere()));
    };
    let (found, warnings) = contributed(&project);
    report::render(global, &SourceMap::default(), &warnings);
    let target = match resolve(&words, &found, &|| known(&project)) {
        Ok(target) => target,
        Err(diagnostic) => return fail(*diagnostic),
    };
    let (source, descriptor, rest) = target;
    let contributor = match load(&project, source) {
        Ok(contributor) => contributor,
        Err(why) => {
            return fail(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot load `{}`: {why}", source.id),
            ));
        }
    };
    let arguments = match arguments(descriptor, &source.short, &rest) {
        Ok(arguments) => arguments,
        Err(error) => {
            let code = if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
            let _ = error.print();
            return code;
        }
    };
    // outside a project, the root holds the working directory and every path named
    if outside {
        project.root = inputs::common_root(descriptor, &arguments, &project.root);
    }
    let tools = backends::granted_tools(&project, &source.id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let invocation = match invocation(&project, &source.id, descriptor, arguments, &tools, &cwd) {
        Ok(invocation) => invocation,
        Err(why) => {
            return fail(Diagnostic::new(
                codes::P055,
                Severity::Error,
                format!("`{}` cannot run: {why}", descriptor.name),
            ));
        }
    };
    match descriptor.style {
        Style::Plan => plan::run(
            &project,
            &*contributor,
            descriptor,
            &invocation,
            &tools,
            global,
        ),
        Style::Session => session::serve(
            &project,
            &*contributor,
            descriptor,
            &invocation,
            &tools,
            global,
        ),
    }
}

/// The call of `descriptor` with `arguments`: their paths relative to the
/// project's root, and the files the command reads. Tools allowed in `[tools]`
/// for `id` that are missing on `PATH` appear with empty `version` and `sha256`
/// so the contributor can distinguish a missing tool from a disallowed one.
fn invocation(
    project: &Project,
    id: &str,
    descriptor: &Descriptor,
    arguments: Vec<Argument>,
    tools: &[Tool],
    cwd: &Path,
) -> Result<Invocation, String> {
    let root = &project.root;
    let sources =
        (project.source_root().strip_prefix(root)).map_or_else(|_| String::new(), inputs::slashed);
    let mut pinned: Vec<command::Tool> = tools.iter().map(|t| plan::tool(&t.pin)).collect();
    for name in project.config.tools.get(id).into_iter().flatten() {
        if !pinned.iter().any(|t| &t.id == name) {
            pinned.push(command::Tool {
                id: name.clone(),
                version: String::new(),
                sha256: String::new(),
            });
        }
    }
    let mut invocation = Invocation {
        name: descriptor.name.clone(),
        arguments: inputs::relative(descriptor, arguments, root, cwd)?,
        sources,
        inputs: Vec::new(),
        tools: pinned,
        dependencies: Vec::new(),
    };
    let paths = inputs::expand(&descriptor.inputs, &invocation)?;
    invocation.inputs = inputs::read(root, &paths)?;
    Ok(invocation)
}

type Target<'a> = (&'a Source, &'a Descriptor, Vec<String>);

/// The command `words` names and the words left for its arguments.
fn resolve<'a>(
    words: &[String],
    found: &'a [(Source, Contributed)],
    known: &dyn Fn() -> Vec<(String, Vec<Descriptor>)>,
) -> Result<Target<'a>, Box<Diagnostic>> {
    let Some(first) = words.first() else {
        return Err(Box::new(Diagnostic::new(
            codes::P052,
            Severity::Error,
            "no command",
        )));
    };
    let (short, head) = match first.split_once(':') {
        Some((short, head)) => (Some(short), head),
        None => (None, first.as_str()),
    };
    let mut words: Vec<String> = words.to_vec();
    head.clone_into(&mut words[0]);
    let mut matches: Vec<(&Source, &Descriptor, usize)> = Vec::new();
    for (source, contributed) in found {
        if short.is_some_and(|s| s != contributed.short) {
            continue;
        }
        // the longest name whose words begin the command line
        let best = (contributed.commands.iter())
            .filter_map(|d| {
                let names: Vec<&str> = d.name.split_whitespace().collect();
                let fits =
                    names.len() <= words.len() && names.iter().zip(&words).all(|(a, b)| a == b);
                fits.then_some((d, names.len()))
            })
            .max_by_key(|(_, n)| *n);
        if let Some((d, n)) = best {
            matches.push((source, d, n));
        }
    }
    match matches.as_slice() {
        [] => Err(Box::new(unknown(&words, found, &known()))),
        [(source, descriptor, n)] => Ok((source, descriptor, words[*n..].to_vec())),
        many => {
            let names: Vec<String> = (many.iter())
                .map(|(s, d, _)| format!("`flick {}:{}`", s.short, d.name))
                .collect();
            Err(Box::new(
                Diagnostic::new(
                    codes::P053,
                    Severity::Error,
                    format!("several contributors add the command `{}`", many[0].1.name),
                )
                .with_fix(|| (format!("name one: {}", names.join(", ")), Vec::new())),
            ))
        }
    }
}

/// No command of flick or of the project has the name `words` starts with;
/// matching multi-word commands of active contributors or a cached contributor
/// that adds it are named.
fn unknown(
    words: &[String],
    found: &[(Source, Contributed)],
    cached: &[(String, Vec<Descriptor>)],
) -> Diagnostic {
    let name = words.first().map_or("", String::as_str);
    let head = name.rsplit(':').next().unwrap_or(name);
    let active_prefix: Vec<String> = found
        .iter()
        .flat_map(|(_, c)| c.commands.iter())
        .filter(|d| first_word(&d.name) == head)
        .map(|d| format!("`flick {}`", d.name))
        .collect();
    let diagnostic = Diagnostic::new(
        codes::P052,
        Severity::Error,
        format!("flick has no command `{name}`"),
    );
    if !active_prefix.is_empty() {
        return diagnostic.with_fix(|| {
            (
                format!("did you mean {}?", active_prefix.join(", ")),
                Vec::new(),
            )
        });
    }
    let by: Vec<&str> = (cached.iter())
        .filter(|(_, ds)| ds.iter().any(|d| first_word(&d.name) == head))
        .map(|(id, _)| id.as_str())
        .collect();
    if by.is_empty() {
        diagnostic
    } else {
        let hint = format!(
            "`{}` adds it: run it in a project whose backend or dependencies include that",
            by.join("`, `")
        );
        diagnostic.with_fix(|| (hint, Vec::new()))
    }
}

/// The arguments of `rest`, parsed as `descriptor` declares them.
fn arguments(
    descriptor: &Descriptor,
    short: &str,
    rest: &[String],
) -> Result<Vec<Argument>, clap::Error> {
    let mut command = clap::Command::new(format!("flick {short}:{}", descriptor.name))
        .about(descriptor.summary.clone())
        .no_binary_name(true);
    let arg = |p: &command::Parameter| {
        let action = match (p.kind, p.multiple) {
            (ValueKind::Flag, _) => ArgAction::SetTrue,
            (_, true) => ArgAction::Append,
            (_, false) => ArgAction::Set,
        };
        let arg = Arg::new(p.name.clone()).help(p.help.clone()).action(action);
        // several values after one option, or a positional that takes the rest
        if p.multiple && p.kind != ValueKind::Flag {
            arg.num_args(1..)
        } else {
            arg
        }
    };
    for p in &descriptor.positionals {
        command = command.arg(arg(p).required(p.required));
    }
    for p in &descriptor.options {
        command = command.arg(arg(p).long(p.name.clone()).required(p.required));
    }
    let matches = command.try_get_matches_from(rest)?;
    let mut arguments = Vec::new();
    for p in descriptor.positionals.iter().chain(&descriptor.options) {
        let values: Vec<String> = if p.kind == ValueKind::Flag {
            if matches.get_flag(&p.name) {
                vec!["true".into()]
            } else {
                vec![]
            }
        } else {
            (matches.get_many::<String>(&p.name))
                .map(|v| v.cloned().collect())
                .unwrap_or_default()
        };
        if !values.is_empty() {
            arguments.push(Argument {
                name: p.name.clone(),
                values,
            });
        }
    }
    Ok(arguments)
}

#[cfg(test)]
#[path = "contributed_tests.rs"]
mod tests;
