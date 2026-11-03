//! Fusion: a `Seq` or `List` combinator applied to a `for` runs in the
//! loop of the `for` instead of a generator or a list of its own.
//!
//! - `Seq.map f`, `Seq.filter p` and `Seq.flat_map g` of a lazy `for`, and
//!   `List.map f`, `List.filter p` of a strict one, rewrite its element;
//! - `Seq.take n` of a lazy `for` ends it after `n` elements;
//! - `Seq.to_list` of a lazy `for` is the strict one;
//! - a `for` over a `for` (or a `[for …]`) nests the clauses of the inner
//!   one, without a generator inside a generator.
//!
//! A lazy `for` and a lazy combinator already run per element, so fusing
//! them keeps the order of evaluation. A strict `[for …]` builds its whole
//! list first: pure code is not total (`1n / 0n` throws), so its clauses
//! are interleaved with the code that consumes the list only when one side
//! cannot throw. A `for` whose body makes Sync calls is left as it is: its
//! steps read the services it captured. Combinators are recognized by the
//! binding they refer to, not by the name in scope.

use std::mem;

use super::inline::{exits, simple};
use super::walk::droppable;
use crate::ir::{Expr, ExprKind, Lit, Pat, Stage, Stmt};
use crate::source::Span;
use crate::types::lang::{self, Lang};

/// Fuses combinators of `e` and below into the `for`s they take; `fresh`
/// numbers the names it binds.
pub fn fuse(e: &mut Expr, fresh: &mut usize) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= fuse(c, fresh));
    changed |= combinator(e, fresh) || nested(e);
    changed
}

/// The function a combinator applies: a lambda of one simple parameter is
/// applied in place, any other function is bound once before the loop.
enum Function {
    Lambda(Pat, Box<Expr>),
    Bound(String),
}

impl Function {
    fn of(f: Expr, in_place: bool, fresh: &mut usize, hoisted: &mut Vec<Stmt>) -> Function {
        if in_place
            && let ExprKind::Lambda { params, body } = &f.kind
            && let [param] = &params[..]
            && param.stage == Stage::Pure
            && simple(&param.pat)
            // its `o?` returns from the lambda, not from the loop's function
            && !exits(body)
        {
            return Function::Lambda(param.pat.clone(), body.clone());
        }
        let name = fresh_name(fresh);
        hoisted.push(Stmt::Let(Pat::Bind(name.clone()), f));
        Function::Bound(name)
    }

    /// The function applied to `arg`.
    fn apply(&self, arg: Expr) -> Expr {
        let span = arg.span;
        match self {
            Function::Lambda(Pat::Bind(name), body) => Expr::new(
                ExprKind::Block(vec![Stmt::Let(Pat::Bind(name.clone()), arg)], body.clone()),
                span,
            ),
            Function::Lambda(_, body) => {
                Expr::new(ExprKind::Block(vec![Stmt::Expr(arg)], body.clone()), span)
            }
            Function::Bound(name) => Expr::new(
                ExprKind::App {
                    func: Box::new(Expr::new(ExprKind::Local(name.clone()), span)),
                    arg: Box::new(arg),
                    stage: Stage::Pure,
                    arg_first: false,
                },
                span,
            ),
        }
    }
}

fn fresh_name(fresh: &mut usize) -> String {
    *fresh += 1;
    format!("$fuse{fresh}")
}

/// `module.name a…` of a known function applied to all of `args`.
fn call(e: &mut Expr, arity: usize) -> Option<(&str, &str, Vec<&mut Expr>)> {
    let mut args = Vec::new();
    let mut head = e;
    while args.len() < arity {
        let ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            arg_first: false,
        } = &mut head.kind
        else {
            return None;
        };
        args.push(&mut **arg);
        head = func;
    }
    args.reverse();
    match &head.kind {
        ExprKind::Global { module, name } => Some((module.as_str(), name.as_str(), args)),
        _ => None,
    }
}

/// Whether `e` is a `for` that fusion may rewrite: lazy or strict as asked,
/// without Sync calls or a limit.
fn fusable(e: &Expr, strict: bool) -> bool {
    matches!(
        &e.kind,
        ExprKind::Generate { strict: s, sync: false, limit: None, .. } if *s == strict
    )
}

/// The combinators a `for` takes in: lazy on sequences, strict on lists.
const COMBINATORS: [Lang; 6] = [
    Lang::SeqMap,
    Lang::SeqFilter,
    Lang::SeqFlatMap,
    Lang::SeqTake,
    Lang::ListMap,
    Lang::ListFilter,
];

