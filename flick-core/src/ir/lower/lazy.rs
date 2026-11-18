//! `lazy { … }` and `await` of a `Lazy` (`docs/formal/lazy.md`): calls of
//! the Prelude's `lazy_make` and `lazy_force`, which the runtimes implement;
//! `let lazy`, a variable of its function; and the tasks of arguments passed
//! unstarted to a parameter marked `~`.

use super::Lower;
use crate::ir::{Arm, Expr, ExprKind, ExprLang, Handler, Lit, Param, Pat, Stage, Stmt};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};
use crate::types::{Constructor, Rep, StageKey, let_parts};

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

    /// The initializer of `let lazy name = body`, which reads the owner of
    /// where it runs, as the task of `lazy { body }` does.
    pub(super) fn lazy_let(&mut self, decl: &Node, body: Option<&Node>) -> Expr {
        let Some(body) = body else {
            return Expr::new(ExprKind::Lit(Lit::Unit), self.span(decl));
        };
        let key = StageKey::Task { node: decl.range() };
        self.register_owner_entry(key);
        let body = self.expr(body);
        self.owner_entry(key, body)
    }

    /// The statements of a local `let`: those of a `let lazy`, or the one of
    /// any other.
    pub(super) fn local_let_stmts(&mut self, decl: &Node) -> Vec<Stmt> {
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            return vec![self.local_let(decl)];
        };
        if !self.typing.lazy_lets.contains(&decl.range()) {
            return vec![self.local_let(decl)];
        }
        let at =
            (decl.nodes().find(|n| n.kind == K::VarPat)).map_or(decl.range().0, |n| n.range().0);
        let init = self.lazy_let(decl, parts.body);
        let unique = self.bind(&name, at);
        self.lazy_state(unique, at, init)
    }

    /// `let lazy x = e`, written at `at`, as the variable `x` of its state:
    /// `None` until an `await` settles it to `Some outcome`. `e` is lowered
    /// once, here, with the names in scope here, as `attempt e`, which gives
    /// its outcome. The one `await` of `x` runs it in place; with several,
    /// it is the body of a local function of the declaration that each of
    /// them calls, so its code is not repeated. `e` reads no `let mut`
    /// (`FLICK-G005`), so where it runs it reads the values it would read here.
    fn lazy_state(&mut self, unique: String, at: u32, init: Expr) -> Vec<Stmt> {
        let span = init.span;
        let awaits = (self.typing.read_lazy.values())
            .filter(|&&binder| binder == at)
            .count();
        let attempt = self.attempt(init);
        let mut stmts = Vec::new();
        let run = if awaits > 1 {
            let function = self.hidden(&format!("{unique}$init"));
            let params = vec![Param {
                pat: Pat::Wild,
                stage: Stage::Runtime,
                ty: None,
            }];
            let value = Expr::new(
                ExprKind::Lambda {
                    params,
                    body: Box::new(attempt),
                },
                span,
            );
            stmts.push(Stmt::Fun {
                name: function.clone(),
                rec: false,
                dicts: Vec::new(),
                value,
            });
            Expr::new(
                ExprKind::App {
                    func: Box::new(Expr::new(ExprKind::Local(function), span)),
                    arg: Box::new(Expr::new(ExprKind::Lit(Lit::Unit), span)),
                    stage: Stage::Runtime,
                    arg_first: false,
                },
                span,
            )
        } else {
            attempt
        };
        self.lazies.insert(unique.clone(), run);
        self.vars.insert(unique.clone());
        let none = constructor(Lang::None, Lang::Option, 0);
        stmts.push(Stmt::Var(unique, Expr::new(ExprKind::Con(none), span)));
        stmts
    }

    /// `attempt e`: `try Ok e with | err -> Err err`, the outcome of `e`.
    fn attempt(&mut self, init: Expr) -> Expr {
        let span = init.span;
        let error = self.hidden("$error");
        let caught = Expr::new(ExprKind::Local(error.clone()), span);
        let handler = Handler {
            effects: Vec::new(),
            body: ok(init),
            ret: None,
            errors: vec![Arm {
                pat: Pat::Bind(error),
                guard: None,
                body: apply(constructor(Lang::Err, Lang::Result, 1), caught),
            }],
        };
        Expr::new(ExprKind::Handle(Box::new(handler)), span)
    }

    /// `await x` of a `let lazy`: the kept outcome, or the outcome of its
    /// initializer, kept before it is given back.
    ///
    /// ```text
    /// match x with
    /// | Some o -> from_result o
    /// | _ -> let o = attempt e in x = Some o; from_result o
    /// ```
    ///
    /// A signal or a defect leaves `e` before `x` is written, so `x` stays
    /// `None`.
    pub(super) fn lazy_await(&mut self, operand: &Expr, span: Span) -> ExprKind {
        let (ExprKind::Var(x) | ExprKind::Local(x)) = &operand.kind else {
            panic!("the checker read `await` of a `let lazy` name")
        };
        let x = x.clone();
        let attempt = self.lazies.get(&x).cloned().unwrap_or_else(|| {
            panic!("`{x}` is a `let lazy` the lowering declared before its `await`")
        });
        let at = |kind| Expr::new(kind, span);
        let outcome = self.hidden("$outcome");
        let local = |name: &str| at(ExprKind::Local(name.to_owned()));
        let give = |o: Expr| {
            at(ExprKind::App {
                func: Box::new(at(ExprKind::lang(Lang::FromResult))),
                arg: Box::new(o),
                stage: Stage::Runtime,
                arg_first: false,
            })
        };
        let run = at(ExprKind::Block(
            vec![
                Stmt::Let(Pat::Bind(outcome.clone()), attempt),
                Stmt::Assign(x.clone(), forced(local(&outcome))),
            ],
            Box::new(give(local(&outcome))),
        ));
        let kept = self.hidden("$kept");
        let some = constructor(Lang::Some, Lang::Option, 1);
        ExprKind::Match(
            Box::new(at(ExprKind::Var(x))),
            vec![
                Arm {
                    pat: Pat::Con(some, vec![Pat::Bind(kept.clone())]),
                    guard: None,
                    body: give(local(&kept)),
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: run,
                },
            ],
        )
    }
}

