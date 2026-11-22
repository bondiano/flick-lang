//! IDE queries, control-flow highlighting, completion, navigation, rename, and
//! code actions for `let ... else`.

use super::actions::Action;
use super::tests::fixture_of;
use super::{HighlightKind, Target};
use crate::diagnostics::{Diagnostic, Severity};

fn u32_len(s: &str) -> u32 {
    u32::try_from(s.len()).expect("len fits in u32")
}

fn apply(text: &str, action: &Action) -> String {
    let mut edits = action.edits.clone();
    edits.sort_by_key(|e| e.span.start);
    let mut out = text.to_owned();
    for e in edits.iter().rev() {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.replacement);
    }
    out
}

#[test]
fn hover_on_let_else_keywords_and_shadowed_bindings() {
    let src = "module App.Main exposing (greet)\n\ngreet : String -> Option String -> String\nlet greet x opt =\n  let Some x = opt else \"outer:\" ++ x\n  \"inner:\" ++ x\n";
    let f = fixture_of(&[("src/App/Main.fl", src)]);
    let file = f.file("App.Main");
    let ide = f.ide();

    let let_at = f.at("App.Main", "let Some x", 0);
    let (let_hover, _) = ide.hover(file, let_at).expect("hover on let");
    assert!(
        let_hover.contains("String") && let_hover.contains("current block"),
        "{let_hover}"
    );

    let else_at = f.at("App.Main", "else \"outer:", 0);
    let (else_hover, _) = ide.hover(file, else_at).expect("hover on else");
    assert!(
        else_hover.contains("String") && else_hover.contains("does not match"),
        "{else_hover}"
    );

    // Shadowed `x` in fallback resolves to parameter `x`; in remainder resolves to pattern `x`.
    let param_x = f.at("App.Main", "greet x opt", 0) + u32_len("greet ");
    let pat_x = f.at("App.Main", "Some x", 0) + u32_len("Some ");
    let fb_x = f.at("App.Main", "\"outer:\" ++ x", 0) + u32_len("\"outer:\" ++ ");
    let rem_x = f.at("App.Main", "\"inner:\" ++ x", 0) + u32_len("\"inner:\" ++ ");

    let (fb_sym, _) = ide.symbol_at(file, fb_x).expect("fallback sym");
    let fb_def = ide.definition(&fb_sym).expect("fallback def");
    assert_eq!(fb_def.target, Target::File(file));
    assert_eq!(
        fb_def.start, param_x,
        "fallback `x` resolves to outer parameter"
    );

    let (rem_sym, _) = ide.symbol_at(file, rem_x).expect("remainder sym");
    let rem_def = ide.definition(&rem_sym).expect("remainder def");
    assert_eq!(rem_def.target, Target::File(file));
    assert_eq!(
        rem_def.start, pat_x,
        "remainder `x` resolves to pattern binder"
    );

    // Rename of pattern `x` renames only the pattern binder and remainder use, NOT parameter/fallback.
    let mut edits = ide
        .rename(file, rem_x, "inner_x")
        .expect("rename pattern x");
    let mut after = src.to_owned();
    edits.sort_by_key(|(_, (s, _))| *s);
    for (_, (s, e)) in edits.iter().rev() {
        after.replace_range(*s as usize..*e as usize, "inner_x");
    }
    assert_eq!(
        after,
        "module App.Main exposing (greet)\n\ngreet : String -> Option String -> String\nlet greet x opt =\n  let Some inner_x = opt else \"outer:\" ++ x\n  \"inner:\" ++ inner_x\n"
    );
}

#[test]
fn document_highlights_mark_let_else_fallback_and_exact_nearest_remainder() {
    let src = "module App.Main exposing (greet)\n\ngreet : Option String -> String\nlet greet raw =\n  let name =\n    let Some s = raw else \"guest\"\n    String.trim s\n  \"hi \" ++ name\n";
    let f = fixture_of(&[("src/App/Main.fl", src)]);
    let file = f.file("App.Main");
    let ide = f.ide();

    let else_at = f.at("App.Main", "else \"guest\"", 0);
    let highlights = ide.document_highlights(file, else_at);
    let slices: Vec<(HighlightKind, &str)> = highlights
        .iter()
        .map(|h| (h.kind, &src[h.start as usize..h.end as usize]))
        .collect();
    assert_eq!(
        slices,
        vec![
            (HighlightKind::Write, "let"),
            (HighlightKind::Write, "else"),
            (HighlightKind::Read, "\"guest\""),
            (HighlightKind::Text, "String.trim s"),
        ],
        "nested let-else must highlight ONLY the inner block remainder, not the outer `\"hi \" ++ name`"
    );

    // Same result when cursor is on `let`.
    let let_at = f.at("App.Main", "let Some s", 0);
    assert_eq!(ide.document_highlights(file, let_at), highlights);
}

#[test]
fn completion_after_let_else_expects_enclosing_block_type() {
    let src = "module App.Main exposing (pick)\n\ntype Status = Ready | Missing\n\npick : Option Status -> Status\nlet pick m =\n  let Some s = m else \n  s\n";
    let f = fixture_of(&[("src/App/Main.fl", src)]);
    let file = f.file("App.Main");
    let at = f.at("App.Main", "else \n", 0) + u32_len("else ");
    let completions = f.ide().complete(file, at, false);
    let names: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
    assert!(
        names.contains(&"Missing") && names.contains(&"Ready"),
        "expected constructors of enclosing return type `Status`, got {names:?}"
    );
}

