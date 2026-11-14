//! The official emitter consumes the public request, independently of core.

use super::*;
use crate::backends::Selected;
use crate::commands::build::settings::with_settings;
#[cfg(feature = "wasm-backends")]
use crate::component;
use flick_backend::{Backend, BackendId, DeclaredInputs, Limits, Policy, generate};
use flick_backend_js::Javascript;
use flick_core::diagnostics::Severity;
#[cfg(feature = "wasm-backends")]
use std::fs;
use std::path::Path;
use std::str;

fn fixture(typescript: bool, release: bool, rt: bool) -> (Prepared, Vec<build::Artifact>) {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/bootstrap/inline_nested.fl");
    let (diagnostics, compilation, _) =
        project::compile_with(&host, &mut sources, None, &[root], None, rt, None);
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: "unused-component-output".into(),
        entries: vec![(
            "main".into(),
            compilation
                .modules
                .iter()
                .find(|m| m.interface.exports_value("main"))
                .expect("entry module")
                .name
                .clone(),
        )],
        typescript,
        go: false,
        suites: vec![],
        extras: build::Extras {
            rt,
            ..build::Extras::default()
        },
        release,
    };
    let (native, diagnostics) = with_settings(&host, None, &compilation, &output, |settings| {
        build::emit(&compilation, &sources, settings)
    });
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        BackendPin {
            identity: BackendId {
                id: "flick/js".into(),
                version: "0.1.0".into(),
            },
            sha256: flick_backend::sha256(b"native parity"),
        },
    );
    assert!(
        prepared.diagnostics.is_empty(),
        "{:?}",
        prepared.diagnostics
    );
    (prepared, native)
}