/// `Some outcome`: the state of a `let lazy` whose initializer has ended.
pub(super) fn forced(outcome: Expr) -> Expr {
    apply(constructor(Lang::Some, Lang::Option, 1), outcome)
}

/// `Ok value`, the outcome of an initializer that gave `value`, or of an
/// assignment to a `let lazy mut`.
pub(super) fn ok(value: Expr) -> Expr {
    apply(constructor(Lang::Ok, Lang::Result, 1), value)
}

fn apply(c: Constructor, arg: Expr) -> Expr {
    let span = arg.span;
    Expr::new(
        ExprKind::App {
            func: Box::new(Expr::new(ExprKind::Con(c), span)),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        },
        span,
    )
}

/// The constructor `name` of the Prelude type `ty`, both lang items.
fn constructor(name: Lang, ty: Lang, arity: usize) -> Constructor {
    let item = lang::item(name);
    Constructor {
        module: item.module.clone(),
        ty: lang::item(ty).name.clone(),
        name: item.name.clone(),
        arity,
        rep: Rep::Tagged,
        dicts: 0,
    }
}

/// `await` of a `Lazy`: `lazy_force` of it.
pub(super) fn force(operand: Expr, span: Span) -> ExprKind {
    runtime_call(Lang::LazyForce, operand, span)
}

fn runtime_call(function: Lang, arg: Expr, span: Span) -> ExprKind {
    ExprKind::App {
        func: Box::new(Expr::new(ExprKind::lang(function), span)),
        arg: Box::new(arg),
        stage: Stage::Runtime,
        arg_first: false,
    }
}
