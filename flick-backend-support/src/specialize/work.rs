//! Work-item processors for specialized values, fallback values, direct
//! instance methods, and runtime class instance tables.

use std::collections::BTreeMap;

use flick_ir::{Dict, Expr, ExprKind, TypeRef, default_method};

use super::SpecializeError;
use super::classify::ItemEntry;
use super::engine::Specializer;
use super::ir::{SpecDict, SpecExpr, SpecExprKind};
use super::lower::{LowerCtx, LowerMode, coerce};
use super::type_util::{is_concrete_layout_type, subst_dict, subst_type};
use super::types::{
    CanonicalDict, DefKey, DefMode, DictSpec, FallbackReason, SpecializedDef, SpecializedInstance,
    SymbolId, ValueRep,
};

/// One pending work item processed deterministically in FIFO order.
pub(crate) enum WorkItem {
    SpecializedValue {
        symbol: SymbolId,
        def: DefKey,
        type_args: Vec<TypeRef>,
        dict_specs: Vec<DictSpec>,
    },
    DirectMethod {
        symbol: SymbolId,
        instance: DefKey,
        method: String,
        context_dicts: Vec<CanonicalDict>,
        type_args: Vec<TypeRef>,
        method_dict_specs: Vec<DictSpec>,
    },
    FallbackValue {
        symbol: SymbolId,
        def: DefKey,
        reason: FallbackReason,
    },
    RuntimeInstance {
        symbol: SymbolId,
        def: DefKey,
    },
}

