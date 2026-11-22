//! Local function reachability analysis inside `ExprKind::Block`.
//!
//! Deferred local bindings (`Stmt::Fun` and lambda `Stmt::Let`) do not execute
//! eagerly when entering a block; their bodies are reachable only if referenced
//! from an eager statement, the block's tail expression, or another reachable
//! local function within their lexical scope.

use std::collections::HashSet;

use crate::ir::{Expr, ExprKind, Item, Pat, Stmt};
use crate::source::Span;

/// If `stmt` defines a deferred local function binding whose body does not run
/// eagerly when the enclosing block executes, returns `(name, is_rec, value)`.
pub(super) fn deferred_local(stmt: &Stmt) -> Option<(&str, bool, &Expr)> {
    match stmt {
        Stmt::Fun {
            name, rec, value, ..
        } => Some((name.as_str(), *rec, value)),
        Stmt::Let {
            pat: Pat::Bind(name),
            value,
            ..
        } if matches!(value.kind, ExprKind::Lambda { .. } | ExprKind::Abstract(..)) => {
            Some((name.as_str(), false, value))
        }
        _ => None,
    }
}

/// Returns the subexpression of `stmt`.
pub(super) fn stmt_expr(stmt: &Stmt) -> &Expr {
    match stmt {
        Stmt::Let { value: v, .. }
        | Stmt::Var { value: v, .. }
        | Stmt::Assign(_, v)
        | Stmt::Expr(v)
        | Stmt::Fun { value: v, .. } => v,
    }
}

/// Returns a reachability mask parallel to `stmts`: eager statements are always
/// active, while deferred local functions (`Stmt::Fun` and lambda `Stmt::Let`)
/// become active only if referenced from an active expression in their scope.
pub(super) fn reachable_block_stmts(stmts: &[Stmt], result: &Expr) -> Vec<bool> {
    let mut active = vec![false; stmts.len()];
    if !stmts.iter().any(|s| deferred_local(s).is_some()) {
        active.fill(true);
        return active;
    }

    let mut stmt_uses: Vec<Option<HashSet<String>>> = vec![None; stmts.len()];
    for (idx, stmt) in stmts.iter().enumerate() {
        if deferred_local(stmt).is_none() {
            active[idx] = true;
            let mut uses = HashSet::new();
            collect_stmt_free_locals(stmt, &mut uses);
            stmt_uses[idx] = Some(uses);
        }
    }
    let mut result_uses = HashSet::new();
    collect_free_locals(result, &mut HashSet::new(), &mut result_uses);

    let shadow_end: Vec<usize> = (0..stmts.len())
        .map(|i| {
            let Some((name, _, _)) = deferred_local(&stmts[i]) else {
                return stmts.len();
            };
            ((i + 1)..stmts.len())
                .find(|&j| stmt_binds_name(&stmts[j], name))
                .unwrap_or(stmts.len())
        })
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..stmts.len() {
            if active[i] {
                continue;
            }
            let Some((name, rec, value)) = deferred_local(&stmts[i]) else {
                continue;
            };
            let end = shadow_end[i];
            let used_by_later = ((i + 1)..end)
                .any(|j| active[j] && stmt_uses[j].as_ref().is_some_and(|u| u.contains(name)));
            let used_by_result = end == stmts.len() && result_uses.contains(name);
            if used_by_later || used_by_result {
                active[i] = true;
                let mut bound = HashSet::new();
                if rec {
                    bound.insert(name.to_owned());
                }
                let mut uses = HashSet::new();
                collect_free_locals(value, &mut bound, &mut uses);
                stmt_uses[i] = Some(uses);
                changed = true;
            }
        }
    }
    active
}

/// Spans of unreachable deferred local function bodies inside `item`.
pub(super) fn dead_local_fun_spans(item: &Item) -> Vec<Span> {
    fn walk(expr: &Expr, out: &mut Vec<Span>) {
        if let ExprKind::Block(stmts, result) = &expr.kind {
            let active = reachable_block_stmts(stmts, result);
            for (stmt, is_active) in stmts.iter().zip(active) {
                let child = stmt_expr(stmt);
                if is_active {
                    walk(child, out);
                } else {
                    out.push(child.span);
                }
            }
            walk(result, out);
            return;
        }
        expr.each_child(&mut |child| walk(child, out));
    }
    let mut out = Vec::new();
    item.each_body(&mut |expr| walk(expr, &mut out));
    out
}

fn stmt_binds_name(stmt: &Stmt, name: &str) -> bool {
    match stmt {
        Stmt::Let { pat, .. } => pat_binds_name(pat, name),
        Stmt::Var { name: bound, .. } | Stmt::Fun { name: bound, .. } => bound == name,
        Stmt::Assign(..) | Stmt::Expr(_) => false,
    }
}

fn pat_binds_name(pat: &Pat, name: &str) -> bool {
    match pat {
        Pat::Bind(bound) => bound == name,
        Pat::Tuple(items) | Pat::Or(items) | Pat::Con(_, items) => {
            items.iter().any(|p| pat_binds_name(p, name))
        }
        Pat::List(items, rest) => items
            .iter()
            .chain(rest.as_deref())
            .any(|p| pat_binds_name(p, name)),
        Pat::Record(fields) => fields.iter().any(|(_, p)| pat_binds_name(p, name)),
        Pat::Wild | Pat::Lit(_) => false,
    }
}

