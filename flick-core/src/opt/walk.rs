//! What the passes ask of an expression: how a local is used, whether the
//! expression may be dropped or moved, its size; and the rewrites they share:
//! substitution and renaming of locals.

use std::collections::HashMap;

use crate::ir::{BinOp, Expr, ExprKind, Item, ItemKind, Pat, Stage, Stmt};
use crate::numeric::{Arith, Num};
use crate::source::Span;
use crate::types::{Constructor, Dict};

/// How often `e` reads the local `name`.
pub fn uses(e: &Expr, name: &str) -> usize {
    let mut n = 0;
    each_use(e, &mut |l| n += usize::from(l == name));
    n
}

/// Each read of a local in `e`, by its name.
pub fn each_use<'e>(e: &'e Expr, read: &mut dyn FnMut(&'e str)) {
    if let ExprKind::Local(l) | ExprKind::Within(l, _) = &e.kind {
        read(l);
    }
    e.each_child(&mut |c| each_use(c, read));
}

/// Each read of a local in `e` that is evaluated exactly once whenever `e`
/// is, by its name: not inside a function, task, branch, arm or handler.
pub fn each_strict_use<'e>(e: &'e Expr, read: &mut dyn FnMut(&'e str)) {
    match &e.kind {
        ExprKind::Local(l) => read(l),
        // only catching errors, it runs its body at once
        ExprKind::Handle(h) if h.is_try() => each_strict_use(&h.body, read),
        ExprKind::Lambda { .. }
        | ExprKind::Task(_)
        | ExprKind::Within(..)
        | ExprKind::Handle(_)
        | ExprKind::Lift(..)
        | ExprKind::Abstract(..)
        | ExprKind::Generate { strict: false, .. }
        | ExprKind::ControlLoop { .. }
        | ExprKind::LoopExit { .. }
        | ExprKind::Loop { .. } => {}
        // the body runs once per element, maybe never
        ExprKind::Each { source, .. } => each_strict_use(source, read),
        ExprKind::If(c, _, _) => each_strict_use(c, read),
        ExprKind::Match(s, _) => each_strict_use(s, read),
        // `&&` and `||` may skip their right operand
        ExprKind::Binary(BinOp::And | BinOp::Or, l, _) => each_strict_use(l, read),
        ExprKind::Block(stmts, result) => {
            for s in stmts {
                match s {
                    Stmt::Let(_, v) | Stmt::Var(_, v) | Stmt::Assign(_, v) | Stmt::Expr(v) => {
                        each_strict_use(v, read);
                    }
                    Stmt::Fun { .. } => {}
                }
            }
            each_strict_use(result, read);
        }
        _ => e.each_child(&mut |c| each_strict_use(c, read)),
    }
}

/// `e` at `span`, every part of it: code copied from another module, whose
/// spans are of its file.
pub fn respan(e: &mut Expr, span: Span) {
    e.span = span;
    e.each_child_mut(&mut |c| respan(c, span));
}

/// The number of nodes of `e`, patterns included: a `match` emits a test
/// per pattern node.
pub fn size(e: &Expr) -> usize {
    let mut n = 1;
    match &e.kind {
        ExprKind::Match(_, arms) => {
            n += arms.iter().map(|a| pat_size(&a.pat)).sum::<usize>();
        }
        ExprKind::Handle(h) => n += h.errors.iter().map(|a| pat_size(&a.pat)).sum::<usize>(),
        ExprKind::Each { pat, .. } => n += pat_size(pat),
        ExprKind::Block(stmts, _) => {
            n += stmts
                .iter()
                .map(|s| match s {
                    Stmt::Let(p, _) => pat_size(p),
                    _ => 0,
                })
                .sum::<usize>();
        }
        _ => {}
    }
    e.each_child(&mut |c| n += size(c));
    n
}

fn pat_size(p: &Pat) -> usize {
    match p {
        Pat::Wild | Pat::Bind(_) => 0,
        Pat::Lit(_) => 1,
        Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
            1 + items.iter().map(pat_size).sum::<usize>()
        }
        Pat::List(items, rest) => {
            1 + items
                .iter()
                .chain(rest.as_deref())
                .map(pat_size)
                .sum::<usize>()
        }
        Pat::Record(fields) => 1 + fields.iter().map(|(_, p)| pat_size(p)).sum::<usize>(),
    }
}