/// A combinator applied to a `for`, rewritten into it.
fn combinator(e: &mut Expr, fresh: &mut usize) -> bool {
    let span = e.span;
    // `Seq.to_list (for …)`
    if let Some((module, name, mut args)) = call(e, 1)
        && lang::is(Lang::SeqToList, module, name)
        && fusable(args[0], false)
    {
        let mut generate = mem::replace(args.remove(0), unit(span));
        if let ExprKind::Generate { strict, .. } = &mut generate.kind {
            *strict = true;
        }
        *e = generate;
        return true;
    }
    let Some((module, name, mut args)) = call(e, 2) else {
        return false;
    };
    let Some(step) = COMBINATORS.into_iter().find(|&c| lang::is(c, module, name)) else {
        return false;
    };
    let lazy = !matches!(step, Lang::ListMap | Lang::ListFilter);
    if !fusable(args[1], !lazy) {
        return false;
    }
    // the callback would run before the rest of a strict list is built
    if !lazy && !generated_total(args[1]) && !lambda_total(args[0]) {
        return false;
    }
    let mut generate = mem::replace(&mut *args.pop().expect("two"), unit(span));
    let f = mem::replace(&mut *args.pop().expect("two"), unit(span));
    let ExprKind::Generate { body, limit, .. } = &mut generate.kind else {
        unreachable!("fusable")
    };
    if step == Lang::SeqTake {
        *limit = Some(Box::new(f));
        *e = generate;
        return true;
    }
    let mut hoisted = Vec::new();
    // a lambda applied in place is copied once per element it yields
    let function = Function::of(f, yields(body) == 1, fresh, &mut hoisted);
    let element = |value: Expr, fresh: &mut usize| -> Expr {
        let at = value.span;
        match step {
            Lang::SeqMap | Lang::ListMap => yield_of(function.apply(value)),
            Lang::SeqFilter | Lang::ListFilter => {
                let v = fresh_name(fresh);
                let local = || Expr::new(ExprKind::Local(v.clone()), at);
                let test = function.apply(local());
                let keep = Expr::new(
                    ExprKind::If(
                        Box::new(test),
                        Box::new(yield_of(local())),
                        Box::new(unit(at)),
                    ),
                    at,
                );
                Expr::new(
                    ExprKind::Block(vec![Stmt::Let(Pat::Bind(v.clone()), value)], Box::new(keep)),
                    at,
                )
            }
            _ => {
                let v = fresh_name(fresh);
                Expr::new(
                    ExprKind::Each {
                        pat: Pat::Bind(v.clone()),
                        source: Box::new(function.apply(value)),
                        body: Box::new(yield_of(Expr::new(ExprKind::Local(v), at))),
                    },
                    at,
                )
            }
        }
    };
    map_yields(body, &mut |value| element(value, fresh));
    *e = if hoisted.is_empty() {
        generate
    } else {
        Expr::new(ExprKind::Block(hoisted, Box::new(generate)), span)
    };
    true
}

/// `for { x <- (for { … } yield e); … }`: the clauses of the inner `for`
/// around the rest, `x` bound to `e`.
fn nested(e: &mut Expr) -> bool {
    let ExprKind::Each { pat, source, body } = &mut e.kind else {
        return false;
    };
    // a strict inner list is built before the outer clauses run
    let inner_ok = match &source.kind {
        ExprKind::Generate {
            sync: false,
            limit: None,
            strict,
            body,
        } => !*strict || total(body),
        _ => false,
    };
    if !inner_ok || !matches!(pat, Pat::Bind(_) | Pat::Wild) {
        return false;
    }
    let ExprKind::Generate { body: inner, .. } = &mut source.kind else {
        return false;
    };
    if yields(inner) != 1 {
        return false;
    }
    let (pat, rest) = (pat.clone(), mem::replace(&mut **body, unit(e.span)));
    let mut inner = mem::replace(&mut **inner, unit(e.span));
    let mut rest = Some(rest);
    map_yields(&mut inner, &mut |value| {
        let at = value.span;
        let rest = rest.take().expect("one yield");
        let stmt = match &pat {
            Pat::Bind(_) => Stmt::Let(pat.clone(), value),
            _ => Stmt::Expr(value),
        };
        Expr::new(ExprKind::Block(vec![stmt], Box::new(rest)), at)
    });
    *e = inner;
    true
}

/// Whether the elements of the `for` `e` are computed by code that cannot
/// throw or diverge.
fn generated_total(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Generate { body, .. } if total(body))
}

/// Whether `f` is a lambda whose body cannot throw or diverge.
fn lambda_total(f: &Expr) -> bool {
    matches!(&f.kind, ExprKind::Lambda { body, .. } if droppable(body))
}

/// Whether the body of a `for` evaluates nothing that can throw or
/// diverge: its sources, guards, bindings and elements.
fn total(body: &Expr) -> bool {
    match &body.kind {
        ExprKind::Yield(value) => droppable(value),
        ExprKind::Each { source, body, .. } => droppable(source) && total(body),
        ExprKind::If(c, t, f) => droppable(c) && total(t) && total(f),
        ExprKind::Block(stmts, tail) => {
            stmts.iter().all(|s| match s {
                Stmt::Let(_, v) | Stmt::Expr(v) => droppable(v),
                Stmt::Fun { .. } => false,
            }) && total(tail)
        }
        _ => droppable(body),
    }
}

fn yield_of(value: Expr) -> Expr {
    let span = value.span;
    Expr::new(ExprKind::Yield(Box::new(value)), span)
}

fn unit(span: Span) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Unit), span)
}

/// The `Yield`s of a `for`'s body: not those of a `for` or a function in it.
fn yields(body: &Expr) -> usize {
    match &body.kind {
        ExprKind::Yield(_) => 1,
        ExprKind::Generate { .. } | ExprKind::Lambda { .. } => 0,
        _ => {
            let mut n = 0;
            body.each_child(&mut |c| n += yields(c));
            n
        }
    }
}

/// Replaces each `Yield v` of a `for`'s body by `f v`.
fn map_yields(body: &mut Expr, f: &mut dyn FnMut(Expr) -> Expr) {
    match &mut body.kind {
        ExprKind::Yield(value) => {
            let value = mem::replace(&mut **value, unit(body.span));
            *body = f(value);
        }
        ExprKind::Generate { .. } | ExprKind::Lambda { .. } => {}
        _ => body.each_child_mut(&mut |c| map_yields(c, f)),
    }
}
