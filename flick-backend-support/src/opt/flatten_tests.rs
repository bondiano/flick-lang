use super::flatten;
use flick_ir::{Expr, ExprKind, FileId, Lit, Pat, Span, Stmt};

fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(FileId::new(0), 0, 0))
}

fn int(n: i32) -> Expr {
    expr(ExprKind::Lit(Lit::Int(n)))
}

fn block(stmts: Vec<Stmt>, result: Expr) -> Expr {
    expr(ExprKind::Block(stmts, Box::new(result)))
}

fn rhs() -> Expr {
    block(vec![Stmt::Expr(int(1))], int(2))
}

fn assert_stable(stmts: &mut Vec<Stmt>, result: &mut Box<Expr>) {
    let expected_stmts = stmts.clone();
    let expected_result = result.clone();
    for _ in 0..2 {
        let changed = flatten(stmts, result);
        assert_eq!(*stmts, expected_stmts);
        assert_eq!(*result, expected_result);
        assert!(!changed, "flatten reported a rewrite on an unchanged block");
    }
}

#[test]
fn var_with_block_rhs_is_stable() {
    assert_stable(
        &mut vec![Stmt::var("x".into(), None, rhs())],
        &mut Box::new(int(3)),
    );
}

#[test]
fn assign_with_block_rhs_is_stable() {
    assert_stable(
        &mut vec![Stmt::Assign("x".into(), rhs())],
        &mut Box::new(int(3)),
    );
}

#[test]
fn simplify_stops_on_a_stable_mutable_binding() {
    let rhs = block(vec![Stmt::Assign("other".into(), int(1))], int(2));
    let mut body = block(
        vec![
            Stmt::var("other".into(), None, int(0)),
            Stmt::var("x".into(), None, rhs),
        ],
        expr(ExprKind::Local("x".into())),
    );
    let original = body.clone();
    let changed = super::pass(&mut body, &|_, _| None, &crate::Known::of(&[]), false);
    assert_eq!(body, original);
    assert!(!changed);
}

fn assert_rewrite(mut stmts: Vec<Stmt>, result: Expr, expected: &[Stmt], value: &Expr) {
    let mut result = Box::new(result);
    assert!(flatten(&mut stmts, &mut result));
    assert_eq!(stmts, expected);
    assert_eq!(result.as_ref(), value);
    assert_stable(&mut stmts, &mut result);
}

#[test]
fn let_expr_and_result_blocks_each_report_their_rewrite() {
    let bind = |value| Stmt::let_bind(Pat::Bind("x".into()), value);
    assert_rewrite(
        vec![bind(rhs())],
        int(3),
        &[Stmt::Expr(int(1)), bind(int(2))],
        &int(3),
    );
    assert_rewrite(
        vec![Stmt::Expr(rhs())],
        int(3),
        &[Stmt::Expr(int(1)), Stmt::Expr(int(2))],
        &int(3),
    );
    assert_rewrite(
        vec![Stmt::Expr(int(0))],
        block(
            vec![Stmt::Expr(int(1))],
            block(vec![Stmt::Expr(int(2))], int(3)),
        ),
        &[Stmt::Expr(int(0)), Stmt::Expr(int(1)), Stmt::Expr(int(2))],
        &int(3),
    );
}

#[test]
fn flatten_keeps_mutable_bindings_in_order_around_a_rewrite() {
    let var = Stmt::var("x".into(), None, rhs());
    let assign = Stmt::Assign("x".into(), rhs());
    assert_rewrite(
        vec![var.clone(), Stmt::Expr(rhs()), assign.clone()],
        int(3),
        &[var, Stmt::Expr(int(1)), Stmt::Expr(int(2)), assign],
        &int(3),
    );
}
