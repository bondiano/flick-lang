//! Materializes a `#743` [`SpecializedProgram`] back into monomorphized
//! [`flick_ir::Module`] items so Perceus source lowering (#744) sees concrete
//! instantiated types in specialized bodies while keeping original signatures
//! on fallback definitions.

mod parts;

use std::collections::BTreeMap;

use flick_backend_support::specialize::{
    DefMode, SpecDict, SpecExpr, SpecExprKind, SpecializedProgram, SymbolId,
};
use flick_ir::{Exports, Expr, ExprKind, Initialization, Item, ItemKind, Module, Span, TypeRef};

use parts::{
    binder_reps_to_types, spec_arm_to_ir, spec_dict_to_ir, spec_handler_to_ir, spec_param_to_ir,
    spec_pat_to_ir, spec_stmt_to_ir,
};

pub(super) struct SymbolMap {
    defs: BTreeMap<SymbolId, (String, String)>,
    instances: BTreeMap<SymbolId, (String, String)>,
}

impl SymbolMap {
    fn from_specialized(specialized: &SpecializedProgram) -> Self {
        let mut defs = BTreeMap::new();
        for (sym, def) in &specialized.definitions {
            let target = match def.mode {
                DefMode::Fallback(_) => def.source.clone(),
                DefMode::Specialized => {
                    let prefix = format!("{}.", def.source.0);
                    let item_name = sym.0.strip_prefix(&prefix).unwrap_or(&sym.0).to_owned();
                    (def.source.0.clone(), item_name)
                }
            };
            defs.insert(sym.clone(), target);
        }
        let mut instances = BTreeMap::new();
        for (sym, inst) in &specialized.instances {
            instances.insert(sym.clone(), (inst.module.clone(), inst.name.clone()));
        }
        Self { defs, instances }
    }

    pub(super) fn resolve_def(&self, sym: &SymbolId) -> (String, String) {
        if let Some(pair) = self.defs.get(sym) {
            return pair.clone();
        }
        match sym.0.rsplit_once('.') {
            Some((m, n)) => (m.to_owned(), n.to_owned()),
            None => (String::new(), sym.0.clone()),
        }
    }

    pub(super) fn resolve_instance(
        &self,
        sym: &SymbolId,
        fallback_mod: &str,
        fallback_name: &str,
    ) -> (String, String) {
        self.instances
            .get(sym)
            .cloned()
            .unwrap_or_else(|| (fallback_mod.to_owned(), fallback_name.to_owned()))
    }
}

pub(super) fn materialize_specialized_modules(
    modules: &[Module],
    specialized: &SpecializedProgram,
) -> Vec<Module> {
    let sym_map = SymbolMap::from_specialized(specialized);
    let mut out = modules.to_vec();
    let mut mod_index: BTreeMap<String, usize> = out
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.clone(), i))
        .collect();

    for (sym, def) in &specialized.definitions {
        if !matches!(def.mode, DefMode::Specialized) {
            continue;
        }
        let (target_mod, target_name) = sym_map.resolve_def(sym);
        let body = spec_expr_to_ir(&def.body, &sym_map);
        let new_item = Item {
            span: def.span,
            kind: ItemKind::Value {
                name: target_name.clone(),
                type_params: Vec::new(),
                dicts: def.runtime_dicts.clone(),
                dict_constraints: Vec::new(),
                ty: def.ty.clone(),
                body,
            },
        };

        let Some(&idx) = mod_index.get(&target_mod) else {
            let idx = out.len();
            mod_index.insert(target_mod.clone(), idx);
            out.push(Module {
                name: target_mod,
                items: vec![new_item],
                types: Vec::new(),
                preambles: Vec::new(),
                labels: BTreeMap::new(),
                initialization: Initialization::default(),
                records: BTreeMap::new(),
                package: None,
                exports: Exports::default(),
            });
            continue;
        };

        let module = &mut out[idx];
        if let Some(existing) = module
            .items
            .iter_mut()
            .find(|item| matches!(&item.kind, ItemKind::Value { name, .. } if name == &target_name))
        {
            *existing = new_item;
        } else {
            module.items.push(new_item);
        }
    }

    out
}

