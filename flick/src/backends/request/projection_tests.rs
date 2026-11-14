use super::*;

use crate::backends::Selected;
use flick_backend::{
    BackendId, DeclaredInputs, JavascriptEntry, JavascriptProjection, Limits,
    TypescriptModuleProjection, validate_request,
};
use flick_core::diagnostics::Severity;
use std::io;
use std::path::Path;

fn prepared_fixture(path: &str) -> Prepared {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path);
    let (diagnostics, compilation) = project::compile(&host, &mut sources, None, &[root]);
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: "unused-projection-output".into(),
        entries: vec![],
        typescript: true,
        go: false,
        suites: vec![],
        extras: build::Extras::default(),
        release: false,
    };
    Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        BackendPin {
            identity: BackendId {
                id: "test/projections".into(),
                version: "1.0.0".into(),
            },
            sha256: flick_backend::sha256(b"test component"),
        },
    )
}

struct Text(&'static str);

impl project::Host for Text {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/fixture/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else if path.file_name().is_some_and(|n| n == "Child.fl") {
            Ok(b"module App.Child exposing (value)\nvalue : Int\nlet value = 42\n".to_vec())
        } else if path.file_name().is_some_and(|n| n == "Contracts.fl") {
            Ok(
                b"module App.Contracts exposing (Output)\ntype module Output =\n  value : Int\n"
                    .to_vec(),
            )
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

pub(super) fn prepared_prefix() -> Prepared {
    let text = Text(
        "module App.Main exposing (main)\nseed : Int\nlet seed = 41\nmodule Narrow : App.Contracts.Output = App.Child\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (Narrow.value + seed))\n",
    );
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = project::compile(
        &text,
        &mut sources,
        None,
        &[
            "/fixture/Main.fl".into(),
            "/fixture/Child.fl".into(),
            "/fixture/Contracts.fl".into(),
        ],
    );
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: "unused-projection-output".into(),
        entries: vec![("main".into(), "App.Main".into())],
        typescript: true,
        go: false,
        suites: vec![],
        extras: build::Extras::default(),
        release: false,
    };
    Prepared::new(
        &FsHost::new(),
        &mut sources,
        None,
        &compilation,
        &output,
        BackendPin {
            identity: BackendId {
                id: "test/projections".into(),
                version: "1.0.0".into(),
            },
            sha256: flick_backend::sha256(b"test component"),
        },
    )
}

#[test]
fn javascript_generation_needs_versioned_frontend_facts() {
    let prepared = prepared_fixture("tests/run/bootstrap/inline_nested.fl");
    assert!(
        prepared
            .request
            .projections
            .iter()
            .any(|p| { p.schema.name == "flick.javascript" && p.schema.version == "0.1.0" }),
        "the component needs entry classification and source-map paths without checker callbacks"
    );
}

#[test]
fn typescript_and_foreign_inputs_are_explicit_frontend_projections() {
    let prepared = prepared_fixture("tests/run/bootstrap/inline_nested.fl");
    let request = &prepared.request;
    assert!(
        !request.projections.is_empty(),
        "TypeScript generation cannot call the checker"
    );
    assert!(
        !request.foreign.is_empty(),
        "builtin foreign specifiers are resolved by the host"
    );
    assert!(
        request
            .target
            .options
            .iter()
            .any(|o| o.key == "js.effect-bridge"),
        "runtime ownership is explicit as well as Effect/RT mode"
    );
    let mut inputs = DeclaredInputs::new(&request.inputs, |id| prepared.bytes.get(id).cloned())
        .expect("declared side inputs");
    validate_request(request, &mut inputs, Limits::default())
        .expect("the full projection graph validates");
}

#[test]
fn a_logical_typescript_module_retains_its_physical_prefix_files() {
    let prepared = prepared_prefix();
    let again = prepared_prefix();
    assert_eq!(
        prepared.bytes, again.bytes,
        "frontend inputs are reproducible"
    );
    assert_eq!(prepared.request.projections, again.request.projections);
    assert_eq!(prepared.request.foreign, again.request.foreign);
    let projections: Vec<_> = prepared
        .request
        .projections
        .iter()
        .filter(|p| p.schema.name == "flick.typescript")
        .map(|p| {
            assert_eq!(p.schema.name, "flick.typescript");
            assert_eq!(p.schema.version, "0.2.0");
            let decoded: TypescriptModuleProjection =
                ciborium::from_reader(&prepared.bytes[&p.input][..]).expect("versioned TS files");
            (p.module.as_str(), decoded)
        })
        .collect();
    let files: Vec<_> = projections.iter().flat_map(|(_, p)| &p.files).collect();
    assert!(
        files.iter().any(|f| f.module.contains("$lexical$")),
        "captured prefixes keep separate physical output"
    );
    assert!(
        files
            .iter()
            .all(|f| Path::new(&f.output).extension().is_some_and(|e| e == "ts"))
    );
    let (_, main) = projections
        .iter()
        .find(|(name, _)| *name == "App.Main")
        .expect("main projection");
    let main = main
        .files
        .iter()
        .find(|f| f.module == "App.Main")
        .expect("root file");
    let annotation = main
        .projection
        .annotations
        .iter()
        .find(|a| a.key == "main")
        .expect("typed export");
    assert!(
        annotation.value.contains("Effect.Effect<undefined"),
        "{}",
        annotation.value
    );
    assert!(!annotation.value.contains("any"));
}

#[test]
fn javascript_facts_preserve_entry_rules_and_physical_source_paths() {
    let prepared = prepared_prefix();
    let projection = prepared
        .request
        .projections
        .iter()
        .find(|p| p.module == "App.Main" && p.schema.name == "flick.javascript")
        .expect("JS facts");
    let facts: JavascriptProjection =
        ciborium::from_reader(&prepared.bytes[&projection.input][..]).expect("facts CBOR");
    assert_eq!(facts.exports, ["main"]);
    assert_eq!(
        facts.entries,
        [JavascriptEntry {
            name: "main".into(),
            module: "App.Main".into(),
            pure: false
        }]
    );
    assert!(facts.files.iter().any(|f| f.module.contains("$lexical$")));
    let file = facts
        .files
        .iter()
        .find(|f| f.module == "App.Main")
        .expect("root file");
    assert_eq!(file.output, "App/Main.ts");
    assert_eq!(file.source_paths.len(), prepared.request.sources.len());
    let source = prepared
        .request
        .sources
        .iter()
        .position(|s| s.name == "/fixture/Main.fl")
        .expect("own source");
    assert!(
        file.source_paths[source].ends_with("/fixture/Main.fl"),
        "{}",
        file.source_paths[source]
    );
}
