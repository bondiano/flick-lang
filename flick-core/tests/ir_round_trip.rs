//! The IR of the whole corpus survives its JSON and CBOR forms: every
//! built-in module, every program of `tests/run`, every passing fixture and
//! every example project, as lowered and as optimized for each target.

#![cfg(feature = "native-project")]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::slice;

use flick_core::build;
use flick_core::diagnostics::Severity;
use flick_core::ir;
use flick_core::project::{self, Host};
use flick_core::source::{FileId, SourceMap};
use flick_ir::Program;
use flick_test_support::files as fixture_files;

struct Files;

impl Host for Files {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

/// `modules` survive both forms, and each form writes the same again.
fn round_trips(what: &str, modules: Vec<ir::Module>, path: &dyn Fn(FileId) -> String) {
    let program = Program::new("flick test", modules, ir::lang_items(), path);
    let json = program.to_json();
    let back = Program::from_json(&json).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(back == program, "{what}: the JSON form loses something");
    assert_eq!(
        back.to_json(),
        json,
        "{what}: JSON is not written the same again"
    );
    let cbor = program.to_cbor();
    let back = Program::from_cbor(&cbor).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(back == program, "{what}: the CBOR form loses something");
    assert_eq!(
        back.to_cbor(),
        cbor,
        "{what}: CBOR is not written the same again"
    );
}

/// The modules of `roots` as lowered and as optimized, when they compile.
fn compiled(project: Option<&project::Project>, roots: &[PathBuf], what: &str) -> usize {
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = project::compile(&Files, &mut sources, project, roots);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return 0;
    }
    let compiled = &compilation.modules;
    let modules: Vec<ir::Module> = compiled.iter().map(|m| m.ir.clone()).collect();
    let optimized: Vec<ir::Module> = compiled
        .iter()
        .map(|m| build::optimized(compiled, m, false))
        .collect();
    let n = modules.len();
    let path = |f: FileId| sources.get(f).path.clone();
    round_trips(what, modules, &path);
    round_trips(&format!("{what}, optimized"), optimized, &path);
    n
}

#[test]
fn the_built_in_modules_round_trip() {
    let modules = build::builtin_modules().to_vec();
    assert!(modules.len() > 100);
    // each built-in module is checked in a source map of its own
    round_trips("the built-in modules", modules, &|f| {
        format!("builtin {}", f.index())
    });
}

#[test]
fn every_program_and_fixture_round_trips() {
    let mut paths = Vec::new();
    paths.extend(fixture_files(&repo("tests/run"), "fl"));
    paths.extend(fixture_files(&repo("tests/check/pass"), "fl"));
    assert!(paths.len() > 100);
    let mut modules = 0;
    for path in &paths {
        modules += compiled(None, slice::from_ref(path), &path.display().to_string());
    }
    assert!(modules > paths.len() / 2, "only {modules} modules compiled");
}

#[test]
fn every_example_project_round_trips() {
    for name in [
        "cli-tool",
        "first-slice",
        "hom-repository",
        "mtl",
        "observability",
        "typed-interpreter",
    ] {
        let dir = repo(&format!("examples/{name}"));
        let mut sources_map = SourceMap::default();
        let project = project::discover(&Files, &mut sources_map, &dir, None)
            .unwrap_or_else(|d| panic!("{name}: {}", d.message))
            .unwrap_or_else(|| panic!("{name}: no project"));
        let mut roots = Vec::new();
        for root in [project.source_root(), project.test_root()] {
            if root.is_dir() {
                roots.extend(fixture_files(&root, "fl"));
            }
        }
        assert!(
            compiled(Some(&project), &roots, name) > 0,
            "{name} does not compile"
        );
    }
}
