mod common;

use flick_backend::{
    Abi, InlineForeign, InputKind, Limits, Mode, ModuleRole, Projection, Span, validate_request,
};

#[test]
fn a_complete_whole_program_request_is_accepted() {
    let request = common::request();
    let program = validate_request(&request, &mut common::inputs(&request), Limits::default())
        .expect("complete graph");
    assert_eq!(program.modules[0].name, "App");
}

#[test]
fn per_module_mode_has_an_explicit_root_and_keeps_the_graph() {
    let mut request = common::request();
    request.target.mode = Mode::PerModule;
    assert!(validate_request(&request, &mut common::inputs(&request), Limits::default()).is_err());
    request.module_root = Some("App".into());
    assert!(validate_request(&request, &mut common::inputs(&request), Limits::default()).is_ok());
    request.module_root = Some("Unknown".into());
    assert!(validate_request(&request, &mut common::inputs(&request), Limits::default()).is_err());
}

#[test]
fn incomplete_graphs_and_inconsistent_source_metadata_are_rejected() {
    for case in [
        "module",
        "sources",
        "line-index",
        "source-name",
        "source-kind",
        "entry",
        "test-root",
        "projection",
        "external-interface",
        "ir",
        "ir-old",
        "api",
        "pin",
    ] {
        let mut request = common::request();
        match case {
            "module" => request.modules.clear(),
            "sources" => request.sources.clear(),
            "line-index" => request.sources[0].line_starts = vec![1],
            "source-name" => request.sources[0].name = "other.fl".into(),
            "source-kind" => request.inputs[0].kind = InputKind::Resource,
            "entry" => request.entries[0].symbol.module = "Unknown".into(),
            "test-root" => request.test_roots.push(flick_backend::Symbol {
                module: "Unknown".into(),
                name: "tests".into(),
            }),
            "projection" => request.projections.push(Projection {
                module: "App".into(),
                schema: Abi {
                    name: "flick.typescript".into(),
                    version: "0.1.0".into(),
                },
                input: "missing".into(),
            }),
            "external-interface" => request.modules[0].role = ModuleRole::External,
            "ir" => request.target.ir_version = "0.18.0".into(),
            "ir-old" => request.target.ir_version = "0.15.0".into(),
            "api" => request.target.backend_api = "0.2.0".into(),
            "pin" => request.backend.sha256 = "missing".into(),
            _ => unreachable!(),
        }
        let error = validate_request(&request, &mut common::inputs(&request), Limits::default())
            .expect_err(case);
        assert!(!error.is_empty(), "{case}");
    }
}

#[test]
fn inline_foreign_entries_are_validated() {
    let mut request = common::request();
    request.inline_foreign.push(InlineForeign {
        module: "App".into(),
        name: None,
        code: "const k = 1;\n".repeat(2000),
        span: Span {
            file: 0,
            start: 0,
            end: 5,
        },
    });
    request.inline_foreign.push(InlineForeign {
        module: "App".into(),
        name: Some("cos".into()),
        code: "(d) => Math.cos(d)".into(),
        span: Span {
            file: 0,
            start: 0,
            end: 5,
        },
    });
    validate_request(&request, &mut common::inputs(&request), Limits::default())
        .expect("large preamble and binding accepted");

    for case in ["unknown-module", "empty-name", "nul-code", "bad-span"] {
        let mut bad = common::request();
        bad.inline_foreign.push(match case {
            "unknown-module" => InlineForeign {
                module: "Unknown".into(),
                name: None,
                code: "const k = 1;".into(),
                span: Span {
                    file: 0,
                    start: 0,
                    end: 5,
                },
            },
            "empty-name" => InlineForeign {
                module: "App".into(),
                name: Some(String::new()),
                code: "x".into(),
                span: Span {
                    file: 0,
                    start: 0,
                    end: 5,
                },
            },
            "nul-code" => InlineForeign {
                module: "App".into(),
                name: Some("f".into()),
                code: "a\0b".into(),
                span: Span {
                    file: 0,
                    start: 0,
                    end: 5,
                },
            },
            "bad-span" => InlineForeign {
                module: "App".into(),
                name: None,
                code: "const k = 1;".into(),
                span: Span {
                    file: 0,
                    start: 0,
                    end: 9999,
                },
            },
            _ => unreachable!(),
        });
        assert!(
            validate_request(&bad, &mut common::inputs(&bad), Limits::default()).is_err(),
            "{case}"
        );
    }
}