/// Whether evaluating `e` can do nothing observable: no effect, no throw,
/// no divergence, no allocation anyone could tell apart. Such an expression
/// may be dropped when its value is unused.
pub fn droppable(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(_)
        | ExprKind::Local(_)
        | ExprKind::Global { .. }
        | ExprKind::Con(_)
        | ExprKind::Label(_)
        | ExprKind::ServiceTag(_)
        | ExprKind::Lambda { .. }
        // a task or a sequence is a description: building it runs nothing
        | ExprKind::Task(_)
        | ExprKind::Generate {
            strict: false,
            limit: None,
            ..
        } => true,
        ExprKind::Tuple(items) | ExprKind::List(items) => items.iter().all(droppable),
        ExprKind::Record(fields) => fields.iter().all(|(_, v)| droppable(v)),
        ExprKind::Field(inner, _) | ExprKind::Not(inner) | ExprKind::Neg(_, inner) => droppable(inner),
        ExprKind::Binary(op, l, r) => total(*op) && droppable(l) && droppable(r),
        // a constructor applied builds a value, and throws nothing
        ExprKind::App { .. } => applied(e).is_some_and(|(_, args)| args.into_iter().all(droppable)),
        _ => false,
    }
}

/// `C a b` as the constructor and its arguments, in order: pure stages
/// applied left to right.
pub fn applied(e: &Expr) -> Option<(&Constructor, Vec<&Expr>)> {
    let mut args = Vec::new();
    let mut head = e;
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
    let ExprKind::Con(c) = &head.kind else {
        return None;
    };
    args.reverse();
    Some((c, args))
}

/// How many arguments the top-level function `name` of `module` takes
/// before its body runs, when a call with fewer only builds a closure.
pub type Arity<'a> = &'a dyn Fn(&str, &str) -> Option<usize>;

/// The name of a top-level function and the arguments it takes before its
/// body runs: the parameters of a function without dictionaries, or what a
/// foreign function takes at once.
pub fn arity(item: &Item) -> Option<(&str, usize)> {
    match &item.kind {
        ItemKind::Value { name, dicts, body } if dicts.is_empty() => match &body.kind {
            ExprKind::Lambda { params, .. } => Some((name, params.len())),
            _ => None,
        },
        ItemKind::Foreign(f) if f.dicts == 0 => Some((&f.name, f.taken())),
        _ => None,
    }
}

/// [`droppable`], or a value whose evaluation shows nothing but a closure:
/// a function applied to fewer arguments than it takes, a method of a
/// dictionary already built, or a block that binds such values.
pub fn inert(e: &Expr, arity: Arity) -> bool {
    match &e.kind {
        _ if droppable(e) || known_method(e) => true,
        ExprKind::App { .. } => partial(e, arity).is_some(),
        ExprKind::Block(stmts, value) => {
            let bound =
                |s: &Stmt| matches!(s, Stmt::Let(p, v) if p.irrefutable() && inert(v, arity));
            stmts.iter().all(bound) && inert(value, arity)
        }
        _ => false,
    }
}

/// A method of a dictionary already built: of an instance without a
/// context, or one the code was given. Reading it runs nothing, where an
/// instance applied to its context runs the instance's code.
pub fn known_method(e: &Expr) -> bool {
    fn built(dict: &Dict) -> bool {
        match dict {
            Dict::Instance { args, .. } => args.is_empty(),
            Dict::Given(_) | Dict::Value { .. } | Dict::Field(_) => true,
            Dict::Super { dict, .. } => built(dict),
            _ => false,
        }
    }
    matches!(&e.kind, ExprKind::Method { dict, .. } if built(dict))
}

/// `f a b` as the arguments `f` still takes after them, when `f` is a
/// top-level function that takes more than these, applied in pure stages to
/// arguments that are themselves [`inert`].
pub fn partial(e: &Expr, arity: Arity) -> Option<usize> {
    let mut args = Vec::new();
    let mut head = e;
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
    let taken = arity(module, name)?;
    let inert_args = args.iter().all(|a| inert(a, arity));
    (!args.is_empty() && args.len() < taken && inert_args).then(|| taken - args.len())
}

/// `x |> f` as `f x` when one of the two shows nothing when evaluated, so
/// the order of the two cannot be told: then it is a call as any other,
/// which inlining, fusion and workers know.
pub fn ordered(e: &mut Expr, arity: Arity) -> bool {
    if let ExprKind::App {
        func,
        arg,
        arg_first: arg_first @ true,
        ..
    } = &mut e.kind
        && (inert(arg, arity) || inert(func, arity))
    {
        *arg_first = false;
        return true;
    }
    false
}

/// Whether an operator never throws, whatever its well-typed operands: a
/// negative power of a `BigInt` does.
pub fn total(op: BinOp) -> bool {
    op != BinOp::Arith(Arith::Pow, Num::BigInt)
}

