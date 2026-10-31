use super::import::{Generated, Status, flick_name, generate};
use super::{ExportPolicy, Policy, Surface, module_part};
use std::collections::BTreeMap;

fn lookup_surface() -> Surface {
    Surface::parse(include_str!("fixtures/lookup.json")).expect("fixture parses")
}

#[cfg(feature = "native-project")]
const LOOKUP_POLICY: &str = r#"
[exports.lookup]
undefined = "none"
catch = "JsError"

[exports.each]
retention = "call"
"#;

fn lookup_policy() -> Policy {
    Policy {
        exports: BTreeMap::from([
            (
                "lookup".into(),
                ExportPolicy {
                    undefined: Some("none".into()),
                    catch: Some("JsError".into()),
                    ..ExportPolicy::default()
                },
            ),
            (
                "each".into(),
                ExportPolicy {
                    retention: Some("call".into()),
                    ..ExportPolicy::default()
                },
            ),
        ]),
        ..Policy::default()
    }
}

#[cfg(feature = "native-project")]
#[test]
fn lookup_policy_parses() {
    assert_eq!(
        Policy::parse(LOOKUP_POLICY).expect("policy parses"),
        lookup_policy()
    );
}

#[test]
fn lookup_with_its_policy_is_the_binding_of_the_spec() {
    let policy = lookup_policy();
    let generated = generate(&lookup_surface(), &policy, None, "Ffi.LookupLib.Generated");
    insta::assert_snapshot!(generated.text);
    insta::assert_snapshot!("lookup_report", generated.report(&lookup_surface()));
}

#[test]
fn without_policies_promises_callbacks_and_nullable_results_are_blocked() {
    let generated = generate(
        &lookup_surface(),
        &Policy::default(),
        None,
        "LookupLib.Generated",
    );
    let status = |name: &str| {
        let entry = generated
            .entries
            .iter()
            .find(|e| e.export == name)
            .expect("entry");
        (entry.status, entry.detail.clone())
    };
    assert_eq!(status("lookup").0, Status::NeedsPolicy);
    assert!(status("lookup").1.contains("may return `undefined`"));
    assert_eq!(status("each").0, Status::NeedsPolicy);
    // `nextId` is never pure: its result type cannot show that
    assert_eq!(
        status("nextId"),
        (Status::Supported, "Unit ->{} Float".into())
    );
    assert!(
        generated
            .text
            .contains("foreign blocked lookup \"needs policy: ")
    );
}

#[test]
fn strict_mode_refuses_unknown() {
    let policy = Policy {
        strict: true,
        ..Policy::default()
    };
    let generated = generate(&lookup_surface(), &policy, None, "LookupLib.Generated");
    let describe = generated
        .entries
        .iter()
        .find(|e| e.export == "describe")
        .expect("entry");
    assert_eq!(describe.status, Status::Unsupported);
    assert!(describe.detail.contains("refused by `strict = true`"));
}

#[test]
fn select_limits_the_surface_and_names_what_is_missing() {
    let select = ["nextId".to_owned(), "nope".to_owned()];
    let generated = generate(
        &lookup_surface(),
        &Policy::default(),
        Some(&select),
        "L.Generated",
    );
    let names: Vec<&str> = generated
        .entries
        .iter()
        .map(|e| e.export.as_str())
        .collect();
    assert_eq!(names, ["nextId"]);
    assert_eq!(generated.missing, ["nope"]);
    assert!(
        generated
            .text
            .starts_with("// flick ffi import lookup-lib --select nextId,nope\n")
    );
}

#[cfg(feature = "native-project")]
#[test]
fn policies_are_checked() {
    let error = Policy::parse("[exports.lookup]\ncatch = \"Oops\"").expect_err("refused");
    assert_eq!(
        error,
        "`catch` of `lookup` is \"Oops\"; expected \"JsError\""
    );
    assert!(Policy::parse("[exports.lookup]\ncatches = \"JsError\"").is_err());
}

