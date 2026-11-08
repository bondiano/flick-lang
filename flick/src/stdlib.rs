//! The implicit packages of a run. The compiler holds only the core modules;
//! each implicit package comes from the package the project depends on, by its
//! name in `[dependencies]` or, without one, the tag of this compiler's
//! release, which `flick install` fetches and `flick.lock` pins. A compiler
//! built from a checkout takes that checkout's own instead of the tag.
//! `FLICK_STD_PATH` names a directory to use for the first of them instead,
//! the others beside it, as this repository's own builds do.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{env, fs};

use flick_core::implicit_packages::{IMPLICIT_PACKAGES, is_implicit_package};
use flick_core::project::config::{Dependency, Source};
use flick_core::project::lock::{LOCK_FILE, Lock};
use flick_core::project::{self, Project, store};
use flick_core::source::{FileId, SourceMap, Span};
use flick_core::types;

use crate::fetch;
use crate::sources::FsHost;
use crate::toolchain;

/// The repository whose tags publish the implicit packages.
const REPOSITORY: &str = "https://github.com/bondiano/flick-lang";

/// The implicit package `name` of this compiler's release.
fn default_source(name: &str) -> Source {
    Source::Git {
        url: REPOSITORY.to_owned(),
        tag: format!("std-v{}", toolchain::COMPILER),
        subdir: Some(format!("packages/{name}")),
    }
}

fn dependency(name: &str) -> Dependency {
    Dependency {
        name: name.to_owned(),
        source: default_source(name),
        span: Span::new(FileId::default(), 0, 0),
    }
}

/// `dependencies` with each implicit package of this release they do not name,
/// when `FLICK_STD_PATH` does not stand for them.
pub fn with_implicit(dependencies: &[Dependency]) -> Vec<Dependency> {
    let mut all = dependencies.to_vec();
    if !overridden() {
        for name in IMPLICIT_PACKAGES {
            if !all.iter().any(|d| d.name == *name) {
                all.push(dependency(name));
            }
        }
    }
    all
}

