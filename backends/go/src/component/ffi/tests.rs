//! Unit tests for `go:ffi import` and `go:ffi check` surface parsing,
//! policy handling, Go-to-Flick mapping, and drift detection.

use flick_ffi::ffi::check::reviewed;
use flick_ffi::ffi::import::Status;
use flick_ir::FileId;

use super::check::{check_reviewed, required_metadata};
use super::generate::{generate, go_flick_name, report};
use super::surface::{GoPolicy, GoSurface, go_module_part};

const SAMPLE_SURFACE_JSON: &str = r#"{
  "package": "example.com/flicktest/fix",
  "version": "v1.0.0",
  "declarations": "go/types (fix)",
  "exports": [
    {
      "name": "Add",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "a", "type": { "kind": "basic", "basic": "int32", "text": "int32" } },
          { "name": "b", "type": { "kind": "basic", "basic": "int32", "text": "int32" } }
        ],
        "results": [
          { "kind": "basic", "basic": "int32", "text": "int32" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:add1"
    },
    {
      "name": "Fetch",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "ctx", "type": { "kind": "named", "package": "context", "name": "Context", "exported": true, "underlying": "interface", "is_context": true, "is_interface": true, "text": "context.Context" } },
          { "name": "url", "type": { "kind": "basic", "basic": "string", "text": "string" } }
        ],
        "results": [
          { "kind": "slice", "elem": { "kind": "basic", "basic": "byte", "text": "byte" }, "text": "[]byte" },
          { "kind": "named", "package": "", "name": "error", "exported": true, "underlying": "interface", "is_error": true, "is_interface": true, "text": "error" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:fetch1"
    },
    {
      "name": "Lookup",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "k", "type": { "kind": "basic", "basic": "string", "text": "string" } }
        ],
        "results": [
          { "kind": "basic", "basic": "string", "text": "string" },
          { "kind": "basic", "basic": "bool", "text": "bool" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:lookup1"
    },
    {
      "name": "MaybePtr",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "x", "type": { "kind": "pointer", "elem": { "kind": "basic", "basic": "int32", "text": "int32" }, "text": "*int32" } }
        ],
        "results": [
          { "kind": "pointer", "elem": { "kind": "basic", "basic": "int32", "text": "int32" }, "text": "*int32" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:ptr1"
    },
    {
      "name": "NewCounter",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "start", "type": { "kind": "basic", "basic": "int32", "text": "int32" } }
        ],
        "results": [
          { "kind": "pointer", "elem": { "kind": "named", "package": "example.com/flicktest/fix", "name": "Counter", "exported": true, "underlying": "struct", "text": "example.com/flicktest/fix.Counter" }, "text": "*example.com/flicktest/fix.Counter" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:newc1"
    },
    {
      "name": "Counter.Add",
      "kind": "method",
      "receiver": {
        "package": "example.com/flicktest/fix",
        "name": "Counter",
        "ptr": true,
        "is_interface": false,
        "generic": false
      },
      "signature": {
        "params": [
          { "name": "delta", "type": { "kind": "basic", "basic": "int32", "text": "int32" } }
        ],
        "results": [
          { "kind": "basic", "basic": "int32", "text": "int32" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:cadd1"
    },
    {
      "name": "Sum",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "xs", "type": { "kind": "slice", "elem": { "kind": "basic", "basic": "int32", "text": "int32" }, "text": "[]int32" } }
        ],
        "results": [
          { "kind": "basic", "basic": "int32", "text": "int32" }
        ],
        "variadic": true
      },
      "fingerprint": "sha256:sum1"
    },
    {
      "name": "GenericId",
      "kind": "func",
      "signature": {
        "type_params": ["T"],
        "params": [
          { "name": "x", "type": { "kind": "type_param", "name": "T", "text": "T" } }
        ],
        "results": [
          { "kind": "type_param", "name": "T", "text": "T" }
        ],
        "variadic": false
      },
      "fingerprint": "sha256:gen1"
    },
    {
      "name": "Watch",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "ch", "type": { "kind": "chan", "text": "<-chan int32" } }
        ],
        "results": [],
        "variadic": false
      },
      "fingerprint": "sha256:chan1"
    },
    {
      "name": "Printf",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "format", "type": { "kind": "basic", "basic": "string", "text": "string" } },
          { "name": "args", "type": { "kind": "slice", "elem": { "kind": "empty_interface", "text": "any" }, "text": "[]any" } }
        ],
        "results": [],
        "variadic": true
      },
      "fingerprint": "sha256:printf1"
    },
    {
      "name": "Hidden",
      "kind": "func",
      "signature": {
        "params": [
          { "name": "s", "type": { "kind": "named", "package": "example.com/flicktest/fix", "name": "secret", "exported": false, "underlying": "struct", "text": "example.com/flicktest/fix.secret" } }
        ],
        "results": [],
        "variadic": false
      },
      "fingerprint": "sha256:hidden1"
    },
    {
      "name": "Pi",
      "kind": "const",
      "type": { "kind": "basic", "basic": "untyped_float", "text": "untyped float" },
      "const_kind": "float",
      "fingerprint": "sha256:pi1"
    },
    {
      "name": "MaxInt",
      "kind": "const",
      "type": { "kind": "basic", "basic": "untyped_int", "text": "untyped int" },
      "const_kind": "int",
      "int32_ok": false,
      "fingerprint": "sha256:maxint1"
    }
  ]
}"#;

