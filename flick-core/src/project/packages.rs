//! The libraries a project compiles from their sources. A library of
//! `[dependencies]` is a source package with the `source-dir` of its own
//! `flick.toml`, and so are its dependencies: a path names its directory, and
//! a git or url source is found through `flick.lock` in the package store.
//! Packages of a target backend's ecosystem that ship Flick sources are
//! reported through [`Host::foreign`].

use std::path::{Component, Path, PathBuf};

use super::config::{self, Dependency, HostConfig, Kind, Source, TargetDependency};
use super::lock::{LOCK_FILE, Lock};
use super::store;
use super::{Host, Project};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::implicit_packages::is_implicit_package;
use crate::source::{SourceMap, Span};

#[derive(Debug, Default)]
pub(super) struct Packages {
    /// Direct dependencies in declaration order.
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
    /// The names whose places of call that plugin checks.
    watch: Vec<String>,
    /// Package-level `[package] when = "<cond>"`, if declared.
    when: Option<(String, Span)>,
    /// Host declaration `[host]`, if this package defines a host environment.
    host: Option<HostConfig>,
}

/// What the modules of a library of `[dependencies]` depend on besides the
/// project: its sources, which `flick.lock` pins by checksum when they are
/// fetched, and the libraries it depends on.
#[derive(Debug)]
pub struct Library {
    pub checksum: Option<String>,
    pub dependencies: Vec<String>,
    /// The target ecosystem packages it needs under `[target.<name>.dependencies]`.
    pub target_dependencies: Vec<TargetDependency>,
}

/// The project's own name in its backend's ecosystem, which owns its modules.
pub fn project_name(host: &dyn Host, project: &Project) -> Option<String> {
    host.foreign(project).name
}

impl Packages {
    /// Reads the path/fetched dependencies of the project, then the packages
    /// reported by its backend through [`Host::foreign`].
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
        // the packages of the backend's ecosystem, as it reports them
        let foreign = host.foreign(project);
        diagnostics.extend(foreign.diagnostics);
        let mut installed = Vec::new();
        for f in foreign.packages {
            if packages.iter().any(|p| p.name == f.name) {
                continue;
            }
            let source_dir = match f.source_dir {
                Some(dir) if is_relative_normal(&dir) => Some(super::normalize(&root.join(dir))),
                Some(dir) => {
                    diagnostics.push(Diagnostic::new(
                        codes::P013,
                        Severity::Error,
                        format!(
                            "the backend reported a package root outside the project for `{}`: `{}`",
                            f.name,
                            dir.display()
                        ),
                    ));
                    None
                }
                None => None,
            };
            installed.push(Package {
                name: f.name,
                installed: f.version.is_some() || source_dir.is_some(),
                source_dir,
                library: None,
                plugin: None,
                load: Vec::new(),
                hooks: Vec::new(),
                watch: Vec::new(),
                when: None,
                host: None,
            });
        }
        packages.extend(installed);
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

    /// Each library whose plugin watches names, with those names, in the
    /// order of the dependencies.
    pub fn watchers(&self) -> impl Iterator<Item = (&str, &[String])> {
        (self.packages.iter())
            .filter(|p| !p.watch.is_empty())
            .map(|p| (p.name.as_str(), p.watch.as_slice()))
    }

    /// The component of the plugin the library `name` provides.
    pub fn plugin(&self, name: &str) -> Option<&Path> {
        self.packages
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.plugin.as_deref())
    }

    /// Declared dependencies missing from the backend's installed packages.
    pub fn missing(&self) -> Vec<&str> {
        self.packages
            .iter()
            .filter(|p| !p.installed)
            .map(|p| p.name.as_str())
            .collect()
    }

    /// Each dependency package that declares `[package] when`.
    pub fn package_when(&self) -> impl Iterator<Item = (&str, &str, Span)> {
        self.packages.iter().filter_map(|p| {
            let (cond, span) = p.when.as_ref()?;
            Some((p.name.as_str(), cond.as_str(), *span))
        })
    }

    /// Each dependency package that declares `[host]`.
    pub fn hosts(&self) -> impl Iterator<Item = (&str, &HostConfig)> {
        self.packages
            .iter()
            .filter_map(|p| Some((p.name.as_str(), p.host.as_ref()?)))
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
        library_dir(dir, dependency, &self.lock, self.store.as_deref())
            .map_err(|diagnostic| diagnostic.with_span(dependency.span))
    }
}

/// The directory of the library `dependency` of the manifest in `dir`: a path
/// relative to it, or the fetched package `lock` pins in `store`. A fetched
/// source the lock or the store does not hold is `P034` without a place, for
/// the caller to put at the dependency.
pub fn library_dir(
    dir: &Path,
    dependency: &Dependency,
    lock: &Lock,
    store: Option<&Path>,
) -> Result<PathBuf, Diagnostic> {
    if let Source::Path(path) = &dependency.source {
        return Ok(super::normalize(&dir.join(path)));
    }
    let missing = |what: &str| {
        Diagnostic::new(
            codes::P034,
            Severity::Error,
            format!(
                "dependency `{}` is not {what}: run `flick install`",
                dependency.name
            ),
        )
    };
    let locked = (lock)
        .find(&dependency.name, &dependency.source)
        .ok_or_else(|| missing("in `flick.lock`"))?;
    let store = store.ok_or_else(|| missing("fetched"))?;
    let fetched = locked.dir(store);
    let at = match dependency.source.subdir() {
        Some(subdir) => fetched.join(subdir),
        None => fetched,
    };
    if !at.join(super::CONFIG_FILE).is_file() {
        return Err(missing("fetched"));
    }
    Ok(at)
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
                target_dependencies: library.target_dependencies,
            }),
            plugin: (library.package.as_ref())
                .and_then(|p| p.plugin.as_ref())
                .map(|plugin| at.join(plugin)),
            load: (library.package.as_ref()).map_or_else(Vec::new, |p| p.load.clone()),
            hooks: (library.package.as_ref()).map_or_else(Vec::new, |p| p.hooks.clone()),
            watch: (library.package.as_ref()).map_or_else(Vec::new, |p| p.watch.clone()),
            when: library.package.as_ref().and_then(|p| p.when.clone()),
            host: library.host,
        });
        Some((at, library.dependencies))
    }
}

/// Whether `path` is a non-empty relative path made solely of normal segments.
fn is_relative_normal(path: &Path) -> bool {
    let mut has_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal = true,
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => return false,
        }
    }
    has_normal
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