#[test]
fn names() {
    assert_eq!(flick_name("nextId"), "next_id");
    assert_eq!(flick_name("parseISO"), "parse_iso");
    assert_eq!(flick_name("getHTTPResponse"), "get_http_response");
    assert_eq!(flick_name("match"), "match_");
    assert_eq!(flick_name("$"), "js_");
    assert_eq!(module_part("date-fns"), "DateFns");
    assert_eq!(module_part("@acme/lookup-lib"), "LookupLib");
    assert_eq!(module_part("p-map"), "PMap");
}

const REVIEWED: &str = r#"module Ffi.LookupLib exposing (lookup)

import JsError exposing (JsError)

lookup : String ->{; JsError} Option String
foreign async lookup from "lookup-lib" "lookup"
  arity 1
  catch JsError.from_js
  reviewed "lookup-lib@0.9.0 sha256:aa"

each : List Float -> (Float -> Unit) ->{} Unit
foreign sync each from "lookup-lib" "each"
  arity 2
  retention call
  reviewed "lookup-lib@1.0.0 sha256:old"

gone : Unit ->{} Unit
foreign sync gone from "lookup-lib" "gone"
  arity 0
  reviewed "lookup-lib@1.0.0 sha256:zz"
"#;

#[test]
fn reviewed_bindings_are_checked_against_the_declarations() {
    use super::check::{check_reviewed, reviewed};
    use crate::source::SourceMap;
    let mut sources = SourceMap::default();
    let file = sources.add("LookupLib.fl", REVIEWED);
    let bindings = reviewed(REVIEWED, file);
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0].fingerprint, "sha256:aa");
    let messages: Vec<String> = check_reviewed(&bindings, &lookup_surface())
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(
        messages,
        [
            "`lookup` may return `undefined` or `null`: its reviewed binding `lookup` needs `nullable`",
            "the declaration of `each` changed since `each` was reviewed against lookup-lib@1.0.0 sha256:old",
            "`lookup-lib@1.0.0` no longer exports `gone`, which `gone` was reviewed against",
        ]
    );
}

#[test]
fn headers_record_the_import() {
    use super::check::{Header, header};
    assert_eq!(
        header("// flick ffi import date-fns --select format,addDays\nmodule X"),
        Some(Header {
            package: "date-fns".into(),
            select: Some(vec!["format".into(), "addDays".into()]),
        })
    );
    assert_eq!(header("module X"), None);
}

#[test]
fn any_pure_and_cancellation_are_never_implied() {
    let surface = Surface::parse(
        r#"{"package": "p", "version": "1.0.0", "declarations": "p/index.d.ts", "exports": [
          {"name": "wrap", "local": "wrap", "kind": "function", "fingerprint": "sha256:a",
           "signatures": [{"params": [{"name": "x", "optional": false, "rest": false, "type": {"k": "any"}}],
             "this": false, "typeParams": [], "result": {"k": "any"}}]},
          {"name": "fetchIt", "local": "fetchIt", "kind": "function", "fingerprint": "sha256:b",
           "signatures": [{"params": [], "this": false, "typeParams": [],
             "result": {"k": "promise", "type": {"k": "number"}}}]}]}"#,
    )
    .expect("surface parses");
    let entry = |generated: &Generated, name: &str| {
        let e = generated
            .entries
            .iter()
            .find(|e| e.export == name)
            .expect("entry");
        (e.status, e.detail.clone())
    };
    let plain = Policy {
        exports: BTreeMap::from([(
            "fetchIt".into(),
            ExportPolicy {
                catch: Some("JsError".into()),
                ..ExportPolicy::default()
            },
        )]),
        ..Policy::default()
    };
    let generated = generate(&surface, &plain, None, "P.Generated");
    // `any` becomes the opaque `JsValue`, and the call stays effectful
    assert_eq!(
        entry(&generated, "wrap"),
        (Status::Supported, "JsValue ->{} JsValue".into())
    );
    assert!(!generated.text.contains("foreign pure"));
    // an async call is not cancellable unless a policy says so
    assert!(!generated.text.contains("cancel signal"));

    let strict = generate(
        &surface,
        &Policy {
            strict: true,
            ..Policy::default()
        },
        None,
        "P.Generated",
    );
    assert_eq!(entry(&strict, "wrap").0, Status::Unsupported);
    assert_eq!(entry(&strict, "fetchIt").0, Status::NeedsPolicy);

    let explicit = Policy {
        exports: BTreeMap::from([
            (
                "fetchIt".into(),
                ExportPolicy {
                    catch: Some("JsError".into()),
                    cancel: Some("signal".into()),
                    ..ExportPolicy::default()
                },
            ),
            (
                "wrap".into(),
                ExportPolicy {
                    mode: Some("pure".into()),
                    ..ExportPolicy::default()
                },
            ),
        ]),
        ..Policy::default()
    };
    let generated = generate(&surface, &explicit, None, "P.Generated");
    assert!(generated.text.contains("cancel signal"));
    assert!(generated.text.contains("foreign pure wrap"));

    let wrong = Policy {
        exports: BTreeMap::from([
            (
                "fetchIt".into(),
                ExportPolicy {
                    catch: Some("JsError".into()),
                    mode: Some("pure".into()),
                    ..ExportPolicy::default()
                },
            ),
            (
                "wrap".into(),
                ExportPolicy {
                    cancel: Some("signal".into()),
                    ..ExportPolicy::default()
                },
            ),
        ]),
        ..Policy::default()
    };
    let generated = generate(&surface, &wrong, None, "P.Generated");
    assert_eq!(entry(&generated, "fetchIt").0, Status::Unsupported);
    assert_eq!(entry(&generated, "wrap").0, Status::Unsupported);
}

