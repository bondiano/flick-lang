//! Legacy bare-constructor type projection for backends (currently Go) that
//! still consume only IR 0.13 bare scalar annotations while layout-aware
//! backends consume full IR 0.14 types.

use flick_ir::{Expr, ExprKind, IrType, ItemKind, Module, Pat, Stmt, TypeRef};

use super::Generic;

fn keep_bare(ty: &mut Option<TypeRef>) {
    if ty.as_deref().and_then(IrType::as_bare_con).is_none() {
        *ty = None;
    }
}

fn is_owner_entry_block(stmts: &[Stmt]) -> bool {
    matches!(
        stmts,
        [Stmt::Let {
            pat: Pat::Bind(_),
            value: Expr {
                kind: ExprKind::ReadOwnerEvidence,
                ..
            },
            ..
        }]
    )
}

/// Projects `module` onto the legacy bare-constructor `ty` subset.
pub fn strip_legacy_bare_types(module: &mut Module) {
    module.types.clear();
    for item in &mut module.items {
        match &mut item.kind {
            ItemKind::Value {
                type_params,
                dict_constraints,
                ty,
                body,
                ..
            } => {
                type_params.clear();
                dict_constraints.clear();
                *ty = None;
                strip_expr(body);
            }
            ItemKind::Destructure {
                pattern,
                binder_types,
                body,
            } => {
                strip_pat(pattern);
                binder_types.clear();
                strip_expr(body);
            }
            ItemKind::Instance { methods, .. } => {
                for (_, body) in methods {
                    strip_expr(body);
                }
            }
            ItemKind::Foreign(f) => {
                f.type_params.clear();
                f.ty = None;
                if let Some(catch) = &mut f.catch {
                    strip_expr(catch);
                }
            }
            ItemKind::Constructor(c) => {
                c.fields.clear();
                c.type_args.clear();
            }
            ItemKind::Service { .. } => {}
        }
    }
}

/// Projects a cross-module `Generic` onto the legacy bare-constructor subset.
pub fn strip_legacy_bare_generic(generic: &mut Generic) {
    generic.type_params.clear();
    generic.dict_constraints.clear();
    strip_expr(&mut generic.body);
}

fn strip_expr(e: &mut Expr) {
    keep_bare(&mut e.ty);
    match &mut e.kind {
        ExprKind::Global { type_args, .. } | ExprKind::Method { type_args, .. } => {
            type_args.clear();
        }
        ExprKind::Con(c) => {
            c.fields.clear();
            c.type_args.clear();
        }
        ExprKind::Lambda { params, body } => {
            for p in params {
                keep_bare(&mut p.ty);
                p.binder_types.clear();
                strip_pat(&mut p.pat);
            }
            strip_expr(body);
        }
        ExprKind::Block(stmts, result) => {
            if is_owner_entry_block(stmts) {
                e.ty = None;
            }
            for stmt in stmts {
                match stmt {
                    Stmt::Let {
                        pat,
                        binder_types,
                        value,
                    } => {
                        strip_pat(pat);
                        binder_types.clear();
                        strip_expr(value);
                    }
                    Stmt::Var { ty, value, .. } => {
                        *ty = None;
                        strip_expr(value);
                    }
                    Stmt::Assign(_, value) | Stmt::Expr(value) => strip_expr(value),
                    Stmt::Fun {
                        type_params,
                        dict_constraints,
                        ty,
                        value,
                        ..
                    } => {
                        type_params.clear();
                        dict_constraints.clear();
                        *ty = None;
                        strip_expr(value);
                    }
                }
            }
            strip_expr(result);
        }
        ExprKind::Match(scrutinee, arms) => {
            strip_expr(scrutinee);
            for arm in arms {
                strip_pat(&mut arm.pat);
                arm.binder_types.clear();
                if let Some(guard) = &mut arm.guard {
                    strip_expr(guard);
                }
                strip_expr(&mut arm.body);
            }
        }
        ExprKind::Handle(h) => {
            for handled in &mut h.effects {
                for op in &mut handled.operations {
                    strip_expr(&mut op.function);
                }
            }
            strip_expr(&mut h.body);
            if let Some(ret) = &mut h.ret {
                strip_expr(ret);
            }
            for arm in &mut h.errors {
                strip_pat(&mut arm.pat);
                arm.binder_types.clear();
                if let Some(guard) = &mut arm.guard {
                    strip_expr(guard);
                }
                strip_expr(&mut arm.body);
            }
        }
        ExprKind::Each {
            stop,
            pat,
            binder_types,
            source,
            body,
        } => {
            if let Some(stop) = stop {
                strip_expr(stop);
            }
            strip_pat(pat);
            binder_types.clear();
            strip_expr(source);
            strip_expr(body);
        }
        _ => e.each_child_mut(&mut strip_expr),
    }
}

fn strip_pat(pat: &mut Pat) {
    match pat {
        Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
        Pat::Tuple(items) | Pat::Or(items) => {
            for p in items {
                strip_pat(p);
            }
        }
        Pat::List(items, rest) => {
            for p in items.iter_mut().chain(rest.as_deref_mut()) {
                strip_pat(p);
            }
        }
        Pat::Record(fields) => {
            for (_, p) in fields {
                strip_pat(p);
            }
        }
        Pat::Con(c, items) => {
            c.fields.clear();
            c.type_args.clear();
            for p in items {
                strip_pat(p);
            }
        }
    }
}
