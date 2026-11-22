use crate::diagnostics::codes;
use crate::source::SourceMap;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parser::parse;

fn parse_src(src: &str) -> crate::syntax::Parse {
    let mut map = SourceMap::default();
    let file = map.add("defer.fl", src);
    parse(src, file)
}

fn has_kind(node: &Node, kind: K) -> bool {
    node.kind == kind || node.nodes().any(|c| has_kind(c, kind))
}

#[test]
fn parses_defer_and_errdefer_statements() {
    let parsed = parse_src(
        r#"let f () = scope {
  defer { close () }
  errdefer {
    log "fail"
    rollback ()
  }
  if ok then defer { close2 () }
  match x with
  | A -> errdefer { undo () }
  | B -> ()
  for i <- xs do {
    defer { step i }
  }
  let x =
    defer { cleanup_for_x () }
    1
  x
}
"#,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(has_kind(&parsed.tree, K::DeferStmt));
}

#[test]
fn preserves_contextual_identifiers_and_record_calls() {
    let parsed = parse_src(
        r#"let defer = 1
let errdefer = 2
let use_vals x =
  let a = defer x
  let b = errdefer ()
  let c = (defer) { field = 1 }
  let d = (errdefer) { field = 2 }
  let e = f defer { field = 3 }
  let g = { defer = 1; errdefer = 2 }
  let h = g.defer + g.errdefer
  provide defer {
    provide errdefer {
      (a, b, c, d, e, h)
    }
  }
"#,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(!has_kind(&parsed.tree, K::DeferStmt));
}

#[test]
fn rejects_defer_and_errdefer_in_expression_and_value_positions() {
    let bad_cases = [
        "let f () = scope {\n  let x = defer { close () }\n  x\n}\n",
        "let f () = scope {\n  let x = errdefer { rollback () }\n  x\n}\n",
        "let f () = scope {\n  let () = defer { close () }\n  ()\n}\n",
        "let f () = scope {\n  let x =\n    let a = 1\n    defer { close a }\n  x\n}\n",
        "let f () = scope {\n  let mut x = ()\n  x = defer { close () }\n}\n",
        "let f () = scope {\n  let r = { a = defer { close () } }\n  r\n}\n",
        "let f () = scope {\n  ignore (defer { close () })\n}\n",
        "let f () = scope {\n  ignore defer { close () }\n}\n",
        "let f () = scope {\n  let xs = [defer { close () }]\n  xs\n}\n",
        "let f () = scope {\n  1 + defer { close () }\n}\n",
        "let f () = scope {\n  defer { close () } |> g\n}\n",
        "let f () = scope {\n  if defer { close () } then 1 else 2\n}\n",
        "let f () = do {\n  defer { close () }\n  pure ()\n}\n",
        "let f () = for {\n  defer { close () }\n  x <- xs\n} yield x\n",
    ];
    for src in bad_cases {
        let parsed = parse_src(src);
        assert!(
            parsed.diagnostics.iter().any(|d| d.code == codes::S007),
            "expected FLICK-S007 for:\n{src}\ngot: {:?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn recovers_cleanly_on_partial_defer_inputs() {
    for src in [
        "let f () = scope {\n  defer {\n",
        "let f () = scope {\n  errdefer { close\n",
        "let f () = scope {\n  defer {}\n}\n",
    ] {
        let parsed = parse_src(src);
        assert!(has_kind(&parsed.tree, K::DeferStmt), "source:\n{src}");
    }
}
