//! A `match` whose scrutinee is known without being run: a local bound to
//! data a few lines above, or a choice (an `if`, a `match`, a block) every
//! branch of which ends in data. The `match` goes to where the data is, and
//! [`devirt::known_match`] then takes the arm each one chooses, so a value
//! built only to be taken apart is never built: `Option.map f (Option.filter
//! p value)`, once both are inlined, makes no `Some` between them.
//!
//! [`devirt::known_match`]: super::devirt::known_match

use std::collections::HashMap;
use std::mem;

use super::devirt::{built, matches};
use super::walk::{applied, rename, size};
use super::{atom, place};
use crate::ir::{Arm, Expr, ExprKind, Lit, Pat, Stage, Stmt};
use flick_ir::{Span, TypeRef};

/// The size of an arm that may be copied to more than one branch.
const SMALL: usize = 8;

/// `let x = C a b` before a `match x with …` of the same block: the `match`
/// reads the data, whose parts are names and literals, and `x` stays for
/// whatever else reads it. Whether a `match` was given its data.
pub fn scrutinees(stmts: &mut [Stmt], result: &mut Expr) -> bool {
    let mut changed = false;
    for i in 0..stmts.len() {
        let Stmt::Let {
            pat: Pat::Bind(name),
            value,
            ..
        } = &stmts[i]
        else {
            continue;
        };
        if !built(value) {
            continue;
        }
        let (name, value) = (name.clone(), value.clone());
        for later in &mut stmts[i + 1..] {
            match later {
                Stmt::Let { value: v, .. }
                | Stmt::Var { value: v, .. }
                | Stmt::Assign(_, v)
                | Stmt::Expr(v) => {
                    changed |= given(v, &name, &value);
                }
                Stmt::Fun { value: v, .. } => changed |= given(v, &name, &value),
            }
        }
        changed |= given(result, &name, &value);
    }
    changed
}

/// `match n with | m when … -> …`: an arm that only names what is taken
/// apart, itself a name or a literal, reads it as it is written there. No
/// second name stands for the same value. Whether an arm lost its name.
pub fn named_again(e: &mut Expr) -> bool {
    let ExprKind::Match(scrutinee, arms) = &mut e.kind else {
        return false;
    };
    if !atom(scrutinee) {
        return false;
    }
    let mut changed = false;
    for arm in arms {
        let Pat::Bind(name) = &arm.pat else {
            continue;
        };
        let mut reads = HashMap::from([(name.clone(), (**scrutinee).clone())]);
        if let Some(guard) = &mut arm.guard {
            place(guard, &mut reads);
        }
        place(&mut arm.body, &mut reads);
        if arm.body.uses(name) == 0 && arm.guard.as_ref().is_none_or(|guard| guard.uses(name) == 0)
        {
            arm.pat = Pat::Wild;
            arm.binder_types.clear();
            changed = true;
        }
    }
    changed
}

/// Gives every `match name with …` in `e` the data `name` is bound to.
fn given(e: &mut Expr, name: &str, value: &Expr) -> bool {
    let mut changed = false;
    if let ExprKind::Match(scrutinee, _) = &mut e.kind
        && matches!(&scrutinee.kind, ExprKind::Local(l) if l == name)
    {
        **scrutinee = value.clone();
        changed = true;
    }
    e.each_child_mut(&mut |c| changed |= given(c, name, value));
    changed
}

/// Whether `e` is a choice, by an `if`, a `match` or a block, every branch
/// of which ends in data.
pub fn chooses(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::If(..) | ExprKind::Match(..) | ExprKind::Block(..)
    ) && ends_in_data(e)
}

/// Whether every branch of `tree` ends in data: a `match` of it would move
/// into its branches.
pub fn ends_in_data(tree: &Expr) -> bool {
    each_end(tree, &mut Vec::new(), &mut |_| Some(())).is_some()
}

