//! Which projects a command runs on in a workspace: the member
//! `--package` names, or, at the root of a workspace without a project of
//! its own, every member that `check`, `build`, `test`, `fmt`, `clean` and
//! `install` go through.

use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Host;
use flick_core::project::workspace::{self, Workspace};
use flick_core::source::SourceMap;

use crate::args::{CommandArguments, Commands};
use crate::command::Outcome;
use crate::sources::FsHost;

/// The project directories the command runs on, one after another; `None`
/// when it runs as it is, on the project of `--project` or the working
/// directory.
pub fn members(args: &CommandArguments) -> Result<Option<Vec<String>>, Outcome> {
    let projectless = matches!(
        args.cmd,
        Commands::Lsp(_)
            | Commands::ContentMapper(_)
            | Commands::New(_)
            | Commands::Init(_)
            | Commands::Explain(_)
            | Commands::Doctor
    );
    if args.global.project.is_some() || (projectless && args.global.package.is_none()) {
        return Ok(None);
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let found = workspace::find(&host, &mut sources, &host.cwd);
    let fail = |sources, diagnostic| Outcome {
        sources,
        diagnostics: vec![diagnostic],
    };
    let found = match found {
        Ok(found) => found,
        Err(diagnostic) => return Err(fail(sources, diagnostic)),
    };
    if let Some(name) = &args.global.package {
        let Some(workspace) = found else {
            let message = format!("`--package {name}` names a member, but no workspace is here");
            return Err(fail(
                sources,
                Diagnostic::new(codes::P031, Severity::Error, message),
            ));
        };
        let Some(member) = workspace.member(name) else {
            let message = format!(
                "the workspace has no member `{name}`; its members: {}",
                listed(&workspace)
            );
            return Err(fail(
                sources,
                Diagnostic::new(codes::P031, Severity::Error, message),
            ));
        };
        return Ok(Some(vec![shown(&host, member)]));
    }
    let Some(workspace) = found else {
        return Ok(None);
    };
    let at_virtual_root = workspace.root == host.cwd && workspace.manifest.project.is_none();
    Ok((at_virtual_root && runs_on_every_member(&args.cmd))
        .then(|| workspace.members.iter().map(|m| shown(&host, m)).collect()))
}

/// Whether the command, as given, is about whole projects, so that at the
/// root of a workspace it goes through each member.
fn runs_on_every_member(cmd: &Commands) -> bool {
    match cmd {
        // a watch runs until it is stopped, so it watches one member
        Commands::Check(o) => o.paths.is_empty() && !o.watch,
        Commands::Build(o) => o.paths.is_empty() && !o.watch,
        Commands::Test(o) => o.paths.is_empty() && !o.watch,
        Commands::Fmt(o) => o.paths.is_empty() && !o.stdin,
        Commands::Clean(_) | Commands::Install(_) => true,
        _ => false,
    }
}

/// `member` as `--project` takes it and messages show it: relative to the
/// working directory.
fn shown(host: &FsHost, member: &Path) -> String {
    match Host::display(host, member) {
        relative if relative.is_empty() => ".".to_owned(),
        relative => relative,
    }
}

fn listed(workspace: &Workspace) -> String {
    workspace
        .names()
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
