//! Inlining and beta reduction. A known full application of a small pure
//! function, of the module or of a built-in one whose body names only what
//! that one exports, becomes its body with the arguments bound in order, and
//! `(fun x -> body) arg` becomes `let x = arg` before `body`: the function
//! value is made without effects, so binding the arguments first keeps the
//! order of evaluation.

use std::cell::RefCell;
use std::collections::HashMap;

use super::Generics;
use super::devirt::built;
use super::known::{chooses, ends_in_data};
use super::project::path;
use super::specialize::Generic;
use super::types::subst_types;
use super::walk::{rename, respan, size};
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage, Stmt};
use flick_ir::TypeRef;

/// The size budget of a body inlined at every use, in IR nodes: no bigger
/// than the call it replaces, give or take.
const SMALL: usize = 8;

/// The size budget of a private function used once, which inlining removes.
const ONCE: usize = 60;

/// The size budget of a body inlined where that spares an object: a
/// `match` of an option and what its arms do.
const SHAPED: usize = 24;

/// The type parameters, parameters and body of a function that may be inlined.
type Inlined = (Vec<String>, Vec<Param>, Expr);

/// A full application to inline: the function, its type arguments, its
/// arguments in the order of its parameters, whether it is of another
/// module, and whether the last argument was written first.
struct Applied {
    function: Inlined,
    type_args: Vec<TypeRef>,
    args: Vec<Expr>,
    theirs: bool,
    piped: bool,
}

/// Functions of a module that may be inlined: name → parameters and body.
pub type Candidates = HashMap<String, Inlined>;

/// The small functions of the built-in modules, as `generics` gives them,
/// each asked for once. Those of the program's other modules stay calls:
/// a body there may be written to be shared, as a `match` that is longer
/// than its call. A projection, of any module of the build, is a path of
/// field reads no longer than [`DEPTH`](super::project::DEPTH), which is
/// inlined as a small body is.
pub struct Foreign<'a> {
    generics: Generics<'a>,
    builtin: &'a dyn Fn(&str) -> bool,
    found: RefCell<HashMap<(String, String), Option<Inlined>>>,
    /// Those that choose, by a `match` or an `if`, what they give.
    shaped: RefCell<HashMap<(String, String), Option<Inlined>>>,
}

impl<'a> Foreign<'a> {
    pub fn new(generics: Generics<'a>, builtin: &'a dyn Fn(&str) -> bool) -> Self {
        Foreign {
            generics,
            builtin,
            found: RefCell::default(),
            shaped: RefCell::default(),
        }
    }

    /// The parameters and body of `name` of a built-in `module` when it is
    /// plain pure code no bigger than `budget`, with a `match` only when
    /// `choosing`: small, it is inlined at every call, as that makes the
    /// module no bigger; one that chooses what it gives is bigger than its
    /// call, and inlined only where that spares an object. Not `choosing`,
    /// a projection of any module of the build.
    pub(super) fn body(&self, module: &str, name: &str, choosing: bool) -> Option<Inlined> {
        let cache = if choosing { &self.shaped } else { &self.found };
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = cache.borrow().get(&key) {
            return found.clone();
        }
        let budget = if choosing { SHAPED } else { SMALL };
        let builtin = (self.builtin)(module);
        let mut found = None;
        if (builtin || !choosing)
            && let Some(Generic {
                type_params,
                dicts,
                body,
                ..
            }) = (self.generics)(module, name)
            && let ExprKind::Lambda { params, body } = body.kind
            && dicts.is_empty()
            && params
                .iter()
                .all(|p| p.stage == Stage::Pure && simple(&p.pat))
            && ((builtin
                && size(&body) <= budget
                && plain(&body, choosing)
                && !calls(&body, module, name))
                || (!choosing && path(&params, &body).is_some()))
        {
            found = Some((type_params, params, *body));
        }
        cache.borrow_mut().insert(key, found.clone());
        found
    }
}

