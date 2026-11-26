//! Contract probes: the existing emitters consume just IR and plain data.
//! This is not the production driver adapter or the official component yet.
mod common;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str;

use flick_backend::{
    Abi, Artifact, ArtifactKind, BuildResult, DeclaredInputs, Input, InputKind, Inputs, Limits,
    Mode, ModuleInput, ModuleRole, OptionValue, Policy, Projection, TypescriptFile,
    TypescriptModuleProjection, TypescriptProjection, sha256, validate_request, validate_result,
};
use flick_core::codegen::go::{Bindings, Input as GoInput};
use flick_core::codegen::ts::Typed;
use flick_core::codegen::{self, EffectBridge, Known, Labels, Options};
use flick_core::source::SourceMap;
use flick_ir::{Expr, ExprKind, FileId, Item, ItemKind, Lit, Module, Param, Pat, Program, Stage};

#[test]
fn js_ts_and_maps_use_ir_and_plain_data_without_compilation_or_settings() {
    let mut request = common::request();
    let projected = typescript_projection();
    request.inputs.push(Input {
        id: "ts/App".into(),
        kind: InputKind::TargetFacts,
        sha256: sha256(&projected),
        size: projected.len() as u64,
    });
    request.projections.push(Projection {
        module: "App".into(),
        schema: Abi {
            name: "flick.typescript".into(),
            version: "0.2.0".into(),
        },
        input: "ts/App".into(),
    });
    let mut inputs = flick_backend::DeclaredInputs::new(&request.inputs, |id| {
        Some(if id == "ts/App" {
            projected.clone()
        } else {
            common::TEXT.as_bytes().to_vec()
        })
    })
    .expect("plain inputs");
    let program = Program::from_cbor(&request.program).expect("portable IR");
    let module = &program.modules[0];
    let known = Known::of(&program.lang);
    let packages: BTreeMap<_, _> = request
        .modules
        .iter()
        .filter_map(|m| Some((m.name.clone(), m.package.clone()?)))
        .collect();
    let options = Options {
        builtins: &[],
        paths: None,
        packages: &packages,
        resolved: &[],
        typescript: None,
        release: false,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::None,
        rt: true,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };
    let js = codegen::js(module, &options);
    // codegen.md specifies Unit's JS ABI as undefined, and the function itself is pure.
    assert!(js.code.contains("=> undefined"), "{}", js.code);
    assert!(!js.code.contains("yield"));
    let projection: TypescriptModuleProjection =
        ciborium::from_reader(&inputs.read("ts/App").expect("projection")[..]).expect("TS data");
    let types: BTreeMap<_, _> = projection
        .files
        .into_iter()
        .map(|file| {
            assert_eq!(file.output, "App.ts");
            let projection = file.projection;
            (
                file.module,
                Typed {
                    declarations: projection.declarations,
                    annotations: projection
                        .annotations
                        .into_iter()
                        .map(|v| (v.key, v.value))
                        .collect(),
                    imports: projection.imports,
                    modules: projection.modules,
                },
            )
        })
        .collect();
    let ts = codegen::js(
        module,
        &Options {
            typescript: Some(&types),
            ..options
        },
    );
    assert!(
        ts.code.contains("(_: undefined) => undefined"),
        "{}",
        ts.code
    );
    let mut sources = SourceMap::default();
    sources.add(
        &request.sources[0].name,
        String::from_utf8(inputs.read("source/App").expect("source")).expect("UTF-8"),
    );
    let map: serde_json::Value = serde_json::from_str(&codegen::sourcemap::source_map(
        "App.js",
        &js,
        &sources,
        &|f| request.sources[f.index() as usize].name.clone(),
    ))
    .expect("map");
    assert_eq!(map["version"], 3);
    assert_eq!(map["sources"], serde_json::json!(["App.fl"]));
    assert!(!map["mappings"].as_str().expect("mappings").is_empty());
}

