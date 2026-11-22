use super::fmt;
use crate::source::FileId;
use crate::syntax::parse;

/// `let ... else` stays on one line when both sides are simple, and keeps layout
/// boundaries and nearest-`else` ownership intact when breaking.
#[test]
fn lays_out_let_else_preserving_nearest_else_and_comments() {
    let src = "let greet m =\n  let   Some  name  =  m   else   \"guest\"\n  \"hi \" ++ name\nlet nested m flag =\n  let Some x =\n    let Some y = m else 0\n    if flag then Some y else None\n  else\n    let fallback = -1\n    fallback\n  x + 1\nlet rec_fb m =\n  let Some r = m else { name = \"guest\" }\n  r.name\nlet commented m =\n  let Some x = m // match\n  else\n    // fallback\n    0\n  x\n";
    let formatted = fmt(src);
    // Fixed-point idempotency: formatting twice yields identical text.
    assert_eq!(
        fmt(&formatted),
        formatted,
        "let-else format must be idempotent"
    );
    // Re-parsing produces zero errors and preserves `ElseKw` ownership on nested `if` and `let-else`.
    let parsed = parse(&formatted, FileId::default());
    assert!(
        parsed.diagnostics.is_empty(),
        "formatted let-else has parse errors: {:?}\n{formatted}",
        parsed.diagnostics
    );
    insta::assert_snapshot!(formatted, @r#"
    let greet m =
      let Some name = m else "guest"
      "hi " ++ name
    let nested m flag =
      let Some x =
        let Some y = m else 0
        if flag then Some y else None
      else
        let fallback = -1
        fallback
      x + 1
    let rec_fb m =
      let Some r = m else { name = "guest" }
      r.name
    let commented m =
      let Some x =
        m // match
      else
        // fallback
        0
      x
    "#);
}

/// `let ... else` composes cleanly with `orelse`, postfix `catch`, and `result { … }`.
#[test]
fn lays_out_let_else_with_orelse_catch_and_result() {
    let src = "let composed opt fallback =\n  let Some x = opt orelse fallback else 0\n  x + 1\nlet caught id =\n  let Some x =\n    fetch id catch | Missing -> None\n  else 0\n  x\nlet in_result opt = result {\n  let Some x = opt else Fail.fail Boom\n  x + 1\n}\n";
    let formatted = fmt(src);
    assert_eq!(
        fmt(&formatted),
        formatted,
        "cross-feature let-else format must be idempotent"
    );
    let parsed = parse(&formatted, FileId::default());
    assert!(
        parsed.diagnostics.is_empty(),
        "formatted cross-feature let-else has parse errors: {:?}\n{formatted}",
        parsed.diagnostics
    );
}