fn sample_surface() -> GoSurface {
    GoSurface::parse(SAMPLE_SURFACE_JSON).expect("valid sample surface")
}

#[test]
fn module_part_strips_major_versions_and_normalizes() {
    assert_eq!(go_module_part("strings"), "Strings");
    assert_eq!(go_module_part("math"), "Math");
    assert_eq!(go_module_part("example.com/foo/v2"), "Foo");
    assert_eq!(go_module_part("gopkg.in/yaml.v3"), "Yaml");
    assert_eq!(go_module_part("example.com/my-pkg/v12"), "MyPkg");
    assert_eq!(go_flick_name("ToUpper"), "to_upper");
    assert_eq!(go_flick_name("Replacer.Replace"), "replacer_replace");
    assert_eq!(go_flick_name("Type"), "type_");
}

#[test]
fn generates_expected_go_ffi_bindings_and_statuses() {
    let surface = sample_surface();
    let policy = GoPolicy::parse(
        r#"
[exports.Add]
mode = "pure"

[exports.MaybePtr]
nullable = true
"#,
    )
    .expect("valid policy");

    let generated = generate(&surface, &policy, None, "Ffi.Fix.Generated");
    let status_of = |export: &str| {
        generated
            .entries
            .iter()
            .find(|e| e.export == export)
            .map(|e| (e.status, e.detail.as_str()))
            .expect("entry exists")
    };

    assert_eq!(status_of("Add"), (Status::Supported, "Int -> Int -> Int"));
    assert_eq!(
        status_of("Fetch"),
        (Status::Supported, "String ->{Async; GoError} Bytes")
    );
    assert_eq!(
        status_of("Lookup"),
        (Status::Supported, "String ->{} Option String")
    );
    assert_eq!(
        status_of("MaybePtr"),
        (Status::Supported, "Option Int ->{} Option Int")
    );
    assert_eq!(
        status_of("NewCounter"),
        (Status::Supported, "Int ->{} Counter")
    );
    assert_eq!(
        status_of("Counter.Add"),
        (Status::Supported, "Counter -> Int ->{} Int")
    );
    assert_eq!(status_of("Sum"), (Status::Supported, "List Int ->{} Int"));
    assert_eq!(status_of("Pi"), (Status::Supported, "Float"));

    assert_eq!(status_of("GenericId").0, Status::Unsupported);
    assert!(status_of("GenericId").1.contains("generic"));
    assert_eq!(status_of("Watch").0, Status::Unsupported);
    assert!(status_of("Watch").1.contains("channel"));
    assert_eq!(status_of("Printf").0, Status::Unsupported);
    assert!(status_of("Printf").1.contains("variadic `any`"));
    assert_eq!(status_of("Hidden").0, Status::Unsupported);
    assert!(status_of("Hidden").1.contains("unexported"));
    assert_eq!(status_of("MaxInt").0, Status::Unsupported);
    assert!(status_of("MaxInt").1.contains("overflows"));

    assert!(
        generated
            .text
            .contains("// flick ffi import example.com/flicktest/fix\n")
    );
    assert!(generated.text.contains("import Bytes exposing (Bytes)\n"));
    assert!(
        generated
            .text
            .contains("type error GoError = GoError { message : String }")
    );
    assert!(
        generated
            .text
            .contains("@go from \"example.com/flicktest/fix\" \"*Counter\"\ntype Counter\n")
    );
    assert!(
        generated
            .text
            .contains("foreign pure add from \"example.com/flicktest/fix\" \"Add\"\n  arity 2\n")
    );
    assert!(generated.text.contains("foreign fetch from \"example.com/flicktest/fix\" \"Fetch\"\n  arity 1\n  catch from_go_error\n  cancel context\n"));
    assert!(generated.text.contains("foreign counter_add from \"example.com/flicktest/fix\" \"Counter.Add\"\n  arity 2\n  this\n"));
    assert!(generated.text.contains(
        "foreign sum from \"example.com/flicktest/fix\" \"Sum\"\n  arity 1\n  variadic\n"
    ));
    assert!(
        generated
            .text
            .contains("foreign pure pi from \"example.com/flicktest/fix\" \"Pi\"\n")
    );

    let summary = report(&generated, &surface);
    assert!(
        summary.contains("8 supported, 0 need a policy, 5 unsupported"),
        "{summary}"
    );

    // Without nullable policy, pointer return is `needs policy`.
    let no_policy = generate(
        &surface,
        &GoPolicy::default(),
        Some(&["MaybePtr".into()]),
        "Ffi.Fix.Generated",
    );
    assert_eq!(no_policy.entries[0].status, Status::NeedsPolicy);
    assert!(no_policy.entries[0].detail.contains("nullable = true"));
}

