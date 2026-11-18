//! `lazy { … }` and `await` of a `Lazy` (`docs/formal/lazy.md`): calls of
//! the Prelude's `lazy_make` and `lazy_force`, which the runtimes implement;
//! `let lazy`, a variable of its function; and the tasks of arguments passed
//! unstarted to a parameter marked `~`.

use super::Lower;
use super::enums::native;
use super::numeric::apply;
use crate::ir::{Arm, Expr, ExprKind, ExprLang, Handler, Lit, Pat, Stage, Stmt};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::lang::Lang;
use crate::types::{Constructor, StageKey};

impl Lower<'_> {
    /// An argument, or the task of one passed unstarted to a parameter
    /// marked `~` (`types::suspended`).
    pub(super) fn argument(&mut self, arg: &Node) -> Expr {
        if !self.typing.suspends.contains(&arg.range()) {
            return self.expr(arg);
        }
        let span = self.span(arg);
        let key = StageKey::Task { node: arg.range() };
        self.register_owner_entry(key);
        let body = self.expr(arg);
        Expr::new(ExprKind::Task(Box::new(self.owner_entry(key, body))), span)
    }

    /// `lazy body`: the task of `body`, kept in a cell by `lazy_make`.
    pub(super) fn lazy_expr(&mut self, node: &Node, body: &Node, span: Span) -> ExprKind {
        let key = StageKey::Task { node: node.range() };
        self.register_owner_entry(key);
        let body = self.expr(body);
        let task = Expr::new(ExprKind::Task(Box::new(self.owner_entry(key, body))), span);
        runtime_call(Lang::LazyMake, task, span)
    }

    /// `task { attempt e }` of `let lazy name = e`, the first state of the
    /// variable: `attempt e`, the outcome of `e`, is `try Ok e with | err ->
    /// Err err`. `e` is lowered once, here, with the names in scope here,
    /// and reads the owner of where it runs, as the task of `lazy { e }`
    /// does. The task captures the values those names have here, which no
    /// `let mut` of `e` can change (`FLICK-G005`).
    pub(super) fn lazy_run(&mut self, decl: &Node, body: Option<&Node>) -> Expr {
        let span = self.span(decl);
        let key = StageKey::Task { node: decl.range() };
        self.register_owner_entry(key);
        let init = body.map_or_else(
            || Expr::new(ExprKind::Lit(Lit::Unit), span),
            |b| self.expr(b),
        );
        let error = self.hidden("$error");
        let caught = Expr::new(ExprKind::Local(error.clone()), span);
        let attempt = Handler {
            effects: Vec::new(),
            body: result(Lang::Ok, init),
            ret: None,
            errors: vec![arm(Pat::Bind(error), result(Lang::Err, caught))],
        };
        let attempt = Expr::new(ExprKind::Handle(Box::new(attempt)), span);
        Expr::new(
            ExprKind::Task(Box::new(self.owner_entry(key, attempt))),
            span,
        )
    }

    /// `await` of a `Lazy`: `lazy_force` of it; or, of a `let lazy` `x`, a
    /// run of its state: the first time the initializer, and after that a
    /// task that gives the outcome back, which the state becomes.
    ///
    /// ```text
    /// let o = await x in x = task { o }; from_result o
    /// ```
    ///
    /// A signal or a defect leaves `await x` before `x` is written, so `x`
    /// still holds the initializer.
    pub(super) fn force(&mut self, operand: Expr, span: Span, local: bool) -> ExprKind {
        // the checker read the operand of a local force as the name of a
        // `let lazy`, a variable
        let (true, ExprKind::Var(x)) = (local, &operand.kind) else {
            return runtime_call(Lang::LazyForce, operand, span);
        };
        let x = x.clone();
        let outcome = self.hidden("$outcome");
        let given = Expr::new(ExprKind::Local(outcome.clone()), span);
        let stmts = vec![
            Stmt::Let(
                Pat::Bind(outcome),
                Expr::new(ExprKind::Await(Box::new(operand)), span),
            ),
            Stmt::Assign(x, task(given.clone())),
        ];
        let give = Expr::new(runtime_call(Lang::FromResult, given, span), span);
        ExprKind::Block(stmts, Box::new(give))
    }

    /// `x = v` of a `let lazy mut`: the state becomes a task that gives
    /// `Ok v`, `v` evaluated here.
    pub(super) fn lazy_write(&mut self, value: Expr) -> Expr {
        let span = value.span;
        let name = self.hidden("$written");
        let written = Expr::new(ExprKind::Local(name.clone()), span);
        let state = task(result(Lang::Ok, written));
        Expr::new(
            ExprKind::Block(vec![Stmt::Let(Pat::Bind(name), value)], Box::new(state)),
            span,
        )
    }
}

/// `task { body }`: a state of a `let lazy`.
fn task(body: Expr) -> Expr {
    let span = body.span;
    Expr::new(ExprKind::Task(Box::new(body)), span)
}

fn arm(pat: Pat, body: Expr) -> Arm {
    Arm {
        pat,
        guard: None,
        body,
    }
}

/// `Ok arg` or `Err arg` of the Prelude's `Result`.
fn result(name: Lang, arg: Expr) -> Expr {
    con(native(name, Lang::Result), arg)
}

fn con(c: Constructor, arg: Expr) -> Expr {
    let span = arg.span;
    apply(Expr::new(ExprKind::Con(c), span), [Box::new(arg)], span)
}

fn runtime_call(function: Lang, arg: Expr, span: Span) -> ExprKind {
    runtime_app(Expr::new(ExprKind::lang(function), span), arg)
}

fn runtime_app(func: Expr, arg: Expr) -> ExprKind {
    ExprKind::App {
        func: Box::new(func),
        arg: Box::new(arg),
        stage: Stage::Runtime,
        arg_first: false,
    }
}