fn status_of(generated: &Generated, export: &str) -> (Status, String) {
    let e = generated
        .entries
        .iter()
        .find(|e| e.export == export)
        .expect("entry");
    (e.status, e.detail.clone())
}

const OPTIONS_SURFACE: &str = r#"{"package": "p", "version": "1.0.0", "declarations": "p/index.d.ts", "exports": [
  {"name": "retry", "local": "retry", "kind": "function", "fingerprint": "sha256:a",
   "signatures": [{"params": [{"name": "options", "optional": false, "rest": false,
     "type": {"k": "object", "name": "Options", "text": "Options"}}],
     "this": false, "typeParams": [], "result": {"k": "void"}}]},
  {"name": "logOptions", "local": "logOptions", "kind": "function", "fingerprint": "sha256:b",
   "signatures": [{"params": [], "this": false, "typeParams": [],
     "result": {"k": "object", "name": "Options", "text": "Options"}}]},
  {"name": "count", "local": "count", "kind": "function", "fingerprint": "sha256:c",
   "signatures": [{"params": [], "this": false, "typeParams": [], "result": {"k": "number"}}]}]"#;

#[test]
fn an_object_type_is_opaque_without_fields() {
    let surface = Surface::parse(&format!("{OPTIONS_SURFACE}}}")).expect("surface parses");
    let generated = generate(&surface, &Policy::default(), None, "P.Generated");
    assert_eq!(
        status_of(&generated, "logOptions"),
        (Status::Supported, "Unit ->{} Options".into())
    );
    // the members of `Options` never reach Flick: no record, no field
    assert!(
        generated.text.contains("\ntype Options\n"),
        "{}",
        generated.text
    );
    assert!(!generated.text.contains("retries"));
}

#[test]
fn a_name_of_two_typescript_types_blocks_what_uses_it() {
    let surface = Surface::parse(&format!(r#"{OPTIONS_SURFACE}, "ambiguous": ["Options"]}}"#))
        .expect("surface parses");
    let generated = generate(&surface, &Policy::default(), None, "P.Generated");
    for export in ["retry", "logOptions"] {
        let (status, detail) = status_of(&generated, export);
        assert_eq!(status, Status::Unsupported, "{export}");
        assert!(
            detail.contains("`Options` names more than one TypeScript type"),
            "{detail}"
        );
    }
    assert_eq!(status_of(&generated, "count").0, Status::Supported);
    assert!(!generated.text.contains("type Options"));
}
