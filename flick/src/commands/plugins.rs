//! `flick plugins`: the plugins a project gets, where each comes from, and
//! whether `flick.lock` pins the build of its component.

use std::fmt::Write;
use std::fs;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::plugin::first_party;
use flick_core::project::config::{self, Dependency, Source};
use flick_core::project::lock::Lock;
use flick_core::project::{self, Project, store};
use flick_core::source::{FileId, SourceMap};
use sha2::{Digest, Sha256};

use super::run::find_project;
use crate::args::GlobalArguments;
use crate::command::Outcome;
use crate::fetch;

/// Lists the plugins of the project, one a line, the compiler's own first.
pub fn plugins(global: &GlobalArguments) -> Outcome {
    let project = match find_project(global) {
        Ok(project) => project,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    let lock = match fetch::lock_of(&project.root) {
        Ok(lock) => lock,
        Err(message) => {
            return Outcome {
                sources: SourceMap::default(),
                diagnostics: vec![Diagnostic::new(codes::P034, Severity::Error, message)],
            };
        }
    };
    let store = store::store_dir();
    let inherited = inherited(&project);
    let mut listed = Vec::new();
    let mut diagnostics = Vec::new();
    for dependency in &project.config.dependencies {
        match plugin_of_dependency(&project.root, dependency, &lock, store.as_deref()) {
            Ok(Some(line)) => listed.push((dependency.name.clone(), line)),
            Ok(None) => {}
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    if !diagnostics.is_empty() {
        return Outcome {
            sources: SourceMap::default(),
            diagnostics,
        };
    }
    let heads: Vec<&str> = first_party::HEADS.iter().map(|(head, _)| *head).collect();
    let mut out = format!(
        "{}: linked into flick, blocks {}\n",
        first_party::PACKAGE,
        heads.join(", ")
    );
    for (name, line) in listed {
        let from = if inherited.contains(&name) {
            " (from the workspace)"
        } else {
            ""
        };
        let _ = writeln!(out, "{name}{from}: {line}");
    }
    print!("{out}");
    Outcome::default()
}

/// The listing line of the plugin `dependency` provides, if it provides one.
fn plugin_of_dependency(
    root: &Path,
    dependency: &Dependency,
    lock: &Lock,
    store: Option<&Path>,
) -> Result<Option<String>, Diagnostic> {
    let library = project::library_dir(root, dependency, lock, store)?;
    let Some(component) = plugin_of(&library) else {
        return Ok(None);
    };
    let path = project::normalize(&library.join(&component));
    let path_source = matches!(dependency.source, Source::Path(_));
    let shown = if path_source {
        fetch::relative(root, &path)
    } else {
        let locked = lock
            .find(&dependency.name, &dependency.source)
            .expect("the library directory names a lock entry");
        format!("{} ({component})", locked.source)
    };
    let status = match fs::read(&path) {
        Err(_) if path_source => {
            "missing: build it, as `flick run plugin` in its package does".to_owned()
        }
        Err(_) => "missing: the package store does not have it".to_owned(),
        Ok(bytes) => {
            let checksum = format!("sha256:{}", fetch::hex(&Sha256::digest(&bytes)));
            if path_source {
                match lock
                    .plugins
                    .iter()
                    .find(|p| p.name == dependency.name && p.component == shown)
                {
                    None => format!("{checksum}, not in `flick.lock`: run `flick install`"),
                    Some(p) if p.checksum != checksum => {
                        format!("{checksum}, another build than `flick.lock` pins")
                    }
                    Some(_) => format!("{checksum}, pinned"),
                }
            } else {
                format!("{checksum}, pinned")
            }
        }
    };
    Ok(Some(format!("{shown}, {status}")))
}

/// The component of the plugin the library at `dir` provides.
fn plugin_of(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join(project::CONFIG_FILE)).ok()?;
    config::parse(&text, FileId::default())
        .ok()?
        .package?
        .plugin
}

/// The dependencies `project` has from its workspace, not from its manifest.
pub(super) fn inherited(project: &Project) -> Vec<String> {
    if project.workspace.is_none() {
        return Vec::new();
    }
    // a manifest that does not read tells nothing
    let Some(written) = fs::read_to_string(project.root.join(project::CONFIG_FILE))
        .ok()
        .and_then(|text| config::parse(&text, FileId::default()).ok())
    else {
        return Vec::new();
    };
    let own = |name: &str| written.dependencies.iter().any(|d| d.name == name);
    (project.config.dependencies.iter())
        .filter(|d| !own(&d.name))
        .map(|d| d.name.clone())
        .collect()
}