fn wrap_dicts(
    base: Expr,
    dicts: &[SpecDict],
    sym_map: &SymbolMap,
    span: Span,
    ty: Option<TypeRef>,
) -> Expr {
    if dicts.is_empty() {
        return base;
    }
    let ir_dicts = dicts.iter().map(|d| spec_dict_to_ir(d, sym_map)).collect();
    Expr {
        kind: ExprKind::Dicts(Box::new(base), ir_dicts),
        span,
        ty,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn spec_expr_to_ir(expr: &SpecExpr, sym_map: &SymbolMap) -> Expr {
    let span = expr.span;
    let ty = expr.ty.clone();
    let kind = match &expr.kind {
        SpecExprKind::Box {
            expr: inner,
            from_ty,
        } => {
            let mut e = spec_expr_to_ir(inner, sym_map);
            if e.ty.is_none() {
                e.ty = from_ty.clone().or(ty);
            }
            return e;
        }
        SpecExprKind::Unbox { expr: inner, to_ty } => {
            let mut e = spec_expr_to_ir(inner, sym_map);
            e.ty = Some(to_ty.clone());
            return e;
        }
        SpecExprKind::CallDirect { target, dicts }
        | SpecExprKind::CallFallback { target, dicts } => {
            let (module, name) = sym_map.resolve_def(target);
            let base = Expr {
                kind: ExprKind::Global {
                    module,
                    name,
                    type_args: Vec::new(),
                },
                span,
                ty: ty.clone(),
            };
            return wrap_dicts(base, dicts, sym_map, span, ty);
        }
        SpecExprKind::CallForeign {
            module,
            name,
            dicts,
            ..
        } => {
            let base = Expr {
                kind: ExprKind::Global {
                    module: module.clone(),
                    name: name.clone(),
                    type_args: Vec::new(),
                },
                span,
                ty: ty.clone(),
            };
            return wrap_dicts(base, dicts, sym_map, span, ty);
        }
        SpecExprKind::MethodCall { dict, name } => ExprKind::Method {
            dict: spec_dict_to_ir(dict, sym_map),
            name: name.clone(),
            type_args: Vec::new(),
        },
        SpecExprKind::FieldOffset { record, label, .. }
        | SpecExprKind::FieldByLabel { record, label } => {
            ExprKind::Field(Box::new(spec_expr_to_ir(record, sym_map)), label.clone())
        }
        SpecExprKind::UpdateOffset { record, fields } => ExprKind::Update(
            Box::new(spec_expr_to_ir(record, sym_map)),
            fields
                .iter()
                .map(|(l, _, _, v)| (l.clone(), spec_expr_to_ir(v, sym_map)))
                .collect(),
        ),
        SpecExprKind::UpdateByLabel { record, fields } => ExprKind::Update(
            Box::new(spec_expr_to_ir(record, sym_map)),
            fields
                .iter()
                .map(|(l, v)| (l.clone(), spec_expr_to_ir(v, sym_map)))
                .collect(),
        ),
        SpecExprKind::Construct { ctor, dicts, .. } => {
            let base = Expr {
                kind: ExprKind::Con(ctor.clone()),
                span,
                ty: ty.clone(),
            };
            return wrap_dicts(base, dicts, sym_map, span, ty);
        }
        SpecExprKind::Lit(lit) => ExprKind::Lit(lit.clone()),
        SpecExprKind::Local(name) => ExprKind::Local(name.clone()),
        SpecExprKind::Var(name) => ExprKind::Var(name.clone()),
        SpecExprKind::Perform {
            effect,
            op,
            stages,
            dicts,
        } => {
            let base = Expr {
                kind: ExprKind::Perform {
                    effect: effect.clone(),
                    op: op.clone(),
                    stages: stages.clone(),
                },
                span,
                ty: ty.clone(),
            };
            return wrap_dicts(base, dicts, sym_map, span, ty);
        }
        SpecExprKind::ServiceTag(s) => ExprKind::ServiceTag(s.clone()),
        SpecExprKind::Label(l) => ExprKind::Label(l.clone()),
        SpecExprKind::Generate {
            body,
            strict,
            sync,
            limit,
        } => ExprKind::Generate {
            body: Box::new(spec_expr_to_ir(body, sym_map)),
            strict: *strict,
            sync: *sync,
            limit: limit
                .as_ref()
                .map(|l| Box::new(spec_expr_to_ir(l, sym_map))),
        },
        SpecExprKind::Each {
            stop,
            pat,
            binder_types,
            source,
            body,
        } => ExprKind::Each {
            stop: stop.as_ref().map(|s| Box::new(spec_expr_to_ir(s, sym_map))),
            pat: spec_pat_to_ir(pat),
            binder_types: binder_reps_to_types(binder_types),
            source: Box::new(spec_expr_to_ir(source, sym_map)),
            body: Box::new(spec_expr_to_ir(body, sym_map)),
        },
        SpecExprKind::Yield(inner) => ExprKind::Yield(Box::new(spec_expr_to_ir(inner, sym_map))),
        SpecExprKind::Loop { condition, body } => ExprKind::Loop {
            condition: Box::new(spec_expr_to_ir(condition, sym_map)),
            body: Box::new(spec_expr_to_ir(body, sym_map)),
        },
        SpecExprKind::ControlLoop {
            id,
            condition,
            body,
            iterable,
        } => ExprKind::ControlLoop {
            id: id.clone(),
            condition: condition
                .as_ref()
                .map(|c| Box::new(spec_expr_to_ir(c, sym_map))),
            body: Box::new(spec_expr_to_ir(body, sym_map)),
            iterable: *iterable,
        },
        SpecExprKind::LoopExit { id, value } => ExprKind::LoopExit {
            id: id.clone(),
            value: value
                .as_ref()
                .map(|v| Box::new(spec_expr_to_ir(v, sym_map))),
        },
        SpecExprKind::Dicts(inner, dicts) => ExprKind::Dicts(
            Box::new(spec_expr_to_ir(inner, sym_map)),
            dicts.iter().map(|d| spec_dict_to_ir(d, sym_map)).collect(),
        ),
        SpecExprKind::Abstract(names, inner) => {
            ExprKind::Abstract(names.clone(), Box::new(spec_expr_to_ir(inner, sym_map)))
        }
        SpecExprKind::App {
            func,
            arg,
            stage,
            arg_first,
        } => ExprKind::App {
            func: Box::new(spec_expr_to_ir(func, sym_map)),
            arg: Box::new(spec_expr_to_ir(arg, sym_map)),
            stage: *stage,
            arg_first: *arg_first,
        },
        SpecExprKind::Lambda { params, body } => ExprKind::Lambda {
            params: params.iter().map(spec_param_to_ir).collect(),
            body: Box::new(spec_expr_to_ir(body, sym_map)),
        },
        SpecExprKind::Block(stmts, tail) => ExprKind::Block(
            stmts.iter().map(|s| spec_stmt_to_ir(s, sym_map)).collect(),
            Box::new(spec_expr_to_ir(tail, sym_map)),
        ),
        SpecExprKind::If(c, t, e) => ExprKind::If(
            Box::new(spec_expr_to_ir(c, sym_map)),
            Box::new(spec_expr_to_ir(t, sym_map)),
            Box::new(spec_expr_to_ir(e, sym_map)),
        ),
        SpecExprKind::Match(scrut, arms) => ExprKind::Match(
            Box::new(spec_expr_to_ir(scrut, sym_map)),
            arms.iter().map(|a| spec_arm_to_ir(a, sym_map)).collect(),
        ),
        SpecExprKind::Tuple(items) => {
            ExprKind::Tuple(items.iter().map(|i| spec_expr_to_ir(i, sym_map)).collect())
        }
        SpecExprKind::List(items) => {
            ExprKind::List(items.iter().map(|i| spec_expr_to_ir(i, sym_map)).collect())
        }
        SpecExprKind::Record { fields, .. } => ExprKind::Record(
            fields
                .iter()
                .map(|(l, v)| (l.clone(), spec_expr_to_ir(v, sym_map)))
                .collect(),
        ),
        SpecExprKind::Dictionary(fields) => ExprKind::Dictionary(
            fields
                .iter()
                .map(|(l, v)| (l.clone(), spec_expr_to_ir(v, sym_map)))
                .collect(),
        ),
        SpecExprKind::Binary(op, l, r) => ExprKind::Binary(
            *op,
            Box::new(spec_expr_to_ir(l, sym_map)),
            Box::new(spec_expr_to_ir(r, sym_map)),
        ),
        SpecExprKind::Not(inner) => ExprKind::Not(Box::new(spec_expr_to_ir(inner, sym_map))),
        SpecExprKind::Unwrap(inner) => ExprKind::Unwrap(Box::new(spec_expr_to_ir(inner, sym_map))),
        SpecExprKind::Neg(num, inner) => {
            ExprKind::Neg(*num, Box::new(spec_expr_to_ir(inner, sym_map)))
        }
        SpecExprKind::Ignore(inner) => ExprKind::Ignore(Box::new(spec_expr_to_ir(inner, sym_map))),
        SpecExprKind::Await(inner) => ExprKind::Await(Box::new(spec_expr_to_ir(inner, sym_map))),
        SpecExprKind::Task(inner) => ExprKind::Task(Box::new(spec_expr_to_ir(inner, sym_map))),
        SpecExprKind::Within(owner, inner) => {
            ExprKind::Within(owner.clone(), Box::new(spec_expr_to_ir(inner, sym_map)))
        }
        SpecExprKind::ReadOwnerEvidence => ExprKind::ReadOwnerEvidence,
        SpecExprKind::WithOwnerEvidence(ev, inner) => ExprKind::WithOwnerEvidence(
            Box::new(spec_expr_to_ir(ev, sym_map)),
            Box::new(spec_expr_to_ir(inner, sym_map)),
        ),
        SpecExprKind::Handle(h) => ExprKind::Handle(Box::new(spec_handler_to_ir(h, sym_map))),
        SpecExprKind::Lift(inner, lift) => {
            ExprKind::Lift(Box::new(spec_expr_to_ir(inner, sym_map)), lift.clone())
        }
    };
    Expr { kind, span, ty }
}
