//! `flick plugins`: the plugins a project gets, where each comes from, and
//! whether `flick.lock` pins the build of its component.

use std::fmt::Write;
use std::fs;
use std::path::Path;

use flick_core::plugin::first_party;
use flick_core::project::config::{self, Source};
use flick_core::project::{self, Project};
use flick_core::source::FileId;
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
    let lock = fetch::lock_of(&project.root).unwrap_or_default();
    let heads: Vec<&str> = first_party::HEADS.iter().map(|(head, _)| *head).collect();
    let mut out = format!(
        "{}: linked into flick, blocks {}\n",
        first_party::PACKAGE,
        heads.join(", ")
    );
    let inherited = inherited(&project);
    for dependency in &project.config.dependencies {
        let Source::Path(dir) = &dependency.source else {
            continue;
        };
        let library = project::normalize(&project.root.join(dir));
        let Some(component) = plugin_of(&library) else {
            continue;
        };
        let path = project::normalize(&library.join(component));
        let shown = fetch::relative(&project.root, &path);
        let status = match fs::read(&path) {
            Err(_) => "missing: build it, as `flick run plugin` in its package does".to_owned(),
            Ok(bytes) => {
                let checksum = format!("sha256:{}", fetch::hex(&Sha256::digest(&bytes)));
                match lock.plugins.iter().find(|p| p.name == dependency.name) {
                    None => format!("{checksum}, not in `flick.lock`: run `flick install`"),
                    Some(p) if p.checksum != checksum => {
                        format!("{checksum}, another build than `flick.lock` pins")
                    }
                    Some(_) => format!("{checksum}, pinned"),
                }
            }
        };
        let from = if inherited.contains(&dependency.name) {
            " (from the workspace)"
        } else {
            ""
        };
        let _ = writeln!(out, "{}{from}: {shown}, {status}", dependency.name);
    }
    print!("{out}");
    Outcome::default()
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
