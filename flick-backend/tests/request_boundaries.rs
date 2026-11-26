mod common;

use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, BuildResult, DeclaredInputs, Input, InputError,
    InputKind, Inputs, Limits, Mode, ModuleInput, ModuleRole, Policy, RootHandler, Symbol, sha256,
    validate_request, validate_result,
};
use flick_ir::{ExprKind, FileId, ItemKind, Program, Span};

fn check(request: &BuildRequest, source: &[u8]) -> Result<Program, String> {
    let mut inputs =
        DeclaredInputs::new(&request.inputs, |_| Some(source.to_vec())).expect("inputs");
    validate_request(request, &mut inputs, Limits::default())
}

fn two_modules() -> BuildRequest {
    let mut request = common::request();
    let mut program = Program::from_cbor(&request.program).expect("program");
    let mut module = program.modules[0].clone();
    module.name = "Other".into();
    program.modules.push(module);
    request.program = program.to_cbor();
    let mut module = request.modules[0].clone();
    module.name = "Other".into();
    module.output = "Other.js".into();
    request.modules.push(module);
    request
}

#[test]
fn complete_module_layout_rejects_escapes_and_cross_root_aliases() {
    for mode in [Mode::WholeProgram, Mode::PerModule] {
        let mut request = two_modules();
        request.target.mode = mode;
        request.module_root = (mode == Mode::PerModule).then(|| "App".into());
        assert!(check(&request, common::TEXT.as_bytes()).is_ok());
        for output in ["../escape.js", "/absolute.js", "C:/escape.js", "CON.js"] {
            let mut bad = request.clone();
            bad.modules[0].output = output.into();
            assert!(
                check(&bad, common::TEXT.as_bytes()).is_err(),
                "{mode:?}: {output}"
            );
        }
        for output in ["App.js", "APP.JS", "App.js/child"] {
            let mut bad = request.clone();
            bad.modules[1].output = output.into();
            assert!(
                check(&bad, common::TEXT.as_bytes()).is_err(),
                "{mode:?}: {output}"
            );
        }
        request.modules[1].output = "λ/雪.js".into();
        assert!(check(&request, common::TEXT.as_bytes()).is_ok());
    }
}

#[test]
fn module_ownership_must_match_the_portable_program() {
    for (ir, metadata) in [
        (None, Some("demo")),
        (Some("demo"), None),
        (Some("other"), Some("demo")),
    ] {
        let mut request = common::request();
        let mut program = Program::from_cbor(&request.program).expect("program");
        program.modules[0].package = ir.map(str::to_owned);
        request.program = program.to_cbor();
        request.modules[0].package = metadata.map(str::to_owned);
        assert!(
            check(&request, common::TEXT.as_bytes()).is_err(),
            "{ir:?}: {metadata:?}"
        );
    }
    for package in [None, Some("demo")] {
        let mut request = common::request();
        let mut program = Program::from_cbor(&request.program).expect("program");
        program.modules[0].package = package.map(str::to_owned);
        request.program = program.to_cbor();
        request.modules[0].package = package.map(str::to_owned);
        assert!(check(&request, common::TEXT.as_bytes()).is_ok());
    }
}

#[test]
fn item_and_nested_expression_spans_must_fit_the_utf8_source() {
    let text = "é\n";
    let mut request = common::request();
    request.inputs[0].size = text.len() as u64;
    request.inputs[0].sha256 = sha256(text.as_bytes());
    request.sources[0].line_starts = vec![0, 3];
    let mut valid = Program::from_cbor(&request.program).expect("program");
    valid.modules[0].each_span_mut(&mut |span| *span = Span::new(FileId::new(0), 0, 2));
    request.program = valid.to_cbor();
    assert!(check(&request, text.as_bytes()).is_ok());
    for level in ["item", "lambda", "nested"] {
        for (start, end) in [(0, 99), (2, 0), (1, 2), (0, 1)] {
            let mut program = valid.clone();
            let item = &mut program.modules[0].items[0];
            let invalid = Span {
                file: FileId::new(0),
                start,
                end,
            };
            if level == "item" {
                item.span = invalid;
            } else {
                let ItemKind::Value { body, .. } = &mut item.kind else {
                    panic!("main")
                };
                if level == "lambda" {
                    body.span = invalid;
                } else {
                    let ExprKind::Lambda { body, .. } = &mut body.kind else {
                        panic!("lambda")
                    };
                    body.span = invalid;
                }
            }
            request.program = program.to_cbor();
            assert!(
                check(&request, text.as_bytes()).is_err(),
                "{level}: {start}..{end}"
            );
        }
    }
    valid.modules[0].each_span_mut(&mut |span| *span = Span::new(FileId::new(0), 3, 3));
    request.program = valid.to_cbor();
    assert!(
        check(&request, text.as_bytes()).is_ok(),
        "zero-width EOF spans are valid"
    );
}

#[test]
fn root_handlers_cannot_refer_outside_the_closed_module_graph() {
    for missing in ["service", "handler"] {
        let mut request = common::request();
        let mut handler = RootHandler {
            environment: request.target.environment.clone(),
            service: Symbol {
                module: "App".into(),
                name: "Service".into(),
            },
            handler: request.entries[0].symbol.clone(),
        };
        request.root_handlers.push(handler.clone());
        assert!(check(&request, common::TEXT.as_bytes()).is_ok());
        if missing == "service" {
            handler.service.module = "Missing".into();
        } else {
            handler.handler.module = "Missing".into();
        }
        request.root_handlers[0] = handler;
        assert!(
            check(&request, common::TEXT.as_bytes()).is_err(),
            "{missing}"
        );
    }
}

