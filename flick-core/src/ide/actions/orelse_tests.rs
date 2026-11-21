use super::super::tests::fixture_of;
use super::tests::{action, applied, main};
use crate::diagnostics::Severity;

#[test]
fn orelse_expands_hygienically_and_folds_back_from_exact_option_match() {
    let src = "module App.Main exposing (pick)\n\npick : Option String -> String -> String\nlet pick opt value =\n  opt /* fallback */ orelse value\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "orelse", 0) + 1;
    let expand = action(&f, at, at, "expand `orelse`").expect("expand action");
    let expanded = applied(&text, &expand);
    assert!(expanded.contains("match opt with"), "{expanded}");
    assert!(expanded.contains("/* fallback */"), "{expanded}");
    assert!(
        expanded.contains("| Some value2 -> value2"),
        "must avoid capturing parameter `value`: {expanded}"
    );
    assert!(expanded.contains("| None -> value"), "{expanded}");
    let checked = fixture_of(&[("src/App/Main.fl", &expanded)]);
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        checked.ide().analysis.diagnostics
    );

    // Exact match without dropped comments folds cleanly in either arm order.
    for match_src in [
        "module App.Main exposing (pick)\n\npick : Option Int -> Int -> Int\nlet pick opt fallback =\n  match opt with\n  | Some x -> x\n  | None -> fallback\n",
        "module App.Main exposing (pick)\n\npick : Option Int -> Int -> Int\nlet pick opt fallback =\n  match opt with\n  | None -> fallback\n  | Some x -> x\n",
    ] {
        let (f, text) = main(match_src);
        let at = f.at("App.Main", "match opt", 0) + 1;
        let fold = action(&f, at, at, "fold `match`").expect("fold action");
        let folded = applied(&text, &fold);
        assert!(folded.contains("opt orelse fallback"), "{folded}");
        let checked = fixture_of(&[("src/App/Main.fl", &folded)]);
        assert!(
            checked
                .ide()
                .analysis
                .diagnostics
                .iter()
                .all(|d| d.severity != Severity::Error),
            "{:?}",
            checked.ide().analysis.diagnostics
        );
    }

    // Expanding inside an application adds outer parentheses; folding a pipe adds grouping parens.
    let nested = "module App.Main exposing (shown)\n\nshown : Option Int -> String\nlet shown opt = show (opt orelse 0)\n";
    let (f, text) = main(nested);
    let at = f.at("App.Main", "orelse", 0) + 1;
    let expand = action(&f, at, at, "expand `orelse`").expect("expand in parens");
    let expanded = applied(&text, &expand);
    let checked = fixture_of(&[("src/App/Main.fl", &expanded)]);
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        checked.ide().analysis.diagnostics
    );
}

#[test]
fn orelse_refuses_semantic_changing_actions_and_unsafe_fixes() {
    // 1. Eager `Option.with_default` must never offer an automatic or safe-fix rewrite to lazy `orelse`.
    let eager = "module App.Main exposing (pick)\n\npick : Option Int -> Int -> Int\nlet pick opt fallback = Option.with_default fallback opt\n";
    let (f, _) = main(eager);
    let at = f.at("App.Main", "with_default", 0) + 1;
    assert!(action(&f, at, at, "fold").is_none());
    assert!(
        f.ide()
            .analysis
            .diagnostics
            .iter()
            .all(|d| d.fixes.is_empty()),
        "{:?}",
        f.ide().analysis.diagnostics
    );

    // 2. Guarded arm, extra computation, multi-statement None, nested pattern, shadowed Option, and comments block fold.
    for non_foldable in [
        "module App.Main exposing (f)\nf : Option Int -> Int\nlet f opt =\n  match opt with\n  | Some x when x > 0 -> x\n  | _ -> 0\n",
        "module App.Main exposing (f)\nf : Option Int -> Int\nlet f opt =\n  match opt with\n  | Some x -> x + 1\n  | None -> 0\n",
        "module App.Main exposing (f)\nf : Option Int -> Int\nlet f opt =\n  match opt with\n  | Some x -> x\n  | None ->\n    let y = 1\n    y\n",
        "module App.Main exposing (f)\nf : Option (Option Int) -> Int\nlet f opt =\n  match opt with\n  | Some (Some x) -> x\n  | _ -> 0\n",
        "module App.Main exposing (f, MyOpt(..))\ntype MyOpt a = Some a | None\nf : MyOpt Int -> Int\nlet f opt =\n  match opt with\n  | Some x -> x\n  | None -> 0\n",
        "module App.Main exposing (f)\nf : Option Int -> Int\nlet f opt =\n  match opt with\n  // keep me\n  | Some x -> x\n  | None -> 0\n",
    ] {
        let (f, _) = main(non_foldable);
        let at = f.at("App.Main", "match opt", 0) + 1;
        assert!(
            action(&f, at, at, "fold `match`").is_none(),
            "should not fold:\n{non_foldable}"
        );
    }

    // 3. Invalid `orelse` gets no unsafe diagnostic quickfix and no expand action.
    let bad = "module App.Main exposing (bad)\nbad : Int\nlet bad = 7 orelse 0\n";
    let (f, _) = main(bad);
    let at = f.at("App.Main", "orelse", 0) + 1;
    assert!(action(&f, at, at, "expand `orelse`").is_none());
    let t003 = f
        .ide()
        .analysis
        .diagnostics
        .iter()
        .find(|d| d.code.to_string() == "FLICK-T003")
        .expect("T003");
    assert!(t003.fixes.is_empty(), "{t003:?}");
}
