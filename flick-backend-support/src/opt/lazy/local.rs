//! O3: unsynchronized fiber-local lazy cell (`docs/formal/lazy.md` §6).
//!
//! Rewrites `let l = lazy_make t` to `let l = lazy_make_local t` when `l`
//! provably never escapes its creating fiber/scope: every reference to `l` in
//! the rest of its block is a direct `lazy_force l` (or `lazy_read l` /
//! `ignore l`) running synchronously on the creating fiber, and `l` never
//! enters a `task`, closure/lambda, local function, lazy `Seq` generator,
//! handler operation arm, record/constructor/tuple/list field, `Ref`/mutable
//! variable, return value, or non-inlined call argument.

use super::is;
use crate::Known;
use crate::ir::{Expr, ExprKind, Pat, Stmt};
use flick_ir::Lang;

/// Rewrites non-escaping `lazy_make` bindings in `e` to `lazy_make_local`.
/// Returns whether any cell was rewritten.
pub fn localize(e: &mut Expr, lang: &Known<'_>) -> bool {
    let Some((local_mod, local_name)) = lang.get(Lang::LazyMakeLocal) else {
        return false;
    };
    localize_in(e, local_mod, local_name, lang)
}

fn localize_in(e: &mut Expr, local_mod: &str, local_name: &str, lang: &Known<'_>) -> bool {
    let mut changed = false;
    if let ExprKind::Block(stmts, result) = &mut e.kind {
        for i in 0..stmts.len() {
            let (head, rest) = stmts.split_at_mut(i + 1);
            if let Stmt::Let {
                pat: Pat::Bind(name),
                value,
                ..
            } = &mut head[i]
                && is_lazy_make(value, lang)
                && value.uses(name) == 0
                && !rest.iter().any(|s| stmt_escapes(s, name, lang))
                && !escapes(result, name, lang)
            {
                rewrite_to_local(value, local_mod, local_name);
                changed = true;
            }
        }
    }
    e.each_child_mut(&mut |c| changed |= localize_in(c, local_mod, local_name, lang));
    changed
}

fn is_lazy_make(e: &Expr, lang: &Known<'_>) -> bool {
    matches!(&e.kind, ExprKind::App { func, .. } if is(func, Lang::LazyMake, lang))
}

fn rewrite_to_local(e: &mut Expr, local_mod: &str, local_name: &str) {
    let ExprKind::App { func, .. } = &mut e.kind else {
        return;
    };
    if let ExprKind::Global { module, name, .. } = &mut func.kind {
        module.clear();
        module.push_str(local_mod);
        name.clear();
        name.push_str(local_name);
    }
}

/// Whether `stmt` uses `l` in any way that might escape its creating fiber/scope.
fn stmt_escapes(stmt: &Stmt, l: &str, lang: &Known<'_>) -> bool {
    match stmt {
        Stmt::Fun { value, .. } => value.uses(l) > 0,
        Stmt::Assign(name, value) => name == l || escapes(value, l, lang),
        Stmt::Let { value, .. } | Stmt::Var { value, .. } | Stmt::Expr(value) => {
            escapes(value, l, lang)
        }
    }
}

/// Strict allowlist escape analysis for local `l`.
///
/// Returns `false` only when every occurrence of `l` in `e` is a direct
/// same-fiber `lazy_force l` (or `lazy_read l` / `ignore l`). Any bare
/// occurrence of `l` or any occurrence inside a deferred/callable boundary
/// returns `true`.
fn escapes(e: &Expr, l: &str, lang: &Known<'_>) -> bool {
    if let ExprKind::App { func, arg, .. } = &e.kind
        && (is(func, Lang::LazyForce, lang) || is(func, Lang::LazyRead, lang))
        && matches!(&arg.kind, ExprKind::Local(name) if name == l)
    {
        return false;
    }
    if let ExprKind::Ignore(inner) = &e.kind
        && matches!(&inner.kind, ExprKind::Local(name) if name == l)
    {
        return false;
    }

    match &e.kind {
        ExprKind::Local(name) | ExprKind::Var(name) | ExprKind::Within(name, _) if name == l => {
            true
        }
        ExprKind::Lambda { .. }
        | ExprKind::Task(_)
        | ExprKind::Abstract(..)
        | ExprKind::Lift(..)
        | ExprKind::Method { .. } => e.uses(l) > 0,
        ExprKind::Generate {
            strict: false,
            limit,
            body,
            ..
        } => limit.as_ref().is_some_and(|lim| escapes(lim, l, lang)) || body.uses(l) > 0,
        ExprKind::Handle(h) => {
            h.effects
                .iter()
                .any(|eff| eff.operations.iter().any(|op| op.function.uses(l) > 0))
                || h.ret.as_ref().is_some_and(|r| r.uses(l) > 0)
                || escapes(&h.body, l, lang)
                || h.errors.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|g| escapes(g, l, lang))
                        || escapes(&arm.body, l, lang)
                })
        }
        ExprKind::Dicts(inner, _) => e.uses(l) > inner.uses(l) || escapes(inner, l, lang),
        ExprKind::Block(stmts, result) => {
            stmts.iter().any(|s| stmt_escapes(s, l, lang)) || escapes(result, l, lang)
        }
        _ => {
            let mut escaped = false;
            e.each_child(&mut |c| escaped |= escapes(c, l, lang));
            escaped
        }
    }
}
