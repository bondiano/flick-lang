//! Dictionaries, patterns, statements, parameters, and handlers translation
//! for [`super::materialize_specialized_modules`].

use flick_backend_support::specialize::{
    SpecArm, SpecDict, SpecHandled, SpecHandler, SpecOpArm, SpecParam, SpecPat, SpecStmt,
    ValueRep as SpecValueRep,
};
use flick_ir::{Arm, Dict, Handled, Handler, OpArm, Param, Pat, Stmt, TypeRef};

use super::{SymbolMap, spec_expr_to_ir};

pub(super) fn rep_to_ty(rep: &SpecValueRep) -> Option<TypeRef> {
    match rep {
        SpecValueRep::Unboxed(ty) => Some(ty.clone()),
        SpecValueRep::Boxed => None,
    }
}

pub(super) fn binder_reps_to_types(reps: &[(String, SpecValueRep)]) -> Vec<(String, TypeRef)> {
    reps.iter()
        .filter_map(|(name, rep)| rep_to_ty(rep).map(|ty| (name.clone(), ty)))
        .collect()
}

pub(super) fn spec_dict_to_ir(dict: &SpecDict, sym_map: &SymbolMap) -> Dict {
    match dict {
        SpecDict::Instance {
            target,
            module,
            name,
            args,
        } => {
            let (m, n) = sym_map.resolve_instance(target, module, name);
            Dict::Instance {
                module: m,
                name: n,
                args: args.iter().map(|a| spec_dict_to_ir(a, sym_map)).collect(),
            }
        }
        SpecDict::Given(name) => Dict::Given(name.clone()),
        SpecDict::Value { target } => {
            let (module, name) = sym_map.resolve_def(target);
            Dict::Value { module, name }
        }
        SpecDict::Opened { binder, field } => Dict::Opened {
            binder: binder.clone(),
            field: field.clone(),
        },
        SpecDict::Field(name) => Dict::Field(name.clone()),
        SpecDict::Super { dict, class } => Dict::Super {
            dict: Box::new(spec_dict_to_ir(dict, sym_map)),
            class: class.clone(),
        },
        SpecDict::Structural { class, fields } => Dict::Structural {
            class: class.clone(),
            fields: fields.iter().map(|f| spec_dict_to_ir(f, sym_map)).collect(),
        },
        SpecDict::Primitive { constraint, labels } => Dict::Primitive {
            constraint: constraint.clone(),
            labels: labels.clone(),
        },
        SpecDict::Site { place, definition } => Dict::Site {
            place: place.clone(),
            definition: definition.clone(),
        },
        SpecDict::Error => Dict::Error,
    }
}

pub(super) fn spec_param_to_ir(p: &SpecParam) -> Param {
    Param {
        pat: spec_pat_to_ir(&p.pat),
        stage: p.stage,
        ty: rep_to_ty(&p.rep),
        binder_types: binder_reps_to_types(&p.binder_reps),
    }
}

pub(super) fn spec_stmt_to_ir(stmt: &SpecStmt, sym_map: &SymbolMap) -> Stmt {
    match stmt {
        SpecStmt::Let {
            pat,
            binder_reps,
            value,
        } => Stmt::Let {
            pat: spec_pat_to_ir(pat),
            binder_types: binder_reps_to_types(binder_reps),
            value: spec_expr_to_ir(value, sym_map),
        },
        SpecStmt::Var { name, rep, value } => Stmt::Var {
            name: name.clone(),
            ty: rep_to_ty(rep),
            value: spec_expr_to_ir(value, sym_map),
        },
        SpecStmt::Assign(name, value) => {
            Stmt::Assign(name.clone(), spec_expr_to_ir(value, sym_map))
        }
        SpecStmt::Fun {
            name,
            rec,
            dicts,
            rep,
            value,
            ..
        } => Stmt::Fun {
            name: name.clone(),
            rec: *rec,
            type_params: Vec::new(),
            dicts: dicts.clone(),
            dict_constraints: Vec::new(),
            ty: rep_to_ty(rep),
            value: spec_expr_to_ir(value, sym_map),
        },
        SpecStmt::Expr(expr) => Stmt::Expr(spec_expr_to_ir(expr, sym_map)),
    }
}

pub(super) fn spec_arm_to_ir(arm: &SpecArm, sym_map: &SymbolMap) -> Arm {
    Arm {
        pat: spec_pat_to_ir(&arm.pat),
        binder_types: binder_reps_to_types(&arm.binder_reps),
        guard: arm.guard.as_ref().map(|g| spec_expr_to_ir(g, sym_map)),
        body: spec_expr_to_ir(&arm.body, sym_map),
    }
}

pub(super) fn spec_pat_to_ir(pat: &SpecPat) -> Pat {
    match pat {
        SpecPat::Wild => Pat::Wild,
        SpecPat::Bind(name) => Pat::Bind(name.clone()),
        SpecPat::Lit(lit) => Pat::Lit(lit.clone()),
        SpecPat::Tuple(items) => Pat::Tuple(items.iter().map(spec_pat_to_ir).collect()),
        SpecPat::List(items, rest) => Pat::List(
            items.iter().map(spec_pat_to_ir).collect(),
            rest.as_ref().map(|r| Box::new(spec_pat_to_ir(r))),
        ),
        SpecPat::Record { fields, .. } => Pat::Record(
            fields
                .iter()
                .map(|(label, _, sub)| (label.clone(), spec_pat_to_ir(sub)))
                .collect(),
        ),
        SpecPat::Con { ctor, args, .. } => {
            Pat::Con(ctor.clone(), args.iter().map(spec_pat_to_ir).collect())
        }
        SpecPat::Or(alts) => Pat::Or(alts.iter().map(spec_pat_to_ir).collect()),
    }
}

pub(super) fn spec_handler_to_ir(h: &SpecHandler, sym_map: &SymbolMap) -> Handler {
    Handler {
        effects: h
            .effects
            .iter()
            .map(|e| spec_handled_to_ir(e, sym_map))
            .collect(),
        body: spec_expr_to_ir(&h.body, sym_map),
        ret: h.ret.as_ref().map(|r| spec_expr_to_ir(r, sym_map)),
        errors: h
            .errors
            .iter()
            .map(|a| spec_arm_to_ir(a, sym_map))
            .collect(),
    }
}

fn spec_handled_to_ir(h: &SpecHandled, sym_map: &SymbolMap) -> Handled {
    Handled {
        effect: h.effect.clone(),
        operations: h
            .operations
            .iter()
            .map(|op| spec_op_arm_to_ir(op, sym_map))
            .collect(),
    }
}

fn spec_op_arm_to_ir(op: &SpecOpArm, sym_map: &SymbolMap) -> OpArm {
    OpArm {
        name: op.name.clone(),
        class: op.class,
        dicts: op.dicts,
        arity: op.arity,
        covers: op.covers,
        function: spec_expr_to_ir(&op.function, sym_map),
    }
}
