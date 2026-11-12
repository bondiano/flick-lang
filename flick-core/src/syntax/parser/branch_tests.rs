//! The branches of an `if` in braces.

use super::tests::{errors, sexp};

#[test]
fn braces_after_then_and_else_hold_statements() {
    let tree = sexp("let f () =\n  if c then {\n    a = 1\n    b = 2\n  } else { a = 3; b = 4 }\n");
    assert!(!tree.contains("RecordExpr"), "{tree}");
    assert_eq!(tree.matches("AssignStmt").count(), 4, "{tree}");
    assert_eq!(
        errors("let f () =\n  if c then {\n    a = 1\n  }\n  else {\n    a = 2\n  }\n"),
        Vec::<String>::new()
    );
    // a record there is in parentheses
    assert!(sexp("let f r = if c then ({ r with a = 1 }) else r\n").contains("RecordUpdate"));
    // the old form of a record update stays a record, which the checker explains
    let old = sexp("let f r = if c then { r with a = 1 } else r\n");
    assert!(old.contains("RecordUpdate"), "{old}");
    assert_eq!(
        errors("let f r = if c then { r with a = 1 } else r\n"),
        Vec::<String>::new()
    );
    // the `with` of a `match` in a statement is no record update
    assert_eq!(
        errors("let f () = if true then { while match false with | _ -> false do () } else ()\n"),
        Vec::<String>::new()
    );
    // any expression may be the base of the old form
    assert!(
        sexp("let f () = if c then { { x = 0 } with x = 1 } else ({ x = 2 })\n")
            .contains("RecordUpdate")
    );
}
