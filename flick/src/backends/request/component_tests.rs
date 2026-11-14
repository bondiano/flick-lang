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
