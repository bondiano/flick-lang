//! Expression lowering core (`LowerCtx`, `LowerMode`, control flow, blocks,
//! lambdas, and `Box`/`Unbox` coercions).

mod calls;
mod data;
mod pat;

use std::collections::BTreeMap;

use flick_ir::{Arm, Dict, Expr, ExprKind, Param, Pat, Span, Stmt, TypeRef};

use super::SpecializeError;
use super::engine::Specializer;
use super::ir::{SpecArm, SpecExpr, SpecExprKind, SpecParam, SpecStmt};
use super::type_util::{
    has_free_type_vars, is_concrete_layout_type, is_local_polymorphic_recursion, subst_type,
};
use super::types::ValueRep;

/// Active representation mode while lowering an expression body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LowerMode {
    /// Concrete monomorphic types, layout offsets, zero unnecessary boxing.
    Specialized,
    /// Uniform boxed cells, by-label record lookups.
    Fallback,
}

/// Per-definition lowering environment.
pub(crate) struct LowerCtx<'a, 'p> {
    pub spec: &'a mut Specializer<'p>,
    pub mode: LowerMode,
    pub type_subst: BTreeMap<String, TypeRef>,
    /// Static dictionary substitutions for enclosing `Dict::Given` params.
    pub dict_env: BTreeMap<String, Dict>,
    /// Representation of locals in scope (`Unboxed(ty)` or `Boxed`).
    pub local_reps: BTreeMap<String, ValueRep>,
    /// Local functions (`Stmt::Fun`) forced into boxed fallback.
    pub boxed_local_funs: BTreeMap<String, bool>,
}

impl<'a, 'p> LowerCtx<'a, 'p> {
    pub(crate) fn new(
        spec: &'a mut Specializer<'p>,
        mode: LowerMode,
        type_subst: BTreeMap<String, TypeRef>,
        dict_env: BTreeMap<String, Dict>,
    ) -> Self {
        LowerCtx {
            spec,
            mode,
            type_subst,
            dict_env,
            local_reps: BTreeMap::new(),
            boxed_local_funs: BTreeMap::new(),
        }
    }

    pub(crate) fn subst_opt(&mut self, ty: Option<&TypeRef>) -> Option<TypeRef> {
        ty.map(|t| subst_type(t, &self.type_subst, &mut self.spec.interner))
    }

    pub(crate) fn rep_for_ty(&mut self, ty: Option<&TypeRef>) -> ValueRep {
        if self.mode == LowerMode::Fallback {
            return ValueRep::Boxed;
        }
        match ty {
            Some(t) if is_concrete_layout_type(t, &self.spec.index) => {
                let _ = self.spec.ensure_layout(t);
                ValueRep::Unboxed(t.clone())
            }
            _ => ValueRep::Boxed,
        }
    }

    /// Lowers `expr` and coerces its result to `expected`.
    pub(crate) fn lower_as(
        &mut self,
        expr: &Expr,
        expected: &ValueRep,
    ) -> Result<SpecExpr, SpecializeError> {
        let lowered = self.lower_expr(expr)?;
        Ok(coerce(lowered, expected, expr.span))
    }

