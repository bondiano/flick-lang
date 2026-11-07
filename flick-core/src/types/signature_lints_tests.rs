use std::cmp::Reverse;

use crate::diagnostics::{Edit, Severity, codes};
use crate::source::SourceMap;

/// `signature` of the foreign function `f` after the fixes of its T059, with
/// the metadata line `meta`, and what the check says of the result.
fn fixed(signature: &str, meta: &str) -> String {
    let declaration = |signature: &str| {
        format!(
            "module Probe exposing (..)\n{signature}\nforeign effect f from \"./x.js\" \"f\"\n  {meta}\n"
        )
    };
    let text = declaration(signature);
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.clone());
    let diagnostics = crate::check(&sources, file);
    let mut edits: Vec<&Edit> = (diagnostics.iter())
        .filter(|d| d.code == codes::T059)
        .flat_map(|d| &d.fixes)
        .inspect(|f| assert!(f.safe, "{f:?}"))
        .flat_map(|f| &f.edits)
        .collect();
    edits.sort_by_key(|e| Reverse(e.span.start));
    let mut out = text;
    for edit in edits {
        out.replace_range(edit.span.range(), &edit.replacement);
    }
    let start = out.find("f :").expect("the signature");
    let end = out.find("\nforeign").expect("the declaration");
    let written = out[start..end].to_owned();

    let mut sources = SourceMap::default();
    let again = sources.add("Probe.fl", out.clone());
    let left: Vec<_> = (crate::check(&sources, again).into_iter())
        .filter(|d| d.code == codes::T059 || d.severity == Severity::Error)
        .collect();
    assert!(left.is_empty(), "{out}: {left:?}");
    written
}

#[test]
fn the_fix_leaves_what_a_stage_qualifier_qualifies() {
    assert_eq!(
        fixed("f : (Int -> Int)^c -> (Int -> Int)^c", "retention call"),
        "f : (Int -> Int)^c -> Int -> Int"
    );
    assert_eq!(
        fixed(
            "f : (Int -> Int)^c -> (Int -> (Int -> Int)^c)^c",
            "retention call"
        ),
        "f : (Int -> Int)^c -> Int -> Int -> Int"
    );
    assert_eq!(
        fixed("f : (Int -> Int)^c -> Int -> Int", "retention call"),
        "f : (Int -> Int)^c -> Int -> Int"
    );
}

#[test]
fn the_fix_leaves_the_constraints_retention_owner_does_not_give() {
    assert_eq!(
        fixed(
            "f : (Fiber.Outlives c s, Fiber.Provided r s, Detached r) => (Eff r none Unit)^c ->{Scope s | r;} Unit",
            "retention owner"
        ),
        "f : (Eff r none Unit)^c ->{Scope s | r;} Unit"
    );
    assert_eq!(
        fixed(
            "f : (Fiber.Outlives c s, Fiber.Provided r s, Fiber.Provided r2 s, Detached r, Detached r2) => Eff r2 none Unit -> (Eff r none Unit)^c ->{Scope s | r + r2;} Unit",
            "retention owner"
        ),
        "f : Fiber.Provided r2 s => Eff r2 none Unit -> (Eff r none Unit)^c ->{Scope s | r + r2;} Unit"
    );
    // nothing of `retention call`: not the region of an owner
    let written = "f : Fiber.Outlives c s => (Eff none none Unit)^c ->{Scope s;} Unit";
    assert_eq!(fixed(written, "retention call"), written);
}

#[test]
fn a_fix_would_not_take_a_comment_away() {
    let text = "module Probe exposing (..)\nf :\n  (Int -> Int)^c -> (\n    // the rest\n    Int -> Int\n  )^c\nforeign pure f from \"./x.js\" \"f\"\n  retention call\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text);
    let diagnostics = crate::check(&sources, file);
    let warning = (diagnostics.iter())
        .find(|d| d.code == codes::T059)
        .expect("the warning");
    assert!(warning.fixes.is_empty(), "{warning:?}");
}
