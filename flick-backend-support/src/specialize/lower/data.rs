//! Data aggregate, record field offset/label, dictionary, handler, and
//! remaining expression lowering.

use flick_ir::{BinOp, Expr, ExprKind, Handler, IrType, Span, TypeRef};

use super::super::SpecializeError;
use super::super::ir::{
    SpecArm, SpecExpr, SpecExprKind, SpecHandled, SpecHandler, SpecOpArm, SpecParam, SpecPat,
};
use super::super::type_util::{
    contains_exists_or_module, contains_open_record, is_concrete_layout_type,
};
use super::super::types::ValueRep;
use super::{LowerCtx, LowerMode, coerce};

impl LowerCtx<'_, '_> {
    pub(super) fn lower_tuple(
        &mut self,
        items: &[Expr],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let rep = self.rep_for_ty(ty.as_ref());
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            if matches!(rep, ValueRep::Boxed) {
                out.push(self.lower_as(item, &ValueRep::Boxed)?);
            } else {
                let it_ty = self.subst_opt(item.ty.as_ref());
                let it_rep = self.rep_for_ty(it_ty.as_ref());
                out.push(self.lower_as(item, &it_rep)?);
            }
        }
        let raw = SpecExpr {
            kind: SpecExprKind::Tuple(out),
            span,
            ty: ty.clone(),
            rep: rep.clone(),
        };
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

    pub(super) fn lower_list(
        &mut self,
        items: &[Expr],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let rep = self.rep_for_ty(ty.as_ref());
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            if matches!(rep, ValueRep::Boxed) {
                out.push(self.lower_as(item, &ValueRep::Boxed)?);
            } else {
                let it_ty = self.subst_opt(item.ty.as_ref());
                let it_rep = self.rep_for_ty(it_ty.as_ref());
                out.push(self.lower_as(item, &it_rep)?);
            }
        }
        let raw = SpecExpr {
            kind: SpecExprKind::List(out),
            span,
            ty: ty.clone(),
            rep,
        };
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

