//! Runs the Effect coverage linter on the repository and on fixtures.

mod surface;

#[cfg(feature = "ide")]
use std::collections::BTreeSet;
use std::fs;
#[cfg(feature = "ide")]
use std::io;
use std::path::{Path, PathBuf};

#[cfg(feature = "ide")]
use flick_core::iface::Decl;
#[cfg(feature = "ide")]
use flick_core::ir::{Expr, ExprKind, ItemKind};
#[cfg(feature = "ide")]
use flick_core::project::{self, Host};
#[cfg(feature = "ide")]
use flick_core::source::SourceMap;
#[cfg(feature = "ide")]
use flick_core::types::{self, Ref, Type};
use surface::{Manifest, Snapshot, Stdlib, lint};
#[cfg(feature = "ide")]
use surface::{Status, coverage};

#[cfg(feature = "ide")]
struct Fs;

#[cfg(feature = "ide")]
impl Host for Fs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn files(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir)
        .expect("readable manifest directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            files(&path, base, out);
        } else if path.extension().is_some_and(|e| e == "toml") {
            let rel = path.strip_prefix(base).expect("under the base");
            let text = fs::read_to_string(&path).expect("readable manifest file");
            out.push((rel.to_string_lossy().replace('\\', "/"), text));
        }
    }
}