impl Specializer<'_> {
    pub(crate) fn process(&mut self, item: WorkItem) -> Result<(), SpecializeError> {
        match item {
            WorkItem::SpecializedValue {
                symbol,
                def,
                type_args,
                dict_specs,
            } => self.process_specialized_value(symbol, def, type_args, &dict_specs),
            WorkItem::DirectMethod {
                symbol,
                instance,
                method,
                context_dicts,
                type_args,
                method_dict_specs,
            } => self.process_direct_method(
                symbol,
                instance,
                &method,
                &context_dicts,
                type_args,
                &method_dict_specs,
            ),
            WorkItem::FallbackValue {
                symbol,
                def,
                reason,
            } => self.process_fallback_value(symbol, def, reason),
            WorkItem::RuntimeInstance { symbol, def } => {
                self.process_runtime_instance(symbol, &def)
            }
        }
    }

    fn process_specialized_value(
        &mut self,
        symbol: SymbolId,
        def: DefKey,
        type_args: Vec<TypeRef>,
        dict_specs: &[DictSpec],
    ) -> Result<(), SpecializeError> {
        let Some(ItemEntry::Value {
            type_params,
            dicts,
            ty,
            body,
            span,
        }) = self.index.items.get(&def).copied()
        else {
            return Ok(());
        };

        let type_subst: BTreeMap<String, TypeRef> = type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();
        let mut dict_env = BTreeMap::new();
        let mut runtime_dicts = Vec::new();
        for (i, dname) in dicts.iter().enumerate() {
            match dict_specs.get(i) {
                Some(DictSpec::Known(canon)) => {
                    dict_env.insert(dname.clone(), canon.to_dict());
                }
                _ => {
                    runtime_dicts.push(dname.clone());
                }
            }
        }

        let inst_ty = ty.map(|t| subst_type(t, &type_subst, &mut self.interner));
        if let Some(ref t) = inst_ty
            && is_concrete_layout_type(t, &self.index)
        {
            let _ = self.ensure_layout(t);
        }

        let mut ctx = LowerCtx::new(self, LowerMode::Specialized, type_subst, dict_env);
        let lowered_body = ctx.lower_expr(body)?;
        let rep = lowered_body.rep.clone();
        self.check_output_budget(&def, &lowered_body)?;

        self.definitions.insert(
            symbol.clone(),
            SpecializedDef {
                symbol,
                source: def,
                mode: DefMode::Specialized,
                type_args,
                runtime_dicts,
                ty: inst_ty,
                rep,
                body: lowered_body,
                span,
            },
        );
        Ok(())
    }

    fn process_fallback_value(
        &mut self,
        symbol: SymbolId,
        def: DefKey,
        reason: FallbackReason,
    ) -> Result<(), SpecializeError> {
        let Some(ItemEntry::Value {
            dicts,
            ty,
            body,
            span,
            ..
        }) = self.index.items.get(&def).copied()
        else {
            return Ok(());
        };

        let mut ctx = LowerCtx::new(self, LowerMode::Fallback, BTreeMap::new(), BTreeMap::new());
        let lowered = ctx.lower_expr(body)?;
        let boxed_body = coerce(lowered, &ValueRep::Boxed, span);
        self.check_output_budget(&def, &boxed_body)?;

        self.definitions.insert(
            symbol.clone(),
            SpecializedDef {
                symbol,
                source: def,
                mode: DefMode::Fallback(reason),
                type_args: Vec::new(),
                runtime_dicts: dicts.to_vec(),
                ty: ty.cloned(),
                rep: ValueRep::Boxed,
                body: boxed_body,
                span,
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn process_direct_method(
        &mut self,
        symbol: SymbolId,
        instance: DefKey,
        method: &str,
        context_dicts: &[CanonicalDict],
        type_args: Vec<TypeRef>,
        method_dict_specs: &[DictSpec],
    ) -> Result<(), SpecializeError> {
        let Some(ItemEntry::Instance {
            info,
            methods,
            span,
        }) = self.index.items.get(&instance).copied()
        else {
            return Ok(());
        };

        let mut dict_env: BTreeMap<String, Dict> = info
            .context
            .iter()
            .cloned()
            .zip(context_dicts.iter().map(CanonicalDict::to_dict))
            .collect();

        let (method_expr, runtime_dicts): (Expr, Vec<String>) = if let Some((_, body)) =
            methods.iter().find(|(m, _)| m == method)
        {
            match &body.kind {
                ExprKind::Abstract(names, inner) => {
                    let mut rt = Vec::new();
                    for (i, n) in names.iter().enumerate() {
                        match method_dict_specs.get(i) {
                            Some(DictSpec::Known(c)) => {
                                dict_env.insert(n.clone(), c.to_dict());
                            }
                            _ => rt.push(n.clone()),
                        }
                    }
                    ((**inner).clone(), rt)
                }
                _ => (body.clone(), Vec::new()),
            }
        } else if let Some((_, _, extra_dicts)) = info.defaults.iter().find(|(m, _, _)| m == method)
        {
            let def_key = (info.class.0.clone(), default_method(&info.class.1, method));
            let Some(ItemEntry::Value {
                dicts: def_dicts,
                body: def_body,
                ..
            }) = self.index.items.get(&def_key).copied()
            else {
                return Ok(());
            };
            let self_dict = Dict::Instance {
                module: info.module.clone(),
                name: info.name.clone(),
                args: context_dicts.iter().map(CanonicalDict::to_dict).collect(),
            };
            let mut passed = vec![self_dict];
            passed.extend(extra_dicts.iter().map(|d| subst_dict(d, &dict_env)));
            let mut rt = Vec::new();
            for (i, dname) in def_dicts.iter().enumerate() {
                if let Some(d) = passed.get(i) {
                    dict_env.insert(dname.clone(), d.clone());
                } else {
                    rt.push(dname.clone());
                }
            }
            (def_body.clone(), rt)
        } else {
            return Ok(());
        };

        let mut ctx = LowerCtx::new(self, LowerMode::Specialized, BTreeMap::new(), dict_env);
        let lowered = ctx.lower_expr(&method_expr)?;
        let inst_ty = lowered.ty.clone();
        let rep = lowered.rep.clone();
        self.check_output_budget(&instance, &lowered)?;

        self.definitions.insert(
            symbol.clone(),
            SpecializedDef {
                symbol,
                source: instance,
                mode: DefMode::Specialized,
                type_args,
                runtime_dicts,
                ty: inst_ty,
                rep,
                body: lowered,
                span,
            },
        );
        Ok(())
    }

    fn process_runtime_instance(
        &mut self,
        symbol: SymbolId,
        def: &DefKey,
    ) -> Result<(), SpecializeError> {
        let Some(ItemEntry::Instance {
            info,
            methods,
            span,
        }) = self.index.items.get(def).copied()
        else {
            return Ok(());
        };

        let mut spec_supers: Vec<(String, SpecDict)> = Vec::with_capacity(info.supers.len());
        for (class_name, super_dict) in &info.supers {
            let mut ctx =
                LowerCtx::new(self, LowerMode::Fallback, BTreeMap::new(), BTreeMap::new());
            let sd = ctx.lower_runtime_dict(super_dict)?;
            spec_supers.push((class_name.clone(), sd));
        }

        let mut spec_methods = BTreeMap::new();
        for (mname, mbody) in methods {
            let mut ctx =
                LowerCtx::new(self, LowerMode::Fallback, BTreeMap::new(), BTreeMap::new());
            let lowered = ctx.lower_expr(mbody)?;
            let boxed = coerce(lowered, &ValueRep::Boxed, mbody.span);
            self.check_output_budget(def, &boxed)?;
            spec_methods.insert(mname.clone(), boxed);
        }

        for (mname, _, extra_dicts) in &info.defaults {
            let default_key = (info.class.0.clone(), default_method(&info.class.1, mname));
            let default_sym =
                self.enqueue_fallback(&default_key, FallbackReason::HigherRankCallback);
            let mut ctx =
                LowerCtx::new(self, LowerMode::Fallback, BTreeMap::new(), BTreeMap::new());
            let mut passed = vec![SpecDict::Instance {
                target: symbol.clone(),
                module: info.module.clone(),
                name: info.name.clone(),
                args: info
                    .context
                    .iter()
                    .map(|c| SpecDict::Given(c.clone()))
                    .collect(),
            }];
            passed.extend(ctx.lower_runtime_dicts(extra_dicts)?);
            let call = SpecExpr::boxed(
                SpecExprKind::CallFallback {
                    target: default_sym,
                    dicts: passed,
                },
                span,
                None,
            );
            spec_methods.insert(mname.clone(), call);
        }

        self.instances.insert(
            symbol.clone(),
            SpecializedInstance {
                symbol,
                module: info.module.clone(),
                name: info.name.clone(),
                class: info.class.clone(),
                context: info.context.clone(),
                supers: spec_supers,
                methods: spec_methods,
            },
        );
        Ok(())
    }
}
