//! Loading source files into a `SourceMap`, with read and UTF-8 failures as
//! diagnostics.

use std::io::Read;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::{FileId, SourceMap};

pub fn load(sources: &mut SourceMap, path: &str) -> Result<FileId, Diagnostic> {
    let bytes = std::fs::read(path).map_err(|err| {
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
    std::io::stdin().read_to_end(&mut bytes).map_err(|err| {
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
    let Ok(entries) = std::fs::read_dir(path) else {
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
