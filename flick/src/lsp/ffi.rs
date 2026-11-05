//! The rewrite of a module `flick ffi import` wrote: on its header line,
//! the module as the import would write it now, a stub of the header
//! alone included.

use std::path::Path;

use flick_core::diagnostics::Diagnostic;
use flick_core::ffi::check::{self, Header};
use flick_core::ffi::import;
use flick_core::project;
use flick_core::source::SourceMap;
use serde_json::{Map, Value, json};

use super::super::protocol;
use crate::commands::ffi::{module_prefix, read_policy, read_surface};
use crate::sources::FsHost;

/// "Regenerate from <pkg>@<version>" when the range touches the header
/// and the module differs from a fresh import; disabled, with the reason,
/// when the declarations cannot be read.
pub(super) fn regenerate(path: &Path, text: &str, params: &Value) -> Option<Value> {
    // reading the declarations runs Node: not for the requests of cursor moves
    let automatic = params["context"]["triggerKind"]
        .as_u64()
        .is_some_and(|k| k != 1);
    if automatic || params["range"]["start"]["line"] != 0 {
        return None;
    }
    let header = check::header(text)?;
    let uri = params["textDocument"]["uri"].as_str()?;
    match generate(path, &header) {
        Ok((_, fresh)) if fresh == text => None,
        Ok((provenance, fresh)) => {
            let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
            let edits = json!([{ "range": protocol::range(text, 0, end), "newText": fresh }]);
            Some(json!({
                "title": format!("Regenerate from {provenance}"),
                "kind": "refactor.rewrite",
                "edit": { "changes": Map::from_iter([(uri.to_owned(), edits)]) },
            }))
        }
        Err(d) => Some(json!({
            "title": format!("Regenerate from {}", header.package),
            "kind": "refactor.rewrite",
            "disabled": { "reason": d.message },
        })),
    }
}

/// The package's provenance and what `flick ffi import` writes at `path`:
/// the module is named after the file, the policy is the one beside it.
fn generate(path: &Path, header: &Header) -> Result<(String, String), Diagnostic> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let project = project::discover(&FsHost::new(), &mut SourceMap::default(), dir, None)?;
    let root = project.as_ref().map_or(dir, |p| p.root.as_path());
    let surface = read_surface(root, &header.package)?;
    let policy = read_policy(dir)?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let module = match module_prefix(project.as_ref(), dir)? {
        prefix if prefix.is_empty() => stem.into_owned(),
        prefix => format!("{prefix}.{stem}"),
    };
    let generated = import::generate(&surface, &policy, header.select.as_deref(), &module);
    Ok((surface.provenance(), generated.text))
}