/// Small functions, projections, and private ones used once: inlining them
/// makes the module no bigger.
pub fn candidates(module: &str, items: &[Item], exported: &dyn Fn(&str) -> bool) -> Candidates {
    let mut refs = HashMap::new();
    for item in items {
        item_refs(item, module, &mut refs);
    }
    items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value {
                name,
                type_params,
                dicts,
                body,
                ..
            } if dicts.is_empty() => match &body.kind {
                ExprKind::Lambda { params, body }
                    if params
                        .iter()
                        .all(|p| p.stage == Stage::Pure && simple(&p.pat))
                        && (size(body) <= SMALL
                            || path(params, body).is_some()
                            || (size(body) <= ONCE
                                && !exported(name)
                                && refs.get(name.as_str()) == Some(&1)))
                        && inlinable(body)
                        && !calls(body, module, name) =>
                {
                    Some((
                        name.clone(),
                        (type_params.clone(), params.clone(), (**body).clone()),
                    ))
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
    if let ExprKind::Global {
        module: m, name, ..
    } = &e.kind
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
            | ExprKind::ReadOwnerEvidence
            | ExprKind::WithOwnerEvidence(..)
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
    if matches!(&e.kind, ExprKind::Global { module: m, name: n, .. } if m == module && n == name) {
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
    preserve_types: bool,
) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| {
        changed |= inline(c, module, candidates, foreign, fresh, preserve_types);
    });
    let Some(Applied {
        function: (type_params, params, mut body),
        type_args,
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
    let (params, body) = prepare_lambda(&type_params, params, body, &type_args, e.span, fresh);
    let mut stmts: Vec<Stmt> = params.iter().zip(args).map(|(p, a)| bind(p, a)).collect();
    // the argument written first, of `a |> f`, is evaluated first
    if piped {
        stmts.rotate_right(1);
    }
    let ty = preserve_types
        .then(|| e.ty.clone().or_else(|| body.ty.clone()))
        .flatten();
    *e = Expr::with_ty(ExprKind::Block(stmts, body), e.span, ty);
    true
}

fn prepare_lambda(
    type_params: &[String],
    params: Vec<Param>,
    body: Expr,
    type_args: &[TypeRef],
    span: flick_ir::Span,
    fresh: &mut usize,
) -> (Vec<Param>, Box<Expr>) {
    *fresh += 1;
    let suffix = format!("$i{fresh}");
    let mut lambda = Expr::new(
        ExprKind::Lambda {
            params,
            body: Box::new(body),
        },
        span,
    );
    if !type_params.is_empty() && type_params.len() == type_args.len() {
        let subst: Vec<(&str, &TypeRef)> = type_params
            .iter()
            .map(String::as_str)
            .zip(type_args)
            .collect();
        subst_types(&mut lambda, &subst);
    }
    rename(&mut lambda, &suffix);
    let ExprKind::Lambda { params, body } = lambda.kind else {
        unreachable!("a lambda stays one when renamed")
    };
    (params, body)
}

/// `f a b` or `b |> f a` of a candidate `f`, of the module or a small one
/// of another, taking exactly the stages applied, all pure.
fn full_application(
    e: &Expr,
    module: &str,
    candidates: &Candidates,
    foreign: &Foreign,
) -> Option<Applied> {
    let (m, name, type_args, args, piped) = applied_global(e)?;
    let function = if m == module {
        candidates.get(name).cloned()?
    } else {
        foreign.body(m, name, false)?
    };
    (args.len() == function.1.len()).then(|| Applied {
        function,
        type_args: type_args.to_vec(),
        args: args.into_iter().cloned().collect(),
        theirs: m != module,
        piped,
    })
}

/// `let pat = value`; a `()` or `_` parameter only evaluates its argument.
pub(super) fn bind(param: &Param, value: Expr) -> Stmt {
    match &param.pat {
        Pat::Bind(name) => {
            let bt = param
                .binder_type(name)
                .cloned()
                .or_else(|| value.ty.clone())
                .map(|t| vec![(name.clone(), t)])
                .unwrap_or_default();
            Stmt::let_typed(param.pat.clone(), bt, value)
        }
        _ => Stmt::Expr(value),
    }
}

type AppliedGlobal<'a> = (&'a str, &'a str, &'a [TypeRef], Vec<&'a Expr>, bool);

/// `f a b` or `b |> f a` of a top-level function, every stage pure: the
/// function, its type arguments, and its arguments in the order of its
/// parameters; whether the last one was written first, and so is evaluated
/// first.
fn applied_global(e: &Expr) -> Option<AppliedGlobal<'_>> {
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
    let (head, mut args) = head.pure_spine();
    let ExprKind::Global {
        module,
        name,
        type_args,
    } = &head.kind
    else {
        return None;
    };
    args.extend(piped);
    Some((module, name, type_args, args, piped.is_some()))
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
    body.uses(name) == 1 && called(body, name)
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
pub fn shaped(
    e: &mut Expr,
    foreign: &Foreign,
    fresh: &mut usize,
    data: &mut Vec<String>,
    preserve_types: bool,
) -> bool {
    let mut changed = false;
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        let mark = data.len();
        for stmt in stmts.iter_mut() {
            match stmt {
                Stmt::Let { pat, value: v, .. } => {
                    changed |= shaped(v, foreign, fresh, data, preserve_types);
                    if let Pat::Bind(name) = pat
                        && (built(v) || chooses(v))
                    {
                        data.push(name.clone());
                    }
                }
                Stmt::Var { value: v, .. } | Stmt::Assign(_, v) | Stmt::Expr(v) => {
                    changed |= shaped(v, foreign, fresh, data, preserve_types);
                }
                Stmt::Fun { value, .. } => {
                    changed |= shaped(value, foreign, fresh, data, preserve_types);
                }
            }
        }
        changed |= shaped(result, foreign, fresh, data, preserve_types);
        data.truncate(mark);
        return changed;
    }
    e.each_child_mut(&mut |c| changed |= shaped(c, foreign, fresh, data, preserve_types));
    if let ExprKind::Match(scrutinee, _) = &mut e.kind {
        changed |= shape(scrutinee, foreign, fresh, data, true, preserve_types);
    }
    changed | shape(e, foreign, fresh, data, false, preserve_types)
}

/// Inlines the application `e` when it spares an object; `taken_apart`
/// tells that a `match` reads its result.
fn shape(
    e: &mut Expr,
    foreign: &Foreign,
    fresh: &mut usize,
    data: &[String],
    taken_apart: bool,
    preserve_types: bool,
) -> bool {
    let Some((module, name, type_args, args, piped)) = applied_global(e) else {
        return false;
    };
    let Some((type_params, params, mut body)) = foreign.body(module, name, true) else {
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
    let type_args: Vec<TypeRef> = type_args.to_vec();
    let args: Vec<Expr> = args.into_iter().cloned().collect();
    respan(&mut body, e.span);
    let (params, mut body) = prepare_lambda(&type_params, params, body, &type_args, e.span, fresh);
    let last = params.len() - 1;
    let mut stmts = Vec::new();
    for (i, ((p, a), called)) in params.iter().zip(args).zip(called).enumerate() {
        match &p.pat {
            // made where it is called: making a function shows nothing
            Pat::Bind(x) if called => substitute(&mut body, x, &mut Some(a)),
            // the argument written first, of `a |> f`, is evaluated first
            _ if piped && i == last => stmts.insert(0, bind(p, a)),
            _ => stmts.push(bind(p, a)),
        }
    }
    let ty = preserve_types
        .then(|| e.ty.clone().or_else(|| body.ty.clone()))
        .flatten();
    *e = Expr::with_ty(ExprKind::Block(stmts, body), e.span, ty);
    true
}
