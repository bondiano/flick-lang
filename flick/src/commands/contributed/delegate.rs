//! The verbs of flick that a project's backend implements for its own
//! ecosystem: flick does its part on the Flick libraries, then runs the
//! backend's plan command of the same name with the verb's arguments.

use std::env;

use flick_backend::command::{Argument, Dependency, Style};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::SourceMap;

use super::plan::{self, InPlaceMode};
use super::registry::SourceKind;
use super::{descriptors, invocation, load, sources};
use crate::args::GlobalArguments;
use crate::backends;

/// The verbs a backend may implement, which no plugin may.
pub(super) const VERBS: [&str; 6] = ["install", "add", "remove", "update", "deps", "pack"];

/// What the backend's command printed and reported.
pub(crate) struct Delegated {
    pub(crate) stdout: Vec<u8>,
    pub(crate) sources: SourceMap,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) executed: bool,
}

/// Runs the plan command `verb` of the backend `project` selects with
/// `arguments`, by their names, and what the closure asks of its ecosystem;
/// `None` when the backend has no such command.
pub(crate) fn delegate(
    project: &Project,
    verb: &str,
    arguments: Vec<Argument>,
    dependencies: Vec<Dependency>,
    global: &GlobalArguments,
) -> Option<Delegated> {
    let failed = |code, message: String| Delegated {
        stdout: Vec::new(),
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
        executed: false,
    };
    let source = sources(project)
        .into_iter()
        .find(|s| matches!(s.kind, SourceKind::Backend { .. }))?;
    let descriptor = match descriptors(project, &source) {
        Ok(all) => all
            .into_iter()
            .find(|d| d.name == verb && d.style == Style::Plan)?,
        Err(why) => {
            let message = format!("the commands of `{}` are unavailable: {why}", source.id);
            return Some(failed(codes::P013, message));
        }
    };
    let declared = |name: &str| {
        (descriptor.positionals.iter())
            .chain(&descriptor.options)
            .any(|p| p.name == name)
    };
    if let Some(argument) = arguments.iter().find(|a| !declared(&a.name)) {
        let message = format!(
            "the backend `{}` takes no `{}` for `flick {verb}`",
            source.id, argument.name
        );
        return Some(failed(codes::P063, message));
    }
    let tools = backends::granted_tools(project, &source.id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let mut invocation = match invocation(project, &source.id, &descriptor, arguments, &tools, &cwd)
    {
        Ok(invocation) => invocation,
        Err(why) => return Some(failed(codes::P055, format!("`{verb}` cannot run: {why}"))),
    };
    invocation.dependencies = dependencies;
    let named = invocation
        .arguments
        .iter()
        .any(|a| matches!(a.name.as_str(), "packages" | "why") && !a.values.is_empty());
    if matches!(verb, "install" | "update" | "deps")
        && !descriptor.inputs.is_empty()
        && invocation.inputs.is_empty()
        && invocation.dependencies.is_empty()
        && !named
    {
        return Some(Delegated {
            stdout: Vec::new(),
            sources: SourceMap::default(),
            diagnostics: Vec::new(),
            executed: false,
        });
    }
    let contributor = match load(project, &source) {
        Ok(contributor) => contributor,
        Err(why) => {
            return Some(failed(
                codes::P013,
                format!("cannot load `{}`: {why}", source.id),
            ));
        }
    };
    let (stdout, sources, diagnostics) = plan::outcome(
        project,
        &*contributor,
        &descriptor,
        &invocation,
        &tools,
        global,
        InPlaceMode::PackageManager,
    );
    Some(Delegated {
        stdout,
        sources,
        diagnostics,
        executed: true,
    })
}