    pub(super) fn lower_record(
        &mut self,
        fields: &[(String, Expr)],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let is_packed_or_exists = ty
            .as_ref()
            .is_some_and(|t| contains_exists_or_module(t, &self.spec.index));
        let is_open = ty.as_ref().is_some_and(|t| contains_open_record(t));
        let boxed = self.mode == LowerMode::Fallback || is_packed_or_exists || is_open;

        let mut out = Vec::with_capacity(fields.len());
        for (label, val_expr) in fields {
            let lowered = if is_packed_or_exists {
                let v = self.lower_expr(val_expr)?;
                self.adapt_for_boxed_cell(v, val_expr.span)
            } else if boxed {
                self.lower_as(val_expr, &ValueRep::Boxed)?
            } else {
                let fty = self.subst_opt(val_expr.ty.as_ref());
                let frep = self.rep_for_ty(fty.as_ref());
                self.lower_as(val_expr, &frep)?
            };
            out.push((label.clone(), lowered));
        }

        let rep = if boxed {
            ValueRep::Boxed
        } else {
            self.rep_for_ty(ty.as_ref())
        };
        let raw = SpecExpr {
            kind: SpecExprKind::Record { fields: out, boxed },
            span,
            ty: ty.clone(),
            rep,
        };
        if boxed {
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

    pub(super) fn lower_dictionary(
        &mut self,
        fields: &[(String, Expr)],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let mut out = Vec::with_capacity(fields.len());
        for (label, val_expr) in fields {
            let lowered = self.lower_expr(val_expr)?;
            let adapted = self.adapt_for_boxed_cell(lowered, val_expr.span);
            out.push((label.clone(), adapted));
        }
        Ok(SpecExpr::boxed(SpecExprKind::Dictionary(out), span, ty))
    }

    pub(super) fn lower_field(
        &mut self,
        rec: &Expr,
        label: &str,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let lowered_rec = self.lower_expr(rec)?;
        let out_rep = self.rep_for_ty(ty.as_ref());

        if self.mode == LowerMode::Specialized
            && !matches!(lowered_rec.rep, ValueRep::Boxed)
            && let Some(rec_ty) = &lowered_rec.ty
            && let Some((index, offset, size)) = self.spec.record_field_offset(rec_ty, label)?
        {
            return Ok(SpecExpr {
                kind: SpecExprKind::FieldOffset {
                    record: Box::new(lowered_rec),
                    label: label.to_owned(),
                    index,
                    offset,
                    size,
                },
                span,
                ty,
                rep: out_rep,
            });
        }

        let lookup = SpecExpr::boxed(
            SpecExprKind::FieldByLabel {
                record: Box::new(lowered_rec),
                label: label.to_owned(),
            },
            span,
            ty,
        );
        if lookup
            .ty
            .as_ref()
            .is_some_and(|t| matches!(&**t, IrType::Fun { .. } | IrType::Forall { .. }))
        {
            return Ok(lookup);
        }
        Ok(coerce(lookup, &out_rep, span))
    }

    pub(super) fn lower_update(
        &mut self,
        rec: &Expr,
        fields: &[(String, Expr)],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let lowered_rec = self.lower_expr(rec)?;
        if self.mode == LowerMode::Specialized
            && !matches!(lowered_rec.rep, ValueRep::Boxed)
            && let Some(rec_ty) = &lowered_rec.ty
            && matches!(&**rec_ty, IrType::Record { tail: None, .. })
        {
            let mut offset_fields = Vec::with_capacity(fields.len());
            let mut all_resolved = true;
            for (label, val) in fields {
                if let Some((index, offset, _)) = self.spec.record_field_offset(rec_ty, label)? {
                    let fty = self.subst_opt(val.ty.as_ref());
                    let frep = self.rep_for_ty(fty.as_ref());
                    let lowered_val = self.lower_as(val, &frep)?;
                    offset_fields.push((label.clone(), index, offset, lowered_val));
                } else {
                    all_resolved = false;
                    break;
                }
            }
            if all_resolved {
                let rep = self.rep_for_ty(ty.as_ref());
                return Ok(SpecExpr {
                    kind: SpecExprKind::UpdateOffset {
                        record: Box::new(lowered_rec),
                        fields: offset_fields,
                    },
                    span,
                    ty,
                    rep,
                });
            }
        }

        let mut by_label = Vec::with_capacity(fields.len());
        for (label, val) in fields {
            let lowered_val = self.lower_as(val, &ValueRep::Boxed)?;
            by_label.push((label.clone(), lowered_val));
        }
        let rec_boxed = coerce(lowered_rec, &ValueRep::Boxed, rec.span);
        let out_rep = self.rep_for_ty(ty.as_ref());
        let updated = SpecExpr::boxed(
            SpecExprKind::UpdateByLabel {
                record: Box::new(rec_boxed),
                fields: by_label,
            },
            span,
            ty,
        );
        Ok(coerce(updated, &out_rep, span))
    }

    pub(super) fn lower_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let out_rep = self.rep_for_ty(ty.as_ref());
        let (lhs, rhs) = if self.mode == LowerMode::Fallback {
            (
                self.lower_as(lhs, &ValueRep::Boxed)?,
                self.lower_as(rhs, &ValueRep::Boxed)?,
            )
        } else {
            let l_ty = self.subst_opt(lhs.ty.as_ref());
            let r_ty = self.subst_opt(rhs.ty.as_ref());
            let l_rep = self.rep_for_ty(l_ty.as_ref());
            let r_rep = self.rep_for_ty(r_ty.as_ref());
            (self.lower_as(lhs, &l_rep)?, self.lower_as(rhs, &r_rep)?)
        };
        Ok(SpecExpr {
            kind: SpecExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
            span,
            ty,
            rep: out_rep,
        })
    }

    pub(super) fn lower_handle(
        &mut self,
        h: &Handler,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let mut effects = Vec::with_capacity(h.effects.len());
        for eff in &h.effects {
            let mut ops = Vec::with_capacity(eff.operations.len());
            for op in &eff.operations {
                ops.push(SpecOpArm {
                    name: op.name.clone(),
                    class: op.class,
                    dicts: op.dicts,
                    arity: op.arity,
                    covers: op.covers,
                    function: self.lower_expr(&op.function)?,
                });
            }
            effects.push(SpecHandled {
                effect: eff.effect.clone(),
                operations: ops,
            });
        }
        let body = self.lower_expr(&h.body)?;
        let ret = match &h.ret {
            Some(r) => Some(self.lower_expr(r)?),
            None => None,
        };
        let out_rep = self.rep_for_ty(ty.as_ref());
        let mut errors = Vec::with_capacity(h.errors.len());
        for arm in &h.errors {
            let (spec_pat, binder_reps, saved) =
                self.lower_pat_and_bind(&arm.pat, &arm.binder_types, None)?;
            let guard = match &arm.guard {
                Some(g) => Some(self.lower_expr(g)?),
                None => None,
            };
            let arm_body = self.lower_as(&arm.body, &out_rep)?;
            self.restore_locals(saved);
            errors.push(SpecArm {
                pat: spec_pat,
                binder_reps,
                guard,
                body: arm_body,
            });
        }
        Ok(SpecExpr {
            kind: SpecExprKind::Handle(Box::new(SpecHandler {
                effects,
                body,
                ret,
                errors,
            })),
            span,
            ty,
            rep: out_rep,
        })
    }

    pub(super) fn lower_misc(
        &mut self,
        expr: &Expr,
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let rep = self.rep_for_ty(ty.as_ref());
        let kind = match &expr.kind {
            ExprKind::Not(inner) => SpecExprKind::Not(Box::new(self.lower_as(inner, &rep)?)),
            ExprKind::Unwrap(inner) => SpecExprKind::Unwrap(Box::new(self.lower_expr(inner)?)),
            ExprKind::Neg(num, inner) => {
                SpecExprKind::Neg(*num, Box::new(self.lower_as(inner, &rep)?))
            }
            ExprKind::Ignore(inner) => SpecExprKind::Ignore(Box::new(self.lower_expr(inner)?)),
            ExprKind::Await(inner) => SpecExprKind::Await(Box::new(self.lower_expr(inner)?)),
            ExprKind::Task(inner) => SpecExprKind::Task(Box::new(self.lower_expr(inner)?)),
            ExprKind::Within(owner, inner) => {
                SpecExprKind::Within(owner.clone(), Box::new(self.lower_expr(inner)?))
            }
            ExprKind::ReadOwnerEvidence => SpecExprKind::ReadOwnerEvidence,
            ExprKind::WithOwnerEvidence(o, b) => SpecExprKind::WithOwnerEvidence(
                Box::new(self.lower_expr(o)?),
                Box::new(self.lower_expr(b)?),
            ),
            ExprKind::Generate {
                body,
                strict,
                sync,
                limit,
            } => SpecExprKind::Generate {
                body: Box::new(self.lower_expr(body)?),
                strict: *strict,
                sync: *sync,
                limit: match limit {
                    Some(l) => Some(Box::new(self.lower_expr(l)?)),
                    None => None,
                },
            },
            ExprKind::Each {
                stop,
                pat,
                binder_types,
                source,
                body,
            } => {
                let stop = match stop {
                    Some(s) => Some(Box::new(self.lower_expr(s)?)),
                    None => None,
                };
                let source = self.lower_expr(source)?;
                let (spec_pat, binder_reps, saved) =
                    self.lower_pat_and_bind(pat, binder_types, None)?;
                let body = self.lower_expr(body)?;
                self.restore_locals(saved);
                SpecExprKind::Each {
                    stop,
                    pat: spec_pat,
                    binder_types: binder_reps,
                    source: Box::new(source),
                    body: Box::new(body),
                }
            }
            ExprKind::Yield(inner) => SpecExprKind::Yield(Box::new(self.lower_expr(inner)?)),
            ExprKind::Loop { condition, body } => SpecExprKind::Loop {
                condition: Box::new(self.lower_expr(condition)?),
                body: Box::new(self.lower_expr(body)?),
            },
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable,
            } => SpecExprKind::ControlLoop {
                id: id.clone(),
                condition: match condition {
                    Some(c) => Some(Box::new(self.lower_expr(c)?)),
                    None => None,
                },
                body: Box::new(self.lower_expr(body)?),
                iterable: *iterable,
            },
            ExprKind::LoopExit { id, value } => SpecExprKind::LoopExit {
                id: id.clone(),
                value: match value {
                    Some(v) => Some(Box::new(self.lower_expr(v)?)),
                    None => None,
                },
            },
            ExprKind::Lift(inner, lift) => {
                SpecExprKind::Lift(Box::new(self.lower_expr(inner)?), lift.clone())
            }
            _ => unreachable!("handled in lower_expr"),
        };
        Ok(SpecExpr {
            kind,
            span,
            ty,
            rep,
        })
    }

