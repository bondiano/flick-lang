//! Support helpers for [`super`]: filesystem host, doc expectation metadata,
//! and temporary JS compilation for self-spawning RPC fixtures (`process.argv[1]`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io, slice};

use flick_core::build::{self, Backend, Extras, Settings};
use flick_core::codegen::EffectBridge;
use flick_core::diagnostics::Severity;
use flick_core::project;
use flick_core::source::{FileId, SourceMap};
use flick_model::memory::{Heap, ProgramOutcome, Reason};

pub(super) struct FsHost;

impl project::Host for FsHost {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

pub(super) fn load_doc_expectations(root: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Ok(raw) = fs::read_to_string(root.join("docs/tour/index.json"))
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(chapters) = json.get("chapters").and_then(serde_json::Value::as_array)
    {
        for ch in chapters {
            let Some(cid) = ch.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if let Some(lessons) = ch.get("lessons").and_then(serde_json::Value::as_array) {
                for l in lessons {
                    if let (Some(id), Some(exp)) = (
                        l.get("id").and_then(serde_json::Value::as_str),
                        l.get("expect").and_then(serde_json::Value::as_str),
                    ) {
                        map.insert(format!("docs/tour/{cid}/{id}.fl"), exp.to_owned());
                    }
                }
            }
        }
    }
    if let Ok(raw) = fs::read_to_string(root.join("docs/src/playground/examples/index.json"))
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(groups) = json.get("groups").and_then(serde_json::Value::as_array)
    {
        for g in groups {
            if let Some(examples) = g.get("examples").and_then(serde_json::Value::as_array) {
                for ex in examples {
                    if let (Some(id), Some(exp)) = (
                        ex.get("id").and_then(serde_json::Value::as_str),
                        ex.get("expect").and_then(serde_json::Value::as_str),
                    ) {
                        map.insert(
                            format!("docs/src/playground/examples/{id}.fl"),
                            exp.to_owned(),
                        );
                    }
                }
            }
        }
    }
    map
}

/// For fixtures whose sibling `.mjs` reads `process.argv[1]` to self-spawn a
/// child server (`stdlib/rpc/{stdio,worker}`), emits the fixture's JS entry
/// inside `target/tmp/counted-self-spawn/` so Node resolves workspace packages.
pub(super) fn compile_self_spawn_entry(
    repo: &Path,
    path: &Path,
    label: &str,
    entry_module: &str,
    release: bool,
) -> Option<PathBuf> {
    let helper = fs::read_to_string(path.with_extension("mjs")).ok()?;
    if !helper.contains("process.argv[1]") {
        return None;
    }
    let canonical = project::normalize(&fs::canonicalize(path).ok()?);
    let mut sources = SourceMap::default();
    let (diags, compilation) =
        project::compile(&FsHost, &mut sources, None, slice::from_ref(&canonical));
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return None;
    }
    let sanitized = label.replace(['/', '\\', ' '], "_");
    let profile = if release { "release" } else { "debug" };
    let out_dir = repo
        .join("target/tmp/counted-self-spawn")
        .join(format!("{sanitized}_{profile}"));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).ok()?;
    let out_dir = project::normalize(&fs::canonicalize(&out_dir).ok()?);
    let entries = [("main".to_owned(), entry_module.to_owned())];
    let source_path = |_: FileId| format!("file://{}", canonical.display());
    let source_dir = |_: &str| canonical.parent().map(Path::to_path_buf);
    let settings = Settings {
        package: None,
        entries: &entries,
        typescript: false,
        release,
        extras: Extras::default(),
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: &out_dir,
        root: None,
        backend: Backend::Effect,
        effect_bridge: EffectBridge::Available,
        ui: None,
        runners: &[],
        environment: None,
    };
    let (artifacts, problems) = build::emit(&compilation, &sources, &settings);
    if problems.iter().any(|d| d.severity == Severity::Error) {
        return None;
    }
    for artifact in artifacts {
        let dest = out_dir.join(&artifact.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok()?;
        }
        fs::write(&dest, artifact.contents).ok()?;
    }
    fs::write(out_dir.join("package.json"), "{\"type\":\"module\"}\n").ok()?;
    Some(out_dir.join(build::runner_path(false)))
}

pub(super) fn describe_outcome_detail(heap: &Heap, outcome: &ProgramOutcome) -> Option<String> {
    match outcome {
        ProgramOutcome::Fault(err) => Some(err.to_string()),
        ProgramOutcome::Failed(cause) => {
            let parts: Vec<String> = heap
                .cause_reasons(cause)
                .ok()
                .into_iter()
                .flatten()
                .map(|r| match r {
                    Reason::Die(v) => heap
                        .defect(v)
                        .map_or_else(|_| "Die(?)".into(), |d| format!("Die({:?})", d.code())),
                    Reason::Fail(v) => heap
                        .tag(v)
                        .map_or_else(|_| "Fail(?)".into(), |t| format!("Fail({t})")),
                    Reason::Interrupt { .. } => "Interrupt".into(),
                })
                .collect();
            Some(parts.join(", "))
        }
        ProgramOutcome::Raised(v) => Some(
            heap.tag(v)
                .map_or_else(|_| "Raised(?)".into(), |t| format!("Raised({t})")),
        ),
        ProgramOutcome::Returned(_) => None,
    }
}
