//! Vertical-slice integration tests for scope-owned `defer` / `errdefer`:
//! formatting fixed-point, IR owner-evidence lowering, and IDE/LSP behavior.

use std::io;
use std::path::{Path, PathBuf};

use flick_core::fmt;
use flick_core::ir::{Expr, ExprKind, ItemKind, Stmt};
use flick_core::project::{self, Host};
use flick_core::source::SourceMap;
use flick_core::syntax::parse;

struct SingleFile<'a>(&'a str);

impl Host for SingleFile<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix("/").unwrap_or(path).display().to_string()
    }
}

fn format_once(src: &str) -> String {
    let mut sources = SourceMap::default();
    let file = sources.add("DeferFmt.fl", src);
    fmt::format(src, file).expect("valid source formats cleanly")
}

#[test]
fn formatter_preserves_defer_errdefer_and_reaches_fixed_point() {
    let inputs = [
        // Single-line and multi-line defer/errdefer with comments
        r#"module App.Main exposing (main)

say : String ->{} Unit
let say s = Host.raw_stdout_line s

main : Unit ->{} Unit
let main () = scope {
  // leading comment
  defer { say "one" }
  errdefer {
    // inside comment
    say "a"
    say "b"
  }
  say "body"
}
"#,
        // Contextual identifiers and grouped record calls must NOT lose parens!
        r"module App.Compat exposing (run)

run : ({ n : Int } -> Int) -> Int
let run defer =
  let errdefer = defer
  (defer) { n = 1 } + (errdefer) { n = 2 }
",
        // Compact/messy spacing normalized cleanly
        r"module App.Messy exposing (f)

f : Unit ->{} Unit
let f () = scope { defer { () }; errdefer { () }; () }
",
    ];

    for input in inputs {
        let first = format_once(input);
        let second = format_once(&first);
        assert_eq!(first, second, "formatter not idempotent on:\n{input}");
        let mut sources = SourceMap::default();
        let file = sources.add("Reparse.fl", &first);
        let reparsed = parse(&first, file);
        assert!(
            reparsed.diagnostics.is_empty(),
            "formatted code failed re-parse:\n{first}\n{:?}",
            reparsed.diagnostics
        );
    }

    // Explicitly verify `(defer) { n = 1 }` keeps parentheses!
    let grouped = format_once(
        "module App.Compat exposing (run)\n\n\
         run : ({ n : Int } -> Int) -> Int\n\
         let run defer = (defer) { n = 1 }\n",
    );
    assert!(
        grouped.contains("(defer) { n = 1 }"),
        "formatter dropped parens from grouped record call:\n{grouped}"
    );
}

fn contains_expr(e: &Expr, pred: &dyn Fn(&ExprKind) -> bool) -> bool {
    if pred(&e.kind) {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| {
        if contains_expr(c, pred) {
            found = true;
        }
    });
    found
}

fn contains_with_owner_defer(e: &Expr) -> bool {
    contains_expr(e, &|kind| {
        matches!(
            kind,
            ExprKind::WithOwnerEvidence(_, body)
                if contains_expr(body, &|k| matches!(
                    k,
                    ExprKind::Global { module, name, .. } if module == "Scope" && name == "defer"
                ))
        ) || matches!(
            kind,
            ExprKind::Block(stmts, tail)
                if stmts.iter().any(|s| matches!(
                    s,
                    Stmt::Let { value: rhs, .. }
                        if matches!(&rhs.kind, ExprKind::Global { module, name, .. } if module == "Scope" && name == "defer")
                )) && contains_expr(tail, &|k| matches!(k, ExprKind::WithOwnerEvidence(..)))
        )
    })
}

#[test]
fn ir_lowers_defer_and_errdefer_with_explicit_owner_evidence_and_hygiene() {
    let src = "\
module App.Lower exposing (run)

service S =
  noop : Unit ->{} Unit

layer : Layer none none {S}
let layer = Layer.succeed S { noop = fun () -> () }

run : Unit ->{} Unit
let run () = scope outer {
  let exit = \"user-local\"
  provide layer {
    S.noop ()
    defer { Host.raw_stdout_line exit }
    errdefer { Host.raw_stdout_line exit }
  }
}
";
    let mut sources = SourceMap::default();
    let path = PathBuf::from("/Main.fl");
    let (diagnostics, compilation) =
        project::compile(&SingleFile(src), &mut sources, None, &[path]);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    let ir = &compilation
        .modules
        .iter()
        .find(|m| m.name == "App.Lower")
        .expect("App.Lower compiled")
        .ir;
    let run_body = ir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == "run" => Some(body),
            _ => None,
        })
        .expect("run item");

    assert!(
        contains_with_owner_defer(run_body),
        "expected WithOwnerEvidence wrapping Scope.defer call in IR:\n{run_body:#?}"
    );
    assert!(
        contains_expr(run_body, &|k| {
            matches!(k, ExprKind::Global { module, name, .. } if module == "Exit" && name == "is_failure")
        }),
        "expected Exit.is_failure in errdefer IR:\n{run_body:#?}"
    );
}

