use super::import::{Status, flick_name, generate};
use super::{Policy, Surface, module_part};

fn lookup_surface() -> Surface {
    Surface::parse(include_str!("fixtures/lookup.json")).expect("fixture parses")
}

const LOOKUP_POLICY: &str = r#"
[exports.lookup]
undefined = "none"
catch = "JsError"

[exports.each]
retention = "call"
"#;

#[test]
fn lookup_with_its_policy_is_the_binding_of_the_spec() {
    let policy = Policy::parse(LOOKUP_POLICY).expect("policy parses");
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
        (Status::Supported, "Unit ->{} Number".into())
    );
    assert!(
        generated
            .text
            .contains("foreign blocked lookup \"needs policy: ")
    );
}

#[test]
fn strict_mode_refuses_unknown() {
    let policy = Policy::parse("strict = true").expect("policy parses");
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

each : List Number -> (Number -> Unit) ->{} Unit
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
