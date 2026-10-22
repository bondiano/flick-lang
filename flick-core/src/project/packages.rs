//! Package graph over the installed `node_modules`: pnpm installs, Flick only
//! reads. A package with Flick modules
//! names its interface directory in `package.json`:
//! `"flick": { "iface-dir": "iface" }`, and module `A.B` is `iface/A/B.iface`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Host;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::{self, Interface};

#[derive(Debug, Default)]
pub(super) struct Packages {
    /// Direct dependencies in `package.json` order.
    packages: Vec<Package>,
}

#[derive(Debug)]
struct Package {
    name: String,
    installed: bool,
    iface_dir: Option<PathBuf>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(default)]
    dependencies: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    dev_dependencies: serde_json::Map<String, serde_json::Value>,
    flick: Option<FlickField>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct FlickField {
    iface_dir: String,
}

impl Packages {
    /// Reads `package.json` of the project and of its installed direct dependencies.
    pub fn load(host: &dyn Host, root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Self {
        let Some(project) = manifest(host, &root.join("package.json"), diagnostics) else {
            return Packages::default();
        };
        let names = project
            .dependencies
            .keys()
            .chain(project.dev_dependencies.keys());
        let packages = names
            .map(|name| {
                let dir = root.join("node_modules").join(name);
                let installed = manifest(host, &dir.join("package.json"), diagnostics);
                Package {
                    name: name.clone(),
                    installed: installed.is_some(),
                    iface_dir: installed.and_then(|m| Some(dir.join(m.flick?.iface_dir))),
                }
            })
            .collect();
        Packages { packages }
    }

    /// Declared dependencies missing from `node_modules`.
    pub fn missing(&self) -> Vec<&str> {
        self.packages
            .iter()
            .filter(|p| !p.installed)
            .map(|p| p.name.as_str())
            .collect()
    }

    /// Installed packages that provide `module`, with its interface.
    pub fn find(
        &self,
        host: &dyn Host,
        module: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<(String, Interface)> {
        let mut found = Vec::new();
        for package in &self.packages {
            let Some(dir) = &package.iface_dir else {
                continue;
            };
            let path = dir.join(super::module_path(module)).with_extension("iface");
            let Ok(bytes) = host.read(&path) else {
                continue;
            };
            let invalid = |reason: String| {
                Diagnostic::new(
                    codes::P010,
                    Severity::Error,
                    format!(
                        "invalid interface `{}` of package `{}`: {reason}",
                        host.display(&path),
                        package.name
                    ),
                )
            };
            match iface::read(&bytes, module) {
                Ok(interface) => found.push((package.name.clone(), interface)),
                Err(reason) => diagnostics.push(invalid(reason)),
            }
        }
        found
    }
}

/// `None` when the file is absent; an unreadable manifest is reported.
fn manifest(host: &dyn Host, path: &Path, diagnostics: &mut Vec<Diagnostic>) -> Option<Manifest> {
    let bytes = host.read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(manifest) => Some(manifest),
        Err(err) => {
            diagnostics.push(Diagnostic::new(
                codes::P005,
                Severity::Error,
                format!("invalid `{}`: {err}", host.display(path)),
            ));
            Some(Manifest::default())
        }
    }
}
