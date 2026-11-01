//! The traces of the e2e cases (X4): every run writes one with
//! `FLICK_TRACE` and the monitor of flick-model checks it against `λRT`;
//! for the cases of the runtime contract the projection of the JS run must
//! be that of the Go run (docs/formal/runtime.md §6.1).

use std::fs;
use std::path::Path;

use flick_model::rt::{monitor, parse, project};

/// The file a run writes its trace to.
pub const FILE: &str = "trace.jsonl";

/// Whether the JS and Go runs of a case, by its path under tests/run, must
/// project alike: the cases of the runtime contract. Elsewhere the
/// standard library of each runtime builds layers and adds finalizers of
/// its own (runtime.md §6.1).
pub fn compared(rel: &Path) -> bool {
    rel.starts_with("runtime")
}

/// The projection of the trace in `out_dir`, or the first step `λRT` does
/// not allow. A run that opened no owner, as a page that only renders,
/// writes none: nothing to check.
pub fn checked(out_dir: &Path) -> Result<String, String> {
    let text = fs::read_to_string(out_dir.join(FILE)).unwrap_or_default();
    let events = parse(&text)?;
    monitor::check(&events).map_err(|v| v.to_string())?;
    Ok(project::project(&events))
}

/// What differs between the projections of the JS and the Go run.
pub fn differ(js: &str, go: &str) -> Option<String> {
    (js != go).then(|| {
        format!(
            "  trace projection:\n    js:\n{}    go:\n{}",
            indent(js),
            indent(go)
        )
    })
}

fn indent(text: &str) -> String {
    text.lines().flat_map(|l| ["      ", l, "\n"]).collect()
}