/// Sets the implicit packages for this run, from the project the working
/// directory or `--project` names, or from the store outside a project.
pub fn prepare(explicit: Option<&str>) -> Result<(), String> {
    if overridden() {
        return if types::has_implicit_packages() {
            Ok(())
        } else {
            Err("`FLICK_STD_PATH` names no `std` package with `src/modules.txt`".to_owned())
        };
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let project = project::discover(&host, &mut sources, &host.cwd, explicit.map(Path::new))
        .ok()
        .flatten();
    let packages = IMPLICIT_PACKAGES
        .iter()
        .map(|name| {
            let dir = match &project {
                Some(project) => in_project(project, name)?,
                None => local_package(&host.cwd, name).map_or_else(|| outside_project(name), Ok)?,
            };
            if *name == IMPLICIT_PACKAGES[0] {
                let _ = READ_FROM.set(dir.clone());
            }
            types::read_package(name, &dir)
                .map_err(|e| format!("cannot read `{name}` at `{}`: {e}", dir.display()))
        })
        .collect::<Result<_, _>>()?;
    types::set_implicit_packages(packages);
    Ok(())
}

/// The directory of the first implicit package, as [`prepare`] chose it.
static READ_FROM: OnceLock<PathBuf> = OnceLock::new();

/// The directory `std` was read from in this run, when it is known.
pub fn std_dir() -> Option<PathBuf> {
    overridden_dir(IMPLICIT_PACKAGES[0]).or_else(|| READ_FROM.get().cloned())
}

fn overridden() -> bool {
    env::var_os("FLICK_STD_PATH").is_some_and(|d| !d.is_empty())
}

/// The directory `FLICK_STD_PATH` gives the implicit package `name`.
fn overridden_dir(name: &str) -> Option<PathBuf> {
    let index = IMPLICIT_PACKAGES.iter().position(|p| *p == name)?;
    let first = PathBuf::from(env::var_os("FLICK_STD_PATH").filter(|d| !d.is_empty())?);
    if index == 0 {
        return Some(first);
    }
    Some(first.parent().unwrap_or(&first).join(name))
}

/// The directory of the package `name`, an implicit one or a dependency of
/// the project the working directory or `explicit` names.
pub fn package_dir(name: &str, explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(dir) = overridden_dir(name) {
        return Ok(dir);
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let project = project::discover(&host, &mut sources, &host.cwd, explicit.map(Path::new))
        .ok()
        .flatten();
    match project {
        Some(project) => in_project(&project, name),
        None if is_implicit_package(name) => {
            local_package(&host.cwd, name).map_or_else(|| outside_project(name), Ok)
        }
        None => Err(format!("`{name}` is no dependency here: no project")),
    }
}

/// The implicit package `name` of a source checkout: the one around `dir`, or
/// the one this binary was built from while it is still there, since the
/// release of the same version can lag behind. A release archive was built at
/// a path that does not exist where it runs.
fn local_package(dir: &Path, name: &str) -> Option<PathBuf> {
    checkout_package(dir, name).or_else(|| {
        let built_from = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
        checkout_package(built_from, name)
    })
}

/// A source checkout uses its own implicit packages when a CLI or editor starts
/// without Cargo's `FLICK_STD_PATH` environment. The release package can lag
/// behind the compiler being developed in this checkout.
fn checkout_package(cwd: &Path, name: &str) -> Option<PathBuf> {
    cwd.ancestors().find_map(|root| {
        let package = root.join("packages").join(name);
        (root.join("flick-core/Cargo.toml").is_file()
            && root.join(".cargo/config.toml").is_file()
            && package.join("src/modules.txt").is_file())
        .then_some(package)
    })
}

/// The directory of the package `name` the project depends on, or of the
/// project itself.
fn in_project(project: &Project, name: &str) -> Result<PathBuf, String> {
    if (project.config.package.as_ref()).is_some_and(|p| p.name == name) {
        return Ok(project.root.clone());
    }
    if is_implicit_package(name)
        && !project.config.dependencies.iter().any(|d| d.name == name)
        && let Some(package) = local_package(&project.root, name)
    {
        return Ok(package);
    }
    let dependencies = with_implicit(&project.config.dependencies);
    let dependency = (dependencies.iter())
        .find(|d| d.name == name)
        .ok_or_else(|| format!("the project has no dependency `{name}`"))?;
    if let Source::Path(path) = &dependency.source {
        return Ok(project::normalize(&project.root.join(path)));
    }
    stored(&project.root, dependency).ok_or_else(|| {
        format!(
            "`{name}` from `{}` is not installed: run `flick install`",
            dependency.source.key()
        )
    })
}

/// The directory of `dependency` the lock at `root` pins, when the store
/// has it.
fn stored(root: &Path, dependency: &Dependency) -> Option<PathBuf> {
    let text = fs::read_to_string(root.join(LOCK_FILE)).ok()?;
    let mut sources = SourceMap::default();
    let file = sources.add(LOCK_FILE.to_owned(), &text);
    let locked = Lock::parse(&text, file)
        .ok()?
        .find(&dependency.name, &dependency.source)?
        .dir(&store::store_dir()?);
    let dir = (dependency.source.subdir()).map_or(locked.clone(), |s| locked.join(s));
    dir.exists().then_some(dir)
}

/// The implicit package `name` of this release outside a project: pinned in
/// the user's cache, and fetched the first time.
fn outside_project(name: &str) -> Result<PathBuf, String> {
    let root = store::cache_dir()
        .ok_or("no cache directory for the implicit packages: set FLICK_STORE")?
        .join(format!("std-{}", toolchain::COMPILER));
    fs::create_dir_all(&root).map_err(|e| format!("cannot create `{}`: {e}", root.display()))?;
    let all: Vec<Dependency> = IMPLICIT_PACKAGES.iter().map(|p| dependency(p)).collect();
    let wanted = dependency(name);
    if stored(&root, &wanted).is_none() {
        fetch::install(&root, &all, &[], fetch::Options::default())?;
    }
    stored(&root, &wanted).ok_or_else(|| format!("`{name}` could not be fetched"))
}
