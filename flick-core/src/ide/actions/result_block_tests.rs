//! Bidirectional code actions between canonical `try … with` and `result { … }`.

use super::tests::{action, applied, main};
use crate::diagnostics::Diagnostic;

#[test]
fn canonical_try_rewrites_to_result_and_rejects_non_canonical_forms() {
    let base = "module App.Main exposing (run)\n\ntype error NotFound = NotFound Int\ntype error Timeout = Timeout\n\nfail_it : Int ->{; NotFound, Timeout} Int\nlet fail_it n = if n == 0 then Fail.fail (NotFound n) else Fail.fail Timeout\n\n";

    // Single-line canonical try -> result
    let single = format!(
        "{base}run : Int ->{{}} Result (Errors {{NotFound, Timeout}}) Int\nlet run n =\n  try fail_it n with\n  | return x -> Ok x\n  | e -> Err e\n"
    );
    let (f, text) = main(&single);
    let at = f.at("App.Main", "try fail_it", 0);
    let a = action(&f, at, at, "rewrite as `result").expect("forward action");
    let fixed = applied(&text, &a);
    assert!(fixed.contains("result { fail_it n }"), "{fixed}");
    let checked = main(&fixed).0;
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );

    // Reversed arm order + `Fail.fail e` + multi-line body with comment preserved
    let reversed = format!(
        "{base}run : Int ->{{}} Result (Errors {{NotFound, Timeout}}) Int\nlet run n =\n  try\n    // compute value\n    let x = fail_it n\n    x + 1\n  with\n  | Fail.fail e -> Prelude.Err e\n  | return v -> Prelude.Ok v\n"
    );
    let (f, text) = main(&reversed);
    let at = f.at("App.Main", "try", 0);
    let a = action(&f, at, at, "rewrite as `result").expect("reversed forward action");
    let fixed = applied(&text, &a);
    assert!(
        fixed.contains("result {\n    // compute value\n    let x = fail_it n\n    x + 1\n  }"),
        "{fixed}"
    );
    let checked = main(&fixed).0;
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );

    // Negative cases: extra arm, guard, narrowed pattern, annotated pattern,
    // comment on arm, shadowed Ok/Err
    let negatives = [
        // extra arm
        format!(
            "{base}run : Int ->{{}} Result (Errors {{Timeout}}) Int\nlet run n =\n  try fail_it n with\n  | return x -> Ok x\n  | NotFound _ -> Ok 0\n  | e -> Err e\n"
        ),
        // guarded arm
        format!(
            "{base}run : Int ->{{; NotFound, Timeout}} Result (Errors {{NotFound, Timeout}}) Int\nlet run n =\n  try fail_it n with\n  | return x when x > 0 -> Ok x\n  | e -> Err e\n"
        ),
        // narrowed pattern (not catch-all)
        format!(
            "{base}run : Int ->{{; Timeout}} Result NotFound Int\nlet run n =\n  try fail_it n with\n  | return x -> Ok x\n  | NotFound k -> Err (NotFound k)\n"
        ),
        // annotated pattern
        format!(
            "{base}run : Int ->{{}} Result (Errors {{NotFound, Timeout}}) Int\nlet run n =\n  try fail_it n with\n  | return (x : Int) -> Ok x\n  | e -> Err e\n"
        ),
        // comment on arm must not be silently dropped
        format!(
            "{base}run : Int ->{{}} Result (Errors {{NotFound, Timeout}}) Int\nlet run n =\n  try fail_it n with\n  // arm note\n  | return x -> Ok x\n  | e -> Err e\n"
        ),
        // shadowed Ok/Err constructor
        format!(
            "{base}type MyRes e a = Ok a | Err e\nrun : Int ->{{}} MyRes (Errors {{NotFound, Timeout}}) Int\nlet run n =\n  try fail_it n with\n  | return x -> Ok x\n  | e -> Err e\n"
        ),
    ];
    for (idx, src) in negatives.iter().enumerate() {
        let (f, _) = main(src);
        let at = f.at("App.Main", "try fail_it", 0);
        assert!(
            action(&f, at, at, "rewrite as `result").is_none(),
            "case {idx} must not offer rewrite: {src}"
        );
    }
}

#[test]
fn result_block_expands_to_try_with_hygiene_comments_and_parens() {
    let base = "module App.Main exposing (run)\n\ntype error NotFound = NotFound Int\n\nfail_it : Int ->{; NotFound} Int\nlet fail_it n = Fail.fail (NotFound n)\n\n";

    // Basic single-line expansion + hygienic binder avoiding `value`/`err`
    let src = format!(
        "{base}run : Int ->{{}} Result NotFound Int\nlet run value =\n  let err = value\n  result {{ fail_it err }}\n"
    );
    let (f, text) = main(&src);
    let at = f.at("App.Main", "result {", 0);
    let a = action(&f, at, at, "expand `result").expect("reverse action");
    let fixed = applied(&text, &a);
    assert!(
        fixed.contains("try fail_it err with") && fixed.contains("| return "),
        "{fixed}"
    );
    let checked = main(&fixed).0;
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );

    // Precedence: `result { ... }?` gets parenthesized
    let postfix =
        format!("{base}run : Int ->{{; NotFound}} Int\nlet run n = result {{ fail_it n }}?\n");
    let (f, text) = main(&postfix);
    let at = f.at("App.Main", "result {", 0);
    let a = action(&f, at, at, "expand `result").expect("postfix reverse action");
    let fixed = applied(&text, &a);
    assert!(fixed.contains("(\n") && fixed.contains(")?"), "{fixed}");
    let checked = main(&fixed).0;
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );

    // Multi-line comment preservation + shadowed Ok/Err qualification
    let shadowed = format!(
        "{base}type Shadow e a = Ok a | Err e\nrun : Int ->{{}} Result NotFound Int\nlet run n =\n  result {{\n    // keep me\n    let x = fail_it n\n    x + 1\n  }}\n"
    );
    let (f, text) = main(&shadowed);
    let at = f.at("App.Main", "result {", 0);
    let a = action(&f, at, at, "expand `result").expect("shadowed reverse action");
    let fixed = applied(&text, &a);
    assert!(
        fixed.contains("// keep me")
            && fixed.contains("Prelude.Ok")
            && fixed.contains("Prelude.Err"),
        "{fixed}"
    );
    let checked = main(&fixed).0;
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        checked.ide().analysis.diagnostics
    );
}
