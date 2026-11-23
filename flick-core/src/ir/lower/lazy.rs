//! `lazy { … }`, the Prelude's `lazy_make` (`docs/formal/lazy.md`);
//! `let lazy`, a variable of its function; and the tasks of arguments passed
//! unstarted to a parameter marked `~`.

use super::Lower;
use super::enums::native;
use super::numeric::apply;
use crate::ir::{Arm, Expr, ExprKind, ExprLang, Handler, Lit, Pat, Stage, Stmt};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;
use crate::types::{Constructor, StageKey, let_parts};

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

    /// `let lazy x = e`: the task `x$run = task (attempt e)`, made once here,
    /// and the variable `x` of its state, `None` until an `await` keeps the
    /// outcome as `Some o`. `attempt e`, the outcome of `e`, is `try Ok e with
    /// | err -> Err err`. `e` is lowered once, here, with the names in scope
    /// here, and reads the owner of where it runs, as the task of
    /// `lazy { e }` does. The task captures the values those names have here,
    /// which no `let mut` of `e` can change (`FLICK-G005`).
    pub(super) fn lazy_state(&mut self, decl: &Node, stmts: &mut Vec<Stmt>) {
        let span = self.span(decl);
        let parts = let_parts(decl, self.text);
        let key = StageKey::Task { node: decl.range() };
        self.register_owner_entry(key);
        let init = (parts.body).map_or_else(
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
        let run = Expr::new(
            ExprKind::Task(Box::new(self.owner_entry(key, attempt))),
            span,
        );
        let at = (decl.nodes().find(|n| n.kind == K::VarPat)).map_or(span.start, |n| n.range().0);
        let unique = self.bind(&parts.name.unwrap_or_default(), at);
        stmts.push(Stmt::let_bind(Pat::Bind(format!("{unique}$run")), run));
        self.vars.insert(unique.clone());
        let none = Constructor {
            arity: 0,
            ..native(Lang::None, Lang::Option)
        };
        stmts.push(Stmt::var(
            unique,
            None,
            Expr::new(ExprKind::Con(none), span),
        ));
    }

    /// `await x` of a confined `let lazy` reads the cached outcome or runs
    /// `x$run` and caches the outcome as `Some o`.
    ///
    /// ```text
    /// from_result (match x with
    ///   | Some o -> o
    ///   | _ -> let o = await x$run in x = Some o; o)
    /// ```
    ///
    /// A signal or a defect leaves the `await` before `x` is written, so `x`
    /// stays `None`.
    pub(super) fn force_local(&mut self, operand: Expr, span: Span) -> ExprKind {
        let ExprKind::Var(x) = operand.kind else {
            unreachable!("the checker resolved a local force to a let lazy variable");
        };
        let kept = self.hidden("$kept");
        let outcome = self.hidden("$outcome");
        let run = Expr::new(ExprKind::Local(format!("{x}$run")), span);
        let local = |name: &String| Expr::new(ExprKind::Local(name.clone()), span);
        let start = vec![
            Stmt::let_bind(
                Pat::Bind(outcome.clone()),
                Expr::new(runtime_call(Lang::AwaitEff, run, span), span),
            ),
            Stmt::Assign(x.clone(), some(local(&outcome))),
        ];
        let start = Expr::new(ExprKind::Block(start, Box::new(local(&outcome))), span);
        let kept_pat = Pat::Con(
            native(Lang::Some, Lang::Option),
            vec![Pat::Bind(kept.clone())],
        );
        let arms = vec![arm(kept_pat, local(&kept)), arm(Pat::Wild, start)];
        let state = Expr::new(ExprKind::Var(x), span);
        let outcome = Expr::new(ExprKind::Match(Box::new(state), arms), span);
        runtime_call(Lang::FromResult, outcome, span)
    }
}

fn arm(pat: Pat, body: Expr) -> Arm {
    Arm {
        pat,
        binder_types: Vec::new(),
        guard: None,
        body,
    }
}

/// `Some (Ok v)`: the state of a `let lazy mut` written `v`.
pub(super) fn written(value: Expr) -> Expr {
    some(result(Lang::Ok, value))
}

/// `Some o`: the state of a `let lazy` that keeps the outcome `o`.
fn some(outcome: Expr) -> Expr {
    let span = outcome.span;
    let con = ExprKind::Con(native(Lang::Some, Lang::Option));
    apply(Expr::new(con, span), [Box::new(outcome)], span)
}

/// `Ok arg` or `Err arg` of the Prelude's `Result`.
fn result(name: Lang, arg: Expr) -> Expr {
    let span = arg.span;
    let con = ExprKind::Con(native(name, Lang::Result));
    apply(Expr::new(con, span), [Box::new(arg)], span)
}

fn runtime_call(function: Lang, arg: Expr, span: Span) -> ExprKind {
    ExprKind::App {
        func: Box::new(Expr::new(ExprKind::lang(function), span)),
        arg: Box::new(arg),
        stage: Stage::Runtime,
        arg_first: false,
    }
}
