use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use flick_backend::command::{Parameter, ValueKind};
use flick_backend::{
    BackendId, Descriptor, Environment, Mode, NamedRange, OptionDescriptor, Profile, Verb,
    VersionRange,
};
use flick_core::diagnostics::codes;
use flick_core::project::{Project, config};
use flick_core::source::SourceMap;

use super::scan::scan_argv;
use super::{resolve, resolve_target};

fn range() -> VersionRange {
    VersionRange {
        minimum: "0.1.0".into(),
        maximum_exclusive: "0.2.0".into(),
    }
}

fn js_descriptor() -> Descriptor {
    Descriptor {
        identity: BackendId {
            id: "flick/js".into(),
            version: "0.1.0".into(),
        },
        backend_api: range(),
        ir: range(),
        language: range(),
        runtime_abis: vec![NamedRange {
            name: "flickrt".into(),
            versions: range(),
        }],
        environments: vec![Environment {
            backend: "js".into(),
            platform: "node".into(),
        }],
        profiles: vec![Profile::Debug, Profile::Release],
        modes: vec![Mode::WholeProgram, Mode::PerModule],
        capabilities: vec![],
        launch_runtimes: vec!["node".into()],
        host_tools: vec!["node".into(), "pnpm".into()],
        projections: vec![],
        unsupported_modules: vec![],
        stages_by_class_option: Some(flick_backend::OptionValue {
            key: "runtime".into(),
            value: "rt".into(),
        }),
        foreign_extension: None,
        foreign_suffixes: vec![],
        handled_services: vec![],
        options: vec![
            OptionDescriptor {
                parameter: Parameter {
                    name: "typescript".into(),
                    kind: ValueKind::Flag,
                    multiple: false,
                    required: false,
                    help: "Emit TypeScript.".into(),
                },
                default: Some("false".into()),
                implicit_value: None,
                choices: vec![],
                verbs: vec![Verb::Build, Verb::Inspect],
            },
            OptionDescriptor {
                parameter: Parameter {
                    name: "declarations".into(),
                    kind: ValueKind::Flag,
                    multiple: false,
                    required: false,
                    help: "Emit declarations.".into(),
                },
                default: Some("false".into()),
                implicit_value: None,
                choices: vec![],
                verbs: vec![Verb::Build],
            },
            OptionDescriptor {
                parameter: Parameter {
                    name: "runtime".into(),
                    kind: ValueKind::Text,
                    multiple: false,
                    required: false,
                    help: "JS runtime.".into(),
                },
                default: Some("rt".into()),
                implicit_value: None,
                choices: vec!["rt".into(), "effect".into()],
                verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test, Verb::Inspect],
            },
            OptionDescriptor {
                parameter: Parameter {
                    name: "devtools".into(),
                    kind: ValueKind::Text,
                    multiple: false,
                    required: false,
                    help: "DevTools URL.".into(),
                },
                default: None,
                implicit_value: Some("ws://localhost:34437".into()),
                choices: vec![],
                verbs: vec![Verb::Run, Verb::Dev],
            },
        ],
    }
}

fn project_from(toml: &str) -> Project {
    let mut sources = SourceMap::default();
    let file = sources.add("flick.toml", toml);
    Project {
        root: PathBuf::from("/project"),
        config: config::parse(toml, file).expect("valid config"),
        config_file: file,
        workspace: None,
    }
}

