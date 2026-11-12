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
use super::devirt::built;
use super::known::{chooses, ends_in_data};
use super::walk::{rename, respan, size, uses};
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage, Stmt};
use crate::types::builtins;

/// The size budget of a body inlined at every use, in IR nodes: no bigger
/// than the call it replaces, give or take.
const SMALL: usize = 8;

/// The size budget of a private function used once, which inlining removes.
const ONCE: usize = 60;

/// The size budget of a body inlined where that spares an object: a
/// `match` of an option and what its arms do.
const SHAPED: usize = 24;

/// The parameters and body of a function that may be inlined.
type Inlined = (Vec<Param>, Expr);

/// A full application to inline: the function, its arguments in the order
/// of its parameters, whether it is of another module, and whether the last
/// argument was written first.
struct Applied {
    function: Inlined,
    args: Vec<Expr>,
    theirs: bool,
    piped: bool,
}

/// Functions of a module that may be inlined: name → parameters and body.
pub type Candidates = HashMap<String, Inlined>;

/// The small functions of the built-in modules, as `generics` gives them,
/// each asked for once. Those of the program's other modules stay calls:
/// a body there may be written to be shared, as a `match` that is longer
/// than its call.
pub struct Foreign<'a> {
    generics: Generics<'a>,
    found: RefCell<HashMap<(String, String), Option<Inlined>>>,
    /// Those that choose, by a `match` or an `if`, what they give.
    shaped: RefCell<HashMap<(String, String), Option<Inlined>>>,
}

impl<'a> Foreign<'a> {
    pub fn new(generics: Generics<'a>) -> Self {
        Foreign {
            generics,
            found: RefCell::default(),
            shaped: RefCell::default(),
        }
    }