/// Moves every `match` of `e` whose scrutinee is a choice between data into
/// the branches of the choice; whether one moved.
pub fn branches(e: &mut Expr, fresh: &mut usize) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= branches(c, fresh));
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        changed |= bound_choices(stmts, result, fresh);
    }
    changed | into_branches(e, fresh)
}

/// `match tree with arms`, where `tree` is a choice between data: the
/// `match` moves into its branches. Whether it moved.
fn into_branches(e: &mut Expr, fresh: &mut usize) -> bool {
    let ExprKind::Match(scrutinee, arms) = &e.kind else {
        return false;
    };
    let branching = matches!(
        scrutinee.kind,
        ExprKind::If(..) | ExprKind::Match(..) | ExprKind::Block(..)
    );
    if !branching || !movable(scrutinee, arms) {
        return false;
    }
    let result_ty = e.ty.clone();
    let ExprKind::Match(mut scrutinee, arms) = mem::replace(&mut e.kind, ExprKind::Lit(Lit::Unit))
    else {
        unreachable!("a match, as tested above")
    };
    moved(&mut scrutinee, &arms, None, result_ty.as_ref(), fresh);
    *e = *scrutinee;
    true
}

/// `let x = choice` right before the one `match x with …` that reads `x`,
/// in its scrutinee and maybe in its arms: the `match` moves into the
/// branches of the choice, which is evaluated where it was. Whether one did.
fn bound_choices(stmts: &mut Vec<Stmt>, result: &mut Expr, fresh: &mut usize) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if !next_takes_apart(stmts, i, result) {
            i += 1;
            continue;
        }
        let Stmt::Let {
            pat: Pat::Bind(name),
            binder_types,
            value: mut choice,
        } = stmts.remove(i)
        else {
            unreachable!("a `let` of a name, as tested")
        };
        let bound_ty = binder_types
            .into_iter()
            .find(|(n, _)| n == &name)
            .map(|(_, t)| t)
            .or_else(|| choice.ty.clone());
        let next = match stmts.get_mut(i) {
            Some(Stmt::Let { value: v, .. } | Stmt::Expr(v)) => v,
            _ => &mut *result,
        };
        let result_ty = next.ty.clone();
        let ExprKind::Match(_, arms) = &next.kind else {
            unreachable!("a `match`, as tested")
        };
        moved(
            &mut choice,
            arms,
            Some((&name, bound_ty.as_ref())),
            result_ty.as_ref(),
            fresh,
        );
        *next = choice;
        changed = true;
    }
    changed
}

/// Whether statement `i` binds a local to a choice between data that the
/// next statement, or the result after the last one, takes apart, and
/// nothing after that reads.
fn next_takes_apart(stmts: &[Stmt], i: usize, result: &Expr) -> bool {
    let Stmt::Let {
        pat: Pat::Bind(name),
        value: choice,
        ..
    } = &stmts[i]
    else {
        return false;
    };
    let next = match stmts.get(i + 1) {
        Some(Stmt::Let { value: v, .. } | Stmt::Expr(v)) => v,
        Some(_) => return false,
        None => result,
    };
    let ExprKind::Match(scrutinee, arms) = &next.kind else {
        return false;
    };
    if !matches!(&scrutinee.kind, ExprKind::Local(l) if l == name) {
        return false;
    }
    let read = |s: &Stmt| match s {
        Stmt::Let { value: v, .. }
        | Stmt::Var { value: v, .. }
        | Stmt::Assign(_, v)
        | Stmt::Expr(v) => v.uses(name),
        Stmt::Fun { value, .. } => value.uses(name),
    };
    let after: usize = stmts
        .get(i + 2..)
        .unwrap_or_default()
        .iter()
        .map(read)
        .sum();
    let last = if i + 1 < stmts.len() {
        result.uses(name)
    } else {
        0
    };
    after + last == 0 && chooses(choice) && movable(choice, arms)
}

