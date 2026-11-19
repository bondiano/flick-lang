//! Commands that backends and library plugins add to `flick`: found in the
//! project, described from a cache in the store, and run by the host from
//! the plan or the session they declare.

mod inputs;
mod plan;
mod preview;
mod session;

pub(crate) use preview::preview;

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::{env, fs};

use clap::{Arg, ArgAction, CommandFactory};
use flick_backend::command::{self, Argument, Descriptor, Invocation, Style, ValueKind};
use flick_backend::host::runtime::Cancellation;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{Project, store};
use flick_core::source::SourceMap;
use sha2::{Digest, Sha256};

use super::run::{find_project, find_project_at};
use crate::args::{CommandArguments, GlobalArguments};
use crate::backends::Tool;
use crate::{backends, fetch, plugins, report, saved};

/// What runs the commands of one backend or plugin.
pub(crate) trait Contributor: Send + Sync {
    fn describe(&self) -> Result<Vec<Descriptor>, String>;
    fn run(&self, invocation: &Invocation) -> Result<command::Plan, Vec<command::Diagnostic>>;
    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>>;
    fn open(&self, invocation: &Invocation) -> Result<Vec<u8>, Vec<command::Diagnostic>>;
    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        cancel: &Cancellation,
    ) -> Result<command::Handled, String>;
    fn close(&self, state: Vec<u8>) -> command::Plan;
}

/// A backend or a plugin of the project that may add commands.
#[derive(Debug)]
struct Source {
    /// The prefix of its qualified names: a plugin's package, or the last
    /// part of a backend's id.
    short: String,
    /// What `[tools]` grants it by.
    id: String,
    /// The digest of its component, which keys its cached descriptors.
    digest: String,
    plugin: Option<PathBuf>,
    /// An official backend other than the project's, described to name
    /// who adds a command the project has not.
    official: bool,
}

/// Cached descriptors, by the digest of their component.
#[derive(serde::Serialize, serde::Deserialize)]
struct Cached {
    short: String,
    id: String,
    descriptors: Vec<Descriptor>,
}

fn cache_path(digest: &str) -> Option<PathBuf> {
    Some(
        store::store_dir()?
            .join("commands")
            .join(format!("{digest}.cbor")),
    )
}

fn read_cache(digest: &str) -> Option<Cached> {
    let bytes = fs::read(cache_path(digest)?).ok()?;
    ciborium::from_reader(bytes.as_slice()).ok()
}

/// The backend and the plugins of `project`, without loading a component.
fn sources(project: &Project) -> Vec<Source> {
    let mut sources = Vec::new();
    if let Some((id, digest)) = backends::selected_pin(project) {
        let short = id.rsplit('/').next().unwrap_or(&id).to_owned();
        sources.push(Source {
            short,
            id,
            digest,
            plugin: None,
            official: false,
        });
    }
    for (package, path) in super::plugins::components(project) {
        let Ok(bytes) = fs::read(&path) else { continue };
        sources.push(Source {
            short: package.clone(),
            id: package,
            digest: fetch::hex(&Sha256::digest(&bytes)),
            plugin: Some(path),
            official: false,
        });
    }
    sources
}

fn load(project: &Project, source: &Source) -> Result<Arc<dyn Contributor>, String> {
    match &source.plugin {
        Some(path) => plugins::contributor(&source.id, path),
        None if source.official => backends::official_contributor(project, &source.id),
        None => backends::contributor(project),
    }
}

/// Every contributor the store has seen, the official backends described
/// first if it has not seen them yet.
fn known(project: &Project) -> Vec<(String, Vec<Descriptor>)> {
    for (id, digest) in backends::officials() {
        let short = id.rsplit('/').next().unwrap_or(&id).to_owned();
        let source = Source {
            short,
            id,
            digest,
            plugin: None,
            official: true,
        };
        let _ = descriptors(project, &source);
    }
    cached_everywhere()
}

/// The descriptors of `source`, from the cache, else from its component,
/// which the cache then keeps.
fn descriptors(project: &Project, source: &Source) -> Result<Vec<Descriptor>, String> {
    if let Some(cached) = read_cache(&source.digest) {
        return Ok(cached.descriptors);
    }
    let descriptors = load(project, source)?.describe()?;
    let cached = Cached {
        short: source.short.clone(),
        id: source.id.clone(),
        descriptors: descriptors.clone(),
    };
    if let Some(path) = cache_path(&source.digest) {
        let mut bytes = Vec::new();
        if ciborium::into_writer(&cached, &mut bytes).is_ok() {
            let _ = saved::write(&path, &bytes);
        }
    }
    Ok(descriptors)
}

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
        for d in taken {
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

/// The help text of contributed commands, by contributor: those of the
/// project's backend and plugins, or outside a project every cached one.
/// Reads only the cache: no component is loaded and nothing is fetched.
pub(crate) fn help() -> String {
    let groups: Vec<(String, Vec<Descriptor>)> = match find_project_at(None) {
        Ok(project) => sources(&project)
            .into_iter()
            .filter_map(|s| Some((s.id, read_cache(&s.digest)?.descriptors)))
            .collect(),
        Err(_) => cached_everywhere(),
    };
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

fn cached_everywhere() -> Vec<(String, Vec<Descriptor>)> {
    let Some(dir) = store::store_dir().map(|d| d.join("commands")) else {
        return Vec::new();
    };
    let mut found: Vec<(String, Vec<Descriptor>)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read(e.path()).ok())
        .filter_map(|bytes| ciborium::from_reader::<Cached, _>(bytes.as_slice()).ok())
        .map(|c| (c.id, c.descriptors))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
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
    let Ok(project) = find_project(global) else {
        return fail(unknown(&words, &cached_everywhere()));
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
            let _ = error.print();
            return ExitCode::from(2);
        }
    };
    let tools = backends::granted_tools(&project, &source.id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let invocation = match invocation(&project, descriptor, arguments, &tools, &cwd) {
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
/// project's root, and the files the command reads.
fn invocation(
    project: &Project,
    descriptor: &Descriptor,
    arguments: Vec<Argument>,
    tools: &[Tool],
    cwd: &Path,
) -> Result<Invocation, String> {
    let root = &project.root;
    let sources =
        (project.source_root().strip_prefix(root)).map_or_else(|_| String::new(), inputs::slashed);
    let mut invocation = Invocation {
        name: descriptor.name.clone(),
        arguments: inputs::relative(descriptor, arguments, root, cwd)?,
        sources,
        inputs: Vec::new(),
        tools: tools.iter().map(|t| plan::tool(&t.pin)).collect(),
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
        [] => Err(Box::new(unknown(&words, &known()))),
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
/// a cached contributor that adds it is named.
fn unknown(words: &[String], cached: &[(String, Vec<Descriptor>)]) -> Diagnostic {
    let name = words.first().map_or("", String::as_str);
    let head = name.rsplit(':').next().unwrap_or(name);
    let by: Vec<&str> = (cached.iter())
        .filter(|(_, ds)| ds.iter().any(|d| first_word(&d.name) == head))
        .map(|(id, _)| id.as_str())
        .collect();
    let diagnostic = Diagnostic::new(
        codes::P052,
        Severity::Error,
        format!("flick has no command `{name}`"),
    );
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
        Arg::new(p.name.clone()).help(p.help.clone()).action(action)
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
