//! Workspaces: a `flick.toml` with `[workspace] members` groups projects
//! that commands at its root run over, dependencies before dependents.

use std::path::{Path, PathBuf};

use super::config::{self, Manifest};
use super::{CONFIG_FILE, Host, normalize};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{SourceMap, Span};

#[derive(Debug)]
pub struct Workspace {
    /// Directory of the root `flick.toml`.
    pub root: PathBuf,
    pub manifest: Manifest,
    /// The members' directories, each after the members it depends on.
    pub members: Vec<PathBuf>,
    /// The members each member depends on by path, by its index.
    dependencies: Vec<Vec<PathBuf>>,
}

impl Workspace {
    /// The member named `name`, the last part of its directory.
    pub fn member(&self, name: &str) -> Option<&Path> {
        self.members
            .iter()
            .map(PathBuf::as_path)
            .find(|m| m.file_name().is_some_and(|n| n == name))
    }

    /// The members that compile the member `dir`, through their path
    /// dependencies or theirs.
    pub fn dependents(&self, dir: &Path) -> Vec<&Path> {
        let mut reached: Vec<&Path> = Vec::new();
        for (member, deps) in self.members.iter().zip(&self.dependencies) {
            // members come after their dependencies, so one pass closes over them
            if deps
                .iter()
                .any(|d| d == dir || reached.contains(&d.as_path()))
            {
                reached.push(member);
            }
        }
        reached
    }

    /// The names of the members, in order.
    pub fn names(&self) -> Vec<String> {
        self.members
            .iter()
            .filter_map(|m| Some(m.file_name()?.to_string_lossy().into_owned()))
            .collect()
    }
}

/// The workspace `dir` is the root of or lies in: the nearest `flick.toml`
/// upwards with `[workspace]`. `Ok(None)` when there is none, or when `dir`
/// is in none of its members.
pub fn find(
    host: &dyn Host,
    sources: &mut SourceMap,
    dir: &Path,
) -> Result<Option<Workspace>, Diagnostic> {
    for root in dir.ancestors() {
        let path = root.join(CONFIG_FILE);
        let Ok(bytes) = host.read(&path) else {
            continue;
        };
        let (file, utf8) = sources.add_bytes(host.display(&path), bytes);
        if let Some(diagnostic) = utf8 {
            return Err(diagnostic);
        }
        let manifest = config::parse_manifest(&sources.get(file).text, file)?;
        let Some(members) = &manifest.members else {
            continue;
        };
        let members: Vec<PathBuf> = members.iter().map(|m| normalize(&root.join(m))).collect();
        if root != dir && !members.iter().any(|m| dir.starts_with(m)) {
            return Ok(None);
        }
        let (members, dependencies) = ordered(host, sources, &members, Span::new(file, 0, 0))?
            .into_iter()
            .unzip();
        return Ok(Some(Workspace {
            root: root.to_path_buf(),
            manifest,
            members,
            dependencies,
        }));
    }
    Ok(None)
}

/// `members` with each after the members it depends on by path, otherwise
/// in their order, and those it depends on; a member without a project or a
/// cycle is an error.
fn ordered(
    host: &dyn Host,
    sources: &mut SourceMap,
    members: &[PathBuf],
    at: Span,
) -> Result<Vec<(PathBuf, Vec<PathBuf>)>, Diagnostic> {
    let mut needs = Vec::new();
    for member in members {
        let path = member.join(CONFIG_FILE);
        let Ok(bytes) = host.read(&path) else {
            return Err(Diagnostic::error(
                codes::P031,
                format!(
                    "workspace member `{}` has no `{CONFIG_FILE}`",
                    host.display(member)
                ),
                at,
            ));
        };
        let (file, utf8) = sources.add_bytes(host.display(&path), bytes);
        if let Some(diagnostic) = utf8 {
            return Err(diagnostic);
        }
        let config = config::parse(&sources.get(file).text, file)?;
        let deps: Vec<PathBuf> = config
            .dependencies
            .iter()
            .map(|d| normalize(&member.join(&d.path)))
            .filter(|d| members.contains(d))
            .collect();
        needs.push(deps);
    }
    let mut out: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    let done = |out: &[(PathBuf, Vec<PathBuf>)], m: &PathBuf| out.iter().any(|(o, _)| o == m);
    while out.len() < members.len() {
        let next = members
            .iter()
            .zip(&needs)
            .find(|(m, deps)| !done(&out, m) && deps.iter().all(|d| done(&out, d)));
        let Some((member, deps)) = next else {
            return Err(Diagnostic::error(
                codes::P031,
                "the path dependencies of the workspace members form a cycle",
                at,
            ));
        };
        out.push((member.clone(), deps.clone()));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
