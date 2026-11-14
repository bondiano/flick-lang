use super::*;

use crate::backends::Selected;
use flick_backend::{BackendId, DeclaredInputs, Limits, validate_request};
use flick_core::diagnostics::{Severity, codes};
use flick_core::project::config;
use std::path::{Path, PathBuf};
use std::{fs, io};

struct Overlay {
    file: PathBuf,
    text: String,
}
impl project::Host for Overlay {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == self.file {
            Ok(self.text.as_bytes().to_vec())
        } else {
            fs::read(path)
        }
    }
    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn library(missing: bool) -> Prepared {
    library_outputs(missing, true, build::Extras::default())
}

fn library_outputs(missing: bool, typescript: bool, extras: build::Extras) -> Prepared {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/backend-inputs");
    let file = dir.join("Library.fl");
    let mut text = fs::read_to_string(&file).expect("foreign fixture");
    if missing {
        text = text.replace("./helper.mjs", "./missing.mjs");
    }
    let mut sources = SourceMap::default();
    let (diagnostics, mut compilation) = project::compile(
        &Overlay {
            file: file.clone(),
            text,
        },
        &mut sources,
        None,
        &[file],
    );
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    for module in &mut compilation.modules {
        module.package = Some("fixture-library".into());
    }
    compilation.libraries.push(("fixture-library".into(), dir));
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: "unused-assets-output".into(),
        entries: vec![],
        typescript,
        go: false,
        suites: vec![],
        extras,
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
                id: "test/assets".into(),
                version: "1.0.0".into(),
            },
            sha256: flick_backend::sha256(b"test component"),
        },
    )
}

#[test]
fn library_foreign_bytes_are_declared_and_shipped_by_the_shared_host() {
    let prepared = library(false);
    assert!(
        prepared.diagnostics.is_empty(),
        "{:?}",
        prepared.diagnostics
    );
    let file = prepared
        .shared
        .iter()
        .find(|a| a.path == "helper.mjs")
        .expect("shared foreign file");
    assert_eq!(file.contents, b"export const inc = (n) => (n + 1) | 0;\n");
    let binding = prepared
        .request
        .foreign
        .iter()
        .find(|f| f.module == "Library" && f.specifier == "./helper.mjs")
        .expect("resolved binding");
    assert_eq!(binding.resolved, "./helper.mjs");
    assert!(!binding.inputs.is_empty());
    assert!(
        binding
            .inputs
            .iter()
            .any(|id| prepared.bytes[id] == file.contents)
    );
    for id in &binding.inputs {
        assert!(
            prepared
                .request
                .inputs
                .iter()
                .any(|i| i.id == *id && i.kind == InputKind::Foreign)
        );
    }
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("side input digests");
    validate_request(&prepared.request, &mut inputs, Limits::default())
        .expect("closed foreign graph");
}

#[test]
fn javascript_declarations_and_facades_remain_shared_host_outputs() {
    let prepared = library_outputs(
        false,
        false,
        build::Extras {
            dts: true,
            promise_facade: true,
            ..build::Extras::default()
        },
    );
    assert!(
        prepared.diagnostics.is_empty(),
        "{:?}",
        prepared.diagnostics
    );
    assert!(prepared.request.projections.is_empty());
    let text = |path: &str| {
        String::from_utf8(
            prepared
                .shared
                .iter()
                .find(|a| a.path == path)
                .unwrap_or_else(|| panic!("missing shared output {path}"))
                .contents
                .clone(),
        )
        .expect("shared declaration is UTF-8")
    };
    let declarations = text("Library.d.ts");
    assert!(declarations.contains("inc:"), "{declarations}");
    assert!(declarations.contains("number"), "{declarations}");
    assert!(
        declarations.contains("Effect.Effect<number"),
        "{declarations}"
    );
    assert!(!declarations.contains("any"), "{declarations}");
    let facade = text("Library.promise.d.ts");
    assert!(
        facade.contains("Promise<$Result.Result<number, never>>"),
        "{facade}"
    );
    let js = text("Library.promise.js");
    assert!(js.contains("\"./Library.js\""), "{js}");
    assert!(js.contains("promiseFacade($m.inc, 1)"), "{js}");
    assert!(!text("Library.iface").is_empty());
}

#[test]
fn a_missing_library_binding_stops_frontend_preparation() {
    let prepared = library(true);
    assert!(
        prepared
            .diagnostics
            .iter()
            .any(|d| d.code == codes::P041 && d.message.contains("missing.mjs")),
        "{:?}",
        prepared.diagnostics
    );
    assert!(
        prepared.request.projections.is_empty(),
        "unavailable files do not become usable projections"
    );
}

#[test]
fn a_project_runtime_setting_reaches_the_component_without_a_cli_override() {
    let host = FsHost::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/backend-inputs");
    let mut sources = SourceMap::default();
    let text = "schema-version = 1\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n[build]\neffect = \"rt\"\n";
    let config_file = sources.add("fixture/flick.toml", text);
    let project = Project {
        root: dir.clone(),
        config: config::parse(text, config_file).expect("RT project"),
        config_file,
        workspace: None,
    };
    let (diagnostics, compilation) = project::compile(
        &host,
        &mut sources,
        Some(&project),
        &[dir.join("Library.fl")],
    );
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: "unused-runtime-output".into(),
        entries: vec![],
        typescript: true,
        go: false,
        suites: vec![],
        extras: build::Extras::default(),
        release: false,
    };
    assert!(!output.extras.rt, "this exercises project configuration");
    let prepared = Prepared::new(
        &host,
        &mut sources,
        Some(&project),
        &compilation,
        &output,
        BackendPin {
            identity: BackendId {
                id: "test/runtime".into(),
                version: "1.0.0".into(),
            },
            sha256: flick_backend::sha256(b"test component"),
        },
    );
    assert!(
        prepared
            .request
            .target
            .options
            .iter()
            .any(|o| o.key == "js.runtime" && o.value == "rt")
    );
}
