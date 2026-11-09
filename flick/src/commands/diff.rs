//! `flick diff [REF]`: the changes of the project's public API since a git
//! revision (the latest tag by default), and the version they call for.
//! The revision is checked out into a temporary worktree and checked like
//! the working tree; both must check without errors.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command as Process};
use std::{env, fs};

use flick_core::api_diff::{self, Bump, Change, ModuleChanges};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::Interface;
use flick_core::source::SourceMap;

use super::inspect::compile;
use super::run::find_project;
use crate::args::{DiffArguments, GlobalArguments};
use crate::command::Outcome;

fn failure(message: String) -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(codes::P025, Severity::Error, message)],
    }
}

pub fn diff(options: &DiffArguments, global: &GlobalArguments) -> Outcome {
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    let revision = match &options.git_ref {
        Some(r) => r.clone(),
        None => match latest_tag(&project.root) {
            Some(tag) => tag,
            None => {
                return failure(
                    "no git tag to compare with; name a revision: `flick diff <ref>`".into(),
                );
            }
        },
    };
    let changes = match changes_since(&project.root, &revision) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    print!(
        "{}",
        report(&revision, &changes, version(&project.root).as_deref())
    );
    Outcome::default()
}

/// The latest tag reachable from `HEAD`.
pub fn latest_tag(root: &Path) -> Option<String> {
    git(root, &["describe", "--tags", "--abbrev=0"]).ok()
}

/// The API changes of the project at `root` since `revision`.
pub fn changes_since(root: &Path, revision: &str) -> Result<Vec<ModuleChanges>, Outcome> {
    let top = PathBuf::from(git(root, &["rev-parse", "--show-toplevel"]).map_err(failure)?);
    let relative = root
        .canonicalize()
        .ok()
        .and_then(|r| Some(r.strip_prefix(top.canonicalize().ok()?).ok()?.to_path_buf()))
        .unwrap_or_default();
    let tree = Worktree::add(&top, revision).map_err(failure)?;
    let old_root = tree.path.join(&relative);
    // the old tree resolves packages as the working tree has them installed
    let modules = root.join("node_modules");
    if modules.is_dir() {
        link(&modules, &old_root.join("node_modules"));
    }
    let old = interfaces(&old_root)?;
    let new = interfaces(root)?;
    Ok(api_diff::diff(&old, &new))
}

/// The interfaces of the project's source modules, after a clean check.
fn interfaces(root: &Path) -> Result<Vec<Interface>, Outcome> {
    let checked = compile(Some(&root.to_string_lossy()))?;
    Ok(checked
        .compilation
        .modules
        .iter()
        .filter(|m| !m.test && !m.interface.private)
        .map(|m| m.interface.clone())
        .collect())
}

/// The changes, a line per change, and the version they call for.
pub fn report(revision: &str, changes: &[ModuleChanges], version: Option<&str>) -> String {
    let mut out = format!("API changes since {revision}:\n");
    if changes.is_empty() {
        out.push_str("\n  none\n");
    }
    for m in changes {
        let _ = write!(out, "\n{}\n", m.module);
        for c in &m.changes {
            let _ = match c {
                Change::Added(what) => writeln!(out, "  MINOR  added {what}"),
                Change::Removed(what) => writeln!(out, "  MAJOR  removed {what}"),
                Change::Changed { name, was, now } => writeln!(
                    out,
                    "  MAJOR  changed {name}\n           was: {was}\n           now: {now}"
                ),
            };
        }
    }
    let bump = api_diff::bump(changes);
    let next = version.and_then(|v| Some(format!(": {v} -> {}", api_diff::next_version(v, bump)?)));
    let _ = writeln!(out, "\nThis is a {bump} change{}", next.unwrap_or_default());
    out
}

/// Whether `changes` break importers.
pub fn breaking(changes: &[ModuleChanges]) -> bool {
    api_diff::bump(changes) == Bump::Major
}

/// The `version` of the project's `package.json`.
fn version(root: &Path) -> Option<String> {
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("package.json")).ok()?).ok()?;
    manifest.get("version")?.as_str().map(str::to_owned)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Process::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// A detached worktree of a revision, removed when dropped.
struct Worktree {
    top: PathBuf,
    path: PathBuf,
}

impl Worktree {
    fn add(top: &Path, revision: &str) -> Result<Worktree, String> {
        let path = env::temp_dir().join(format!("flick-diff-{}", process::id()));
        let at = path.to_string_lossy().into_owned();
        git(
            top,
            &["worktree", "add", "--detach", "--quiet", &at, revision],
        )?;
        Ok(Worktree {
            top: top.to_path_buf(),
            path,
        })
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let at = self.path.to_string_lossy().into_owned();
        // a leftover worktree is reported by `git worktree list`; nothing else to do
        let _ = git(&self.top, &["worktree", "remove", "--force", &at]);
    }
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) {
    use std::os::unix::fs::symlink;
    let _ = symlink(from, to);
}

#[cfg(not(unix))]
fn link(_: &Path, _: &Path) {}
