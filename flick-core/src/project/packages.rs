//! Package graph over the installed `node_modules`: pnpm installs, Flick only
//! reads. A package with Flick modules
//! names its interface directory in `package.json`:
//! `"flick": { "iface-dir": "iface" }`, and module `A.B` is `iface/A/B.iface`.
//! A source package names its source directory instead,
//! `"flick": { "source-dir": "src" }`: each dependent compiles its modules,
//! for whichever target it emits, as modules of that package. A library of
//! `[dependencies]` is a source package too, with the `source-dir` of its own
//! `flick.toml`, and so are its dependencies: a path names its directory, and
//! a git or url source is found through `flick.lock` in the package store.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::config::{self, Dependency, Kind, Source};
use super::lock::{LOCK_FILE, Lock};
use super::store;
use super::{Host, Project};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::{self, Interface};
use crate::source::SourceMap;

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
    /// The resolved `source-dir` of a source package.
    source_dir: Option<PathBuf>,
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
    /// Where a package of Flick modules keeps their interfaces.
    iface_dir: Option<String>,
    /// Where a source package keeps the modules its dependents compile.
    source_dir: Option<String>,
    /// The compiler a project pins; `flick` itself reads it.
    #[serde(rename = "compiler")]
    _compiler: Option<String>,
}

/// The `name` in the project's `package.json`, which owns its modules.
pub fn project_name(host: &dyn Host, root: &Path) -> Option<String> {
    manifest(host, &root.join("package.json"), &mut Vec::new())?.name
}

