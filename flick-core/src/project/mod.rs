//! Project model: `flick.toml`,
//! source and test roots, the module ↔ path mapping and installed packages.
//! File access goes through [`Host`], so the core stays free of I/O.

pub mod config;
mod load;
mod packages;

use std::path::{Component, Path, PathBuf};

pub use config::Config;
pub use load::{Compilation, Compiled, check, compile};
pub use packages::project_name;
use std::io;

#[cfg(test)]
mod tests;

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::{FileId, SourceMap};

pub const CONFIG_FILE: &str = "flick.toml";

/// File access for the project model; paths are absolute.
pub trait Host {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// The path as diagnostics show it.
    fn display(&self, path: &Path) -> String;
}

#[derive(Debug)]
pub struct Project {
    /// Directory of `flick.toml`.
    pub root: PathBuf,
    pub config: Config,
    pub config_file: FileId,
}

impl Project {
    pub fn source_root(&self) -> PathBuf {
        normalize(&self.root.join(&self.config.source_dir))
    }

    pub fn test_root(&self) -> PathBuf {
        normalize(&self.root.join(&self.config.test_dir))
    }
}

/// Finds and parses the project: `explicit` is a directory or a config file
/// (`--project`); otherwise the nearest `flick.toml` from `cwd` upwards.
/// `Ok(None)`: no project and none was asked for.
pub fn discover(
    host: &dyn Host,
    sources: &mut SourceMap,
    cwd: &Path,
    explicit: Option<&Path>,
) -> Result<Option<Project>, Diagnostic> {
    let candidates: Vec<PathBuf> = match explicit {
        Some(path) => {
            let path = normalize(&cwd.join(path));
            if path.extension().is_some_and(|e| e == "toml") {
                vec![path]
            } else {
                vec![path.join(CONFIG_FILE)]
            }
        }
        None => cwd.ancestors().map(|dir| dir.join(CONFIG_FILE)).collect(),
    };
    for path in &candidates {
        let Ok(bytes) = host.read(path) else { continue };
        let (file, utf8) = sources.add_bytes(host.display(path), bytes);
        if let Some(diagnostic) = utf8 {
            return Err(diagnostic);
        }
        let config = config::parse(&sources.get(file).text, file)?;
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        return Ok(Some(Project {
            root,
            config,
            config_file: file,
        }));
    }
    match explicit {
        Some(path) => Err(no_project(format!(
            "no `{CONFIG_FILE}` at `{}`",
            path.display()
        ))),
        None => Ok(None),
    }
}

pub fn no_project(message: String) -> Diagnostic {
    Diagnostic::new(codes::P008, Severity::Error, message).with_fix(
        "run inside a project, select one with `--project <path>`, or pass files",
        Vec::new(),
    )
}

/// Resolves `.` and `..` without touching the file system.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(out.components().next_back(), Some(Component::Normal(_))) =>
            {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `App/Users.fl` → `App.Users`; `None` unless every part is a capitalized name.
pub fn module_name(relative: &Path) -> Option<String> {
    let stem = relative.with_extension("");
    let parts: Option<Vec<&str>> = stem
        .components()
        .map(|c| match c {
            Component::Normal(part) => part.to_str().filter(|p| is_con(p)),
            _ => None,
        })
        .collect();
    let parts = parts?;
    (relative.extension().is_some_and(|e| e == "fl") && !parts.is_empty()).then(|| parts.join("."))
}

/// `App.Users` → `App/Users.fl`.
pub fn module_path(name: &str) -> PathBuf {
    let mut path: PathBuf = name.split('.').collect();
    path.set_extension("fl");
    path
}

pub fn is_module_name(name: &str) -> bool {
    name.split('.').all(is_con)
}

fn is_con(part: &str) -> bool {
    let mut chars = part.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
