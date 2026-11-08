//! The libraries a project compiles from their sources. An npm package of
//! `node_modules`, which pnpm installs, is one when its `package.json` names
//! its source directory, `"flick": { "source-dir": "src" }`: each dependent
//! compiles its modules,
//! for whichever target it emits, as modules of that package. A library of
//! `[dependencies]` is a source package too, with the `source-dir` of its own
//! `flick.toml`, and so are its dependencies: a path names its directory, and
//! a git or url source is found through `flick.lock` in the package store.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::config::{self, Dependency, JsDependency, Kind, Source};
use super::lock::{LOCK_FILE, Lock};
use super::store;
use super::{Host, Project};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::implicit_packages::is_implicit_package;
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
    /// The resolved `source-dir` of a source package.
    source_dir: Option<PathBuf>,
    /// For a library of `[dependencies]`, what its modules depend on.
    library: Option<Library>,
    /// The component of the plugin a library provides.
    plugin: Option<PathBuf>,
    /// The extensions of the files that plugin turns into modules.
    load: Vec<String>,
    /// The stages of a build that plugin takes part in.
    hooks: Vec<String>,
}

/// What the modules of a library of `[dependencies]` depend on besides the
/// project: its sources, which `flick.lock` pins by checksum when they are
/// fetched, and the libraries it depends on.
#[derive(Debug)]
pub struct Library {
    pub checksum: Option<String>,
    pub dependencies: Vec<String>,
    /// The npm packages it needs on the js target.
    pub js_dependencies: Vec<JsDependency>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct Manifest {
    name: Option<String>,
    #[serde(default)]
    pub(super) dependencies: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    dev_dependencies: serde_json::Map<String, serde_json::Value>,
    flick: Option<FlickField>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct FlickField {
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
                let source_dir = flick
                    .and_then(|f| f.source_dir.as_ref())
                    .map(|s| super::normalize(&dir.join(s)));
                Package {
                    name: name.clone(),
                    installed: installed.is_some(),
                    source_dir,
                    library: None,
                    plugin: None,
                    load: Vec::new(),
                    hooks: Vec::new(),
                }
            });
        packages.extend(installed.collect::<Vec<_>>());
        Packages { packages }
    }

    /// The source directory of each source package, in dependency order.
    pub fn sources(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.packages
            .iter()
            .filter_map(|p| Some((p.name.as_str(), p.source_dir.as_deref()?)))
    }

    /// Each library of `[dependencies]`, with its source directory, in
    /// dependency order.
    pub fn libraries(&self) -> impl Iterator<Item = (&str, &Path, &Library)> {
        self.packages.iter().filter_map(|p| {
            Some((
                p.name.as_str(),
                p.source_dir.as_deref()?,
                p.library.as_ref()?,
            ))
        })
    }

    /// Each extension a library's plugin loads, with the library, in the
    /// order of the dependencies.
    pub fn loaders(&self) -> impl Iterator<Item = (&str, &str)> {
        (self.packages.iter())
            .flat_map(|p| p.load.iter().map(move |e| (e.as_str(), p.name.as_str())))
    }

    /// Each library whose plugin takes part in `hook`, with its component,
    /// in the order of the dependencies.
    pub fn hooked(&self, hook: &str) -> Vec<(String, Option<PathBuf>)> {
        (self.packages.iter())
            .filter(|p| p.hooks.iter().any(|h| h == hook))
            .map(|p| (p.name.clone(), p.plugin.clone()))
            .collect()
    }

    /// The component of the plugin the library `name` provides.
    pub fn plugin(&self, name: &str) -> Option<&Path> {
        self.packages
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.plugin.as_deref())
    }

    /// Declared dependencies missing from `node_modules`.
    pub fn missing(&self) -> Vec<&str> {
        self.packages
            .iter()
            .filter(|p| !p.installed)
            .map(|p| p.name.as_str())
            .collect()
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
        // an implicit package is built in: the host hands its modules to the checker
        for dependency in dependencies
            .iter()
            .filter(|d| !is_implicit_package(&d.name))
        {
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
        let checksum = match &dependency.source {
            Source::Path(_) => None,
            source => (self.fetched.lock)
                .find(&dependency.name, source)
                .map(|locked| locked.checksum.clone()),
        };
        let dependencies = (library.dependencies.iter())
            .filter(|d| !is_implicit_package(&d.name))
            .map(|d| d.name.clone())
            .collect();
        out.push(Package {
            name: dependency.name.clone(),
            installed: true,
            source_dir: Some(super::normalize(&at.join(&library.source_dir))),
            library: Some(Library {
                checksum,
                dependencies,
                js_dependencies: library.js_dependencies,
            }),
            plugin: (library.package.as_ref())
                .and_then(|p| p.plugin.as_ref())
                .map(|plugin| at.join(plugin)),
            load: (library.package.as_ref()).map_or_else(Vec::new, |p| p.load.clone()),
            hooks: (library.package.as_ref()).map_or_else(Vec::new, |p| p.hooks.clone()),
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
pub(super) fn manifest(
    host: &dyn Host,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Manifest> {
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
