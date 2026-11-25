//! The printer on hand-built Lua: parentheses exactly where Lua 5.1's
//! priorities need them, one value per list position, blocks and lines.

use super::super::ast::{
    BinOp, Block, Expr, ExprKind, Field, Function, LastKind, Name, Results, Stmt, StmtKind, UnOp,
};
use super::super::print::{chunk, string};
use super::sp;

fn n(text: &str) -> Expr {
    Expr::name(&Name::new(text.to_owned()), sp(0, 0))
}

fn num(text: &str) -> Expr {
    Expr::new(ExprKind::Number(text.into()), sp(0, 0))
}

fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    Expr::binary(op, l, r, sp(0, 0))
}

fn neg(e: Expr) -> Expr {
    Expr::unary(UnOp::Neg, e, sp(0, 0))
}

/// The text of `return e`.
fn returned(e: Expr) -> String {
    let mut block = Block::default();
    block.end(LastKind::Return(e), sp(0, 0));
    chunk(&block).code
}

fn host_call(callee: &str) -> Expr {
    Expr::new(
        ExprKind::Call(Box::new(n(callee)), vec![], Results::Any),
        sp(0, 0),
    )
}

#[test]
fn operands_are_in_parentheses_only_where_lua_needs_them() {
    let cases = [
        (
            bin(BinOp::Mul, bin(BinOp::Add, n("a"), n("b")), n("c")),
            "(a + b) * c",
        ),
        (
            bin(BinOp::Add, n("a"), bin(BinOp::Mul, n("b"), n("c"))),
            "a + b * c",
        ),
        (
            bin(BinOp::Sub, n("a"), bin(BinOp::Sub, n("b"), n("c"))),
            "a - (b - c)",
        ),
        (
            bin(BinOp::Sub, bin(BinOp::Sub, n("a"), n("b")), n("c")),
            "a - b - c",
        ),
        // `..` and `^` group to the right
        (
            bin(BinOp::Concat, n("a"), bin(BinOp::Concat, n("b"), n("c"))),
            "a .. b .. c",
        ),
        (
            bin(BinOp::Concat, bin(BinOp::Concat, n("a"), n("b")), n("c")),
            "(a .. b) .. c",
        ),
        (
            bin(BinOp::Pow, n("a"), bin(BinOp::Pow, n("b"), n("c"))),
            "a ^ b ^ c",
        ),
        (
            bin(BinOp::Pow, bin(BinOp::Pow, n("a"), n("b")), n("c")),
            "(a ^ b) ^ c",
        ),
        // a unary operator binds tighter than all but `^`
        (neg(bin(BinOp::Pow, n("x"), num("2"))), "-x ^ 2"),
        (bin(BinOp::Pow, neg(n("x")), num("2")), "(-x) ^ 2"),
        (neg(bin(BinOp::Add, n("x"), num("2"))), "-(x + 2)"),
        (bin(BinOp::Mod, neg(n("x")), num("2")), "-x % 2"),
        (
            Expr::unary(UnOp::Not, bin(BinOp::Eq, n("a"), n("b")), sp(0, 0)),
            "not (a == b)",
        ),
        (
            bin(BinOp::And, bin(BinOp::Or, n("a"), n("b")), n("c")),
            "(a or b) and c",
        ),
        (
            bin(BinOp::Or, n("a"), bin(BinOp::And, n("b"), n("c"))),
            "a or b and c",
        ),
        // associative: a chain nested either way reads the same
        (
            bin(BinOp::And, n("a"), bin(BinOp::And, n("b"), n("c"))),
            "a and b and c",
        ),
        (
            bin(BinOp::Or, bin(BinOp::Or, n("a"), n("b")), n("c")),
            "a or b or c",
        ),
        (
            bin(BinOp::Eq, bin(BinOp::Lt, n("a"), n("b")), n("c")),
            "a < b == c",
        ),
        // `--` would start a comment
        (neg(neg(n("x"))), "-(-x)"),
        (bin(BinOp::Sub, n("a"), neg(n("b"))), "a - -b"),
        (Expr::unary(UnOp::Len, n("t"), sp(0, 0)), "#t"),
    ];
    for (e, text) in cases {
        assert_eq!(returned(e), format!("return {text}\n"));
    }
}

