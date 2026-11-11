//! Inlining and beta reduction. A known full application of a small pure
//! function, of the module or of a built-in one whose body names only what
//! that one exports, becomes its body with the arguments bound in order, and
//! `(fun x -> body) arg` becomes `let x = arg` before `body`: the function
//! value is made without effects, so binding the arguments first keeps the
//! order of evaluation.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;

use super::Generics;
use super::walk::{rename, respan, size};
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage, Stmt};
use crate::types::builtins;

/// The size budget of a body inlined at every use, in IR nodes: no bigger
/// than the call it replaces, give or take.
const SMALL: usize = 8;

/// The size budget of a private function used once, which inlining removes.
const ONCE: usize = 60;

/// The parameters and body of a function that may be inlined.
type Inlined = (Vec<Param>, Expr);

/// Functions of a module that may be inlined: name → parameters and body.
pub type Candidates = HashMap<String, Inlined>;

/// The small functions of the built-in modules, as `generics` gives them,
/// each asked for once. Those of the program's other modules stay calls:
/// a body there may be written to be shared, as a `match` that is longer
/// than its call.
pub struct Foreign<'a> {
    generics: Generics<'a>,
    found: RefCell<HashMap<(String, String), Option<Inlined>>>,
}

impl<'a> Foreign<'a> {
    pub fn new(generics: Generics<'a>) -> Self {
        Foreign {
            generics,
            found: RefCell::default(),
        }
    }

    /// The parameters and body of `name` of `module` when it is small:
    /// inlining it makes the module no bigger.
    fn small(&self, module: &str, name: &str) -> Option<Inlined> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = self.found.borrow().get(&key) {
            return found.clone();
        }
        let builtin = builtins().iter().any(|i| i.module == module);
        let found = builtin.then(|| (self.generics)(module, name)).flatten();
        let found = found.and_then(|(dicts, body)| {
            let ExprKind::Lambda { params, body } = body.kind else {
                return None;
            };
            let small = dicts.is_empty()
                && params
                    .iter()
                    .all(|p| p.stage == Stage::Pure && simple(&p.pat))
                && size(&body) <= SMALL
                && inlinable(&body)
                && !calls(&body, module, name);
            small.then_some((params, *body))
        });
        self.found.borrow_mut().insert(key, found.clone());
        found
    }
}

/// Small functions, and private ones used once: inlining them makes the
/// module no bigger.
pub fn candidates(module: &str, items: &[Item], exported: &dyn Fn(&str) -> bool) -> Candidates {
    let mut refs = HashMap::new();
    for item in items {
        item_refs(item, module, &mut refs);
    }
    items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, body } if dicts.is_empty() => match &body.kind {
                ExprKind::Lambda { params, body }
                    if params
                        .iter()
                        .all(|p| p.stage == Stage::Pure && simple(&p.pat))
                        && (size(body) <= SMALL
                            || (size(body) <= ONCE
                                && !exported(name)
                                && refs.get(name.as_str()) == Some(&1)))
                        && inlinable(body)
                        && !calls(body, module, name) =>
                {
                    Some((name.clone(), (params.clone(), (**body).clone())))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// How often each top-level value of `module` is named in `item`.
fn item_refs<'a>(item: &'a Item, module: &str, out: &mut HashMap<&'a str, usize>) {
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
            refs(body, module, out);
        }
        ItemKind::Refined { check, .. } => refs(check, module, out),
        ItemKind::Instance { methods, .. } => {
            for (_, m) in methods {
                refs(m, module, out);
            }
        }
        _ => {}
    }
}

fn refs<'a>(e: &'a Expr, module: &str, out: &mut HashMap<&'a str, usize>) {
    if let ExprKind::Global { module: m, name } = &e.kind
        && m == module
    {
        *out.entry(name.as_str()).or_default() += 1;
    }
    e.each_child(&mut |c| refs(c, module, out));
}

pub(super) fn simple(p: &Pat) -> bool {
    matches!(p, Pat::Bind(_) | Pat::Wild | Pat::Lit(Lit::Unit))
}

/// Whether `e` has an `o?` of its own function, outside nested ones.
pub(super) fn exits(e: &Expr) -> bool {
    if matches!(e.kind, ExprKind::Unwrap(_)) {
        return true;
    }
    let mut found = false;
    if !matches!(e.kind, ExprKind::Lambda { .. }) {
        e.each_child(&mut |c| found |= exits(c));
    }
    found
}

/// Whether a body is plain pure code: nothing that depends on where it
/// runs (owners, handlers, services, dictionaries of its definition). A
/// `match` stays a call too: in an expression it becomes statements, which
/// cost more than the call.
fn inlinable(e: &Expr) -> bool {
    let here = !matches!(
        &e.kind,
        ExprKind::ControlLoop { .. }
            | ExprKind::LoopExit { .. }
            | ExprKind::Loop { .. }
            | ExprKind::Match(..)
            | ExprKind::Generate { .. }
            | ExprKind::Each { .. }
            | ExprKind::Yield(_)
            | ExprKind::Within(..)
            | ExprKind::Handle(_)
            | ExprKind::Task(_)
            | ExprKind::Await(_)
            | ExprKind::Abstract(..)
            | ExprKind::Dicts(..)
            | ExprKind::Method { .. }
            | ExprKind::Lift(..)
            | ExprKind::Perform { .. }
            | ExprKind::Unwrap(_)
            | ExprKind::App {
                stage: Stage::Sync | Stage::Runtime,
                ..
            }
    );
    let mut children = true;
    e.each_child(&mut |c| children &= inlinable(c));
    here && children
}