struct Reads {
    inputs: DeclaredInputs,
    calls: usize,
}

impl Inputs for Reads {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
        self.calls += 1;
        self.inputs.read(id)
    }
}

#[test]
fn aliased_source_files_load_and_retain_each_input_once() {
    let mut request = common::request();
    let mut program = Program::from_cbor(&request.program).expect("program");
    program.files.push("Alias.fl".into());
    request.program = program.to_cbor();
    let mut alias = request.sources[0].clone();
    alias.name = "Alias.fl".into();
    request.sources.push(alias);
    let mut inputs = Reads {
        inputs: common::inputs(&request),
        calls: 0,
    };
    validate_request(&request, &mut inputs, Limits::default())
        .expect("same content under two source names");
    assert_eq!(inputs.calls, 1);
}

fn policy_for(req: &BuildRequest) -> Policy<'_> {
    Policy {
        toolchains: &req.toolchains,
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
        authorize_step: &common::authorize,
        authorize_launch: &common::authorize_launch,
    }
}

fn layout_request_with_external() -> BuildRequest {
    let ext_iface = b"ext-iface";
    let mut base = two_modules();
    base.modules.push(ModuleInput {
        name: "Ext".into(),
        role: ModuleRole::External,
        package: Some("ext".into()),
        output: "Ext.js".into(),
        interface: Some("iface/Ext".into()),
    });
    base.inputs.push(Input {
        id: "iface/Ext".into(),
        kind: InputKind::Interface,
        sha256: sha256(ext_iface),
        size: ext_iface.len() as u64,
    });
    base
}

fn layout_inputs(req: &BuildRequest) -> DeclaredInputs {
    DeclaredInputs::new(&req.inputs, |id| {
        Some(if id == "iface/Ext" {
            b"ext-iface".to_vec()
        } else {
            common::TEXT.as_bytes().to_vec()
        })
    })
    .expect("inputs")
}

#[test]
fn per_module_results_enforce_declared_layout_ownership() {
    let mut per_module = layout_request_with_external();
    per_module.target.mode = Mode::PerModule;
    per_module.module_root = Some("App".into());
    let ok_result = BuildResult {
        artifacts: vec![Artifact {
            id: "module:App".into(),
            path: "App.js".into(),
            kind: ArtifactKind::Source,
            bytes: b"export const main = () => undefined;\n".to_vec(),
        }],
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    };
    assert_eq!(
        validate_result(
            &per_module,
            &ok_result,
            &policy_for(&per_module),
            &mut layout_inputs(&per_module)
        ),
        Ok(vec![])
    );
    for forbidden in [
        "Other.js",
        "OTHER.JS",
        "Other.js/child.js",
        "Ext.js",
        "EXT.JS",
    ] {
        let mut bad_result = ok_result.clone();
        bad_result.artifacts.push(Artifact {
            id: "stolen".into(),
            path: forbidden.into(),
            kind: ArtifactKind::Source,
            bytes: b"// stolen\n".to_vec(),
        });
        let err = validate_result(
            &per_module,
            &bad_result,
            &policy_for(&per_module),
            &mut layout_inputs(&per_module),
        )
        .expect_err(forbidden);
        assert!(err.contains("layout ownership"), "{forbidden}: {err}");
    }
    let mut step_stolen = ok_result.clone();
    let mut step = common::step("steal", &[]);
    step.inputs = vec![flick_backend::InputRef::Artifact("module:App".into())];
    step.outputs[0].path = "Other.js".into();
    step_stolen.steps.push(step);
    let err = validate_result(
        &per_module,
        &step_stolen,
        &policy_for(&per_module),
        &mut layout_inputs(&per_module),
    )
    .expect_err("step output targeting sibling module");
    assert!(err.contains("layout ownership"), "{err}");

    let mut wrong_per = per_module;
    wrong_per.module_root = Some("Ext".into());
    assert!(
        validate_result(
            &wrong_per,
            &ok_result,
            &policy_for(&wrong_per),
            &mut layout_inputs(&wrong_per)
        )
        .is_err()
    );
}

#[test]
fn whole_program_results_enforce_declared_layout_ownership() {
    let base = layout_request_with_external();
    let ok_result = BuildResult {
        artifacts: vec![
            Artifact {
                id: "module:App".into(),
                path: "App.js".into(),
                kind: ArtifactKind::Source,
                bytes: b"export const main = () => undefined;\n".to_vec(),
            },
            Artifact {
                id: "module:Other".into(),
                path: "Other.js".into(),
                kind: ArtifactKind::Source,
                bytes: b"export const helper = 1;\n".to_vec(),
            },
        ],
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    };
    assert_eq!(
        validate_result(
            &base,
            &ok_result,
            &policy_for(&base),
            &mut layout_inputs(&base)
        ),
        Ok(vec![])
    );
    let mut whole_stolen_ext = ok_result.clone();
    whole_stolen_ext.artifacts.push(Artifact {
        id: "stolen-ext".into(),
        path: "EXT.JS".into(),
        kind: ArtifactKind::Source,
        bytes: b"// stolen\n".to_vec(),
    });
    let err = validate_result(
        &base,
        &whole_stolen_ext,
        &policy_for(&base),
        &mut layout_inputs(&base),
    )
    .expect_err("whole-program artifact overwriting external module");
    assert!(err.contains("layout ownership"), "{err}");

    let mut wrong_whole = base;
    wrong_whole.module_root = Some("App".into());
    assert!(
        validate_result(
            &wrong_whole,
            &ok_result,
            &policy_for(&wrong_whole),
            &mut layout_inputs(&wrong_whole)
        )
        .is_err()
    );
}
