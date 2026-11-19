//! The verbs of flick that a project's backend implements for its own
//! ecosystem: flick does its part on the Flick libraries, then runs the
//! backend's plan command of the same name with the verb's arguments.

use std::env;

use flick_backend::command::{Argument, Dependency, Style};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::SourceMap;

use super::plan;
use super::{descriptors, invocation, load, sources};
use crate::args::GlobalArguments;
use crate::backends;

/// The verbs a backend may implement, which no plugin may.
pub(super) const VERBS: [&str; 5] = ["install", "add", "remove", "update", "deps"];

/// What the backend's command printed and reported.
pub(crate) struct Delegated {
    pub(crate) stdout: Vec<u8>,
    pub(crate) sources: SourceMap,
    pub(crate) diagnostics: Vec<Diagnostic>,
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
    };
    let source = sources(project).into_iter().find(|s| s.plugin.is_none())?;
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
        return Some(failed(codes::P057, message));
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
    let tools = backends::granted_tools(project, &source.id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let mut invocation = match invocation(project, &source.id, &descriptor, arguments, &tools, &cwd)
    {
        Ok(invocation) => invocation,
        Err(why) => return Some(failed(codes::P055, format!("`{verb}` cannot run: {why}"))),
    };
    invocation.dependencies = dependencies;
    let (stdout, sources, diagnostics) = plan::outcome(
        project,
        &*contributor,
        &descriptor,
        &invocation,
        &tools,
        global,
    );
    Some(Delegated {
        stdout,
        sources,
        diagnostics,
    })
}