fn parity(backend: &dyn Backend, prepared: &Prepared, native: &[build::Artifact]) {
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("closed inputs");
    backend
        .check_target(&prepared.request, &mut inputs)
        .expect("supported target");
    let result = backend
        .generate(&prepared.request, &mut inputs)
        .expect("generated artifacts");
    let shared: Vec<_> = prepared.shared.iter().map(|f| f.path.as_str()).collect();
    let expected: BTreeMap<_, _> = native
        .iter()
        .filter(|f| !shared.contains(&f.path.as_str()))
        .map(|f| (f.path.as_str(), f.contents.as_bytes()))
        .collect();
    let actual: BTreeMap<_, _> = result
        .artifacts
        .iter()
        .map(|f| (f.path.as_str(), f.bytes.as_slice()))
        .collect();
    assert_eq!(
        expected.keys().collect::<Vec<_>>(),
        actual.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in expected {
        assert_eq!(bytes, actual[path], "{path}");
    }
    assert!(result.steps.is_empty());
}

#[test]
fn official_js_native_uses_only_the_public_request_in_both_profiles() {
    for rt in [false, true] {
        for typescript in [false, true] {
            for release in [false, true] {
                let (prepared, native) = fixture(typescript, release, rt);
                parity(&Javascript, &prepared, &native);
            }
        }
    }
}

#[test]
fn official_js_refuses_invalid_spans_without_the_host_driver() {
    let (prepared, _) = fixture(false, false, false);
    let program = flick_ir::Program::from_cbor(&prepared.request.program).expect("IR");
    let module = program
        .modules
        .iter()
        .position(|m| m.name == "App.Main")
        .expect("own module");
    let file = program.modules[module].items[0].span.file.index() as usize;
    for start in [1, 3] {
        let mut request = prepared.request.clone();
        let mut bytes = prepared.bytes.clone();
        let source_id = request.sources[file].input.clone();
        bytes.insert(source_id.clone(), "é".as_bytes().to_vec());
        for source in &mut request.sources {
            if source.input == source_id {
                source.line_starts = vec![0];
            }
        }
        let input = request
            .inputs
            .iter_mut()
            .find(|i| i.id == source_id)
            .expect("source input");
        input.size = 2;
        input.sha256 = flick_backend::sha256("é".as_bytes());
        let mut program = program.clone();
        let item = &mut program.modules[module].items[0];
        item.span.start = start;
        item.span.end = start;
        request.program = program.to_cbor();
        for generate in [false, true] {
            let mut inputs =
                DeclaredInputs::new(&request.inputs, |id| bytes.get(id).cloned()).expect("inputs");
            let errors = if generate {
                Javascript
                    .generate(&request, &mut inputs)
                    .expect_err("invalid span before generation")
            } else {
                Javascript
                    .check_target(&request, &mut inputs)
                    .expect_err("invalid span in preflight")
            };
            assert!(
                errors.iter().any(|d| d.message.contains("span")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn official_js_classifies_barrel_exports_independently_of_output_paths() {
    for (path, builtin, exports) in [
        ("moved/Ordinary.js", false, true),
        ("_flick/Relocated.js", false, true),
        ("moved/BuiltinReplacement.js", true, false),
    ] {
        let (mut prepared, _) = fixture(false, false, false);
        let mut program = flick_ir::Program::from_cbor(&prepared.request.program).expect("IR");
        let module = program
            .modules
            .iter_mut()
            .find(|m| m.name == "App.Main")
            .expect("own module");
        module.exports.reexports.push(flick_ir::Reexport {
            module: "Platform".into(),
            namespace: flick_ir::Namespace::Value,
            name: "stdout_line".into(),
        });
        prepared.request.program = program.to_cbor();
        let meta = prepared
            .request
            .modules
            .iter_mut()
            .find(|m| m.name == "App.Main")
            .expect("metadata");
        meta.output = path.into();
        let projection = prepared
            .request
            .projections
            .iter()
            .find(|p| p.module == "App.Main" && p.schema.name == "flick.javascript")
            .expect("JS facts");
        let mut facts: flick_backend::JavascriptProjection =
            ciborium::from_reader(&prepared.bytes[&projection.input][..]).expect("facts");
        facts.builtin = builtin;
        facts
            .files
            .iter_mut()
            .find(|f| f.module == "App.Main")
            .expect("root output")
            .output
            .clone_from(&meta.output);
        let mut bytes = Vec::new();
        ciborium::into_writer(&facts, &mut bytes).expect("facts encoding");
        let input = prepared
            .request
            .inputs
            .iter_mut()
            .find(|i| i.id == projection.input)
            .expect("facts input");
        input.sha256 = flick_backend::sha256(&bytes);
        input.size = u64::try_from(bytes.len()).expect("facts size");
        prepared.bytes.insert(projection.input.clone(), bytes);
        let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
            prepared.bytes.get(id).cloned()
        })
        .expect("inputs");
        let result = Javascript
            .generate(&prepared.request, &mut inputs)
            .expect("relocated module");
        let output = result
            .artifacts
            .iter()
            .find(|a| a.path == path)
            .expect("requested output");
        let text = str::from_utf8(&output.bytes).expect("JS");
        assert_eq!(
            text.contains("export { stdout_line } from"),
            exports,
            "{path}: {text}"
        );
    }
}

#[cfg(feature = "wasm-backends")]
#[test]
#[ignore = "build the official JS component first"]
fn official_js_component_matches_native_artifacts() {
    use flick_backend::host::{Budget, WasmBackend};
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/backends/js.wasm");
    let bytes = fs::read(path).expect("built JS component");
    for rt in [false, true] {
        for typescript in [false, true] {
            for release in [false, true] {
                let (mut prepared, native) = fixture(typescript, release, rt);
                prepared.request.backend.sha256 = flick_backend::sha256(&bytes);
                let backend = WasmBackend::load(
                    component::engine().expect("engine"),
                    prepared.request.backend.clone(),
                    &bytes,
                    Budget::default(),
                )
                .expect("JS component imports");
                parity(&backend, &prepared, &native);
            }
        }
    }
}

#[test]
fn official_js_rejects_missing_or_incompatible_facts_during_preflight() {
    let (mut prepared, _) = fixture(false, false, false);
    let at = prepared
        .request
        .projections
        .iter()
        .position(|p| p.schema.name == "flick.javascript")
        .expect("JS facts");
    prepared.request.projections[at].schema.version = "99.0.0".into();
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("inputs");
    let errors = Javascript
        .check_target(&prepared.request, &mut inputs)
        .expect_err("unknown schema is refused");
    assert!(errors.iter().any(|d| {
        d.message
            .contains("unsupported projection flick.javascript@99.0.0")
    }));
    prepared.request.projections.remove(at);
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("inputs");
    let errors = Javascript
        .check_target(&prepared.request, &mut inputs)
        .expect_err("facts are required");
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("missing flick.javascript@0.1.0"))
    );
}

#[test]
fn official_js_per_module_output_keeps_the_complete_reference_graph() {
    let mut prepared = super::projection_tests::prepared_prefix();
    prepared.request.target.profile = Profile::Release;
    prepared.request.backend.identity = Javascript.describe().identity;
    let own: Vec<_> = prepared
        .request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own)
        .map(|m| (m.name.clone(), m.output.clone()))
        .collect();
    assert!(
        own.len() > 1,
        "nested module fixture exercises graph context"
    );
    let root = prepared.request.entries[0].symbol.module.clone();
    prepared.request.target.mode = Mode::PerModule;
    prepared.request.module_root = Some(root.clone());
    let reserved: Vec<_> = prepared.shared.iter().map(|a| a.path.clone()).collect();
    let policy = Policy {
        toolchains: &[],
        environment: &[],
        reserved_paths: &reserved,
        limits: Limits::default(),
        authorize_step: &|_| Err("no tools".into()),
    };
    let generated = generate(
        &Javascript,
        &prepared.request,
        &policy,
        |id| prepared.bytes.get(id).cloned(),
        true,
    )
    .expect("validated reproducible request/result");
    let paths: Vec<_> = generated
        .result()
        .artifacts
        .iter()
        .map(|a| a.path.as_str())
        .collect();
    for (name, output) in own {
        assert_eq!(
            paths.contains(&output.as_str()),
            name == root,
            "{name}: {paths:?}"
        );
    }
}
