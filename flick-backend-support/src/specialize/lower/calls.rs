//! Call-site, global, constructor, and dictionary lowering.

use std::collections::BTreeMap;

use flick_ir::{Constructor, Dict, Expr, ExprKind, Span, Stage, TypeRef};

use super::super::SpecializeError;
use super::super::classify::ItemEntry;
use super::super::ir::{SpecDict, SpecExpr, SpecExprKind};
use super::super::type_util::{
    contains_exists_or_module, contains_open_record, contains_quantified_forall,
    has_free_type_vars, is_concrete_layout_type, subst_constructor, subst_dict, subst_type,
};
use super::super::types::{CanonicalDict, DefKey, DictSpec, FallbackReason, ValueRep};
use super::{LowerCtx, LowerMode, coerce};

impl LowerCtx<'_, '_> {
    pub(super) fn lower_dicts_expr(
        &mut self,
        inner: &Expr,
        dicts: &[Dict],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let resolved: Vec<Dict> = dicts
            .iter()
            .map(|d| subst_dict(d, &self.dict_env))
            .collect();
        match &inner.kind {
            ExprKind::Global {
                module,
                name,
                type_args,
            } => self.lower_global_ref(module, name, type_args, &resolved, span, ty),
            ExprKind::Method {
                dict,
                name,
                type_args,
            } => self.lower_method_ref(dict, name, type_args, &resolved, span, ty),
            ExprKind::Con(ctor) => self.lower_con_ref(ctor, &resolved, span, ty),
            ExprKind::Perform { effect, op, stages } => {
                let spec_dicts = self.lower_runtime_dicts(&resolved)?;
                let rep = self.rep_for_ty(ty.as_ref());
                Ok(SpecExpr {
                    kind: SpecExprKind::Perform {
                        effect: effect.clone(),
                        op: op.clone(),
                        stages: stages.clone(),
                        dicts: spec_dicts,
                    },
                    span,
                    ty,
                    rep,
                })
            }
            _ => {
                let target = self.lower_expr(inner)?;
                let spec_dicts = self.lower_runtime_dicts(&resolved)?;
                let rep = target.rep.clone();
                Ok(SpecExpr {
                    kind: SpecExprKind::Dicts(Box::new(target), spec_dicts),
                    span,
                    ty,
                    rep,
                })
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn lower_global_ref(
        &mut self,
        module: &str,
        name: &str,
        raw_type_args: &[TypeRef],
        resolved_dicts: &[Dict],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let def: DefKey = (module.to_owned(), name.to_owned());
        let type_args: Vec<TypeRef> = raw_type_args
            .iter()
            .map(|a| subst_type(a, &self.type_subst, &mut self.spec.interner))
            .collect();

        if let Some(ItemEntry::Foreign { foreign }) = self.spec.index.items.get(&def).copied() {
            let polymorphic = self.spec.classification.fallback_reason(&def).is_some()
                || type_args
                    .iter()
                    .any(|a| !is_concrete_layout_type(a, &self.spec.index));
            let sym = self.spec.enqueue_foreign(&def, foreign, polymorphic);
            let spec_dicts = self.lower_runtime_dicts(resolved_dicts)?;
            let rep = if polymorphic || self.mode == LowerMode::Fallback {
                ValueRep::Boxed
            } else {
                self.rep_for_ty(ty.as_ref())
            };
            return Ok(SpecExpr {
                kind: SpecExprKind::CallForeign {
                    target: sym,
                    module: module.to_owned(),
                    name: name.to_owned(),
                    polymorphic,
                    dicts: spec_dicts,
                },
                span,
                ty,
                rep,
            });
        }

        let static_fallback = self.spec.classification.fallback_reason(&def);
        let non_concrete_arg = type_args.iter().find_map(|a| {
            if contains_open_record(a) {
                Some(FallbackReason::OpenRowPolymorphic)
            } else if contains_exists_or_module(a, &self.spec.index) {
                Some(FallbackReason::PackedOrExistential)
            } else if contains_quantified_forall(a) {
                Some(FallbackReason::HigherRankCallback)
            } else if has_free_type_vars(a) {
                Some(FallbackReason::OpenRowPolymorphic)
            } else {
                None
            }
        });

        let expected_params = match self.spec.index.items.get(&def) {
            Some(ItemEntry::Value { type_params, .. }) => type_params.len(),
            _ => 0,
        };
        let missing_type_args = expected_params > 0 && type_args.len() != expected_params;

        let fallback_reason = static_fallback
            .or(non_concrete_arg)
            .or_else(|| missing_type_args.then_some(FallbackReason::HigherRankCallback));

        if let Some(reason) = fallback_reason {
            let sym = self.spec.enqueue_fallback(&def, reason);
            let spec_dicts = self.lower_runtime_dicts(resolved_dicts)?;
            return Ok(SpecExpr::boxed(
                SpecExprKind::CallFallback {
                    target: sym,
                    dicts: spec_dicts,
                },
                span,
                ty,
            ));
        }

        let mut dict_specs = Vec::with_capacity(resolved_dicts.len());
        let mut runtime_dicts = Vec::new();
        for d in resolved_dicts {
            if self.spec.config.devirtualize_known_dicts
                && let Some(canon) = CanonicalDict::from_static(d)
            {
                dict_specs.push(DictSpec::Known(canon));
            } else {
                dict_specs.push(DictSpec::Runtime);
                runtime_dicts.push(self.lower_runtime_dict(d)?);
            }
        }

        let sym = self
            .spec
            .enqueue_specialized_value(&def, type_args.clone(), dict_specs)?;
        let eff_ty = ty.or_else(|| {
            if let Some(ItemEntry::Value {
                type_params,
                ty: Some(decl_ty),
                ..
            }) = self.spec.index.items.get(&def).copied()
            {
                let sub: BTreeMap<String, TypeRef> = type_params
                    .iter()
                    .cloned()
                    .zip(type_args.iter().cloned())
                    .collect();
                Some(subst_type(decl_ty, &sub, &mut self.spec.interner))
            } else {
                None
            }
        });
        let rep = self.rep_for_ty(eff_ty.as_ref());
        Ok(SpecExpr {
            kind: SpecExprKind::CallDirect {
                target: sym,
                dicts: runtime_dicts,
            },
            span,
            ty: eff_ty,
            rep,
        })
    }

    pub(super) fn lower_method_ref(
        &mut self,
        raw_dict: &Dict,
        method_name: &str,
        raw_type_args: &[TypeRef],
        extra_dicts: &[Dict],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let resolved = subst_dict(raw_dict, &self.dict_env);
        let type_args: Vec<TypeRef> = raw_type_args
            .iter()
            .map(|a| subst_type(a, &self.type_subst, &mut self.spec.interner))
            .collect();

        // Replace dictionary calls with direct calls ONLY when exact dictionary
        // evidence is statically known!
        if self.spec.config.devirtualize_known_dicts
            && type_args
                .iter()
                .all(|a| is_concrete_layout_type(a, &self.spec.index))
            && let Some(canon) = CanonicalDict::from_static(&resolved)
            && let Some((inst_key, ctx_dicts)) = self.spec.resolve_static_instance(&canon)
        {
            let mut method_dict_specs = Vec::with_capacity(extra_dicts.len());
            let mut runtime_extra = Vec::new();
            for d in extra_dicts {
                if let Some(c) = CanonicalDict::from_static(d) {
                    method_dict_specs.push(DictSpec::Known(c));
                } else {
                    method_dict_specs.push(DictSpec::Runtime);
                    runtime_extra.push(self.lower_runtime_dict(d)?);
                }
            }
            if let Some(sym) = self.spec.enqueue_direct_method(
                &inst_key,
                method_name,
                ctx_dicts,
                type_args,
                method_dict_specs,
            )? {
                self.spec.direct_dict_calls += 1;
                let rep = self.rep_for_ty(ty.as_ref());
                return Ok(SpecExpr {
                    kind: SpecExprKind::CallDirect {
                        target: sym,
                        dicts: runtime_extra,
                    },
                    span,
                    ty,
                    rep,
                });
            }
        }

        let spec_dict = self.lower_runtime_dict(&resolved)?;
        let base = SpecExpr::boxed(
            SpecExprKind::MethodCall {
                dict: spec_dict,
                name: method_name.to_owned(),
            },
            span,
            ty.clone(),
        );
        if extra_dicts.is_empty() {
            Ok(base)
        } else {
            let extra = self.lower_runtime_dicts(extra_dicts)?;
            Ok(SpecExpr::boxed(
                SpecExprKind::Dicts(Box::new(base), extra),
                span,
                ty,
            ))
        }
    }

    pub(super) fn lower_con_ref(
        &mut self,
        raw_ctor: &Constructor,
        resolved_dicts: &[Dict],
        span: Span,
        ty: Option<TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let ctor = subst_constructor(raw_ctor, &self.type_subst, &mut self.spec.interner);
        let is_gadt = self.spec.index.is_gadt_dict_or_existential(&ctor);
        let concrete_fields = ctor
            .fields
            .iter()
            .all(|f| is_concrete_layout_type(f, &self.spec.index));
        let concrete_args = ctor
            .type_args
            .iter()
            .all(|a| is_concrete_layout_type(a, &self.spec.index));
        let boxed =
            self.mode == LowerMode::Fallback || is_gadt || !concrete_fields || !concrete_args;

        let spec_dicts = self.lower_runtime_dicts(resolved_dicts)?;
        let result_ty = ty.as_ref().map(|t| {
            let (_, res) = t.fun_stages();
            TypeRef::new(res.clone())
        });
        self.spec.register_ctor(&ctor, result_ty.as_ref(), boxed);

        let rep = if boxed && self.mode == LowerMode::Fallback {
            ValueRep::Boxed
        } else {
            self.rep_for_ty(ty.as_ref())
        };
        Ok(SpecExpr {
            kind: SpecExprKind::Construct {
                ctor,
                boxed,
                dicts: spec_dicts,
            },
            span,
            ty,
            rep,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn lower_app(
        &mut self,
        expr: &Expr,
        final_ty: Option<&TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let mut spine: Vec<(&Expr, Stage, bool, Span, Option<TypeRef>)> = Vec::new();
        let mut cur = expr;
        while let ExprKind::App {
            func,
            arg,
            stage,
            arg_first,
        } = &cur.kind
        {
            let res_ty = self.subst_opt(cur.ty.as_ref());
            spine.push((arg, *stage, *arg_first, cur.span, res_ty));
            cur = func;
        }
        spine.reverse();

        let head = self.lower_expr(cur)?;
        let head_is_boxed_abi = matches!(
            head.kind,
            SpecExprKind::CallFallback { .. }
                | SpecExprKind::CallForeign {
                    polymorphic: true,
                    ..
                }
                | SpecExprKind::MethodCall { .. }
        ) || matches!(head.rep, ValueRep::Boxed)
            || matches!(&head.kind, SpecExprKind::Local(n) if self.boxed_local_funs.get(n).copied().unwrap_or(false));

        let head_is_direct_unboxed = matches!(
            head.kind,
            SpecExprKind::CallDirect { .. }
                | SpecExprKind::CallForeign {
                    polymorphic: false,
                    ..
                }
        ) && !matches!(head.rep, ValueRep::Boxed);

        let head_param_tys: Vec<TypeRef> = head
            .ty
            .as_ref()
            .map(|t| {
                let (params, _) = t.fun_stages();
                params.into_iter().map(|(p, _)| p.clone()).collect()
            })
            .unwrap_or_default();

        let gadt_boxed_fields: Option<Vec<usize>> = match &head.kind {
            SpecExprKind::Construct {
                ctor, boxed: true, ..
            } => Some(self.spec.ctor_boxed_fields(ctor, true)),
            _ => None,
        };

        let mut acc = head;
        for (i, (arg_expr, stage, arg_first, app_span, step_ty)) in spine.into_iter().enumerate() {
            let lowered_arg = if head_is_boxed_abi {
                let arg_val = self.lower_boxed_boundary_arg(arg_expr, head_param_tys.get(i))?;
                coerce(arg_val, &ValueRep::Boxed, arg_expr.span)
            } else if let Some(bf) = &gadt_boxed_fields {
                let arg_val = self.lower_expr(arg_expr)?;
                if bf.contains(&i) {
                    coerce(arg_val, &ValueRep::Boxed, arg_expr.span)
                } else {
                    arg_val
                }
            } else if self.mode == LowerMode::Fallback && head_is_direct_unboxed {
                let raw = self.lower_expr(arg_expr)?;
                if let Some(pty) = head_param_tys.get(i)
                    && is_concrete_layout_type(pty, &self.spec.index)
                {
                    let _ = self.spec.ensure_layout(pty);
                    coerce(raw, &ValueRep::Unboxed(pty.clone()), arg_expr.span)
                } else {
                    raw
                }
            } else {
                self.lower_expr(arg_expr)?
            };

            let step_rep = if head_is_boxed_abi {
                ValueRep::Boxed
            } else if self.mode == LowerMode::Fallback && head_is_direct_unboxed {
                match step_ty.as_ref() {
                    Some(t) if is_concrete_layout_type(t, &self.spec.index) => {
                        let _ = self.spec.ensure_layout(t);
                        ValueRep::Unboxed(t.clone())
                    }
                    _ => ValueRep::Boxed,
                }
            } else {
                self.rep_for_ty(step_ty.as_ref())
            };

            acc = SpecExpr {
                kind: SpecExprKind::App {
                    func: Box::new(acc),
                    arg: Box::new(lowered_arg),
                    stage,
                    arg_first,
                },
                span: app_span,
                ty: step_ty,
                rep: step_rep,
            };
        }

        let final_rep = self.rep_for_ty(final_ty);
        Ok(coerce(acc, &final_rep, expr.span))
    }

    fn lower_boxed_boundary_arg(
        &mut self,
        arg_expr: &Expr,
        param_ty: Option<&TypeRef>,
    ) -> Result<SpecExpr, SpecializeError> {
        let is_higher_rank = param_ty.is_some_and(|t| contains_quantified_forall(t));
        if is_higher_rank && let ExprKind::Global { module, name, .. } = &arg_expr.kind {
            let def = (module.clone(), name.clone());
            let sym = self
                .spec
                .enqueue_fallback(&def, FallbackReason::HigherRankCallback);
            return Ok(SpecExpr::boxed(
                SpecExprKind::CallFallback {
                    target: sym,
                    dicts: Vec::new(),
                },
                arg_expr.span,
                None,
            ));
        }
        let is_fn_literal = matches!(
            arg_expr.kind,
            ExprKind::Lambda { .. } | ExprKind::Abstract(..)
        );
        if is_higher_rank || (is_fn_literal && self.mode == LowerMode::Specialized) {
            let prev = self.mode;
            self.mode = LowerMode::Fallback;
            let res = self.lower_expr(arg_expr);
            self.mode = prev;
            return res;
        }
        self.lower_expr(arg_expr)
    }

    pub(crate) fn lower_runtime_dicts(
        &mut self,
        dicts: &[Dict],
    ) -> Result<Vec<SpecDict>, SpecializeError> {
        dicts.iter().map(|d| self.lower_runtime_dict(d)).collect()
    }

    pub(crate) fn lower_runtime_dict(&mut self, dict: &Dict) -> Result<SpecDict, SpecializeError> {
        match dict {
            Dict::Instance { module, name, args } => {
                let target = self.spec.enqueue_runtime_instance(module, name);
                let spec_args = self.lower_runtime_dicts(args)?;
                Ok(SpecDict::Instance {
                    target,
                    module: module.clone(),
                    name: name.clone(),
                    args: spec_args,
                })
            }
            Dict::Given(name) => Ok(SpecDict::Given(name.clone())),
            Dict::Value { module, name } => {
                let def = (module.clone(), name.clone());
                let target = self
                    .spec
                    .enqueue_specialized_value(&def, Vec::new(), Vec::new())?;
                Ok(SpecDict::Value { target })
            }
            Dict::Opened { binder, field } => Ok(SpecDict::Opened {
                binder: binder.clone(),
                field: field.clone(),
            }),
            Dict::Field(local) => Ok(SpecDict::Field(local.clone())),
            Dict::Super { dict: inner, class } => Ok(SpecDict::Super {
                dict: Box::new(self.lower_runtime_dict(inner)?),
                class: class.clone(),
            }),
            Dict::Structural { class, fields } => Ok(SpecDict::Structural {
                class: class.clone(),
                fields: self.lower_runtime_dicts(fields)?,
            }),
            Dict::Primitive { constraint, labels } => Ok(SpecDict::Primitive {
                constraint: constraint.clone(),
                labels: labels.clone(),
            }),
            Dict::Site { place, definition } => Ok(SpecDict::Site {
                place: place.clone(),
                definition: definition.clone(),
            }),
            Dict::Error => Ok(SpecDict::Error),
        }
    }
}