#[test]
fn a_call_that_may_return_other_than_one_value_is_adjusted_at_the_end_of_a_list() {
    let call = |args| n("f").call(args);
    assert_eq!(
        returned(call(vec![n("a"), host_call("g")])),
        "return f(a, (g()))\n"
    );
    assert_eq!(
        returned(call(vec![host_call("g"), n("a")])),
        "return f(g(), a)\n"
    );
    assert_eq!(
        returned(call(vec![n("a"), n("g").call(vec![])])),
        "return f(a, g())\n"
    );
    assert_eq!(returned(host_call("g")), "return (g())\n");
    let table = Expr::new(
        ExprKind::Table(vec![Field::Item(n("a")), Field::Item(host_call("g"))]),
        sp(0, 0),
    );
    assert_eq!(returned(table), "return {a, (g())}\n");
    let mut block = Block::default();
    block.push(Stmt::new(
        StmtKind::Local(Name::new("x".into()), Some(host_call("g"))),
        sp(0, 0),
    ));
    assert_eq!(chunk(&block).code, "local x = (g())\n");
}

#[test]
fn only_names_indexes_and_calls_are_called_or_indexed_as_written() {
    let f = Function {
        params: vec![Name::new("x".into())],
        body: Block::default(),
        span: sp(0, 0),
    };
    let lambda = Expr::new(ExprKind::Function(Box::new(f)), sp(0, 0));
    assert_eq!(
        returned(lambda.clone().call(vec![num("1")])),
        "return (function(x) end)(1)\n"
    );
    let empty = Expr::new(ExprKind::Table(vec![]), sp(0, 0));
    assert_eq!(returned(empty.field("x")), "return ({}).x\n");
    assert_eq!(returned(n("t").field("end")), "return t[\"end\"]\n");
    assert_eq!(returned(n("t").field("a.b")), "return t[\"a.b\"]\n");
    assert_eq!(returned(n("t").index(num("1"))), "return t[1]\n");
    assert_eq!(
        returned(n("f").call(vec![]).call(vec![n("x")]).field("y")),
        "return f()(x).y\n"
    );
    // a statement starting with `(` would call the line before it
    let mut block = Block::default();
    block.push(Stmt::new(StmtKind::Call(n("f").call(vec![])), sp(0, 0)));
    block.push(Stmt::new(StmtKind::Call(lambda.call(vec![])), sp(0, 0)));
    assert_eq!(chunk(&block).code, "f();\n(function(x) end)()\n");
}

#[test]
fn strings_escape_quotes_backslashes_and_control_characters() {
    assert_eq!(string("a\"b\\c"), r#""a\"b\\c""#);
    assert_eq!(string("\n\r\t"), r#""\n\r\t""#);
    // three digits, so a digit after the escape stays a digit
    assert_eq!(string("\u{0}1\u{7f}"), r#""\0001\127""#);
    assert_eq!(string("привет"), "\"привет\"");
}

#[test]
fn tables_name_identifier_keys_and_index_the_others() {
    let pair = |k: Expr, v: Expr| Field::Pair(k, v);
    let table = Expr::new(
        ExprKind::Table(vec![
            pair(Expr::str("x", sp(0, 0)), num("1")),
            pair(Expr::str("end", sp(0, 0)), num("2")),
            pair(num("3"), num("4")),
            Field::Item(n("y")),
        ]),
        sp(0, 0),
    );
    assert_eq!(
        returned(table),
        "return {x = 1, [\"end\"] = 2, [3] = 4, y}\n"
    );
}

#[test]
fn blocks_are_indented_and_a_list_too_long_for_its_line_breaks() {
    let mut inner = Block::default();
    inner.end(LastKind::Break, sp(0, 0));
    let mut block = Block::default();
    let stmt = |kind| Stmt::new(kind, sp(0, 0));
    block.push(stmt(StmtKind::While(
        Expr::new(ExprKind::Bool(true), sp(0, 0)),
        inner.clone(),
    )));
    block.push(stmt(StmtKind::Do(Block::default())));
    block.push(stmt(StmtKind::If(
        vec![(n("a"), inner.clone()), (n("b"), Block::default())],
        Some(inner),
    )));
    let long: Vec<Expr> = (0..30).map(|i| n(&format!("argument{i}"))).collect();
    block.push(stmt(StmtKind::Call(n("f").call(long))));
    let code = chunk(&block).code;
    let expected_head = "\
while true do
  break
end
do end
if a then
  break
elseif b then
else
  break
end
f(
  argument0,
  argument1,
";
    assert!(code.starts_with(expected_head), "{code}");
    assert!(code.ends_with("  argument28,\n  argument29\n)\n"), "{code}");
}