    /// The parameters and body of `name` of a built-in `module` when it is
    /// plain pure code no bigger than `budget`, with a `match` only when
    /// `choosing`: small, it is inlined at every call, as that makes the
    /// module no bigger; one that chooses what it gives is bigger than its
    /// call, and inlined only where that spares an object.
    fn body(&self, module: &str, name: &str, choosing: bool) -> Option<Inlined> {
        let cache = if choosing { &self.shaped } else { &self.found };
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = cache.borrow().get(&key) {
            return found.clone();
        }
        let budget = if choosing { SHAPED } else { SMALL };
        let mut found = None;
        if builtins().iter().any(|i| i.module == module)
            && let Some((dicts, body)) = (self.generics)(module, name)
            && let ExprKind::Lambda { params, body } = body.kind
            && dicts.is_empty()
            && params
                .iter()
                .all(|p| p.stage == Stage::Pure && simple(&p.pat))
            && size(&body) <= budget
            && plain(&body, choosing)
            && !calls(&body, module, name)
        {
            found = Some((params, *body));
        }
        cache.borrow_mut().insert(key, found.clone());
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
    plain(e, false)
}

/// [`inlinable`], where `choosing` lets the body have a `match`.
fn plain(e: &Expr, choosing: bool) -> bool {
    let here = !matches!(
        &e.kind,
        ExprKind::ControlLoop { .. }
            | ExprKind::LoopExit { .. }
            | ExprKind::Loop { .. }
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
    let here = here && (choosing || !matches!(e.kind, ExprKind::Match(..)));
    let mut children = true;
    e.each_child(&mut |c| children &= plain(c, choosing));
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
    let Some(Applied {
        function: (params, mut body),
        args,
        theirs,
        piped,
    }) = full_application(e, module, candidates, foreign)
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
    let mut stmts: Vec<Stmt> = params
        .iter()
        .zip(args)
        .map(|(p, a)| bind(&p.pat, a))
        .collect();
    // the argument written first, of `a |> f`, is evaluated first
    if piped {
        stmts.rotate_right(1);
    }
    *e = Expr::new(ExprKind::Block(stmts, body), e.span);
    true
}

/// `f a b` or `b |> f a` of a candidate `f`, of the module or a small one
/// of another, taking exactly the stages applied, all pure.
fn full_application(
    e: &Expr,
    module: &str,
    candidates: &Candidates,
    foreign: &Foreign,
) -> Option<Applied> {
    let (m, name, args, piped) = applied_global(e)?;
    let function = if m == module {
        candidates.get(name).cloned()?
    } else {
        foreign.body(m, name, false)?
    };
    (args.len() == function.0.len()).then(|| Applied {
        function,
        args: args.into_iter().cloned().collect(),
        theirs: m != module,
        piped,
    })
}

/// `let pat = value`; a `()` or `_` parameter only evaluates its argument.
fn bind(pat: &Pat, value: Expr) -> Stmt {
    match pat {
        Pat::Bind(_) => Stmt::Let(pat.clone(), value),
        _ => Stmt::Expr(value),
    }
}

/// `(fun p … -> body) arg` → `let p = arg` before the rest of the function,
/// for a pure stage, or a last stage that its call runs;
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
        func, arg, stage, ..
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
        .is_none_or(|p| p.stage != *stage || !simple(&p.pat))
        // a call of an effectful stage runs the body, which then runs here,
        // in the stage of the call; a stage before others runs nothing
        || (*stage != Stage::Pure && params.len() > 1)
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

/// `f a b` or `b |> f a` of a top-level function, every stage pure: the
/// function, and its arguments in the order of its parameters; whether the
/// last one was written first, and so is evaluated first.
fn applied_global(e: &Expr) -> Option<(&str, &str, Vec<&Expr>, bool)> {
    let (mut head, mut piped) = (e, None);
    if let ExprKind::App {
        func,
        arg,
        stage: Stage::Pure,
        arg_first: true,
    } = &e.kind
    {
        (head, piped) = (func, Some(&**arg));
    }
    let mut args = Vec::new();
    while let ExprKind::App {
        func,
        arg,
        stage: Stage::Pure,
        arg_first: false,
    } = &head.kind
    {
        args.push(&**arg);
        head = func;
    }
    let ExprKind::Global { module, name } = &head.kind else {
        return None;
    };
    args.reverse();
    args.extend(piped);
    Some((module, name, args, piped.is_some()))
}

/// Whether `body` reads `name` once, to call it.
fn called_once(body: &Expr, name: &str) -> bool {
    fn called(e: &Expr, name: &str) -> bool {
        if let ExprKind::App { func, .. } = &e.kind
            && matches!(&func.kind, ExprKind::Local(l) if l == name)
        {
            return true;
        }
        let mut found = false;
        e.each_child(&mut |c| found |= called(c, name));
        found
    }
    uses(body, name) == 1 && called(body, name)
}

/// Whether `body` is, under its statements, a `match` of the local `name`.
fn takes_apart(body: &Expr, name: &str) -> bool {
    match &body.kind {
        ExprKind::Block(_, tail) => takes_apart(tail, name),
        ExprKind::Match(scrutinee, _) => {
            matches!(&scrutinee.kind, ExprKind::Local(l) if l == name)
        }
        _ => false,
    }
}

/// Puts `value` where `e` reads the local `name`.
fn substitute(e: &mut Expr, name: &str, value: &mut Option<Expr>) {
    if matches!(&e.kind, ExprKind::Local(l) if l == name) {
        if let Some(value) = value.take() {
            *e = value;
        }
        return;
    }
    e.each_child_mut(&mut |c| substitute(c, name, value));
}

/// Inlines, in `e`, the applications of built-in functions that choose what
/// they give, where that spares an object: a function written where it is
/// passed and called once is called where it is written, a constructor
/// passed to be taken apart is taken apart, and a result taken apart at
/// once is never built. `data` is the locals in scope bound to data, or to
/// a choice between data. Whether anything changed.
pub fn shaped(e: &mut Expr, foreign: &Foreign, fresh: &mut usize, data: &mut Vec<String>) -> bool {
    let mut changed = false;
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        let mark = data.len();
        for stmt in stmts.iter_mut() {
            match stmt {
                Stmt::Let(pat, v) => {
                    changed |= shaped(v, foreign, fresh, data);
                    if let Pat::Bind(name) = pat
                        && (built(v) || chooses(v))
                    {
                        data.push(name.clone());
                    }
                }
                Stmt::Var(_, v) | Stmt::Assign(_, v) | Stmt::Expr(v) => {
                    changed |= shaped(v, foreign, fresh, data);
                }
                Stmt::Fun { value, .. } => changed |= shaped(value, foreign, fresh, data),
            }
        }
        changed |= shaped(result, foreign, fresh, data);
        data.truncate(mark);
        return changed;
    }
    e.each_child_mut(&mut |c| changed |= shaped(c, foreign, fresh, data));
    if let ExprKind::Match(scrutinee, _) = &mut e.kind {
        changed |= shape(scrutinee, foreign, fresh, data, true);
    }
    changed | shape(e, foreign, fresh, data, false)
}

/// Inlines the application `e` when it spares an object; `taken_apart`
/// tells that a `match` reads its result.
fn shape(
    e: &mut Expr,
    foreign: &Foreign,
    fresh: &mut usize,
    data: &[String],
    taken_apart: bool,
) -> bool {
    let Some((module, name, args, piped)) = applied_global(e) else {
        return false;
    };
    let Some((params, mut body)) = foreign.body(module, name, true) else {
        return false;
    };
    if args.len() != params.len() {
        return false;
    }
    // which arguments are functions their parameter calls once
    let called: Vec<bool> = (params.iter().zip(&args))
        .map(|(p, a)| {
            matches!((&p.pat, &a.kind), (Pat::Bind(x), ExprKind::Lambda { .. }) if called_once(&body, x))
        })
        .collect();
    // an argument that is data, or a choice between data, given to be taken apart
    let known = |a: &Expr| {
        built(a) || chooses(a) || matches!(&a.kind, ExprKind::Local(l) if data.contains(l))
    };
    let apart = (params.iter().zip(&args))
        .any(|(p, a)| matches!(&p.pat, Pat::Bind(x) if known(a) && takes_apart(&body, x)));
    if !(called.contains(&true) || apart || (taken_apart && ends_in_data(&body))) {
        return false;
    }
    let args: Vec<Expr> = args.into_iter().cloned().collect();
    respan(&mut body, e.span);
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
    let ExprKind::Lambda { params, mut body } = lambda.kind else {
        unreachable!("a lambda stays one when renamed")
    };
    let last = params.len() - 1;
    let mut stmts = Vec::new();
    for (i, ((p, a), called)) in params.iter().zip(args).zip(called).enumerate() {
        match &p.pat {
            // made where it is called: making a function shows nothing
            Pat::Bind(x) if called => substitute(&mut body, x, &mut Some(a)),
            // the argument written first, of `a |> f`, is evaluated first
            pat if piped && i == last => stmts.insert(0, bind(pat, a)),
            pat => stmts.push(bind(pat, a)),
        }
    }
    *e = Expr::new(ExprKind::Block(stmts, body), e.span);
    true
}
