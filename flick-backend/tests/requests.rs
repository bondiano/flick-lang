mod common;

use flick_backend::{
    Abi, DeclaredInputs, HeaderRequest, HostExport, InlineForeign, Input, InputKind, Limits, Mode,
    ModuleInput, ModuleRole, Projection, Span, Symbol, sha256, validate_request,
};
use flick_ir::Program;

#[test]
fn a_complete_whole_program_request_is_accepted() {
    let request = common::request();
    let program = validate_request(&request, &mut common::inputs(&request), Limits::default())
        .expect("complete graph");
    assert_eq!(program.modules[0].name, "App");
}

#[test]
fn per_module_mode_has_an_explicit_own_root_and_whole_program_forbids_one() {
    let mut whole_with_root = common::request();
    whole_with_root.module_root = Some("App".into());
    assert!(
        validate_request(
            &whole_with_root,
            &mut common::inputs(&whole_with_root),
            Limits::default()
        )
        .is_err(),
        "whole-program mode must reject a module_root"
    );

    let iface = b"ext-iface";
    let mut request = common::request();
    let mut program = Program::from_cbor(&request.program).expect("program");
    let mut builtin = program.modules[0].clone();
    builtin.name = "BuiltinMod".into();
    builtin.package = None;
    program.modules.push(builtin);
    request.program = program.to_cbor();
    request.modules.push(ModuleInput {
        name: "BuiltinMod".into(),
        role: ModuleRole::Builtin,
        package: None,
        output: "BuiltinMod.js".into(),
        interface: None,
    });
    request.modules.push(ModuleInput {
        name: "ExtMod".into(),
        role: ModuleRole::External,
        package: Some("ext".into()),
        output: "ExtMod.js".into(),
        interface: Some("iface/ExtMod".into()),
    });
    request.inputs.push(Input {
        id: "iface/ExtMod".into(),
        kind: InputKind::Interface,
        sha256: sha256(iface),
        size: iface.len() as u64,
    });
    let make_inputs = |req: &flick_backend::BuildRequest| {
        DeclaredInputs::new(&req.inputs, |id| {
            Some(if id == "iface/ExtMod" {
                iface.to_vec()
            } else {
                common::TEXT.as_bytes().to_vec()
            })
        })
        .expect("inputs")
    };

    request.target.mode = Mode::PerModule;
    assert!(
        validate_request(&request, &mut make_inputs(&request), Limits::default()).is_err(),
        "per-module mode without module_root"
    );
    for invalid_root in ["Unknown", "BuiltinMod", "ExtMod"] {
        request.module_root = Some(invalid_root.into());
        assert!(
            validate_request(&request, &mut make_inputs(&request), Limits::default()).is_err(),
            "per-module root `{invalid_root}` must be rejected"
        );
    }
    request.module_root = Some("App".into());
    assert!(
        validate_request(&request, &mut make_inputs(&request), Limits::default()).is_ok(),
        "per-module mode with own root and builtin/external graph accepted"
    );
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
            "ir" => request.target.ir_version = "0.20.0".into(),
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

#[test]
fn valid_host_exports_and_header_requests_are_accepted() {
    let mut request = common::request();
    request.exports.push(HostExport {
        name: "runMain".into(),
        symbol: Symbol {
            module: "App".into(),
            name: "main".into(),
        },
        span: Span {
            file: 0,
            start: 0,
            end: 5,
        },
    });
    request.headers = Some(HeaderRequest {
        enabled: true,
        format: Some("d.ts".into()),
    });
    validate_request(&request, &mut common::inputs(&request), Limits::default())
        .expect("valid host export and header request");

    // Empty export name is allowed at the protocol layer so the backend reports
    // invalid host names at the attribute span.
    let mut empty_host_name = common::request();
    empty_host_name.exports.push(HostExport {
        name: String::new(),
        symbol: Symbol {
            module: "App".into(),
            name: "main".into(),
        },
        span: Span {
            file: 0,
            start: 0,
            end: 5,
        },
    });
    validate_request(
        &empty_host_name,
        &mut common::inputs(&empty_host_name),
        Limits::default(),
    )
    .expect("empty host export name passed to backend for diagnostic");
}

#[test]
fn invalid_host_exports_and_header_requests_are_rejected() {
    for case in [
        "unknown-module",
        "empty-symbol",
        "nul-name",
        "bad-span",
        "empty-format",
    ] {
        let mut bad = common::request();
        match case {
            "unknown-module" => bad.exports.push(HostExport {
                name: "ok".into(),
                symbol: Symbol {
                    module: "Unknown".into(),
                    name: "main".into(),
                },
                span: Span {
                    file: 0,
                    start: 0,
                    end: 5,
                },
            }),
            "empty-symbol" => bad.exports.push(HostExport {
                name: "ok".into(),
                symbol: Symbol {
                    module: "App".into(),
                    name: String::new(),
                },
                span: Span {
                    file: 0,
                    start: 0,
                    end: 5,
                },
            }),
            "nul-name" => bad.exports.push(HostExport {
                name: "a\0b".into(),
                symbol: Symbol {
                    module: "App".into(),
                    name: "main".into(),
                },
                span: Span {
                    file: 0,
                    start: 0,
                    end: 5,
                },
            }),
            "bad-span" => bad.exports.push(HostExport {
                name: "ok".into(),
                symbol: Symbol {
                    module: "App".into(),
                    name: "main".into(),
                },
                span: Span {
                    file: 0,
                    start: 0,
                    end: 9999,
                },
            }),
            "empty-format" => {
                bad.headers = Some(HeaderRequest {
                    enabled: true,
                    format: Some(String::new()),
                });
            }
            _ => unreachable!(),
        }
        assert!(
            validate_request(&bad, &mut common::inputs(&bad), Limits::default()).is_err(),
            "{case}"
        );
    }
}