impl Packages {
    /// Reads the path dependencies of the project, then `package.json` of the
    /// project and of its installed direct dependencies.
    pub fn load(
        host: &dyn Host,
        sources: &mut SourceMap,
        project: &Project,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let root = &project.root;
        let mut packages = Vec::new();
        let dependencies = &project.config.dependencies;
        let fetched = Fetched::load(host, sources, root, dependencies, diagnostics);
        Libraries {
            host,
            sources: &mut *sources,
            project: root,
            fetched: &fetched,
            out: &mut packages,
            diagnostics: &mut *diagnostics,
        }
        .add(root, dependencies);
        let Some(project) = manifest(host, &root.join("package.json"), diagnostics) else {
            return Packages { packages };
        };
        let names = project
            .dependencies
            .keys()
            .chain(project.dev_dependencies.keys());
        let installed = names
            .filter(|name| !packages.iter().any(|p| p.name == **name))
            .map(|name| {
                let dir = root.join("node_modules").join(name);
                let installed = manifest(host, &dir.join("package.json"), diagnostics);
                let flick = installed.as_ref().and_then(|m| m.flick.as_ref());
                let layout = flick.and_then(|f| f.iface_dir.clone());
                let source_dir = flick
                    .and_then(|f| f.source_dir.as_ref())
                    .map(|s| super::normalize(&dir.join(s)));
                Package {
                    name: name.clone(),
                    installed: installed.is_some(),
                    iface_dir: layout.as_ref().map(|l| dir.join(l)),
                    layout,
                    source_dir,
                }
            });
        packages.extend(installed.collect::<Vec<_>>());
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

    /// The source directory of each source package, in dependency order.
    pub fn sources(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.packages
            .iter()
            .filter_map(|p| Some((p.name.as_str(), p.source_dir.as_deref()?)))
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
        if !super::is_module_name(module) {
            diagnostics.push(Diagnostic::new(
                codes::P010,
                Severity::Error,
                format!("invalid installed interface module reference `{module}`"),
            ));
            return Vec::new();
        }
        let mut found = Vec::new();
        // a source package's modules are compiled, not read from interfaces
        for package in self.packages.iter().filter(|p| p.source_dir.is_none()) {
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

/// What `flick.lock` pins and where the package store keeps it.
struct Fetched {
    lock: Lock,
    store: Option<PathBuf>,
}

impl Fetched {
    /// The lock of the project at `root`, read when a dependency is fetched.
    fn load(
        host: &dyn Host,
        sources: &mut SourceMap,
        root: &Path,
        dependencies: &[Dependency],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Fetched {
        let mut lock = Lock::default();
        let path = root.join(LOCK_FILE);
        let fetches = dependencies.iter().any(|d| d.path().is_none());
        if let Some(bytes) = fetches.then(|| host.read(&path).ok()).flatten() {
            let (id, utf8) = sources.add_bytes(host.display(&path), bytes);
            match utf8.map_or_else(|| Lock::parse(&sources.get(id).text, id), Err) {
                Ok(read) => lock = read,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        Fetched {
            lock,
            store: store::store_dir(),
        }
    }

    /// The directory of the library `dependency` names, from the manifest in
    /// `dir`.
    fn dir(&self, dir: &Path, dependency: &Dependency) -> Result<PathBuf, Diagnostic> {
        if let Source::Path(path) = &dependency.source {
            return Ok(super::normalize(&dir.join(path)));
        }
        let missing = |what: &str| {
            Diagnostic::error(
                codes::P034,
                format!(
                    "dependency `{}` is not {what}: run `flick install`",
                    dependency.name
                ),
                dependency.span,
            )
        };
        let locked = (self.lock)
            .find(&dependency.name, &dependency.source)
            .ok_or_else(|| missing("in `flick.lock`"))?;
        let store = self.store.as_ref().ok_or_else(|| missing("fetched"))?;
        let fetched = locked.dir(store);
        Ok(match dependency.source.subdir() {
            Some(subdir) => fetched.join(subdir),
            None => fetched,
        })
    }
}

/// The libraries of a project as its manifests name them.
struct Libraries<'a> {
    host: &'a dyn Host,
    sources: &'a mut SourceMap,
    /// The root of the project being loaded.
    project: &'a Path,
    fetched: &'a Fetched,
    out: &'a mut Vec<Package>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Libraries<'_> {
    /// The libraries `dependencies` of the manifest in `dir` name, then
    /// theirs, each once.
    fn add(&mut self, dir: &Path, dependencies: &[Dependency]) {
        let mut found = Vec::new();
        for dependency in dependencies {
            if let Some(library) = self.library(dir, dependency) {
                found.push(library);
            }
        }
        for (at, dependencies) in found {
            self.add(&at, &dependencies);
        }
    }

    /// The library `dependency` names, with its directory and dependencies.
    fn library(
        &mut self,
        dir: &Path,
        dependency: &Dependency,
    ) -> Option<(PathBuf, Vec<Dependency>)> {
        let (host, sources, out) = (self.host, &mut *self.sources, &mut *self.out);
        let diagnostics = &mut *self.diagnostics;
        let at = match self.fetched.dir(dir, dependency) {
            Ok(at) => at,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                return None;
            }
        };
        let error = |message: String| Diagnostic::error(codes::P031, message, dependency.span);
        if at == self.project {
            diagnostics.push(error(format!(
                "dependency `{}` is the project itself: path dependencies must not form a cycle",
                dependency.name
            )));
            return None;
        }
        if out.iter().any(|p| p.name == dependency.name) {
            return None;
        }
        let file = at.join(super::CONFIG_FILE);
        let Ok(bytes) = host.read(&file) else {
            diagnostics.push(error(format!(
                "dependency `{}`: no `{}` at `{}`",
                dependency.name,
                super::CONFIG_FILE,
                host.display(&at)
            )));
            return None;
        };
        let (id, utf8) = sources.add_bytes(host.display(&file), bytes);
        let parsed = match utf8 {
            Some(diagnostic) => Err(diagnostic),
            None => config::parse(&sources.get(id).text, id),
        };
        let library = match parsed {
            Ok(library) => library,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                return None;
            }
        };
        if library.kind != Kind::Lib {
            diagnostics.push(error(format!(
                "dependency `{}` at `{}` is an app: a dependency is a `kind = \"lib\"`",
                dependency.name,
                host.display(&at)
            )));
            return None;
        }
        if let Some(requirement) = library.package.as_ref().and_then(|p| p.flick.as_ref())
            && !compiles_with(requirement, crate::VERSION)
        {
            diagnostics.push(Diagnostic::error(
                codes::P033,
                format!(
                    "dependency `{}` needs flick {requirement}, and this is flick {}",
                    dependency.name,
                    crate::VERSION
                ),
                dependency.span,
            ));
            return None;
        }
        out.push(Package {
            name: dependency.name.clone(),
            installed: true,
            layout: None,
            iface_dir: None,
            source_dir: Some(super::normalize(&at.join(&library.source_dir))),
        });
        Some((at, library.dependencies))
    }
}

/// Whether the semver requirement `requirement` includes `version`.
fn compiles_with(requirement: &str, version: &str) -> bool {
    match (
        semver::VersionReq::parse(requirement),
        semver::Version::parse(version),
    ) {
        (Ok(requirement), Ok(version)) => requirement.matches(&version),
        _ => false,
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
                out.push(Diagnostic::new(
                    codes::P014,
                    Severity::Error,
                    format!(
                        "`{module}` of package `{package}` depends on `{dependency}`, whose installed interface cannot be found"
                    ),
                ));
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
                    .with_fix(|| (
                        format!("install the versions `{package}` was built with, or a release of `{package}` built against the installed one"),
                        Vec::new(),
                    )),
                );
            }
        }
    }
    out
}
