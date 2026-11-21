use super::Tr;
use crate::syntax::Term;
use flick_core::ir::{Expr, ExprKind, Lit, Stage};
use flick_core::source::{FileId, Span};

fn expression(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(FileId::new(0), 0, 0))
}

fn call(argument: Expr) -> Expr {
    expression(ExprKind::App {
        func: Box::new(expression(ExprKind::Global {
            module: "Test".to_owned(),
            name: "consume".to_owned(),
        })),
        arg: Box::new(argument),
        stage: Stage::Runtime,
        arg_first: true,
    })
}

#[test]
fn argument_first_staging_keeps_task_expectations_but_not_computations() {
    let task = expression(ExprKind::Task(Box::new(expression(ExprKind::Lit(
        Lit::Unit,
    )))));
    let translated = Tr::new("Test").expr(&call(task));
    assert!(matches!(translated, Term::App(_, argument) if matches!(&*argument, Term::Task(_))));
    let computation = call(expression(ExprKind::Lit(Lit::Unit)));
    let translated = Tr::new("Test").expr(&call(computation));
    assert!(
        matches!(translated, Term::Let(_, None, value, _) if !matches!(&*value, Term::Task(_) | Term::Lam(..)))
    );
}
