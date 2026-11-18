//! `lazy { … }` and `await` of a `Lazy` (`docs/formal/lazy.md`): calls of
//! the Prelude's `lazy_make` and `lazy_force`, which the runtimes implement;
//! `let lazy`, a variable of its function; and the tasks of arguments passed
//! unstarted to a parameter marked `~`.

use super::Lower;
use crate::ir::{Arm, Expr, ExprKind, ExprLang, Handler, Lit, Pat, Stage, Stmt};
use crate::opt::rename;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::lang::{self, Lang};
use crate::types::{Constructor, Rep, StageKey};

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

    /// `let lazy x = e` as the variable `x` of its state: `None` until an
    /// `await` settles it to `Some outcome`. `e`, lowered here with the
    /// names in scope here, runs at that `await`; it reads no `let mut`
    /// (`FLICK-G005`), so it reads the values it would read here.
    pub(super) fn lazy_state(&mut self, unique: String, init: Expr) -> Stmt {
        let span = init.span;
        self.lazies.insert(unique.clone(), (init, 0));
        self.vars.insert(unique.clone());
        Stmt::Var(
            unique,
            Expr::new(
                ExprKind::Con(constructor(Lang::None, Lang::Option, 0)),
                span,
            ),
        )
    }

    /// `await x` of a `let lazy`: the kept outcome, or the outcome of its
    /// initializer, kept before it is given back.
    ///
    /// ```text
    /// match x with
    /// | Some o -> from_result o
    /// | _ -> let o = try Ok e with | err -> Err err in x = Some o; from_result o
    /// ```
    ///
    /// A signal or a defect leaves `e` before `x` is written, so `x` stays
    /// `None`. Each `await` after the first runs a copy of `e` whose locals
    /// are renamed, so the locals of the definition stay unique.
    pub(super) fn lazy_await(&mut self, operand: &Expr, span: Span) -> ExprKind {
        let (ExprKind::Var(x) | ExprKind::Local(x)) = &operand.kind else {
            panic!("the checker read `await` of a `let lazy` name")
        };
        let x = x.clone();
        let (init, copies) = self.lazies.get_mut(&x).unwrap_or_else(|| {
            panic!("`{x}` is a `let lazy` the lowering declared before its `await`")
        });
        let mut init = init.clone();
        if *copies > 0 {
            rename(&mut init, &format!("$force{copies}"));
        }
        *copies += 1;
        let at = |kind| Expr::new(kind, span);
        let outcome = self.hidden("$outcome");
        let error = self.hidden("$error");
        let local = |name: &str| at(ExprKind::Local(name.to_owned()));
        let give = |o: Expr| {
            at(ExprKind::App {
                func: Box::new(at(ExprKind::lang(Lang::FromResult))),
                arg: Box::new(o),
                stage: Stage::Runtime,
                arg_first: false,
            })
        };
        let attempt = at(ExprKind::Handle(Box::new(Handler {
            effects: Vec::new(),
            body: ok(init),
            ret: None,
            errors: vec![Arm {
                pat: Pat::Bind(error.clone()),
                guard: None,
                body: apply(constructor(Lang::Err, Lang::Result, 1), local(&error)),
            }],
        })));
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