fn collect_pat_bindings(pat: &Pat, out: &mut Vec<String>) {
    match pat {
        Pat::Bind(name) => out.push(name.clone()),
        Pat::Tuple(items) | Pat::Or(items) | Pat::Con(_, items) => {
            for p in items {
                collect_pat_bindings(p, out);
            }
        }
        Pat::List(items, rest) => {
            for p in items.iter().chain(rest.as_deref()) {
                collect_pat_bindings(p, out);
            }
        }
        Pat::Record(fields) => {
            for (_, p) in fields {
                collect_pat_bindings(p, out);
            }
        }
        Pat::Wild | Pat::Lit(_) => {}
    }
}

fn collect_stmt_free_locals(stmt: &Stmt, used: &mut HashSet<String>) {
    if let Stmt::Assign(name, _) = stmt {
        used.insert(name.clone());
    }
    collect_free_locals(stmt_expr(stmt), &mut HashSet::new(), used);
}

fn with_pat_scope(pat: &Pat, bound: &mut HashSet<String>, f: impl FnOnce(&mut HashSet<String>)) {
    let mut added = Vec::new();
    collect_pat_bindings(pat, &mut added);
    let newly: Vec<String> = added
        .into_iter()
        .filter(|n| bound.insert(n.clone()))
        .collect();
    f(bound);
    for n in newly {
        bound.remove(&n);
    }
}

fn collect_free_locals(expr: &Expr, bound: &mut HashSet<String>, used: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Local(name) | ExprKind::Var(name) => {
            if !bound.contains(name) {
                used.insert(name.clone());
            }
        }
        ExprKind::Within(name, inner) => {
            if !bound.contains(name) {
                used.insert(name.clone());
            }
            collect_free_locals(inner, bound, used);
        }
        ExprKind::Lambda { params, body } => {
            let mut added = Vec::new();
            for p in params {
                collect_pat_bindings(&p.pat, &mut added);
            }
            let newly: Vec<String> = added
                .into_iter()
                .filter(|n| bound.insert(n.clone()))
                .collect();
            collect_free_locals(body, bound, used);
            for n in newly {
                bound.remove(&n);
            }
        }
        ExprKind::Match(scrutinee, arms) => {
            collect_free_locals(scrutinee, bound, used);
            for arm in arms {
                with_pat_scope(&arm.pat, bound, |bound| {
                    if let Some(guard) = &arm.guard {
                        collect_free_locals(guard, bound, used);
                    }
                    collect_free_locals(&arm.body, bound, used);
                });
            }
        }
        ExprKind::Each {
            stop,
            pat,
            source,
            body,
            ..
        } => {
            if let Some(stop) = stop {
                collect_free_locals(stop, bound, used);
            }
            collect_free_locals(source, bound, used);
            with_pat_scope(pat, bound, |bound| collect_free_locals(body, bound, used));
        }
        ExprKind::Handle(handler) => {
            for handled in &handler.effects {
                for arm in &handled.operations {
                    collect_free_locals(&arm.function, bound, used);
                }
            }
            collect_free_locals(&handler.body, bound, used);
            if let Some(ret) = &handler.ret {
                collect_free_locals(ret, bound, used);
            }
            for arm in &handler.errors {
                with_pat_scope(&arm.pat, bound, |bound| {
                    if let Some(guard) = &arm.guard {
                        collect_free_locals(guard, bound, used);
                    }
                    collect_free_locals(&arm.body, bound, used);
                });
            }
        }
        ExprKind::Block(stmts, result) => {
            collect_block_free_locals(stmts, result, bound, used);
        }
        _ => {
            expr.each_child(&mut |child| collect_free_locals(child, bound, used));
        }
    }
}

fn collect_block_free_locals(
    stmts: &[Stmt],
    result: &Expr,
    bound: &mut HashSet<String>,
    used: &mut HashSet<String>,
) {
    let active = reachable_block_stmts(stmts, result);
    let mut newly = Vec::new();
    for (stmt, is_active) in stmts.iter().zip(active) {
        match stmt {
            Stmt::Fun {
                name, rec, value, ..
            } => {
                if *rec && bound.insert(name.clone()) {
                    newly.push(name.clone());
                }
                if is_active {
                    collect_free_locals(value, bound, used);
                }
                if !*rec && bound.insert(name.clone()) {
                    newly.push(name.clone());
                }
            }
            Stmt::Let { pat, value, .. } => {
                if is_active {
                    collect_free_locals(value, bound, used);
                }
                let mut added = Vec::new();
                collect_pat_bindings(pat, &mut added);
                for n in added {
                    if bound.insert(n.clone()) {
                        newly.push(n);
                    }
                }
            }
            Stmt::Var { name, value, .. } => {
                if is_active {
                    collect_free_locals(value, bound, used);
                }
                if bound.insert(name.clone()) {
                    newly.push(name.clone());
                }
            }
            Stmt::Assign(name, value) => {
                if is_active {
                    if !bound.contains(name) {
                        used.insert(name.clone());
                    }
                    collect_free_locals(value, bound, used);
                }
            }
            Stmt::Expr(value) => {
                if is_active {
                    collect_free_locals(value, bound, used);
                }
            }
        }
    }
    collect_free_locals(result, bound, used);
    for n in newly {
        bound.remove(&n);
    }
}
