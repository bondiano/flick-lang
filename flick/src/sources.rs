//! Loading source files into a `SourceMap`, with read and UTF-8 failures as
//! diagnostics.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs, io};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::plugin::Expander;
use flick_core::plugin::reach::Outside;
use flick_core::project::Host;
use flick_core::source::{FileId, SourceMap};

use crate::plugins;

/// The file system, with paths shown relative to the working directory.
pub struct FsHost {
    pub cwd: PathBuf,
}

impl FsHost {
    pub fn new() -> Self {
        FsHost {
            cwd: env::current_dir().unwrap_or_default(),
        }
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

/// `.fl` files under `path` in a stable order; a file path is kept as given.
pub fn source_files(path: &str, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(path) else {
        out.push(path.to_owned());
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if entry.is_dir() && !name.starts_with('.') && name != "node_modules" {
            source_files(&entry.to_string_lossy(), out);
        } else if entry.extension().is_some_and(|e| e == "fl") {
            out.push(entry.to_string_lossy().into_owned());
        }
    }
}