#[test]
fn type_policy_conversion_overrides_opaque_handle() {
    let surface = sample_surface();
    let policy = GoPolicy::parse(
        r#"
[types."*Counter"]
convert = "Int"
"#,
    )
    .expect("valid policy");
    let generated = generate(
        &surface,
        &policy,
        Some(&["NewCounter".into(), "Counter.Add".into()]),
        "Ffi.Fix.Generated",
    );
    assert_eq!(generated.entries[0].export, "NewCounter");
    assert_eq!(generated.entries[0].detail, "Int ->{} Int");
    assert_eq!(generated.entries[1].export, "Counter.Add");
    assert_eq!(generated.entries[1].detail, "Int -> Int ->{} Int");
    assert!(!generated.text.contains("type Counter"));
}

#[test]
fn check_reviewed_detects_fingerprint_drift_missing_export_and_missing_meta() {
    let surface = sample_surface();
    let fetch_export = surface.find("Fetch").expect("Fetch");
    assert_eq!(
        required_metadata(fetch_export),
        vec![
            ("catch", "returns a trailing Go `error`"),
            ("cancel", "takes a leading `context.Context`"),
        ]
    );

    let source = r#"module Ffi.Fix exposing (fetch, counter_add, gone)

fetch : String ->{Async; GoError} Bytes
foreign fetch from "example.com/flicktest/fix" "Fetch"
  arity 1
  reviewed "example.com/flicktest/fix@v1.0.0 sha256:stale"

counter_add : Counter -> Int ->{} Int
foreign counter_add from "example.com/flicktest/fix" "Counter.Add"
  arity 2
  reviewed "example.com/flicktest/fix@v1.0.0 sha256:cadd1"

gone : Unit ->{} Unit
foreign gone from "example.com/flicktest/fix" "Removed"
  arity 0
  reviewed "example.com/flicktest/fix@v1.0.0 sha256:old"
"#;

    let bindings = reviewed(source, FileId::new(0));
    let diags = check_reviewed(&bindings, &surface);
    let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("declaration of `Fetch` changed")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`Fetch` returns a trailing Go `error`")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`Fetch` takes a leading `context.Context`")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`Counter.Add` is a Go method") && m.contains("needs `this`")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("no longer exports `Removed`")),
        "{messages:?}"
    );
}

