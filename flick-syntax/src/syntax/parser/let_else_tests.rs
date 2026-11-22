use super::parse;
use super::tests::{errors, sexp};
use crate::source::FileId;

#[test]
fn let_else_parses_inline_layout_and_nearest_else_ownership() {
    // inline initializer and inline fallback
    let inline = "let f m =\n  let Some x = m else 0\n  x\n";
    assert_eq!(errors(inline), Vec::<String>::new());
    assert!(
        sexp(inline).contains(
            "(LetDecl let (ConPat Some (VarPat x)) = (Block (NameExpr m)) else (Block (Literal 0)))"
        ),
        "{}",
        sexp(inline)
    );

    // layout fallback ends at dedent; next statement is success remainder
    let layout = "let f key =\n  let Some value = lookup key else\n    record_miss key\n    fallback\n  use value\n";
    assert_eq!(errors(layout), Vec::<String>::new());
    assert!(
        sexp(layout).contains(
            "else (Block (AppExpr (NameExpr record_miss) (NameExpr key)) (NameExpr fallback))) (AppExpr (NameExpr use) (NameExpr value))"
        ),
        "{}",
        sexp(layout)
    );

    // nested `if ... else ...` consumes its own `else`; second `else` belongs to `let-else`
    let nested_if = "let f p a b =\n  let Some x = if p then a else b else 0\n  x\n";
    assert_eq!(errors(nested_if), Vec::<String>::new());
    let tree = sexp(nested_if);
    assert!(
        tree.contains("(Block (IfExpr if (NameExpr p) then (Block (NameExpr a)) else (Block (NameExpr b)))) else (Block (Literal 0))"),
        "{tree}"
    );

    // multi-line `if ... else ...` inside initializer, outer `else` aligned with `let`
    let multiline_if = "let f p a b =\n  let Some x =\n    if p then\n      a\n    else\n      b\n  else\n    0\n  x\n";
    assert_eq!(errors(multiline_if), Vec::<String>::new());
    let tree = sexp(multiline_if);
    assert!(
        tree.contains("(Block (IfExpr if (NameExpr p) then (Block (NameExpr a)) else (Block (NameExpr b)))) else (Block (Literal 0))"),
        "{tree}"
    );

    // nested `let-else` inside initializer finishes only the initializer block
    let nested_init = "let f m =\n  let name =\n    let Some user = m else \"guest\"\n    user.name\n  render name\n";
    assert_eq!(errors(nested_init), Vec::<String>::new());

    // `let-else` inside `if` branch preserves outer `if`'s `else`
    let in_if = "let f c m =\n  if c then\n    let Some x = m else 0\n    x\n  else\n    1\n";
    assert_eq!(errors(in_if), Vec::<String>::new());
    let tree = sexp(in_if);
    assert!(
        tree.contains("else (Block (Literal 0))) (NameExpr x)) else (Block (Literal 1))"),
        "{tree}"
    );

    // record literal directly after `else` stays a RecordExpr
    let record_fb = "let f m =\n  let Some r = m else { name = \"guest\" }\n  r\n";
    assert_eq!(errors(record_fb), Vec::<String>::new());
    assert!(
        sexp(record_fb).contains("else (Block (RecordExpr"),
        "{}",
        sexp(record_fb)
    );

    // `for ... do` ordinary body accepts `let-else`
    let in_for_do =
        "let f xs =\n  for item <- xs do\n    let Some x = item else continue\n    use x\n";
    assert_eq!(errors(in_for_do), Vec::<String>::new());
}

#[test]
fn let_else_rejects_excluded_contexts_modifiers_guards_and_bad_indent() {
    let cases = [
        ("let Some x = m else 0\n", "module declaration"),
        (
            "let f m =\n  do {\n    let Some x = m else None\n    pure x\n  }\n",
            "`do` block",
        ),
        (
            "let f xs =\n  for {\n    x <- xs\n    let Some y = x else 0\n  } yield y\n",
            "`for` clauses",
        ),
        (
            "let f xs = [for x <- xs; let Some y = x else 0 yield y]\n",
            "`for` clauses",
        ),
        (
            "let f m =\n  let rec g = m else 0\n  g\n",
            "cannot use `rec`",
        ),
        (
            "let f m =\n  let mut x = m else 0\n  x\n",
            "cannot use `mut`",
        ),
        (
            "let f m =\n  let lazy x = m else 0\n  x\n",
            "cannot use `lazy`",
        ),
        (
            "let f m =\n  let assert Some x = m else 0\n  x\n",
            "cannot be combined with `assert`",
        ),
        (
            "let f m =\n  let g x = m else 0\n  g\n",
            "cannot declare function parameters",
        ),
        (
            "let f m =\n  let x : Int = m else 0\n  x\n",
            "does not take a type annotation",
        ),
        (
            "let f m =\n  let Some x when x > 0 = m else 0\n  x\n",
            "`when` guards",
        ),
        (
            "let f m =\n  let Some x = m else\n  0\n  x\n",
            "expected an indented block",
        ),
        (
            "let f m =\n  let Some x =\n    m\n    else 0\n  x\n",
            "unexpected indentation",
        ),
    ];
    for (src, needle) in cases {
        let diagnostics = parse(src, FileId::default()).diagnostics;
        assert!(
            diagnostics.iter().any(|d| d.message.contains(needle)),
            "for `{src}` expected diagnostic containing `{needle}`, got {diagnostics:?}"
        );
    }
}