#[test]
fn go_uses_ir_and_plain_data_without_compilation_or_settings() {
    let request = common::request();
    let program = Program::from_cbor(&request.program).expect("portable IR");
    let module = &program.modules[0];
    let known = Known::of(&program.lang);
    let paths = BTreeMap::from([("App".into(), request.sources[0].name.clone())]);
    let lines = vec![request.sources[0].line_starts.as_slice()];
    let bindings = Bindings::parse([]);
    let go = codegen::go::go(&GoInput {
        own: &[module],
        builtins: &[],
        entry: "App",
        suites: &[],
        test: false,
        paths: &paths,
        lines: &lines,
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: &bindings,
        lang: &known,
    });
    assert!(go.diagnostics.is_empty(), "{:?}", go.diagnostics);
    // Unit's Go ABI is the runtime's Unit value, not nil or a fabricated int.
    assert!(
        go.files
            .iter()
            .any(|(_, text)| text.contains("return flickrt.Unit")),
        "{:?}",
        go.files
    );
}

fn typescript_projection() -> Vec<u8> {
    let projection = TypescriptModuleProjection {
        files: vec![TypescriptFile {
            module: "App".into(),
            output: "App.ts".into(),
            projection: TypescriptProjection {
                declarations: vec![],
                annotations: vec![OptionValue {
                    key: "main".into(),
                    value: "(_: undefined) => undefined".into(),
                }],
                imports: String::new(),
                modules: vec![],
            },
        }],
    };
    let mut projected = Vec::new();
    ciborium::into_writer(&projection, &mut projected).expect("TS projection");
    projected
}

fn main_calling(module: &str, name: &str) -> Item {
    let span = flick_ir::Span::new(FileId::new(0), 0, 16);
    Item {
        kind: ItemKind::Value {
            name: "main".into(),
            type_params: vec![],
            dicts: vec![],
            dict_constraints: vec![],
            ty: None,
            body: Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Wild,
                        stage: Stage::Pure,
                        ty: None,
                        binder_types: vec![],
                    }],
                    body: Box::new(Expr::new(
                        ExprKind::Global {
                            module: module.into(),
                            name: name.into(),
                            type_args: vec![],
                        },
                        span,
                    )),
                },
                span,
            ),
        },
        span,
    }
}

fn plain_maps(
    request: &flick_backend::BuildRequest,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let paths = request
        .modules
        .iter()
        .map(|m| (m.name.clone(), m.output.clone()))
        .collect();
    let packages = request
        .modules
        .iter()
        .filter_map(|m| Some((m.name.clone(), m.package.clone()?)))
        .collect();
    (paths, packages)
}