#[cfg(feature = "ide")]
fn fl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir)
        .expect("readable test directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            fl_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

#[cfg(feature = "ide")]
/// Every `Module.name` the stdlib tests refer to, as resolved by the checker,
/// and every type their values have.
fn used_names() -> BTreeSet<String> {
    let mut paths = Vec::new();
    fl_files(&repo().join("tests/run/stdlib"), &mut paths);
    let mut used = BTreeSet::new();
    for path in paths {
        let mut sources = SourceMap::default();
        let analysis = project::analyze(&Fs, &mut sources, None, &[path]);
        for module in &analysis.modules {
            for r in module.typing.names.values() {
                match r {
                    Ref::Value { module, name } | Ref::Method { module, name } => {
                        used.insert(format!("{module}.{name}"));
                    }
                    Ref::Constructor(c) => {
                        used.insert(format!("{}.{}", c.module, c.name));
                    }
                    Ref::Operation { service, name, .. } => {
                        used.insert(format!("{service}.{name}"));
                    }
                    Ref::ServiceTag(service) => {
                        used.insert(service.clone());
                    }
                    Ref::Local | Ref::PackedField { .. } => {}
                }
            }
            for (module, name) in module.typing.type_names.values() {
                used.insert(format!("{module}.{name}"));
            }
            // a type is used by a value of it too, written or inferred
            for ty in module.typing.types.values() {
                ty.visit(&mut |t| {
                    if let Type::Con { module, name } = t {
                        used.insert(format!("{module}.{name}"));
                    }
                });
            }
        }
        // and by the code plugins gave, which only the IR holds
        for module in &analysis.compilation.modules {
            for item in &module.ir.items {
                if let ItemKind::Value { body, .. } = &item.kind {
                    globals(body, &mut used);
                }
            }
        }
    }
    used
}

#[cfg(feature = "ide")]
fn globals(e: &Expr, used: &mut BTreeSet<String>) {
    if let ExprKind::Global { module, name } = &e.kind {
        used.insert(format!("{module}.{name}"));
    }
    e.each_child(&mut |child| globals(child, used));
}

#[cfg(feature = "ide")]
/// The export `Module.name` of a built-in module: the module that defines
/// it (a facade re-exports the names of its parts) and its declaration.
fn export(target: &str) -> Option<(String, Decl)> {
    let (module, name) = target.rsplit_once('.')?;
    let interface = types::builtins().iter().find(|i| i.module == module)?;
    let e = interface.exports.iter().find(|e| e.entity.name == name)?;
    Some((format!("{}.{name}", e.entity.module), e.decl))
}

#[cfg(feature = "ide")]
fn pinned() -> String {
    let text = fs::read_to_string(repo().join("packages/effect-ts/package.json"))
        .expect("packages/effect-ts/package.json");
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid package.json");
    json["dependencies"]["effect"]
        .as_str()
        .expect("the adapter pins effect")
        .to_owned()
}

#[cfg(feature = "ide")]
#[test]
fn the_manifest_classifies_the_pinned_effect() {
    let dir = repo().join("packages/effect-ts/coverage");
    let snapshot: Snapshot = serde_json::from_str(
        &fs::read_to_string(dir.join("snapshot.json"))
            .expect("packages/effect-ts/coverage/snapshot.json"),
    )
    .expect("a valid snapshot");
    let mut texts = Vec::new();
    let base = dir.join("surface");
    files(&base, &base, &mut texts);
    let manifest = Manifest::parse(&texts);
    let used = used_names();
    let is_used = |t: &str| export(t).is_some_and(|(defined, _)| used.contains(&defined));
    let exists = |t: &str| export(t).is_some();
    let stdlib = Stdlib {
        exists: &exists,
        used: &is_used,
    };
    let report = coverage(&snapshot, &manifest);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("effect-coverage.md");
    fs::write(&out, &report).expect("writable target directory");
    let mut problems = lint(&snapshot, &manifest, &stdlib, &pinned());
    // every wrapped entrypoint is decided export by export for the release
    problems.extend(
        manifest
            .modules
            .iter()
            .flat_map(|((dir, module), exports)| {
                exports
                    .iter()
                    .filter_map(move |(name, status)| match status {
                        Status::Pending(stage) => {
                            Some(format!("{dir}/{module}.{name}: still pending ({stage})"))
                        }
                        _ => None,
                    })
            }),
    );
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// A snapshot of a fake `effect` with the module `Sched`.
const SNAPSHOT: &str = r#"{
  "package": "effect", "version": "9.9.9",
  "entrypoints": ["effect", "effect/ai"],
  "modules": { "effect": {
    "Sched.spaced": "value 1111111111111111",
    "Sched.forever": "value 2222222222222222",
    "Sched.Input": "type 3333333333333333",
    "Sched.unsafeRun": "value 4444444444444444"
  } }
}"#;

const ENTRYPOINTS: &str = "\"effect\" = { modules = \"core\" }\n\"effect/ai\" = { excluded = \"not wrapped in 0.1.0\" }\n";

const MODULE: &str = "[wrapped]\nspaced = \"Sched.spaced 1111111111111111\"\nforever = \"Sched.forever 2222222222222222\"\n[excluded]\nInput = \"TS-only\"\nunsafeRun = \"unsafe\"\n";

fn fixture(module: &str, snapshot: &str) -> Vec<String> {
    let snapshot: Snapshot = serde_json::from_str(snapshot).expect("fixture snapshot");
    let manifest = Manifest::parse(&[
        ("entrypoints.toml".to_owned(), ENTRYPOINTS.to_owned()),
        ("core/Sched.toml".to_owned(), module.to_owned()),
    ]);
    let exists = |t: &str| t.starts_with("Sched.") && t != "Sched.gone";
    let used = |t: &str| t != "Sched.unused";
    let stdlib = Stdlib {
        exists: &exists,
        used: &used,
    };
    lint(&snapshot, &manifest, &stdlib, "9.9.9")
}

#[test]
fn a_complete_manifest_passes() {
    assert_eq!(fixture(MODULE, SNAPSHOT), Vec::<String>::new());
}

#[test]
fn an_unclassified_export_fails() {
    let module = MODULE.replace("unsafeRun = \"unsafe\"\n", "");
    assert_eq!(
        fixture(&module, SNAPSHOT),
        ["Sched.unsafeRun: not classified"]
    );
}

#[test]
fn an_entry_for_a_missing_export_fails() {
    let module = format!("{MODULE}gone = \"removed\"\n");
    assert_eq!(fixture(&module, SNAPSHOT), ["Sched.gone: no such export"]);
}

#[test]
fn a_changed_signature_of_a_wrapped_export_fails() {
    let snapshot = SNAPSHOT.replace("value 2222222222222222", "value 2222222222222299");
    assert_eq!(
        fixture(MODULE, &snapshot),
        [
            "Sched.forever: the signature changed (fingerprint 2222222222222299, reviewed 2222222222222222): review `Sched.forever`"
        ]
    );
}

#[test]
fn a_wrapper_missing_from_the_stdlib_fails() {
    let module = MODULE.replace("\"Sched.forever 2222", "\"Sched.gone 2222");
    assert_eq!(
        fixture(&module, SNAPSHOT),
        ["Sched.forever: wrapped by `Sched.gone`, which the standard library does not export"]
    );
}

#[test]
fn a_wrapper_no_test_uses_fails() {
    let module = MODULE.replace("\"Sched.forever 2222", "\"Sched.unused 2222");
    assert_eq!(
        fixture(&module, SNAPSHOT),
        ["Sched.forever: no stdlib test uses `Sched.unused`"]
    );
}

#[test]
fn pending_needs_a_reason() {
    let module = MODULE
        .replace(
            "[excluded]\n",
            "[pending]\nInput = \"a parser of its own\"\nunsafeRun = \" \"\n[excluded]\n",
        )
        .replace("Input = \"TS-only\"\nunsafeRun = \"unsafe\"\n", "");
    assert_eq!(
        fixture(&module, SNAPSHOT),
        ["Sched.unsafeRun: pending without a reason"]
    );
}

#[test]
fn a_new_entrypoint_and_a_double_decision_fail() {
    let snapshot = SNAPSHOT.replace("\"effect/ai\"]", "\"effect/ai\", \"effect/new\"]");
    let module = format!("{MODULE}[language]\nspaced = \"a direct call\"\n");
    assert_eq!(
        fixture(&module, &snapshot),
        [
            "Sched.spaced: classified twice",
            "entrypoint `effect/new`: not classified"
        ]
    );
}

#[test]
fn a_whole_module_can_be_excluded() {
    let module = "[excluded]\n\"*\" = \"TS-only module\"\n";
    assert_eq!(fixture(module, SNAPSHOT), Vec::<String>::new());
}

#[test]
fn after_effect_bump_the_linter_is_red_until_review() {
    let fixtures = repo().join("tools/dts-importer/test/fixtures");
    let snapshot: Snapshot = serde_json::from_str(
        &fs::read_to_string(fixtures.join("effect-1.1.0.snapshot.json")).expect("bumped snapshot"),
    )
    .expect("a valid snapshot");
    let base = fixtures.join("repo/packages/effect-ts/coverage/surface");
    let mut texts = Vec::new();
    files(&base, &base, &mut texts);
    let manifest = Manifest::parse(&texts);
    let any = |t: &str| t.starts_with("Sched.");
    let stdlib = Stdlib {
        exists: &any,
        used: &any,
    };
    assert_eq!(
        lint(&snapshot, &manifest, &stdlib, "1.1.0"),
        [
            "entrypoint `effect/extra`: not classified",
            "Sched.added: not classified",
            "Sched.forever: the signature changed (fingerprint 2a307640e3841503, reviewed 45883391242a65f7): review `Sched.forever`",
            "Effect.sync: no such export",
            "Sched.removed: no such export"
        ]
    );
}
