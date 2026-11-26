mod common;

use flick_backend::{
    Artifact, ArtifactKind, BuildResult, DeclaredInputs, Input, InputKind, Limits, Mode,
    ModuleInput, ModuleRole, Policy, sha256, validate_request, validate_result,
};

#[test]
fn component_abi_has_a_reviewable_golden() {
    let mut resolve = wit_parser::Resolve::default();
    let (package, _) = resolve
        .push_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/wit"))
        .expect("WIT");
    let mut printer = wit_component::WitPrinter::default();
    printer
        .emit_docs(false)
        .print(&resolve, package, &[])
        .expect("WIT schema");
    insta::assert_snapshot!("backend_abi", printer.output.to_string());
    assert_eq!(
        resolve.packages[package]
            .name
            .version
            .as_ref()
            .expect("version")
            .to_string(),
        flick_backend::VERSION
    );
}

#[test]
fn native_records_have_reviewable_schema_goldens() {
    let mut request = serde_json::to_value(common::request()).expect("request JSON");
    request["program"] = "CBOR portable IR".into();
    insta::assert_snapshot!(
        "backend_request",
        serde_json::to_string_pretty(&request).expect("JSON")
    );
    insta::assert_snapshot!(
        "backend_result",
        serde_json::to_string_pretty(&common::result()).expect("JSON")
    );
}

#[test]
fn per_module_records_have_reviewable_schema_goldens() {
    let dep_iface = b"dep-iface-v1";
    let mut request = common::request();
    request.target.mode = Mode::PerModule;
    request.module_root = Some("App".into());
    request.modules.push(ModuleInput {
        name: "Dep".into(),
        role: ModuleRole::External,
        package: Some("dep".into()),
        output: "Dep.js".into(),
        interface: Some("iface/Dep".into()),
    });
    request.inputs.push(Input {
        id: "iface/Dep".into(),
        kind: InputKind::Interface,
        sha256: sha256(dep_iface),
        size: dep_iface.len() as u64,
    });
    let mut inputs = DeclaredInputs::new(&request.inputs, |id| {
        Some(if id == "iface/Dep" {
            dep_iface.to_vec()
        } else {
            common::TEXT.as_bytes().to_vec()
        })
    })
    .expect("per-module inputs");
    validate_request(&request, &mut inputs, Limits::default())
        .expect("per-module golden request is valid");

    let result = BuildResult {
        artifacts: vec![
            Artifact {
                id: "module:App".into(),
                path: "App.js".into(),
                kind: ArtifactKind::Source,
                bytes:
                    b"import * as Dep from \"./Dep.js\";\nexport const main = (_) => Dep.ok();\n"
                        .to_vec(),
            },
            Artifact {
                id: "map:App".into(),
                path: "App.js.map".into(),
                kind: ArtifactKind::SourceMap,
                bytes: b"{\"version\":3,\"sources\":[\"App.fl\"],\"mappings\":\"AAAA\"}".to_vec(),
            },
        ],
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    };
    let policy = Policy {
        toolchains: &request.toolchains,
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
        authorize_step: &common::authorize,
        authorize_launch: &common::authorize_launch,
    };
    validate_result(&request, &result, &policy, &mut inputs)
        .expect("per-module golden result is valid");

    let mut request_json = serde_json::to_value(&request).expect("per-module request JSON");
    request_json["program"] = "CBOR portable IR".into();
    insta::assert_snapshot!(
        "backend_request_per_module",
        serde_json::to_string_pretty(&request_json).expect("JSON")
    );
    insta::assert_snapshot!(
        "backend_result_per_module",
        serde_json::to_string_pretty(&result).expect("JSON")
    );
}