/// Whether `match tree with arms` can move into the branches of `tree`:
/// every branch ends in data that chooses its arm, no arm has a guard, and
/// an arm more than one branch chooses, which is copied, is small.
fn movable(tree: &Expr, arms: &[Arm]) -> bool {
    if arms.iter().any(|arm| arm.guard.is_some()) {
        return false;
    }
    let mut chosen = vec![0_usize; arms.len()];
    let known = each_end(tree, &mut Vec::new(), &mut |end| {
        chosen[arm_of(arms, end)?] += 1;
        Some(())
    });
    let copied = |(arm, &times): (&Arm, &usize)| times > 1 && size(&arm.body) > SMALL;
    known.is_some() && !arms.iter().zip(&chosen).any(copied)
}

/// Moves `match … with arms` into the branches of `tree`, which
/// [`movable`] accepted: each branch ends in a `match` of its data. With
/// `bound`, the arms read the value taken apart by that name, and each
/// branch binds it to what it ends in.
fn moved(
    tree: &mut Expr,
    arms: &[Arm],
    bound: Option<(&str, Option<&TypeRef>)>,
    result_ty: Option<&TypeRef>,
    fresh: &mut usize,
) {
    let span = tree.span;
    let mut first = true;
    each_end_mut(tree, result_ty, &mut Vec::new(), &mut |end, mut data| {
        // the parts are evaluated where they were, whatever the arm does
        // with them: each one that is not a name gets one
        let mut stmts = Vec::new();
        name_parts(&mut data, fresh, &mut stmts);
        if let Some((name, bound_ty)) = bound {
            // a name the branch ends in is the value; data is built again
            // only if an arm reads the whole of it
            let whole = match &end.kind {
                ExprKind::Local(_) => end.clone(),
                _ => data.clone(),
            };
            let bt = bound_ty
                .cloned()
                .or_else(|| whole.ty.clone())
                .map(|t| vec![(name.to_owned(), t)])
                .unwrap_or_default();
            stmts.push(Stmt::let_typed(Pat::Bind(name.to_owned()), bt, whole));
        }
        let taken = Expr::with_ty(
            ExprKind::Match(Box::new(data), arms.to_vec()),
            span,
            result_ty.cloned(),
        );
        let mut taken = if stmts.is_empty() {
            taken
        } else {
            Expr::with_ty(
                ExprKind::Block(stmts, Box::new(taken)),
                span,
                result_ty.cloned(),
            )
        };
        // a local is bound once in a definition: every copy but one has
        // names of its own
        if !mem::take(&mut first) {
            *fresh += 1;
            rename(&mut taken, &format!("$k{fresh}"));
        }
        *end = taken;
    });
}

/// Binds each part of the construction `data` that is not a name or a
/// literal to a local of its own, in the order the parts are written.
fn name_parts(data: &mut Expr, fresh: &mut usize, parts: &mut Vec<Stmt>) {
    let ExprKind::App { func, arg, .. } = &mut data.kind else {
        return;
    };
    name_parts(func, fresh, parts);
    if matches!(
        arg.kind,
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Con(_)
    ) {
        return;
    }
    *fresh += 1;
    let name = format!("$p{fresh}");
    let ty = arg.ty.clone();
    let local = Expr::with_ty(ExprKind::Local(name.clone()), arg.span, ty.clone());
    let bt = ty.map(|t| vec![(name.clone(), t)]).unwrap_or_default();
    parts.push(Stmt::let_typed(
        Pat::Bind(name),
        bt,
        mem::replace(&mut **arg, local),
    ));
}

/// The arm of `arms` the data `end` chooses, when that is known here.
fn arm_of(arms: &[Arm], end: &Expr) -> Option<usize> {
    for (i, arm) in arms.iter().enumerate() {
        if matches(&arm.pat, end, &mut Vec::new())? {
            return Some(i);
        }
    }
    None
}

/// What a local is known to be in the branch being walked: the pattern an
/// enclosing `match` of it took, as the data the pattern describes.
type Known = Vec<(String, Expr)>;

