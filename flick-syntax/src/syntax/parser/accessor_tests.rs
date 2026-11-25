//! Accessors `.address.city`: where they start, and what stays as it was.

use super::tests::{errors, parse_str, sexp};
use crate::syntax::cst::{Element, Node};

/// [`sexp`] of a source with syntax errors.
fn shape(src: &str) -> String {
    fn walk(node: &Node, src: &str) -> String {
        let parts: Vec<String> = (node.children.iter())
            .filter_map(|c| match c {
                Element::Node(n) => Some(walk(n, src)),
                Element::Token(t) if t.kind.is_trivia() => None,
                Element::Token(t) => Some(t.text(src).to_owned()),
            })
            .collect();
        format!("({:?} {})", node.kind, parts.join(" "))
    }
    let parsed = parse_str(src);
    let nodes: Vec<String> = parsed.tree.nodes().map(|n| walk(n, src)).collect();
    nodes.join("\n")
}

#[test]
fn an_accessor_is_a_path_of_steps_written_without_spaces() {
    assert_eq!(
        sexp("let c = List.map .address.city users"),
        "(LetDecl let (VarPat c) = (Block (AppExpr (NameExpr List . map) (AccessorExpr \
         (AccessorStep . address) (AccessorStep . city)) (NameExpr users))))"
    );
    // a space between two steps applies one accessor to the other
    assert_eq!(
        sexp("let s = .a .b"),
        "(LetDecl let (VarPat s) = (Block (AppExpr (AccessorExpr (AccessorStep . a)) \
         (AccessorExpr (AccessorStep . b)))))"
    );
}

#[test]
fn an_accessor_starts_wherever_an_argument_does() {
    let accessor = "(AccessorExpr (AccessorStep . name))";
    for src in [
        "let a = .name",
        "let a = user |> .name",
        "let a = .name user",
        "let a = [.name, .age]",
        "let a = { get = .name }",
        "let a = (.name)",
        "let a = f (.name) .name",
    ] {
        let tree = sexp(src);
        assert!(tree.contains(accessor), "{src}: {tree}");
        assert!(!tree.contains("Error"), "{src}: {tree}");
    }
    // `?` after an accessor is postfix, as after any atom
    assert!(sexp("let a = .name?").contains(&format!("(QuestionExpr {accessor} ?)")));
}

#[test]
fn a_dot_after_a_value_still_reads_its_field() {
    assert_eq!(
        sexp("let c = user.address.city"),
        "(LetDecl let (VarPat c) = (Block (FieldExpr (FieldExpr (NameExpr user) . address) . city)))"
    );
    assert!(sexp("let m = List.map f xs").contains("(NameExpr List . map)"));
    assert!(sexp("let r = [0..5]").contains("(RangeExpr (Literal 0) .. (Literal 5))"));
    assert!(sexp("let r = f (1..)").contains("(RangeExpr (Literal 1) ..)"));
    assert!(sexp("let n = 1.5").contains("(Literal 1.5)"));
    assert!(sexp("let a = f x?").contains("(QuestionExpr (NameExpr x) ?)"));
}

#[test]
fn a_missing_label_is_a_local_error() {
    let src = "let c = List.map .address. users\nlet d = 1";
    assert_eq!(errors(src), ["FLICK-S007 ."]);
    // the words after the unfinished step are still arguments
    assert_eq!(
        shape(src),
        "(LetDecl let (VarPat c) = (Block (AppExpr (NameExpr List . map) (AccessorExpr \
         (AccessorStep . address) (AccessorStep .)) (NameExpr users))))\n\
         (LetDecl let (VarPat d) = (Block (Literal 1)))"
    );
    // `.` with a space before its name starts no accessor
    assert_eq!(errors("let a = . name"), ["FLICK-S007 ."]);
    // a number cannot start with `.`, as before
    assert_eq!(errors("let a = .5"), ["FLICK-S006 .5"]);
}

/// A dot alone is an accessor whose first step has no name yet: the error
/// is at the dot, and the words after it stay arguments, so an editor can
/// complete there.
#[test]
fn a_bare_dot_is_an_unfinished_accessor() {
    let src = "let c = List.map . users\nlet d = 1";
    assert_eq!(errors(src), ["FLICK-S007 ."]);
    assert_eq!(
        shape(src),
        "(LetDecl let (VarPat c) = (Block (AppExpr (NameExpr List . map) (AccessorExpr \
         (AccessorStep .)) (NameExpr users))))\n\
         (LetDecl let (VarPat d) = (Block (Literal 1)))"
    );
}

/// The error of an unfinished step stays at its own dot, so the statement
/// after it still reports its own syntax error.
#[test]
fn an_unfinished_step_keeps_the_next_error() {
    assert_eq!(
        errors("let f =\n  .address.\n  .\n"),
        ["FLICK-S007 .", "FLICK-S007 ."]
    );
}

#[test]
fn a_line_starting_with_an_accessor_is_a_statement_of_its_own() {
    let tree = sexp("let f user =\n  user\n  .name\n");
    assert!(
        tree.contains("(Block (NameExpr user) (AccessorExpr (AccessorStep . name)))"),
        "{tree}"
    );
}

/// Foreign metadata takes words and values, not accessors: `.x` there is
/// an error, as it was before accessors.
#[test]
fn foreign_metadata_takes_no_accessor() {
    let src = "foreign read from \"./fs.js\" \"read\"\n  arity .x\n";
    assert_eq!(errors(src), ["FLICK-S007 ."]);
}

/// `.?label` is one optional step: its `?` stays in the step, so `x.?f`
/// and the propagation `x?.f` keep different trees.
#[test]
fn an_optional_step_keeps_its_question_mark() {
    assert_eq!(
        sexp("let c = .?address.?city"),
        "(LetDecl let (VarPat c) = (Block (AccessorExpr (AccessorStep . ? address) \
         (AccessorStep . ? city))))"
    );
    assert_eq!(
        sexp("let c = user.address.?city.name"),
        "(LetDecl let (VarPat c) = (Block (FieldExpr (FieldExpr (FieldExpr (NameExpr user) \
         . address) . ? city) . name)))"
    );
    assert!(
        sexp("let c = user?.city").contains("(FieldExpr (QuestionExpr (NameExpr user) ?) . city)")
    );
    // a `?` after a path is postfix and ends it
    assert!(
        sexp("let c = user.?city?")
            .contains("(QuestionExpr (FieldExpr (NameExpr user) . ? city) ?)")
    );
    assert!(
        sexp("let c = .?city?").contains("(QuestionExpr (AccessorExpr (AccessorStep . ? city)) ?)")
    );
    // a hole takes no field, optional or not
    assert_eq!(errors("let c = List.map _.?city users"), ["FLICK-S018 _"]);
}

#[test]
fn an_optional_step_needs_its_label_right_after() {
    assert_eq!(errors("let a = .? city\nlet b = 1"), ["FLICK-S007 ."]);
    assert_eq!(errors("let a = . ?city"), ["FLICK-S007 ."]);
}

/// `echo` and `todo` stay labels after `.?`, as after `.`.
#[test]
fn a_debugging_keyword_is_a_label_after_an_optional_dot() {
    assert!(sexp("let a = .?echo").contains("(AccessorStep . ? echo)"));
    assert!(sexp("let a = x.?todo").contains("(FieldExpr (NameExpr x) . ? todo)"));
}