#[cfg(feature = "ide")]
mod ide_tests {
    use super::*;
    use flick_core::diagnostics::codes;
    use flick_core::ide::{CompletionKind, Ide, TOKEN_TYPES};
    use flick_core::source::FileId;

    fn analyze(src: &str) -> (SourceMap, FileId, project::Analysis) {
        let mut sources = SourceMap::default();
        let path = PathBuf::from("/Main.fl");
        let analysis = project::analyze(&SingleFile(src), &mut sources, None, &[path]);
        let file = analysis.modules.first().expect("analyzed module").file;
        (sources, file, analysis)
    }

    fn offset_of(src: &str, needle: &str) -> u32 {
        u32::try_from(src.find(needle).expect("needle present")).expect("offset fits u32")
    }

    #[test]
    fn hover_shows_selected_owner_location_and_mode_including_utf16_and_provide() {
        let src = "\
module App.Hover exposing (run)

service S =
  noop : Unit ->{} Unit

layer : Layer none none {S}
let layer = Layer.succeed S { noop = fun () -> () }

run : Unit ->{} Unit
let run () =
  let _ = \"😀\"
  scope owner {
    provide layer {
      S.noop ()
      defer { Host.raw_stdout_line \"d\" }
      errdefer { Host.raw_stdout_line \"e\" }
    }
    scope {
      defer { Host.raw_stdout_line \"inner\" }
    }
  }
";
        let (sources, file, analysis) = analyze(src);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let ide = Ide {
            analysis: &analysis,
            sources: &sources,
        };

        let defer_at = offset_of(src, "defer { Host.raw_stdout_line \"d\" }");
        let (text, range) = ide.hover(file, defer_at + 1).expect("hover on defer");
        assert_eq!(range, (defer_at, defer_at + 5));
        assert!(text.contains("defer { … } : Unit"), "{text}");
        assert!(text.contains("`scope owner`"), "{text}");
        assert!(text.contains("line 12, column 3"), "{text}");
        assert!(text.contains("every close"), "{text}");

        let errdefer_at = offset_of(src, "errdefer { Host.raw_stdout_line \"e\" }");
        let (text, range) = ide.hover(file, errdefer_at + 2).expect("hover on errdefer");
        assert_eq!(range, (errdefer_at, errdefer_at + 8));
        assert!(text.contains("errdefer { … } : Unit"), "{text}");
        assert!(text.contains("`scope owner`"), "{text}");
        assert!(text.contains("line 12, column 3"), "{text}");
        assert!(text.contains("body fails"), "{text}");

        let inner_defer_at = offset_of(src, "defer { Host.raw_stdout_line \"inner\" }");
        let (text, _) = ide
            .hover(file, inner_defer_at + 1)
            .expect("hover on inner defer");
        assert!(
            text.contains("Owned by `scope` at line 18, column 5"),
            "{text}"
        );

        // UTF-16 column check when non-ASCII / surrogate-pair text precedes `scope` on the same line
        let utf16_src = "\
module App.Utf16 exposing (run)

tag : String -> Unit ->{} Unit
let tag _ () = ()

run : Unit ->{} Unit
let run () = tag \"😀\" (scope s { defer { () } })
";
        let (sources2, file2, analysis2) = analyze(utf16_src);
        assert!(
            analysis2.diagnostics.is_empty(),
            "{:?}",
            analysis2.diagnostics
        );
        let ide2 = Ide {
            analysis: &analysis2,
            sources: &sources2,
        };
        let d_at = offset_of(utf16_src, "defer { () }");
        let (hover_text, _) = ide2.hover(file2, d_at + 1).expect("hover utf16");
        // Line 7: `let run () = tag "😀" (scope s { defer { () } })`
        // UTF-16 units before `scope`: `let run () = tag "` (18) + `😀` (2) + `" (` (3) = 23 -> 1-based col 24!
        assert!(
            hover_text.contains("`scope s` at line 7, column 24"),
            "unexpected UTF-16 owner column:\n{hover_text}"
        );
    }

    #[test]
    fn semantic_tokens_distinguish_defer_keyword_from_identifier() {
        let src = "\
module App.Tokens exposing (run)

run : ({ n : Int } -> Int) ->{} Int
let run defer = scope {
  defer { () }
  errdefer { () }
  (defer) { n = 1 }
}
";
        let (sources, file, analysis) = analyze(src);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let ide = Ide {
            analysis: &analysis,
            sources: &sources,
        };
        let tokens = ide.semantic_tokens(file);
        let kw_idx = u32::try_from(
            TOKEN_TYPES
                .iter()
                .position(|&t| t == "keyword")
                .expect("keyword"),
        )
        .expect("token index fits u32");

        let defer_stmt = offset_of(src, "defer { () }");
        let errdefer_stmt = offset_of(src, "errdefer { () }");
        let grouped_call = offset_of(src, "(defer)") + 1;

        assert!(
            tokens.contains(&(defer_stmt, defer_stmt + 5, kw_idx)),
            "expected keyword token for defer statement: {tokens:?}"
        );
        assert!(
            tokens.contains(&(errdefer_stmt, errdefer_stmt + 8, kw_idx)),
            "expected keyword token for errdefer statement: {tokens:?}"
        );
        assert!(
            !tokens.contains(&(grouped_call, grouped_call + 5, kw_idx)),
            "(defer) must NOT be highlighted as keyword: {tokens:?}"
        );
    }

    #[test]
    fn completion_offers_defer_and_errdefer_only_in_explicit_scope_statement_position() {
        let src = "\
module App.Comp exposing (run, outside)

run : Unit ->{} Unit
let run () = scope {
  let _x = 1
  de
}

outside : Unit ->{} Unit
let outside () =
  de
";
        let (sources, file, analysis) = analyze(src);
        let ide = Ide {
            analysis: &analysis,
            sources: &sources,
        };
        let in_scope = offset_of(src, "  de\n}") + 4;
        let items = ide.complete(file, in_scope, false);
        assert!(
            items
                .iter()
                .any(|c| c.label == "defer" && c.kind == CompletionKind::Keyword),
            "expected `defer` completion inside scope"
        );
        assert!(
            items
                .iter()
                .any(|c| c.label == "errdefer" && c.kind == CompletionKind::Keyword),
            "expected `errdefer` completion inside scope"
        );

        let out_scope = offset_of(src, "outside () =\n  de") + 17;
        let out_items = ide.complete(file, out_scope, false);
        assert!(
            !out_items
                .iter()
                .any(|c| c.label == "defer" && c.kind == CompletionKind::Keyword),
            "did not expect `defer` keyword completion outside scope"
        );
    }

    #[test]
    fn code_actions_expand_simplify_keep_in_ref_and_avoid_unsafe_fixes() {
        let src = "\
module App.Actions exposing (run, generic_helper)

run : Unit ->{} Unit
let run () = scope {
  let exit = \"outer\"
  defer { Host.raw_stdout_line \"d\" }
  errdefer { Host.raw_stdout_line exit }
  Scope.defer (fun _ -> Host.raw_stdout_line \"s\")
}

generic_helper : Unit ->{Scope s} Unit
let generic_helper () =
  Scope.defer (fun _ -> Host.raw_stdout_line \"g\")
";
        let (sources, file, analysis) = analyze(src);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let ide = Ide {
            analysis: &analysis,
            sources: &sources,
        };

        // Expand `defer`
        let defer_at = offset_of(src, "defer { Host.raw_stdout_line \"d\" }");
        let actions = ide.actions(file, defer_at, defer_at);
        let expand = actions
            .iter()
            .find(|a| a.title == "expand `defer` to `Scope.defer`")
            .expect("expand defer action");
        assert_eq!(expand.kind, "refactor.rewrite");
        assert_eq!(
            expand.edits[0].replacement,
            "Scope.defer (fun _ -> Host.raw_stdout_line \"d\")"
        );

        // Expand `errdefer` must avoid shadowing `let exit = "outer"`!
        let errdefer_at = offset_of(src, "errdefer { Host.raw_stdout_line exit }");
        let err_actions = ide.actions(file, errdefer_at, errdefer_at);
        let expand_err = err_actions
            .iter()
            .find(|a| a.title == "expand `errdefer` to `Scope.defer`")
            .expect("expand errdefer action");
        assert!(
            expand_err.edits[0]
                .replacement
                .contains("fun scope_exit -> if Exit.is_failure scope_exit"),
            "expected hygienic binder avoiding `exit`: {}",
            expand_err.edits[0].replacement
        );

        // Simplify `Scope.defer` inside explicit `scope`
        let sd_at = offset_of(src, "Scope.defer (fun _ -> Host.raw_stdout_line \"s\")");
        let sd_actions = ide.actions(file, sd_at, sd_at);
        let simplify = sd_actions
            .iter()
            .find(|a| a.title == "simplify `Scope.defer` to `defer`")
            .expect("simplify action");
        assert_eq!(
            simplify.edits[0].replacement,
            "defer { Host.raw_stdout_line \"s\" }"
        );

        // Must NOT offer simplify on generic `Scope s` helper!
        let gen_at = offset_of(src, "Scope.defer (fun _ -> Host.raw_stdout_line \"g\")");
        let gen_actions = ide.actions(file, gen_at, gen_at);
        assert!(
            !gen_actions
                .iter()
                .any(|a| a.title.contains("simplify `Scope.defer`")),
            "must not simplify generic Scope.defer helper: {gen_actions:?}"
        );

        // Must offer `keep x in a Ref` when a `let mut` is used in `defer`,
        // and must NOT have safe fixes for unhandled errors in cleanup.
        let mut_src = "\
module App.MutDefer exposing (run)

type error Boom = Boom

fail_it : Unit ->{; Boom} Unit
let fail_it () = Fail.fail Boom

run : Unit ->{; Boom} Unit
let run () = scope {
  let mut count = 0
  defer {
    count = count + 1
    fail_it ()
  }
}
";
        let (sources2, file2, analysis2) = analyze(mut_src);
        assert!(
            analysis2
                .diagnostics
                .iter()
                .all(|d| d.fixes.iter().all(|f| !f.safe)),
            "cleanup errors must not attach unsafe safe_fix: {:?}",
            analysis2.diagnostics
        );
        let ide2 = Ide {
            analysis: &analysis2,
            sources: &sources2,
        };
        let count_at = offset_of(mut_src, "count = 0");
        let ref_actions = ide2.actions(file2, count_at, count_at);
        assert!(
            ref_actions
                .iter()
                .any(|a| a.title == "keep `count` in a `Ref`"),
            "expected `keep count in a Ref` action: {ref_actions:?}"
        );
    }

    #[test]
    fn stale_edits_refresh_defer_owner_hover() {
        let before = "\
module App.Stale exposing (run)

run : Unit ->{} Unit
let run () = scope outer {
  defer { () }
}
";
        let after = "\
module App.Stale exposing (run)

run : Unit ->{} Unit
let run () = scope outer {
  scope inner {
    defer { () }
  }
}
";
        let (s1, f1, a1) = analyze(before);
        let ide1 = Ide {
            analysis: &a1,
            sources: &s1,
        };
        let (h1, _) = ide1
            .hover(f1, offset_of(before, "defer") + 1)
            .expect("before hover");
        assert!(h1.contains("`scope outer`"), "{h1}");

        let (s2, f2, a2) = analyze(after);
        let ide2 = Ide {
            analysis: &a2,
            sources: &s2,
        };
        let (h2, _) = ide2
            .hover(f2, offset_of(after, "defer") + 1)
            .expect("after hover");
        assert!(h2.contains("`scope inner`"), "{h2}");
    }

    #[test]
    fn cross_feature_defer_with_result_catch_let_else_orelse_and_holes() {
        let ok_src = "\
module App.Cross exposing (run)

type error Boom = Boom

work : Unit ->{; Boom} Int
let work () = 1

run : Option Int ->{} Int
let run opt = scope outer {
  let Some n = opt else
    defer { Host.raw_stdout_line \"else\" }
    0
  let r = result {
    defer { Host.raw_stdout_line \"res\" }
    errdefer { Host.raw_stdout_line \"res-err\" }
    work ()
  }
  let v =
    work () catch
    | Boom ->
      defer { Host.raw_stdout_line \"caught\" }
      0
  let _ = try defer { () } with | Boom -> ()
  let fallback = opt orelse (n + v)
  match r with
  | Ok x -> x + fallback
  | Err _ -> fallback
}
";
        let (sources, file, analysis) = analyze(ok_src);
        assert!(
            analysis.diagnostics.is_empty(),
            "cross-feature defer should check cleanly: {:?}",
            analysis.diagnostics
        );
        let ide = Ide {
            analysis: &analysis,
            sources: &sources,
        };
        // `try defer { () } with | Boom -> ()` must not offer invalid `try_to_catch`
        let try_at = offset_of(ok_src, "try defer");
        assert!(
            !ide.actions(file, try_at, try_at)
                .iter()
                .any(|a| a.title.contains("as `catch`")),
            "must not rewrite `try defer` as postfix catch"
        );
        // `defer` inside `result { … }` and `let ... else` binds to `scope outer`
        let res_defer = offset_of(ok_src, "defer { Host.raw_stdout_line \"res\" }");
        let (hover, _) = ide.hover(file, res_defer + 1).expect("hover in result");
        assert!(hover.contains("`scope outer`"), "{hover}");

        // Expression hole creates a lambda boundary blocking outer `scope` for `defer`
        let bad_hole = "\
module App.HoleDefer exposing (run)

keep : Unit -> Int ->{} Int
let keep _ n = n

run : Unit ->{} (Int ->{} Int)
let run () = scope {
  keep (if true then defer { () }) _
}
";
        let (_, _, hole_analysis) = analyze(bad_hole);
        assert!(
            hole_analysis
                .diagnostics
                .iter()
                .any(|d| d.code == codes::G004),
            "expected FLICK-G004 across expression hole lambda: {:?}",
            hole_analysis.diagnostics
        );
    }
}