#[test]
fn policy_requires_catch_and_error_together_and_emits_go_error_when_referenced() {
    let only_catch = GoPolicy::parse(
        r#"
[exports.Fetch]
catch = "custom_conv"
"#,
    )
    .expect_err("catch without error must be rejected");
    assert!(
        only_catch.contains("must set `catch` and `error` together"),
        "{only_catch}"
    );

    let only_error = GoPolicy::parse(
        r#"
[exports.Fetch]
error = "MyErr"
"#,
    )
    .expect_err("error without catch must be rejected");
    assert!(
        only_error.contains("must set `catch` and `error` together"),
        "{only_error}"
    );

    let surface = sample_surface();

    // Explicit custom catch returning the default `GoError` still declares `GoError`.
    let custom_catch_default_err = GoPolicy::parse(
        r#"
[exports.Fetch]
catch = "custom_conv"
error = "GoError"
"#,
    )
    .expect("valid policy");
    let generated = generate(
        &surface,
        &custom_catch_default_err,
        Some(&["Fetch".into()]),
        "Ffi.Fix.Generated",
    );
    assert!(
        generated
            .text
            .contains("type error GoError = GoError { message : String }"),
        "{}",
        generated.text
    );
    assert!(
        generated.text.contains("catch custom_conv"),
        "{}",
        generated.text
    );

    // Fully custom catch + error omits the default `GoError` declaration.
    let custom_both = GoPolicy::parse(
        r#"
[exports.Fetch]
catch = "MyErr.from_go"
error = "MyErr"
"#,
    )
    .expect("valid policy");
    let generated_custom = generate(
        &surface,
        &custom_both,
        Some(&["Fetch".into()]),
        "Ffi.Fix.Generated",
    );
    assert!(
        !generated_custom.text.contains("type error GoError"),
        "{}",
        generated_custom.text
    );
    assert!(
        generated_custom.text.contains("catch MyErr.from_go")
            && generated_custom
                .text
                .contains("String ->{Async; MyErr} Bytes"),
        "{}",
        generated_custom.text
    );
}

#[test]
fn rejects_project_root_dir_flag() {
    use flick_backend::command::{Argument, Invocation, Tool};

    let tool = Tool {
        id: "go".into(),
        version: "1.27.3".into(),
        sha256: "a".repeat(64),
    };
    let make_invocation = |name: &str, args: Vec<Argument>| Invocation {
        name: name.into(),
        arguments: args,
        sources: "src".into(),
        inputs: vec![],
        tools: vec![tool.clone()],
        dependencies: vec![],
    };

    let imp = make_invocation(
        "ffi import",
        vec![
            Argument {
                name: "package".into(),
                values: vec!["example.com/local".into()],
            },
            Argument {
                name: "out".into(),
                values: vec!["src/Ffi".into()],
            },
            Argument {
                name: "dir".into(),
                values: vec![String::new()],
            },
        ],
    );
    let err = super::run(&imp).expect_err("root --dir rejected");
    assert_eq!(err[0].code, "F008");
    assert!(
        err[0].message.contains("subdirectory"),
        "{}",
        err[0].message
    );

    let chk = make_invocation(
        "ffi check",
        vec![Argument {
            name: "dir".into(),
            values: vec![String::new()],
        }],
    );
    let err = super::run(&chk).expect_err("root --dir rejected");
    assert_eq!(err[0].code, "F008");
    assert!(
        err[0].message.contains("subdirectory"),
        "{}",
        err[0].message
    );
}
