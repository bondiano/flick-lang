//! Contract probes: the existing emitters consume just IR and plain data.
//! This is not the production driver adapter or the official component yet.
mod common;

use std::collections::{BTreeMap, HashMap, HashSet};

use flick_backend::{
    Abi, Input, InputKind, Inputs, OptionValue, Projection, TypescriptFile,
    TypescriptModuleProjection, TypescriptProjection, sha256,
};
use flick_core::codegen::go::{Bindings, Input as GoInput};
use flick_core::codegen::ts::Typed;
use flick_core::codegen::{self, EffectBridge, Known, Labels, Options};
use flick_core::source::SourceMap;
use flick_ir::Program;

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
        paths: &paths,
        lines: &lines,
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
