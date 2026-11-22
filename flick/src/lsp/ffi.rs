//! The rewrite of a module `ffi import` wrote: on its header line, the
//! module as the import would write it now, a stub of the header alone
//! included. The backend's `ffi import` runs without publishing.

use std::path::Path;

use flick_core::ffi::check::{self, Header};
#[cfg(feature = "wasm-backends")]
use flick_core::project;
#[cfg(feature = "wasm-backends")]
use flick_core::source::SourceMap;
use serde_json::{Map, Value, json};

use super::super::protocol;
#[cfg(feature = "wasm-backends")]
use crate::commands::contributed;
#[cfg(feature = "wasm-backends")]
use crate::sources::FsHost;

/// "Regenerate from <pkg>@<version>" when the range touches the header
/// and the module differs from a fresh import; disabled, with the reason,
/// when the import cannot run.
pub(super) fn regenerate(path: &Path, text: &str, params: &Value) -> Option<Value> {
    // the import runs a host tool: not for the requests of cursor moves
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
        Err(reason) => Some(json!({
            "title": format!("Regenerate from {}", header.package),
            "kind": "refactor.rewrite",
            "disabled": { "reason": reason },
        })),
    }
}

/// Without backends, no `ffi import` runs.
#[cfg(not(feature = "wasm-backends"))]
fn generate(_: &Path, _: &Header) -> Result<(String, String), String> {
    Err("this build of flick runs no backend".into())
}

/// The package's provenance and what `ffi import` writes at `path`, run
/// for the directory that holds the package's.
#[cfg(feature = "wasm-backends")]
fn generate(path: &Path, header: &Header) -> Result<(String, String), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let found = project::discover(&FsHost::new(), &mut SourceMap::default(), dir, None);
    let project = found
        .map_err(|d| d.message)?
        .ok_or_else(|| "the file belongs to no project".to_owned())?;
    let relative = |p: &Path| {
        let parts: Vec<String> = (p.strip_prefix(&project.root).unwrap_or(p).components())
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        parts.join("/")
    };
    let out = relative(dir.parent().unwrap_or(dir));
    let mut words = ["ffi", "import", &header.package, "--out", &out]
        .map(str::to_owned)
        .to_vec();
    if let Some(select) = &header.select {
        words.extend(["--select".to_owned(), select.join(",")]);
    }
    let preview = contributed::preview(&project, &words)?;
    let at = relative(path);
    let (_, bytes) = (preview.files.into_iter().find(|(p, _)| *p == at))
        .ok_or_else(|| format!("`ffi import {}` writes no `{at}`", header.package))?;
    // the import's report begins with the package's provenance
    let report = String::from_utf8_lossy(&preview.stdout);
    let provenance = report.lines().next().unwrap_or_default();
    let provenance = provenance
        .split(" (")
        .next()
        .unwrap_or(provenance)
        .to_owned();
    Ok((provenance, String::from_utf8_lossy(&bytes).into_owned()))
}