    /// Adapts a lowered expression (value or concrete function) to the uniform
    /// boxed-cell ABI when storing it in a runtime dictionary or packed module.
    fn adapt_for_boxed_cell(&mut self, expr: SpecExpr, span: Span) -> SpecExpr {
        if matches!(expr.rep, ValueRep::Boxed) {
            return expr;
        }
        let Some(ty) = expr.ty.clone() else {
            return coerce(expr, &ValueRep::Boxed, span);
        };
        let (params, ret) = ty.fun_stages();
        if params.is_empty() {
            return coerce(expr, &ValueRep::Boxed, span);
        }
        let mut spec_params = Vec::with_capacity(params.len());
        let mut call = expr;
        for (i, (pty, stage)) in params.into_iter().enumerate() {
            let pname = format!("$box_arg_{i}");
            spec_params.push(SpecParam {
                pat: SpecPat::Bind(pname.clone()),
                stage,
                rep: ValueRep::Boxed,
                binder_reps: vec![(pname.clone(), ValueRep::Boxed)],
            });
            let arg_local = SpecExpr::boxed(SpecExprKind::Local(pname), span, None);
            let arg_unboxed = if is_concrete_layout_type(pty, &self.spec.index) {
                let _ = self.spec.ensure_layout(pty);
                coerce(arg_local, &ValueRep::Unboxed(pty.clone()), span)
            } else {
                arg_local
            };
            call = SpecExpr::unboxed(
                SpecExprKind::App {
                    func: Box::new(call),
                    arg: Box::new(arg_unboxed),
                    stage,
                    arg_first: false,
                },
                span,
                Some(TypeRef::new(ret.clone())),
            );
        }
        let boxed_ret = coerce(call, &ValueRep::Boxed, span);
        SpecExpr::boxed(
            SpecExprKind::Lambda {
                params: spec_params,
                body: Box::new(boxed_ret),
            },
            span,
            None,
        )
    }
}