#[test]
fn expand_let_else_moves_full_remainder_preserves_comments_signatures_and_shadowing() {
    let src = "module App.Main exposing (run)\n\nrun : String -> Option String -> String\nlet run x opt =\n  let Some x = opt else \"outer:\" ++ x // fallback note\n  // remainder note\n  tag : String -> String\n  let tag s = \"inner:\" ++ s\n  tag x\n";
    let f = fixture_of(&[("src/App/Main.fl", src)]);
    let file = f.file("App.Main");
    let at = f.at("App.Main", "let Some x", 0);
    let actions = f.ide().actions(file, at, at);
    // `add the signature` and `inline` must NOT be offered on `let-else`.
    assert!(
        actions
            .iter()
            .all(|a| !a.title.starts_with("add the signature") && !a.title.starts_with("inline")),
        "unexpected signature/inline action on let-else: {actions:?}"
    );
    let expand = actions
        .iter()
        .find(|a| a.title == "expand `let ... else` to `match`")
        .unwrap_or_else(|| panic!("missing expand action in {actions:?}"));
    let expanded = apply(src, expand);
    assert_eq!(
        expanded,
        "module App.Main exposing (run)\n\nrun : String -> Option String -> String\nlet run x opt =\n  match opt with\n  | Some x ->\n    // remainder note\n    tag : String -> String\n    let tag s = \"inner:\" ++ s\n    tag x\n  | _ -> \"outer:\" ++ x // fallback note\n"
    );
    let checked = fixture_of(&[("src/App/Main.fl", &expanded)]);
    assert!(
        checked
            .ide()
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "expanded match had diagnostics: {:?}",
        checked.ide().analysis.diagnostics
    );
}

#[test]
fn collapse_match_to_let_else_handles_wildcard_and_exhaustive_none_and_rejects_guards() {
    let wildcard = "module App.Main exposing (run)\n\nrun : Option Int -> Int\nlet run m =\n  match m with\n  | Some n ->\n    let doubled = n * 2\n    doubled + 1\n  | _ -> 0\n";
    let f = fixture_of(&[("src/App/Main.fl", wildcard)]);
    let file = f.file("App.Main");
    let at = f.at("App.Main", "match m", 0);
    let action = f
        .ide()
        .actions(file, at, at)
        .into_iter()
        .find(|a| a.title == "rewrite `match` as `let ... else`")
        .expect("collapse wildcard match");
    let collapsed = apply(wildcard, &action);
    assert_eq!(
        collapsed,
        "module App.Main exposing (run)\n\nrun : Option Int -> Int\nlet run m =\n  let Some n = m else 0\n  let doubled = n * 2\n  doubled + 1\n"
    );

    // Exhaustive `| Some n -> ... | None -> ...` also collapses cleanly.
    let none_arm = wildcard.replace("| _ -> 0", "| None -> 0");
    let f2 = fixture_of(&[("src/App/Main.fl", &none_arm)]);
    let at2 = f2.at("App.Main", "match m", 0);
    let action2 = f2
        .ide()
        .actions(f2.file("App.Main"), at2, at2)
        .into_iter()
        .find(|a| a.title == "rewrite `match` as `let ... else`")
        .expect("collapse exhaustive None match");
    assert_eq!(apply(&none_arm, &action2), collapsed);

    // Guarded arm must NOT offer collapse to `let-else`.
    let guarded = "module App.Main exposing (run)\n\nrun : Option Int -> Int\nlet run m =\n  match m with\n  | Some n when n > 0 -> n\n  | _ -> 0\n";
    let f3 = fixture_of(&[("src/App/Main.fl", guarded)]);
    let at3 = f3.at("App.Main", "match m", 0);
    assert!(
        f3.ide()
            .actions(f3.file("App.Main"), at3, at3)
            .iter()
            .all(|a| a.title != "rewrite `match` as `let ... else`")
    );
}

#[test]
fn explicit_refactor_replaces_irrefutable_let_else_only_when_rechecked_clean() {
    let src = "module App.Main exposing (sum)\n\nsum : (Int, Int) -> Int\nlet sum pair =\n  let (a, b) = pair else 0\n  a + b\n";
    let f = fixture_of(&[("src/App/Main.fl", src)]);
    let file = f.file("App.Main");
    let at = f.at("App.Main", "else 0", 0);
    let action = f
        .ide()
        .actions(file, at, at)
        .into_iter()
        .find(|a| a.title == "replace with `let` (removes unreachable `else`)")
        .expect("explicit irrefutable let-else refactor");
    assert_eq!(action.kind, "refactor.rewrite");
    let rewritten = apply(src, &action);
    assert_eq!(
        rewritten,
        "module App.Main exposing (sum)\n\nsum : (Int, Int) -> Int\nlet sum pair =\n  let (a, b) = pair\n  a + b\n"
    );
    let checked = fixture_of(&[("src/App/Main.fl", &rewritten)]);
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
