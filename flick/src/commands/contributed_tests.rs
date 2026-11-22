use std::path::Path;

use clap::error::ErrorKind;
use flick_backend::Limits;
use flick_backend::command::{
    Argument, Descriptor, File, FlickModule, Invocation, Parameter, Plan, RunFlickStep, Span, Step,
    Style, ValueKind,
};
use flick_core::diagnostics::codes;

use super::inputs::common_root;
use super::plan::validate_plan;
use super::{
    Contributed, NativeContributor, Source, SourceKind, arguments, builtin_std_source, help,
    resolve,
};
use crate::commands::contributed::Contributor;

fn descriptor(name: &str) -> Descriptor {
    Descriptor {
        name: name.into(),
        summary: String::new(),
        positionals: vec![Parameter {
            name: "who".into(),
            kind: ValueKind::Text,
            multiple: false,
            required: false,
            help: String::new(),
        }],
        options: vec![Parameter {
            name: "loud".into(),
            kind: ValueKind::Flag,
            multiple: false,
            required: false,
            help: String::new(),
        }],
        inputs: vec![],
        reads: vec![],
        writes: vec![],
        style: Style::Plan,
    }
}

fn found(entries: &[(&str, &[&str])]) -> Vec<(Source, Contributed)> {
    (entries.iter())
        .map(|(short, names)| {
            let source = Source {
                short: (*short).into(),
                id: format!("test/{short}"),
                digest: String::new(),
                kind: SourceKind::Backend { official: false },
            };
            let contributed = Contributed {
                short: (*short).into(),
                commands: names.iter().map(|n| descriptor(n)).collect(),
            };
            (source, contributed)
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

fn sample_invocation() -> Invocation {
    Invocation {
        name: "schema export".into(),
        arguments: vec![],
        sources: "src".into(),
        inputs: vec![File {
            path: "src/Lib/Wire.fl".into(),
            bytes: b"module Lib.Wire exposing (item)\nlet item = 1\n".to_vec(),
            executable: false,
        }],
        tools: vec![],
    }
}

fn sample_run_flick_step() -> RunFlickStep {
    RunFlickStep {
        id: "export".into(),
        entry: "FlickSchemaExport".into(),
        modules: vec![FlickModule {
            name: "FlickSchemaExport".into(),
            source:
                "module FlickSchemaExport exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n"
                    .into(),
        }],
        arguments: vec![],
        output: Some("out.json".into()),
        origin: Some(Span {
            path: "src/Lib/Wire.fl".into(),
            start: 0,
            end: 15,
        }),
    }
}

#[test]
fn a_command_runs_under_its_flat_and_its_qualified_name() {
    let found = found(&[("literal", &["hello", "ffi import"]), ("pairs", &["greet"])]);
    let (source, d, rest) = resolve(&words("hello world"), &found, &Vec::new).expect("flat");
    assert_eq!(
        (source.short.as_str(), d.name.as_str(), rest),
        ("literal", "hello", words("world"))
    );
    let (source, d, rest) =
        resolve(&words("literal:ffi import x"), &found, &Vec::new).expect("qualified");
    assert_eq!(
        (source.short.as_str(), d.name.as_str(), rest),
        ("literal", "ffi import", words("x"))
    );
    let (_, d, _) = resolve(&words("ffi import"), &found, &Vec::new).expect("words of a name");
    assert_eq!(d.name, "ffi import");
}

#[test]
fn a_collision_names_both_and_an_unknown_name_fails() {
    let found = found(&[("literal", &["greet"]), ("pairs", &["greet"])]);
    let collision = resolve(&words("greet x"), &found, &Vec::new).expect_err("a collision");
    assert_eq!(collision.code, codes::P053);
    let fix = format!("{:?}", collision.fixes);
    assert!(
        fix.contains("literal:greet") && fix.contains("pairs:greet"),
        "{fix}"
    );
    let (source, _, _) =
        resolve(&words("pairs:greet x"), &found, &Vec::new).expect("qualified wins");
    assert_eq!(source.short, "pairs");
    assert_eq!(
        resolve(&words("nothing"), &found, &Vec::new)
            .expect_err("unknown")
            .code,
        codes::P052
    );
    assert_eq!(
        resolve(&words("other:greet"), &found, &Vec::new)
            .expect_err("no such contributor")
            .code,
        codes::P052
    );
}

#[test]
fn std_contributes_schema_export_and_unknown_prefix_suggests_it() {
    let std_ds = NativeContributor::std()
        .describe()
        .expect("std descriptors");
    assert!(
        std_ds.iter().any(|d| d.name == "schema export"),
        "{std_ds:?}"
    );
    let help_text = help();
    assert!(help_text.contains("Commands of `std`:"), "{help_text}");
    assert!(help_text.contains("schema export"), "{help_text}");

    let found = vec![(
        builtin_std_source(),
        Contributed {
            short: "std".into(),
            commands: std_ds,
        },
    )];
    let (src, d, rest) =
        resolve(&words("schema export Lib.Wire.item"), &found, &Vec::new).expect("flat");
    assert_eq!(
        (src.short.as_str(), d.name.as_str()),
        ("std", "schema export")
    );
    assert_eq!(rest, words("Lib.Wire.item"));

    let (src, d, _) =
        resolve(&words("std:schema export Lib.Wire.item"), &found, &Vec::new).expect("qualified");
    assert_eq!(
        (src.short.as_str(), d.name.as_str()),
        ("std", "schema export")
    );

    let err = resolve(&words("schema"), &found, &Vec::new).expect_err("incomplete prefix");
    assert_eq!(err.code, codes::P052);
    let fix = format!("{:?}", err.fixes);
    assert!(fix.contains("flick schema export"), "{fix}");
}

#[test]
fn arguments_follow_the_descriptor_and_help_returns_display_help() {
    let parsed =
        arguments(&descriptor("hello"), "literal", &words("world --loud")).expect("parses");
    let named: Vec<(&str, &[String])> = parsed
        .iter()
        .map(|a| (a.name.as_str(), a.values.as_slice()))
        .collect();
    assert_eq!(
        named,
        [("who", &words("world")[..]), ("loud", &words("true")[..])]
    );
    assert!(arguments(&descriptor("hello"), "literal", &words("--quiet")).is_err());

    let help_err = arguments(&descriptor("hello"), "literal", &words("--help"))
        .expect_err("--help produces clap DisplayHelp");
    assert_eq!(help_err.kind(), ErrorKind::DisplayHelp);
}

#[test]
fn outside_a_project_the_root_holds_the_working_directory_and_each_path() {
    let mut d = descriptor("content-mapper");
    d.options.push(Parameter {
        name: "declarations".into(),
        kind: ValueKind::Path,
        multiple: true,
        required: false,
        help: String::new(),
    });
    let arguments = vec![Argument {
        name: "declarations".into(),
        values: vec!["/repo/target/tmp/site/src".into(), "here".into()],
    }];
    let cwd = Path::new("/repo/tools/astro");
    assert_eq!(common_root(&d, &arguments, cwd), Path::new("/repo"));
    assert_eq!(common_root(&d, &[], cwd), cwd);
}

#[test]
fn validate_plan_accepts_valid_run_flick_step() {
    let inv = sample_invocation();
    let mut plan = Plan::empty();
    plan.steps.push(Step::RunFlick(sample_run_flick_step()));
    // Declared write succeeds when resume is None
    assert!(validate_plan(&plan, &inv, &["out.json".into()], &[], Path::new(".")).is_ok());
    // Undeclared output succeeds when resume is Some (consumed by resume, not published)
    plan.resume = Some(b"state".to_vec());
    assert!(validate_plan(&plan, &inv, &[], &[], Path::new(".")).is_ok());
}

fn assert_step_rejected(
    inv: &Invocation,
    step: RunFlickStep,
    writes: &[String],
    resume: Option<Vec<u8>>,
    needle: &str,
) {
    let mut plan = Plan::empty();
    plan.resume = resume;
    plan.steps.push(Step::RunFlick(step));
    let err = validate_plan(&plan, inv, writes, &[], Path::new("."))
        .err()
        .expect("rejected");
    assert!(err.contains(needle), "expected `{needle}` in `{err}`");
}

#[test]
fn validate_plan_refuses_invalid_run_flick_module_and_step_names() {
    let inv = sample_invocation();
    let writes = vec!["out.json".into()];

    let mut s = sample_run_flick_step();
    s.id = String::new();
    assert_step_rejected(&inv, s, &writes, None, "empty id");

    let mut s = sample_run_flick_step();
    s.entry = "../Escape".into();
    assert_step_rejected(&inv, s, &writes, None, "invalid Flick entry module");

    let mut s = sample_run_flick_step();
    s.entry = "lowercase".into();
    assert_step_rejected(&inv, s, &writes, None, "invalid Flick entry module");

    let mut s = sample_run_flick_step();
    s.modules[0].name = "Bad..Name".into();
    assert_step_rejected(&inv, s, &writes, None, "invalid module name");

    let mut s = sample_run_flick_step();
    let dup = s.modules[0].clone();
    s.modules.push(dup);
    assert_step_rejected(&inv, s, &writes, None, "duplicate module");

    let mut plan = Plan::empty();
    let mut s1 = sample_run_flick_step();
    s1.output = None;
    let mut s2 = sample_run_flick_step();
    s2.output = None;
    plan.steps.push(Step::RunFlick(s1));
    plan.steps.push(Step::RunFlick(s2));
    let err = validate_plan(&plan, &inv, &writes, &[], Path::new("."))
        .err()
        .expect("dup id");
    assert!(err.contains("duplicate step id"), "{err}");
}

#[test]
fn validate_plan_refuses_invalid_run_flick_paths_origins_and_limits() {
    let inv = sample_invocation();
    let writes = vec!["out.json".into()];

    let mut s = sample_run_flick_step();
    s.output = Some("../escape.json".into());
    assert_step_rejected(&inv, s, &writes, Some(b"state".to_vec()), "escap");

    let mut s = sample_run_flick_step();
    s.output = Some("secret.json".into());
    assert_step_rejected(&inv, s, &writes, None, "does not declare");

    let mut s = sample_run_flick_step();
    s.output = Some(String::new());
    assert_step_rejected(&inv, s, &writes, None, "empty output path");

    let mut s = sample_run_flick_step();
    s.origin = Some(Span {
        path: "src/Other.fl".into(),
        start: 0,
        end: 1,
    });
    assert_step_rejected(&inv, s, &writes, None, "not in invocation inputs");

    let mut s = sample_run_flick_step();
    s.origin = Some(Span {
        path: "src/Lib/Wire.fl".into(),
        start: 0,
        end: 99_999,
    });
    assert_step_rejected(&inv, s, &writes, None, "out of bounds");

    let mut s = sample_run_flick_step();
    s.origin = Some(Span {
        path: "src/Lib/Wire.fl".into(),
        start: 10,
        end: 5,
    });
    assert_step_rejected(&inv, s, &writes, None, "out of bounds");

    let mut s = sample_run_flick_step();
    s.arguments = vec!["bad\0arg".into()];
    assert_step_rejected(&inv, s, &writes, None, "NUL");

    let mut s = sample_run_flick_step();
    s.arguments = vec!["a".into(); Limits::default().files + 1];
    assert_step_rejected(&inv, s, &writes, None, "more than");

    let mut s = sample_run_flick_step();
    s.modules[0].source = "a".repeat(Limits::default().artifact_bytes + 1);
    assert_step_rejected(&inv, s, &writes, None, "exceed");
}

#[test]
fn validate_plan_refuses_non_utf8_and_mid_char_run_flick_origins() {
    let mut inv = sample_invocation();
    inv.inputs.push(File {
        path: "src/Lib/Utf8.fl".into(),
        bytes: "// café\n".as_bytes().to_vec(),
        executable: false,
    });
    inv.inputs.push(File {
        path: "src/Lib/Bin.fl".into(),
        bytes: vec![0xff, 0xfe, b'\n'],
        executable: false,
    });
    let writes = vec!["out.json".into()];

    // `é` occupies bytes 6..8 in "// café\n"; splitting it at byte 7 must be refused.
    let mut s = sample_run_flick_step();
    s.origin = Some(Span {
        path: "src/Lib/Utf8.fl".into(),
        start: 0,
        end: 7,
    });
    assert_step_rejected(&inv, s, &writes, None, "UTF-8");

    let mut s = sample_run_flick_step();
    s.origin = Some(Span {
        path: "src/Lib/Utf8.fl".into(),
        start: 7,
        end: 8,
    });
    assert_step_rejected(&inv, s, &writes, None, "UTF-8");

    // Non-UTF-8 input file referenced by `origin` must be refused.
    let mut s = sample_run_flick_step();
    s.origin = Some(Span {
        path: "src/Lib/Bin.fl".into(),
        start: 0,
        end: 2,
    });
    assert_step_rejected(&inv, s, &writes, None, "not valid UTF-8");

    // Valid UTF-8 char boundaries pass validation.
    let mut valid_step = sample_run_flick_step();
    valid_step.origin = Some(Span {
        path: "src/Lib/Utf8.fl".into(),
        start: 3,
        end: 8,
    });
    let mut plan = Plan::empty();
    plan.steps.push(Step::RunFlick(valid_step));
    assert!(validate_plan(&plan, &inv, &writes, &[], Path::new(".")).is_ok());
}
