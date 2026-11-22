//! A plan command carried out without publishing, for an editor to show
//! what it would write as an edit.

use flick_backend::command::{Diagnostic, Severity, Style};
use flick_core::diagnostics::Severity as HostSeverity;
use flick_core::project::Project;

use super::plan::{Done, chain};
use super::{arguments, contributed, invocation, known, load, resolve};
use crate::backends;

/// What a command would write and print.
pub(crate) struct Preview {
    /// Each file by its path relative to the project's root.
    pub(crate) files: Vec<(String, Vec<u8>)>,
    pub(crate) stdout: Vec<u8>,
}

/// The files the plan command `words` would write in `project`, its path
/// arguments relative to the root; an error gives the first reason it
/// cannot.
pub(crate) fn preview(project: &Project, words: &[String]) -> Result<Preview, String> {
    let (found, _) = contributed(project);
    let (source, descriptor, rest) =
        resolve(words, &found, &|| known(project)).map_err(|d| d.message)?;
    if descriptor.style != Style::Plan {
        return Err(format!("`{}` is a session", descriptor.name));
    }
    let contributor = load(project, source)?;
    let arguments = arguments(descriptor, &source.short, &rest).map_err(|e| e.to_string())?;
    let tools = backends::granted_tools(project, &source.id);
    let invocation = invocation(project, descriptor, arguments, &tools, &project.root)?;
    let first = |ds: &[Diagnostic]| {
        ds.iter()
            .find(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
    };
    let plan = contributor
        .run(&invocation)
        .map_err(|ds| first(&ds).unwrap_or_default())?;
    let mut done = Done::default();
    let call = (
        project,
        &*contributor,
        descriptor,
        &invocation,
        tools.as_slice(),
    );
    let files = chain(call, plan, &mut done).map_err(|d| d.message)?;
    if let Some(why) = first(&done.diagnostics) {
        return Err(why);
    }
    if let Some(why) = done
        .refused
        .iter()
        .find(|d| d.severity == HostSeverity::Error)
        .map(|d| d.message.clone())
    {
        return Err(why);
    }
    Ok(Preview {
        files: (files.into_iter().flatten())
            .map(|a| (a.path, a.contents))
            .collect(),
        stdout: done.stdout,
    })
}