/// Gives every local `e` binds a name ending in `suffix`, and renames its
/// reads to match: a copy of a body inlined next to the body it came from
/// keeps the locals of each definition unique.
pub fn rename(e: &mut Expr, suffix: &str) {
    let mut names = HashMap::new();
    binders(e, &mut names, suffix);
    apply(e, &names);
}

fn fresh(names: &mut HashMap<String, String>, name: &str, suffix: &str) {
    names.insert(name.to_owned(), format!("{name}{suffix}"));
}

fn pat_binders(p: &Pat, names: &mut HashMap<String, String>, suffix: &str) {
    let mut bound = Vec::new();
    p.binders(&mut bound);
    for b in bound {
        fresh(names, b, suffix);
    }
}

fn binders(e: &Expr, names: &mut HashMap<String, String>, suffix: &str) {
    match &e.kind {
        ExprKind::ControlLoop { id, .. } => fresh(names, id, suffix),
        ExprKind::Lambda { params, .. } => {
            for p in params {
                pat_binders(&p.pat, names, suffix);
            }
        }
        ExprKind::Block(stmts, _) => {
            for s in stmts {
                match s {
                    Stmt::Let(p, _) => pat_binders(p, names, suffix),
                    Stmt::Var(name, _) | Stmt::Fun { name, .. } => fresh(names, name, suffix),
                    Stmt::Assign(..) | Stmt::Expr(_) => {}
                }
            }
        }
        ExprKind::Match(_, arms) => {
            for arm in arms {
                pat_binders(&arm.pat, names, suffix);
            }
        }
        ExprKind::Handle(h) => {
            for arm in &h.errors {
                pat_binders(&arm.pat, names, suffix);
            }
        }
        ExprKind::Each { pat, .. } => pat_binders(pat, names, suffix),
        _ => {}
    }
    e.each_child(&mut |c| binders(c, names, suffix));
}

fn apply(e: &mut Expr, names: &HashMap<String, String>) {
    let new = |n: &mut String| {
        if let Some(to) = names.get(n.as_str()) {
            n.clone_from(to);
        }
    };
    match &mut e.kind {
        ExprKind::ControlLoop { id, .. } | ExprKind::LoopExit { id, .. } => new(id),
        ExprKind::Local(n) | ExprKind::Var(n) | ExprKind::Within(n, _) => new(n),
        // a dictionary a matched value holds is a local of the pattern too
        ExprKind::Dicts(_, dicts) => {
            for d in dicts {
                rename_dict(d, names);
            }
        }
        ExprKind::Method { dict, .. } => rename_dict(dict, names),
        ExprKind::Lambda { params, .. } => {
            for p in params {
                apply_pat(&mut p.pat, names);
            }
        }
        ExprKind::Block(stmts, _) => {
            for s in stmts {
                match s {
                    Stmt::Let(p, _) => apply_pat(p, names),
                    Stmt::Var(name, _) | Stmt::Assign(name, _) | Stmt::Fun { name, .. } => {
                        new(name);
                    }
                    Stmt::Expr(_) => {}
                }
            }
        }
        ExprKind::Match(_, arms) => {
            for arm in arms {
                apply_pat(&mut arm.pat, names);
            }
        }
        ExprKind::Handle(h) => {
            for arm in &mut h.errors {
                apply_pat(&mut arm.pat, names);
            }
        }
        ExprKind::Each { pat, .. } => apply_pat(pat, names),
        _ => {}
    }
    e.each_child_mut(&mut |c| apply(c, names));
}

fn rename_dict(d: &mut Dict, names: &HashMap<String, String>) {
    match d {
        Dict::Field(local) => {
            if let Some(to) = names.get(local.as_str()) {
                local.clone_from(to);
            }
        }
        Dict::Instance { args: dicts, .. } | Dict::Structural { fields: dicts, .. } => {
            for d in dicts {
                rename_dict(d, names);
            }
        }
        Dict::Super { dict, .. } => rename_dict(dict, names),
        Dict::Given(_)
        | Dict::Value { .. }
        | Dict::Opened { .. }
        | Dict::Primitive { .. }
        | Dict::Site { .. }
        | Dict::Error => {}
    }
}

fn apply_pat(p: &mut Pat, names: &HashMap<String, String>) {
    match p {
        Pat::Bind(n) => {
            if let Some(to) = names.get(n.as_str()) {
                n.clone_from(to);
            }
        }
        Pat::Wild | Pat::Lit(_) => {}
        Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
            for i in items {
                apply_pat(i, names);
            }
        }
        Pat::List(items, rest) => {
            for i in items.iter_mut().chain(rest.as_deref_mut()) {
                apply_pat(i, names);
            }
        }
        Pat::Record(fields) => {
            for (_, f) in fields {
                apply_pat(f, names);
            }
        }
    }
}
