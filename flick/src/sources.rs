//! Loading source files into a `SourceMap`, with read and UTF-8 failures as
//! diagnostics.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs, io};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::plugin::Expander;
use flick_core::plugin::reach::Outside;
#[cfg(feature = "wasm-backends")]
use flick_core::project::Foreign;
#[cfg(feature = "wasm-backends")]
use flick_core::project::config::BackendEnvironment;
#[cfg(feature = "wasm-backends")]
use flick_core::project::target::Wanted;
use flick_core::project::{Host, Project, is_companion};
use flick_core::source::{FileId, SourceMap};

#[cfg(feature = "wasm-backends")]
use crate::backends;
use crate::plugins;

/// The file system, with paths shown relative to the working directory.
pub struct FsHost {
    pub cwd: PathBuf,
    #[cfg(feature = "wasm-backends")]
    load_backends: bool,
}

impl FsHost {
    pub fn new() -> Self {
        FsHost {
            cwd: env::current_dir().unwrap_or_default(),
            #[cfg(feature = "wasm-backends")]
            load_backends: true,
        }
    }

    /// A host that never loads backend components (for frontend-only commands
    /// such as `inspect --emit iface`).
    pub fn without_backends() -> Self {
        FsHost {
            cwd: env::current_dir().unwrap_or_default(),
            #[cfg(feature = "wasm-backends")]
            load_backends: false,
        }
    }

    /// Directories the project's backend treats as installed package trees,
    /// returning none when backend loading is disabled on this host.
    pub fn skip_dirs(&self, project: Option<&Project>) -> Vec<String> {
        #[cfg(feature = "wasm-backends")]
        if !self.load_backends {
            return Vec::new();
        }
        skip_dirs(project)
    }
}

impl Host for FsHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.cwd)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    fn plugin(&self, package: &str, component: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        plugins::plugin(package, component)
    }

    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        plugins::cached(key)
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        plugins::save(key, value);
    }

    fn outside(&self) -> Option<Arc<dyn Outside>> {
        Some(plugins::outside(false))
    }

    #[cfg(feature = "wasm-backends")]
    fn foreign(&self, project: &Project) -> Foreign {
        if self.load_backends {
            backends::foreign(project)
        } else {
            Foreign::default()
        }
    }

    #[cfg(feature = "wasm-backends")]
    fn verify(&self, project: &Project, wanted: &[Wanted]) -> Vec<Diagnostic> {
        if self.load_backends {
            backends::verify(project, wanted, false)
        } else {
            Vec::new()
        }
    }

    #[cfg(feature = "wasm-backends")]
    fn unsupported_modules(&self, project: &Project) -> Vec<String> {
        if self.load_backends {
            backends::unsupported_modules(project)
        } else {
            Vec::new()
        }
    }

    #[cfg(feature = "wasm-backends")]
    fn stages_by_class(&self, project: &Project) -> bool {
        if self.load_backends {
            backends::stages_by_class(project)
        } else {
            backends::stages_by_class_cached(project)
        }
    }

    #[cfg(feature = "wasm-backends")]
    fn handles_service(
        &self,
        project: &Project,
        environment: &BackendEnvironment,
        service: &str,
    ) -> Option<bool> {
        if self.load_backends {
            backends::handles_service(project, environment, service)
        } else {
            None
        }
    }
}

pub fn load(sources: &mut SourceMap, path: &str) -> Result<FileId, Diagnostic> {
    let bytes = fs::read(path).map_err(|err| {
        Diagnostic::new(
            codes::P001,
            Severity::Error,
            format!("cannot read `{path}`: {err}"),
        )
    })?;
    added(sources.add_bytes(path, bytes))
}

/// Reads stdin, shown in diagnostics as `path`.
pub fn load_stdin(sources: &mut SourceMap, path: &str) -> Result<FileId, Diagnostic> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes).map_err(|err| {
        Diagnostic::new(
            codes::P001,
            Severity::Error,
            format!("cannot read stdin: {err}"),
        )
    })?;
    added(sources.add_bytes(path, bytes))
}

fn added((file, utf8): (FileId, Option<Diagnostic>)) -> Result<FileId, Diagnostic> {
    utf8.map_or(Ok(file), Err)
}

/// Directories the project's backend treats as installed package trees.
pub fn skip_dirs(project: Option<&Project>) -> Vec<String> {
    #[cfg(feature = "wasm-backends")]
    {
        project.map_or_else(Vec::new, backends::package_dirs)
    }
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = project;
        Vec::new()
    }
}

/// `.fl` files under `path` in a stable order, but test companions
/// (`Users.test.fl`), which only `flick test` and `flick fmt` read; a file
/// path is kept as given.
pub fn source_files(path: &str, out: &mut Vec<String>) {
    source_files_skipping(path, &[], out);
}

/// Like [`source_files`], skipping top-level directory names in `skip`.
pub fn source_files_skipping(path: &str, skip: &[String], out: &mut Vec<String>) {
    walk(path, skip, out, &|p| !is_companion(p));
}

/// [`source_files`] with the test companions, skipping directory names in `skip`.
pub fn all_files_skipping(path: &str, skip: &[String], out: &mut Vec<String>) {
    walk(path, skip, out, &|_| true);
}

/// The test companions under `path`, in a stable order, skipping `skip`.
pub fn companions_skipping(path: &str, skip: &[String], out: &mut Vec<String>) {
    walk(path, skip, out, &is_companion);
}

fn walk(path: &str, skip: &[String], out: &mut Vec<String>, keep: &dyn Fn(&Path) -> bool) {
    let Ok(entries) = fs::read_dir(path) else {
        out.push(path.to_owned());
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if entry.is_dir() && !name.starts_with('.') && !skip.iter().any(|s| s == name) {
            walk(&entry.to_string_lossy(), skip, out, keep);
        } else if entry.extension().is_some_and(|e| e == "fl") && keep(&entry) {
            out.push(entry.to_string_lossy().into_owned());
        }
    }
}