    /// Lowers `expr` in the current [`LowerMode`].
    #[allow(clippy::too_many_lines)]
    pub(crate) fn lower_expr(&mut self, expr: &Expr) -> Result<SpecExpr, SpecializeError> {
        let span = expr.span;
        let ty = self.subst_opt(expr.ty.as_ref());

        if matches!(expr.kind, ExprKind::App { .. }) {
            return self.lower_app(expr, ty.as_ref());
        }
        if let ExprKind::Dicts(inner, dicts) = &expr.kind {
            return self.lower_dicts_expr(inner, dicts, span, ty);
        }

        match &expr.kind {
            ExprKind::Lit(lit) => {
                let raw = SpecExpr::unboxed(SpecExprKind::Lit(lit.clone()), span, ty.clone());
                if self.mode == LowerMode::Fallback {
                    Ok(SpecExpr::boxed(
                        SpecExprKind::Box {
                            expr: Box::new(raw),
                            from_ty: ty,
                        },
                        span,
                        None,
                    ))
                } else {
                    Ok(raw)
                }
            }
            ExprKind::Local(name) => {
                let rep = self
                    .local_reps
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| self.rep_for_ty(ty.as_ref()));
                Ok(SpecExpr {
                    kind: SpecExprKind::Local(name.clone()),
                    span,
                    ty,
                    rep,
                })
            }
            ExprKind::Var(name) => {
                let rep = self
                    .local_reps
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| self.rep_for_ty(ty.as_ref()));
                Ok(SpecExpr {
                    kind: SpecExprKind::Var(name.clone()),
                    span,
                    ty,
                    rep,
                })
            }
            ExprKind::Global {
                module,
                name,
                type_args,
            } => self.lower_global_ref(module, name, type_args, &[], span, ty),
            ExprKind::Method {
                dict,
                name,
                type_args,
            } => self.lower_method_ref(dict, name, type_args, &[], span, ty),
            ExprKind::Con(ctor) => self.lower_con_ref(ctor, &[], span, ty),
            ExprKind::Perform { effect, op, stages } => {
                let rep = self.rep_for_ty(ty.as_ref());
                Ok(SpecExpr {
                    kind: SpecExprKind::Perform {
                        effect: effect.clone(),
                        op: op.clone(),
                        stages: stages.clone(),
                        dicts: Vec::new(),
                    },
                    span,
                    ty,
                    rep,
                })
            }
            ExprKind::ServiceTag(key) => {
                let rep = self.rep_for_ty(ty.as_ref());
                Ok(SpecExpr {
                    kind: SpecExprKind::ServiceTag(key.clone()),
                    span,
                    ty,
                    rep,
                })
            }
            ExprKind::Label(lbl) => {
                let rep = self.rep_for_ty(ty.as_ref());
                Ok(SpecExpr {
                    kind: SpecExprKind::Label(lbl.clone()),
                    span,
                    ty,
                    rep,
                })
            }
            ExprKind::Abstract(names, inner) => {
                let mut saved = Vec::new();
                for n in names {
                    saved.push((n.clone(), self.dict_env.remove(n)));
                }
                let inner_expr = self.lower_expr(inner)?;
                for (n, prev) in saved {
                    if let Some(d) = prev {
                        self.dict_env.insert(n, d);
                    }
                }
                let rep = inner_expr.rep.clone();
                Ok(SpecExpr {
                    kind: SpecExprKind::Abstract(names.clone(), Box::new(inner_expr)),
                    span,
                    ty,
                    rep,
                })
            }
            ExprKind::Lambda { params, body } => self.lower_lambda(params, body, span, ty),
            ExprKind::Block(stmts, tail) => self.lower_block(stmts, tail, span, ty),
            ExprKind::If(cond, then_e, else_e) => self.lower_if(cond, then_e, else_e, span, ty),
            ExprKind::Match(scrut, arms) => self.lower_match(scrut, arms, span, ty),
            ExprKind::Tuple(items) => self.lower_tuple(items, span, ty),
            ExprKind::List(items) => self.lower_list(items, span, ty),
            ExprKind::Record(fields) => self.lower_record(fields, span, ty),
            ExprKind::Dictionary(fields) => self.lower_dictionary(fields, span, ty),
            ExprKind::Update(rec, fields) => self.lower_update(rec, fields, span, ty),
            ExprKind::Field(rec, label) => self.lower_field(rec, label, span, ty),
            ExprKind::Binary(op, lhs, rhs) => self.lower_binary(*op, lhs, rhs, span, ty),
            ExprKind::Handle(h) => self.lower_handle(h, span, ty),
            _ => self.lower_misc(expr, span, ty),
        }
    }

    fn lower_lambda(
        &mut self,
        params: &[Param],
        body: &Expr,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let mut spec_params = Vec::with_capacity(params.len());
        let mut saved_all = Vec::new();
        for p in params {
            let p_ty = self.subst_opt(p.ty.as_ref());
            let p_rep = self.rep_for_ty(p_ty.as_ref());
            let (spec_pat, binder_reps, saved) =
                self.lower_pat_and_bind(&p.pat, &p.binder_types, p_ty.as_ref())?;
            saved_all.extend(saved);
            spec_params.push(SpecParam {
                pat: spec_pat,
                stage: p.stage,
                rep: p_rep,
                binder_reps,
            });
        }
        let lowered_body = self.lower_expr(body)?;
        self.restore_locals(saved_all);
        let rep = self.rep_for_ty(ty.as_ref());
        Ok(SpecExpr {
            kind: SpecExprKind::Lambda {
                params: spec_params,
                body: Box::new(lowered_body),
            },
            span,
            ty,
            rep,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn lower_block(
        &mut self,
        stmts: &[Stmt],
        tail: &Expr,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let mut spec_stmts = Vec::with_capacity(stmts.len());
        let mut saved_locals = Vec::new();
        let mut saved_funs = Vec::new();

        for stmt in stmts {
            match stmt {
                Stmt::Let {
                    pat,
                    binder_types,
                    value,
                } => {
                    let val = self.lower_expr(value)?;
                    let val_ty = val.ty.clone();
                    let (spec_pat, binder_reps, saved) =
                        self.lower_pat_and_bind(pat, binder_types, val_ty.as_ref())?;
                    if matches!(val.rep, ValueRep::Boxed)
                        && let Pat::Bind(n) = pat
                    {
                        self.local_reps.insert(n.clone(), ValueRep::Boxed);
                    }
                    saved_locals.extend(saved);
                    spec_stmts.push(SpecStmt::Let {
                        pat: spec_pat,
                        binder_reps,
                        value: val,
                    });
                }
                Stmt::Var {
                    name,
                    ty: vty,
                    value,
                } => {
                    let sub_ty = self.subst_opt(vty.as_ref());
                    let rep = self.rep_for_ty(sub_ty.as_ref());
                    let val = self.lower_as(value, &rep)?;
                    saved_locals.push((
                        name.clone(),
                        self.local_reps.insert(name.clone(), rep.clone()),
                    ));
                    spec_stmts.push(SpecStmt::Var {
                        name: name.clone(),
                        rep,
                        value: val,
                    });
                }
                Stmt::Assign(name, value) => {
                    let target_rep = self
                        .local_reps
                        .get(name)
                        .cloned()
                        .unwrap_or(ValueRep::Boxed);
                    let val = self.lower_as(value, &target_rep)?;
                    spec_stmts.push(SpecStmt::Assign(name.clone(), val));
                }
                Stmt::Fun {
                    name,
                    rec,
                    type_params,
                    dicts,
                    ty: fty,
                    value,
                    ..
                } => {
                    let sub_ty = self.subst_opt(fty.as_ref());
                    let local_poly_rec = *rec
                        && is_local_polymorphic_recursion(name, type_params, fty.as_ref(), value);
                    let boxed_fallback = self.mode == LowerMode::Fallback
                        || local_poly_rec
                        || (!type_params.is_empty()
                            && sub_ty.as_ref().is_some_and(|t| has_free_type_vars(t)));
                    let rep = if boxed_fallback {
                        ValueRep::Boxed
                    } else {
                        self.rep_for_ty(sub_ty.as_ref())
                    };
                    saved_locals.push((
                        name.clone(),
                        self.local_reps.insert(name.clone(), rep.clone()),
                    ));
                    saved_funs.push((
                        name.clone(),
                        self.boxed_local_funs.insert(name.clone(), boxed_fallback),
                    ));

                    let prev_mode = self.mode;
                    if boxed_fallback {
                        self.mode = LowerMode::Fallback;
                    }
                    let mut saved_dicts = Vec::new();
                    for d in dicts {
                        saved_dicts.push((d.clone(), self.dict_env.remove(d)));
                    }
                    let lowered_val = self.lower_expr(value)?;
                    for (d, prev) in saved_dicts {
                        if let Some(p) = prev {
                            self.dict_env.insert(d, p);
                        }
                    }
                    self.mode = prev_mode;
                    spec_stmts.push(SpecStmt::Fun {
                        name: name.clone(),
                        rec: *rec,
                        dicts: dicts.clone(),
                        rep,
                        boxed_fallback,
                        value: lowered_val,
                    });
                }
                Stmt::Expr(e) => {
                    spec_stmts.push(SpecStmt::Expr(self.lower_expr(e)?));
                }
            }
        }

        let lowered_tail = self.lower_expr(tail)?;
        self.restore_locals(saved_locals);
        for (n, prev) in saved_funs.into_iter().rev() {
            match prev {
                Some(b) => {
                    self.boxed_local_funs.insert(n, b);
                }
                None => {
                    self.boxed_local_funs.remove(&n);
                }
            }
        }
        let rep = lowered_tail.rep.clone();
        Ok(SpecExpr {
            kind: SpecExprKind::Block(spec_stmts, Box::new(lowered_tail)),
            span,
            ty,
            rep,
        })
    }

    fn lower_if(
        &mut self,
        cond: &Expr,
        then_e: &Expr,
        else_e: &Expr,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let cond_ty = self.subst_opt(cond.ty.as_ref());
        let cond_rep = self.rep_for_ty(cond_ty.as_ref());
        let cond = self.lower_as(cond, &cond_rep)?;
        let out_rep = self.rep_for_ty(ty.as_ref());
        let then_e = self.lower_as(then_e, &out_rep)?;
        let else_e = self.lower_as(else_e, &out_rep)?;
        Ok(SpecExpr {
            kind: SpecExprKind::If(Box::new(cond), Box::new(then_e), Box::new(else_e)),
            span,
            ty,
            rep: out_rep,
        })
    }

    fn lower_match(
        &mut self,
        scrut: &Expr,
        arms: &[Arm],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let lowered_scrut = self.lower_expr(scrut)?;
        let scrut_ty = lowered_scrut.ty.clone();
        let out_rep = self.rep_for_ty(ty.as_ref());
        let mut spec_arms = Vec::with_capacity(arms.len());
        for arm in arms {
            let (spec_pat, binder_reps, saved) =
                self.lower_pat_and_bind(&arm.pat, &arm.binder_types, scrut_ty.as_ref())?;
            let guard = match &arm.guard {
                Some(g) => Some(self.lower_expr(g)?),
                None => None,
            };
            let body = self.lower_as(&arm.body, &out_rep)?;
            self.restore_locals(saved);
            spec_arms.push(SpecArm {
                pat: spec_pat,
                binder_reps,
                guard,
                body,
            });
        }
        Ok(SpecExpr {
            kind: SpecExprKind::Match(Box::new(lowered_scrut), spec_arms),
            span,
            ty,
            rep: out_rep,
        })
    }

    pub(super) fn restore_locals(&mut self, saved: Vec<(String, Option<ValueRep>)>) {
        for (name, prev) in saved.into_iter().rev() {
            match prev {
                Some(rep) => {
                    self.local_reps.insert(name, rep);
                }
                None => {
                    self.local_reps.remove(&name);
                }
            }
        }
    }
}

/// Inserts an explicit `Box` or `Unbox` coercion iff `expr.rep` differs from
/// `target` across the `Unboxed` / `Boxed` boundary.
pub(crate) fn coerce(expr: SpecExpr, target: &ValueRep, span: Span) -> SpecExpr {
    match (&expr.rep, target) {
        (ValueRep::Unboxed(from_ty), ValueRep::Boxed) => {
            let from_ty = Some(from_ty.clone());
            SpecExpr::boxed(
                SpecExprKind::Box {
                    expr: Box::new(expr),
                    from_ty,
                },
                span,
                None,
            )
        }
        (ValueRep::Boxed, ValueRep::Unboxed(to_ty)) => SpecExpr::unboxed(
            SpecExprKind::Unbox {
                expr: Box::new(expr),
                to_ty: to_ty.clone(),
            },
            span,
            Some(to_ty.clone()),
        ),
        _ => expr,
    }
}