#[test]
fn removed_cli_flags_fail_with_p057_and_stop_at_double_dash() {
    let cases: &[(&[&str], &str)] = &[
        (&["build", "--emit", "js"], "--target js"),
        (&["build", "--emit=js"], "--target js"),
        (&["build", "--emit", "ts"], "--target js --typescript"),
        (&["build", "--emit=ts"], "--target js --typescript"),
        (&["build", "--emit", "go"], "--target go"),
        (&["build", "--emit=go"], "--target go"),
        (&["build", "--emit-dts"], "--declarations"),
        (&["build", "--rt"], "--runtime rt"),
        (&["test", "--emit", "js"], "--target js"),
        (&["test", "--emit", "go"], "--target go"),
        (&["test", "--rt"], "--runtime rt"),
        (&["run", "--rt"], "--runtime rt"),
        (&["dev", "--rt"], "--runtime rt"),
        (
            &["inspect", "App.Main", "--emit", "js"],
            "--emit output --target js",
        ),
        (
            &["inspect", "App.Main", "--emit=ts"],
            "--emit output --target js --typescript",
        ),
    ];
    for (argv, expected_fix) in cases {
        let os: Vec<OsString> = argv.iter().map(OsString::from).collect();
        let scan = scan_argv(&os);
        let diag = scan
            .removed
            .unwrap_or_else(|| panic!("expected P057 for {argv:?}"));
        assert_eq!(diag.code, codes::P057, "{argv:?}");
        let fixes = format!("{:?}", diag.fixes);
        assert!(fixes.contains(expected_fix), "{argv:?}: {fixes}");
    }
    // Passthrough after `--` is untouched.
    let passthrough: Vec<OsString> = ["run", "app", "--", "--rt", "--emit", "go", "--emit-dts"]
        .iter()
        .map(OsString::from)
        .collect();
    assert!(scan_argv(&passthrough).removed.is_none());
    // Valid `inspect --emit output` and `build --emit-ir` are untouched.
    let ok_inspect: Vec<OsString> = ["inspect", "App.Main", "--emit", "output"]
        .iter()
        .map(OsString::from)
        .collect();
    let scanned_output = scan_argv(&ok_inspect);
    assert!(scanned_output.removed.is_none());
    assert_eq!(scanned_output.emit.as_deref(), Some("output"));

    let ok_iface: Vec<OsString> = ["inspect", "App.Main", "--emit=iface"]
        .iter()
        .map(OsString::from)
        .collect();
    let scanned_iface = scan_argv(&ok_iface);
    assert!(scanned_iface.removed.is_none());
    assert_eq!(scanned_iface.emit.as_deref(), Some("iface"));

    let ok_ir: Vec<OsString> = ["build", "--emit-ir", "json"]
        .iter()
        .map(OsString::from)
        .collect();
    assert!(scan_argv(&ok_ir).removed.is_none());
}

#[test]
fn resolve_merges_cli_over_toml_over_defaults_and_scopes_by_verb() {
    let desc = js_descriptor();
    let project = project_from(
        "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n[target.js]\ntypescript = true\ndeclarations = true\nruntime = \"effect\"\ndevtools = true\n",
    );
    // On `build`, typescript/declarations/runtime are active; devtools is validated in TOML
    // but not activated for `build`.
    let built = resolve(&desc, Verb::Build, Some(&project), &BTreeMap::new()).expect("build");
    assert!(built.flag("typescript"));
    assert!(built.flag("declarations"));
    assert_eq!(built.get("runtime"), Some("effect"));
    assert_eq!(built.get("devtools"), None);

    // CLI overrides TOML.
    let mut cli = BTreeMap::new();
    cli.insert("runtime".into(), vec!["rt".into()]);
    let overridden = resolve(&desc, Verb::Build, Some(&project), &cli).expect("override");
    assert_eq!(overridden.get("runtime"), Some("rt"));
    assert_ne!(built.cache_key(), overridden.cache_key());

    // On `run`, devtools = true expands to implicit value, while Build-only flags stay inactive.
    let run = resolve(&desc, Verb::Run, Some(&project), &BTreeMap::new()).expect("run");
    assert_eq!(run.get("devtools"), Some("ws://localhost:34437"));
    assert_eq!(run.get("runtime"), Some("effect"));
    assert_eq!(run.get("typescript"), None);
    assert_eq!(run.get("declarations"), None);
}

#[test]
fn resolve_rejects_unknown_toml_keys_wrong_types_and_bad_choices_with_p058() {
    let desc = js_descriptor();
    let typo = project_from(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\ntypscript = true\n",
    );
    let errs = resolve(&desc, Verb::Build, Some(&typo), &BTreeMap::new()).expect_err("typo");
    assert_eq!(errs[0].code, codes::P058);
    assert!(errs[0].span.is_some());
    assert!(format!("{:?}", errs[0].fixes).contains("typescript"));

    let bad_type = project_from(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\ntypescript = \"yes\"\n",
    );
    let errs =
        resolve(&desc, Verb::Build, Some(&bad_type), &BTreeMap::new()).expect_err("bad type");
    assert_eq!(errs[0].code, codes::P058);

    let bad_choice = project_from(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\nruntime = \"efect\"\n",
    );
    let errs =
        resolve(&desc, Verb::Build, Some(&bad_choice), &BTreeMap::new()).expect_err("bad choice");
    assert_eq!(errs[0].code, codes::P058);
    assert!(format!("{:?}", errs[0].fixes).contains("effect"));
}

#[test]
fn unknown_target_fails_with_p059_and_suggests_closest() {
    let err = resolve_target(Some("gp"), None).expect_err("unknown target");
    assert_eq!(err.code, codes::P059);
    assert!(format!("{:?}", err.fixes).contains("--target go"));

    let project =
        project_from("schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"go\"\n");
    let (choice, overridden) = resolve_target(None, Some(&project)).expect("go from config");
    assert_eq!(choice.short(), "go");
    assert!(!overridden);

    let (choice, overridden) =
        resolve_target(Some("js"), Some(&project)).expect("cli overrides config");
    assert!(choice.is_official_js());
    assert!(overridden);
}
