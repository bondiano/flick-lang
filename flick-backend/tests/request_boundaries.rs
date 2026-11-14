mod common;

use flick_backend::{
    BuildRequest, DeclaredInputs, InputError, Inputs, Limits, Mode, RootHandler, Symbol, sha256,
    validate_request,
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