#[test]
fn per_module_probe_emits_owned_module_and_dependency_reference_without_checker_callbacks() {
    let dep_iface = b"dep-iface-v1";
    let mut request = common::request();
    let mut program = Program::from_cbor(&request.program).expect("portable IR");
    program.modules[0].items[0] = main_calling("Dep", "unit");
    request.program = program.to_cbor();
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
    let program = validate_request(&request, &mut inputs, Limits::default())
        .expect("per-module request with external interface");
    let root = request.module_root.as_deref().expect("module root");
    let module = program
        .modules
        .iter()
        .find(|m| m.name == root)
        .expect("owned root module");
    let known = Known::of(&program.lang);
    let (paths, packages) = plain_maps(&request);
    let options = Options {
        builtins: &[],
        paths: Some(&paths),
        packages: &packages,
        resolved: &[],
        typescript: None,
        release: false,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::None,
        rt: true,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };
    let emitted = codegen::js(module, &options);
    assert!(emitted.imports.iter().any(|imp| imp == "Dep"));
    assert!(emitted.code.contains("./Dep.js"), "{}", emitted.code);

    let result = BuildResult {
        artifacts: vec![Artifact {
            id: "module:App".into(),
            path: "App.js".into(),
            kind: ArtifactKind::Source,
            bytes: emitted.code.into_bytes(),
        }],
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
    assert_eq!(
        validate_result(&request, &result, &policy, &mut inputs),
        Ok(vec![])
    );

    let mut bad_result = result;
    bad_result.artifacts.push(Artifact {
        id: "module:Dep".into(),
        path: "Dep.js".into(),
        kind: ArtifactKind::Source,
        bytes: b"export const unit = undefined;\n".to_vec(),
    });
    assert!(
        validate_result(&request, &bad_result, &policy, &mut inputs)
            .expect_err("unowned external artifact")
            .contains("layout ownership")
    );
}

fn whole_program_two_module_request() -> flick_backend::BuildRequest {
    let mut request = common::request();
    let mut program = Program::from_cbor(&request.program).expect("portable IR");
    let span = flick_ir::Span::new(FileId::new(0), 0, 16);
    program.modules[0].items[0] = main_calling("Helper", "unit");
    program.modules.push(Module {
        name: "Helper".into(),
        types: vec![],
        preambles: vec![],
        items: vec![Item {
            kind: ItemKind::Value {
                name: "unit".into(),
                type_params: vec![],
                dicts: vec![],
                dict_constraints: vec![],
                ty: None,
                body: Expr::new(ExprKind::Lit(Lit::Unit), span),
            },
            span,
        }],
        labels: BTreeMap::new(),
        initialization: flick_ir::Initialization::default(),
        records: BTreeMap::new(),
        package: Some("demo".into()),
        exports: flick_ir::Exports::default(),
    });
    request.program = program.to_cbor();
    request.modules.push(ModuleInput {
        name: "Helper".into(),
        role: ModuleRole::Own,
        package: Some("demo".into()),
        output: "Helper.js".into(),
        interface: None,
    });
    request
}

#[test]
fn whole_program_probe_emits_root_reachable_modules_without_checker_callbacks() {
    let request = whole_program_two_module_request();
    let mut inputs = common::inputs(&request);
    let program =
        validate_request(&request, &mut inputs, Limits::default()).expect("whole-program request");
    let known = Known::of(&program.lang);
    let (paths, packages) = plain_maps(&request);
    let options = Options {
        builtins: &[],
        paths: Some(&paths),
        packages: &packages,
        resolved: &[],
        typescript: None,
        release: false,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::None,
        rt: true,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };

    let artifacts: Vec<Artifact> = program
        .modules
        .iter()
        .map(|m| {
            let emitted = codegen::js(m, &options);
            Artifact {
                id: format!("module:{}", m.name),
                path: paths[&m.name].clone(),
                kind: ArtifactKind::Source,
                bytes: emitted.code.into_bytes(),
            }
        })
        .collect();
    assert_eq!(artifacts.len(), 2);
    assert!(
        str::from_utf8(&artifacts[0].bytes)
            .expect("UTF-8")
            .contains("./Helper.js")
    );
    let result = BuildResult {
        artifacts,
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    };
    assert_eq!(common::validate(&request, &result), Ok(vec![]));

    let own_refs: Vec<&Module> = program.modules.iter().collect();
    let go_paths = BTreeMap::from([
        ("App".into(), request.sources[0].name.clone()),
        ("Helper".into(), request.sources[0].name.clone()),
    ]);
    let lines = vec![request.sources[0].line_starts.as_slice()];
    let bindings = Bindings::parse([]);
    let go = codegen::go::go(&GoInput {
        own: &own_refs,
        builtins: &[],
        entry: "App",
        suites: &[],
        test: false,
        paths: &go_paths,
        lines: &lines,
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: &bindings,
        lang: &known,
    });
    assert!(go.diagnostics.is_empty(), "{:?}", go.diagnostics);
    assert!(
        go.files
            .iter()
            .any(|(_, text)| text.contains("flickrt.Unit")),
        "{:?}",
        go.files
    );

    let mut bad = request;
    bad.module_root = Some("App".into());
    assert!(validate_request(&bad, &mut common::inputs(&bad), Limits::default()).is_err());
}
