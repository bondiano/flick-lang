//! The `std` package of a run. The compiler holds only the core modules;
//! `std` comes from the package the project depends on, `std` in its
//! `[dependencies]` or, without one, the tag of this compiler's release,
//! which `flick install` fetches and `flick.lock` pins. `FLICK_STD_PATH`
//! names a directory to use instead, as this repository's own builds do.

use std::path::{Path, PathBuf};
use std::{env, fs};

use flick_core::project::config::{Dependency, Source};
use flick_core::project::lock::{LOCK_FILE, Lock};
use flick_core::project::{self, Project, store};
use flick_core::source::{FileId, SourceMap, Span};
use flick_core::types;

use crate::fetch;
use crate::sources::FsHost;
use crate::toolchain;

/// The repository whose tags publish `std`.
const REPOSITORY: &str = "https://github.com/bondiano/flick-lang";

/// The name of the `std` dependency.
pub const NAME: &str = "std";

/// The `std` of this compiler's release.
pub fn default_source() -> Source {
    Source::Git {
        url: REPOSITORY.to_owned(),
        tag: format!("std-v{}", toolchain::COMPILER),
        subdir: Some("packages/std".to_owned()),
    }
}

/// `dependencies` with the `std` of this release when they name none and
/// `FLICK_STD_PATH` does not stand for it.
pub fn with_std(dependencies: &[Dependency]) -> Vec<Dependency> {
    let mut all = dependencies.to_vec();
    if !overridden() && !all.iter().any(|d| d.name == NAME) {
        all.push(Dependency {
            name: NAME.to_owned(),
            source: default_source(),
            span: Span::new(FileId::default(), 0, 0),
        });
    }
    all
}

/// Sets the modules of `std` for this run, from the project the working
/// directory or `--project` names, or from the store outside a project.
pub fn prepare(explicit: Option<&str>) -> Result<(), String> {
    if overridden() {
        return if types::has_std() {
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
    let dir = match project {
        Some(project) => in_project(&project, NAME)?,
        None => checkout_std(&host.cwd).map_or_else(outside_project, Ok)?,
    };
    let package = types::read_std(&dir)
        .map_err(|e| format!("cannot read `std` at `{}`: {e}", dir.display()))?;
    types::set_std(package);
    Ok(())
}

fn overridden() -> bool {
    env::var_os("FLICK_STD_PATH").is_some_and(|d| !d.is_empty())
}

/// The directory of the package `name`, `std` or a dependency of the
/// project the working directory or `explicit` names.
pub fn package_dir(name: &str, explicit: Option<&str>) -> Result<PathBuf, String> {
    if name == NAME
        && let Some(dir) = env::var_os("FLICK_STD_PATH").filter(|d| !d.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let project = project::discover(&host, &mut sources, &host.cwd, explicit.map(Path::new))
        .ok()
        .flatten();
    match project {
        Some(project) => in_project(&project, name),
        None if name == NAME => checkout_std(&host.cwd).map_or_else(outside_project, Ok),
        None => Err(format!("`{name}` is no dependency here: no project")),
    }
}

/// A source checkout uses its own `std` when a CLI or editor starts without
/// Cargo's `FLICK_STD_PATH` environment. The release package can lag behind
/// the compiler being developed in this checkout.
fn checkout_std(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors().find_map(|root| {
        let std = root.join("packages/std");
        (root.join("flick-core/Cargo.toml").is_file()
            && root.join(".cargo/config.toml").is_file()
            && std.join("src/modules.txt").is_file())
        .then_some(std)
    })
}

/// The directory of the package `name` the project depends on, or of the
/// project itself.
fn in_project(project: &Project, name: &str) -> Result<PathBuf, String> {
    if (project.config.package.as_ref()).is_some_and(|p| p.name == name) {
        return Ok(project.root.clone());
    }
    if name == NAME
        && !project.config.dependencies.iter().any(|d| d.name == NAME)
        && let Some(std) = checkout_std(&project.root)
    {
        return Ok(std);
    }
    let dependencies = with_std(&project.config.dependencies);
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

/// The `std` of this release outside a project: pinned in the user's
/// cache, and fetched the first time.
fn outside_project() -> Result<PathBuf, String> {
    let root = store::cache_dir()
        .ok_or("no cache directory for `std`: set FLICK_STORE")?
        .join(format!("std-{}", toolchain::COMPILER));
    fs::create_dir_all(&root).map_err(|e| format!("cannot create `{}`: {e}", root.display()))?;
    let std = [Dependency {
        name: NAME.to_owned(),
        source: default_source(),
        span: Span::new(FileId::default(), 0, 0),
    }];
    if stored(&root, &std[0]).is_none() {
        fetch::install(&root, &std, &[], fetch::Options::default())?;
    }
    stored(&root, &std[0]).ok_or_else(|| "`std` could not be fetched".to_owned())
}