/// The data a pattern of names describes: `Some x` as the pattern, `Some x`
/// as the value. A pattern that ignores a part describes no one value.
fn described(
    pat: &Pat,
    binder_types: &[(String, TypeRef)],
    ty: Option<&TypeRef>,
    span: Span,
) -> Option<Expr> {
    match pat {
        Pat::Bind(name) => {
            let local_ty = binder_types
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, t)| t.clone())
                .or_else(|| ty.cloned());
            Some(Expr::with_ty(ExprKind::Local(name.clone()), span, local_ty))
        }
        Pat::Con(c, parts) if c.dicts == 0 => {
            let mut data = Expr::new(ExprKind::Con(c.clone()), span);
            for (idx, part) in parts.iter().enumerate() {
                let field_ty = c.fields.get(idx);
                let app = ExprKind::App {
                    func: Box::new(data),
                    arg: Box::new(described(part, binder_types, field_ty, span)?),
                    stage: Stage::Pure,
                    arg_first: false,
                };
                data = Expr::new(app, span);
            }
            data.ty = ty.cloned();
            Some(data)
        }
        _ => None,
    }
}

/// What a `match` of the local of `scrutinee` finds it to be in `arm`.
fn found(scrutinee: &Expr, arm: &Arm) -> Option<(String, Expr)> {
    let ExprKind::Local(name) = &scrutinee.kind else {
        return None;
    };
    let data = described(
        &arm.pat,
        &arm.binder_types,
        scrutinee.ty.as_ref(),
        scrutinee.span,
    )?;
    built(&data).then(|| (name.clone(), data))
}

/// What the branch end `end` is: a construction, or what the `match`
/// around it found its local to be.
fn resolved(end: &Expr, known: &Known) -> Option<Expr> {
    if applied(end).is_some() {
        return Some(end.clone());
    }
    let ExprKind::Local(name) = &end.kind else {
        return None;
    };
    let found = known.iter().rev().find(|(local, _)| local == name)?;
    Some(found.1.clone())
}

/// Calls `f` with the data each branch of `tree` ends in; `None` when a
/// branch ends in something else, or `f` gives up.
fn each_end(tree: &Expr, known: &mut Known, f: &mut dyn FnMut(&Expr) -> Option<()>) -> Option<()> {
    match &tree.kind {
        ExprKind::Block(_, tail) => each_end(tail, known, f),
        ExprKind::If(_, then, other) => {
            each_end(then, known, f)?;
            each_end(other, known, f)
        }
        ExprKind::Match(scrutinee, arms) => {
            for arm in arms {
                let mark = known.len();
                known.extend(found(scrutinee, arm));
                let ended = each_end(&arm.body, known, f);
                known.truncate(mark);
                ended?;
            }
            Some(())
        }
        _ => f(&resolved(tree, known)?),
    }
}

/// As [`each_end`], on a tree it has accepted: gives `f` each end to
/// replace, with the data it is known to be, and updates enclosing branch
/// result types to `result_ty`.
fn each_end_mut(
    tree: &mut Expr,
    result_ty: Option<&TypeRef>,
    known: &mut Known,
    f: &mut dyn FnMut(&mut Expr, Expr),
) {
    match &mut tree.kind {
        ExprKind::Block(_, tail) => {
            tree.ty = result_ty.cloned();
            each_end_mut(tail, result_ty, known, f);
        }
        ExprKind::If(_, then, other) => {
            tree.ty = result_ty.cloned();
            each_end_mut(then, result_ty, known, f);
            each_end_mut(other, result_ty, known, f);
        }
        ExprKind::Match(scrutinee, arms) => {
            tree.ty = result_ty.cloned();
            for arm in arms {
                let mark = known.len();
                known.extend(found(scrutinee, arm));
                each_end_mut(&mut arm.body, result_ty, known, f);
                known.truncate(mark);
            }
        }
        _ => {
            if let Some(data) = resolved(tree, known) {
                f(tree, data);
            }
        }
    }
}
