//! Accessors on records of different fields and on polymorphic fields.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::SourceMap;

fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    crate::check(&sources, file)
}

/// A local accessor is generic over the other fields of a record as well,
/// as an explicit `fun` is (`tests/check/pass/types/accessors.fl` runs it
/// through the model), and a record without the field is still rejected.
#[test]
fn a_local_accessor_takes_records_of_different_fields() {
    for get in [".value", "fun r -> r.value"] {
        let text = format!(
            "module Probe exposing (values)\n\nvalues : Unit ->{{}} (Int, Bool)\nlet values () =\n  let get = {get}\n  (get {{ value = 1 }}, get {{ value = true; other = 2 }})\n"
        );
        let diagnostics = check(&text);
        assert!(diagnostics.is_empty(), "{get}: {diagnostics:?}");
        // a record without the field is still rejected
        let missing = text.replace("value = true; other = 2", "other = 2");
        let diagnostics = check(&missing);
        assert!(
            (diagnostics.iter()).any(|d| d.code == codes::T003 && d.severity == Severity::Error),
            "{get}: {diagnostics:?}"
        );
    }
}

const POLY: &str = "module Probe exposing (..)\n\ntype alias Poly = { id : forall a. a -> a }\n\n";

/// A function applied where it is written takes the type of its argument,
/// so a polymorphic field it reads is used at an instance, as `p.id` is.
/// (No list holds such a record: a type variable cannot stand for it.)
#[test]
fn a_polymorphic_field_is_read_by_a_function_applied_to_its_record() {
    for (signature, body) in [
        ("Poly -> (Int, String)", "(.id p 1, .id p \"a\")"),
        ("Poly -> (Int, String)", "((p |> .id) 1, (p |> .id) \"a\")"),
        (
            "Poly -> (Int, String)",
            "((fun r -> r.id) p 1, (fun r -> r.id) p \"a\")",
        ),
        ("Poly -> Int", "(p |> fun r -> r.id) 1"),
    ] {
        let text = format!("{POLY}read : {signature}\nlet read p = {body}\n");
        let diagnostics = check(&text);
        assert!(diagnostics.is_empty(), "{body}: {diagnostics:?}");
    }
}

/// A function whose type is fixed before its argument, as a named one,
/// still takes a monomorphic record.
#[test]
fn a_polymorphic_field_needs_its_record_before_a_named_function() {
    for (signature, body) in [
        ("Poly -> Int", "\n  let get = .id\n  get p 1"),
        ("Poly -> Int", "\n  let get = fun r -> r.id\n  get p 1"),
    ] {
        let text = format!("{POLY}read : {signature}\nlet read p = {body}\n");
        let diagnostics = check(&text);
        assert!(
            (diagnostics.iter()).any(|d| d.code == codes::T003
                && d.message.contains("cannot stand for the polymorphic type")),
            "{body}: {diagnostics:?}"
        );
    }
}
