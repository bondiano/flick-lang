//! Package graph over the installed `node_modules`: pnpm installs, Flick only
//! reads. A package with Flick modules
//! names its interface directory in `package.json`:
//! `"flick": { "iface-dir": "iface" }`, and module `A.B` is `iface/A/B.iface`.

use std::collections::BTreeMap;
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
    /// The `iface-dir` as written, and resolved.
    layout: Option<String>,
    iface_dir: Option<PathBuf>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    name: Option<String>,
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

/// The `name` in the project's `package.json`, which owns its modules.
pub fn project_name(host: &dyn Host, root: &Path) -> Option<String> {
    manifest(host, &root.join("package.json"), &mut Vec::new())?.name
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
                let layout = installed
                    .as_ref()
                    .and_then(|m| Some(m.flick.as_ref()?.iface_dir.clone()));
                Package {
                    name: name.clone(),
                    installed: installed.is_some(),
                    iface_dir: layout.as_ref().map(|l| dir.join(l)),
                    layout,
                }
            })
            .collect();
        Packages { packages }
    }

    /// The import specifier of the compiled JS of module `A.B` of `package`:
    /// it sits next to its interface, `<package>/<iface-dir>/A/B.js`.
    pub fn js_specifier(&self, package: &str, file: &str) -> String {
        let dir = self
            .packages
            .iter()
            .find(|p| p.name == package)
            .and_then(|p| p.layout.as_deref())
            .unwrap_or(".");
        let dir: Vec<&str> = dir
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect();
        [package]
            .into_iter()
            .chain(dir)
            .chain([file])
            .collect::<Vec<_>>()
            .join("/")
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

/// A package module built against another interface of a module than the
/// one installed: its compiled code may no longer fit.
pub(super) fn stale(
    interfaces: &BTreeMap<String, Interface>,
    owners: &BTreeMap<String, String>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (module, package) in owners {
        let Some(interface) = interfaces.get(module) else {
            continue;
        };
        for (dependency, recorded) in &interface.dependencies {
            let Some(installed) = interfaces.get(dependency) else {
                continue;
            };
            let actual = iface::fingerprint(installed);
            if actual != *recorded {
                let from = owners
                    .get(dependency)
                    .map_or(String::new(), |p| format!(" of `{p}`"));
                out.push(
                    Diagnostic::new(
                        codes::P014,
                        Severity::Error,
                        format!(
                            "`{module}` of package `{package}` was built against another interface of `{dependency}`{from}: fingerprint {recorded}, installed {actual}"
                        ),
                    )
                    .with_fix(
                        format!("install the versions `{package}` was built with, or a release of `{package}` built against the installed one"),
                        Vec::new(),
                    ),
                );
            }
        }
    }
    out
}