fn calls(e: &Expr, module: &str, name: &str) -> bool {
    if matches!(&e.kind, ExprKind::Global { module: m, name: n } if m == module && n == name) {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= calls(c, module, name));
    found
}

/// Inlines known full applications of `candidates` and of the small
/// functions of other modules in `e`; `fresh` numbers the copies. Whether
/// anything changed.
pub fn inline(
    e: &mut Expr,
    module: &str,
    candidates: &Candidates,
    foreign: &Foreign,
    fresh: &mut usize,
) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= inline(c, module, candidates, foreign, fresh));
    let Some((params, mut body, args, theirs)) = full_application(e, module, candidates, foreign)
    else {
        return changed;
    };
    // the spans of another module are of its file
    if theirs {
        respan(&mut body, e.span);
    }
    *fresh += 1;
    let suffix = format!("$i{fresh}");
    let mut lambda = Expr::new(
        ExprKind::Lambda {
            params,
            body: Box::new(body),
        },
        e.span,
    );
    rename(&mut lambda, &suffix);
    let ExprKind::Lambda { params, body } = lambda.kind else {
        unreachable!("a lambda stays one when renamed")
    };
    let stmts = params
        .iter()
        .zip(args)
        .map(|(p, a)| bind(&p.pat, a))
        .collect();
    *e = Expr::new(ExprKind::Block(stmts, body), e.span);
    true
}

/// `f a b` of a candidate `f`, of the module or a small one of another,
/// taking exactly the stages applied, all pure, arguments evaluated left to
/// right (not `a |> f`): its parameters, body and arguments, and whether
/// it is of another module.
fn full_application(
    e: &Expr,
    module: &str,
    candidates: &Candidates,
    foreign: &Foreign,
) -> Option<(Vec<Param>, Expr, Vec<Expr>, bool)> {
    let mut args = Vec::new();
    let mut head = e;
    while let ExprKind::App {
        func,
        arg,
        stage: Stage::Pure,
        arg_first: false,
    } = &head.kind
    {
        args.push((**arg).clone());
        head = func;
    }
    let ExprKind::Global { module: m, name } = &head.kind else {
        return None;
    };
    let (params, body) = if m == module {
        candidates.get(name).cloned()?
    } else {
        foreign.small(m, name)?
    };
    args.reverse();
    (args.len() == params.len()).then_some((params, body, args, m != module))
}

/// `let pat = value`; a `()` or `_` parameter only evaluates its argument.
fn bind(pat: &Pat, value: Expr) -> Stmt {
    match pat {
        Pat::Bind(_) => Stmt::Let(pat.clone(), value),
        _ => Stmt::Expr(value),
    }
}

/// `(fun p … -> body) arg` → `let p = arg` before the rest of the function;
/// `{ stmts; f } arg` → `{ stmts; f arg }`, which evaluates in the same order.
pub fn beta(e: &mut Expr) -> bool {
    if let ExprKind::App {
        func,
        arg_first: false,
        ..
    } = &mut e.kind
        && let ExprKind::Block(stmts, value) = &mut func.kind
    {
        let stmts = mem::take(stmts);
        let value = mem::replace(&mut **value, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
        let span = e.span;
        let ExprKind::App { arg, stage, .. } = mem::replace(&mut e.kind, ExprKind::Lit(Lit::Unit))
        else {
            unreachable!("matched above")
        };
        let app = ExprKind::App {
            func: Box::new(value),
            arg,
            stage,
            arg_first: false,
        };
        *e = Expr::new(ExprKind::Block(stmts, Box::new(Expr::new(app, span))), span);
        return true;
    }
    let ExprKind::App {
        func,
        arg,
        stage: Stage::Pure,
        ..
    } = &mut e.kind
    else {
        return false;
    };
    // `a |> (fun x -> …)` evaluates `a` first too: the function value is inert
    let ExprKind::Lambda { params, body } = &mut func.kind else {
        return false;
    };
    if params
        .first()
        .is_none_or(|p| p.stage != Stage::Pure || !simple(&p.pat))
        // an `o?` returns from the function it is in, not from the one around
        || (params.len() == 1 && exits(body))
    {
        return false;
    }
    let first = params.remove(0);
    let arg = mem::replace(&mut **arg, Expr::new(ExprKind::Lit(Lit::Unit), e.span));
    let rest = if params.is_empty() {
        mem::replace(&mut **body, Expr::new(ExprKind::Lit(Lit::Unit), e.span))
    } else {
        mem::replace(&mut **func, Expr::new(ExprKind::Lit(Lit::Unit), e.span))
    };
    *e = Expr::new(
        ExprKind::Block(vec![bind(&first.pat, arg)], Box::new(rest)),
        e.span,
    );
    true
}
