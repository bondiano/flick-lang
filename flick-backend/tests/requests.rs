mod common;

use flick_backend::{Abi, InputKind, Limits, Mode, ModuleRole, Projection, validate_request};

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
            "ir" => request.target.ir_version = "0.12.0".into(),
            "api" => request.target.backend_api = "0.2.0".into(),
            "pin" => request.backend.sha256 = "missing".into(),
            _ => unreachable!(),
        }
        let error = validate_request(&request, &mut common::inputs(&request), Limits::default())
            .expect_err(case);
        assert!(!error.is_empty(), "{case}");
    }
}
