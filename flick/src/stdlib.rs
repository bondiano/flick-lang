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
        Some(project) => in_project(&project)?,
        None => outside_project()?,
    };
    let package = types::read_std(&dir)
        .map_err(|e| format!("cannot read `std` at `{}`: {e}", dir.display()))?;
    types::set_std(package);
    Ok(())
}

fn overridden() -> bool {
    env::var_os("FLICK_STD_PATH").is_some_and(|d| !d.is_empty())
}

/// The directory of the `std` the project depends on.
fn in_project(project: &Project) -> Result<PathBuf, String> {
    // the std package itself
    if project
        .config
        .package
        .as_ref()
        .is_some_and(|p| p.name == NAME)
    {
        return Ok(project.root.clone());
    }
    let dependencies = with_std(&project.config.dependencies);
    let std = dependencies
        .iter()
        .find(|d| d.name == NAME)
        .expect("with_std adds it unless overridden");
    if let Source::Path(path) = &std.source {
        return Ok(project::normalize(&project.root.join(path)));
    }
    stored(&project.root, std).ok_or_else(|| {
        format!(
            "`std` from `{}` is not installed: run `flick install`",
            std.source.key()
        )
    })
}

/// The directory of `std` the lock at `root` pins, when the store has it.
fn stored(root: &Path, std: &Dependency) -> Option<PathBuf> {
    let text = fs::read_to_string(root.join(LOCK_FILE)).ok()?;
    let mut sources = SourceMap::default();
    let file = sources.add(LOCK_FILE.to_owned(), &text);
    let locked = Lock::parse(&text, file)
        .ok()?
        .find(NAME, &std.source)?
        .dir(&store::store_dir()?);
    let dir = std
        .source
        .subdir()
        .map_or(locked.clone(), |s| locked.join(s));
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
