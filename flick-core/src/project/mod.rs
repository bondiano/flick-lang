//! Project model: `flick.toml`,
//! source and test roots, the module ↔ path mapping and installed packages.
//! File access goes through [`Host`], so the core stays free of I/O.

#[cfg(feature = "native-project")]
pub mod cache;
pub mod config;
#[cfg(feature = "native-project")]
mod inherit;
#[cfg(feature = "native-project")]
pub mod js;
mod load;
#[cfg(feature = "native-project")]
pub mod lock;
#[cfg(feature = "native-project")]
mod packages;
#[cfg(feature = "native-project")]
pub mod pnpm;
#[cfg(feature = "native-project")]
pub mod store;
#[cfg(feature = "native-project")]
pub mod workspace;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub use config::Config;
#[cfg(all(test, feature = "build-artifacts"))]
pub(crate) use load::js_only;
#[cfg(feature = "ide")]
pub use load::{Analysis, Analyzed, analysis_fingerprint, analyze, analyze_with};
pub use load::{
    CachedModule, Compilation, Compiled, FormatPlugins, Fresh, MainKind, ModuleCache, PackageCache,
    check, check_with, compile, compile_with, main_kind,
};
#[cfg(feature = "native-project")]
pub use packages::project_name;
use std::io;

#[cfg(test)]
mod tests;

#[cfg(feature = "native-project")]
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::plugin::Expander;
use crate::plugin::reach::Outside;
use crate::source::FileId;
#[cfg(feature = "native-project")]
use crate::source::SourceMap;

#[cfg(feature = "native-project")]
pub const CONFIG_FILE: &str = "flick.toml";

/// File access for the project model; paths are absolute.
pub trait Host {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// The path as diagnostics show it.
    fn display(&self, path: &Path) -> String;
    /// The plugin of the package `package`, which a module imports, or why
    /// there is none; `component` is the WebAssembly component the package
    /// names for it.
    fn plugin(&self, package: &str, component: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        let _ = component;
        Err(format!("no dependency provides plugin \"{package}\""))
    }
    /// What a plugin gave for the call `key` in an earlier run.
    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        let _ = key;
        None
    }
    /// Keeps what a plugin gave for the call `key` for later runs.
    fn save(&self, key: &[u8], value: &[u8]) {
        let _ = (key, value);
    }
    /// The world outside the compiler a plugin reaches as a project grants
    /// it; none reaches nothing.
    fn outside(&self) -> Option<Arc<dyn Outside>> {
        None
    }
}

#[derive(Debug)]
pub struct Project {
    /// Directory of `flick.toml`.
    pub root: PathBuf,
    pub config: Config,
    pub config_file: FileId,
    /// The manifest of the workspace the project is a member of, whose
    /// `[[workspace.plugins]]` it got.
    pub workspace: Option<PathBuf>,
}

impl Project {
    pub fn source_root(&self) -> PathBuf {
        normalize(&self.root.join(&self.config.source_dir))
    }

    pub fn test_root(&self) -> PathBuf {
        normalize(&self.root.join(&self.config.test_dir))
    }
}

#[cfg(feature = "native-project")]
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
        let mut config = config::parse(&sources.get(file).text, file)?;
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let workspace = inherit::around(host, sources, &root)?;
        if let Some((manifest, plugins)) = &workspace {
            let at = manifest.parent().unwrap_or(Path::new("/"));
            inherit::inherit(&mut config, &root, at, plugins)?;
        }
        return Ok(Some(Project {
            root,
            config,
            config_file: file,
            workspace: workspace.map(|(manifest, _)| manifest),
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

#[cfg(feature = "native-project")]
pub fn no_project(message: String) -> Diagnostic {
    Diagnostic::new(codes::P008, Severity::Error, message).with_fix(|| {
        (
            "run inside a project, select one with `--project <path>`, or pass files",
            Vec::new(),
        )
    })
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
