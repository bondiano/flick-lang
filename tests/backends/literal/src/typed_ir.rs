//! Typed-IR inspection mode (`case == "typed-ir"`) for the literal fixture backend.
//!
//! Validates the incoming `Program` and emits `typed-ir-report.json` recording
//! declared type/constructor shapes, polymorphic binder types, and instantiation
//! type arguments read solely from the portable IR.

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{Artifact, ArtifactKind, BuildRequest, Diagnostic, ModuleRole, Profile};
use flick_ir::{
    Arith, BinOp, Expr, ExprKind, FileId, ItemKind, Lit, Module, Num, Pat, Program, Stmt,
    TypeInterner, TypeRef,
};
use serde_json::{Map, Value, json};

use super::diagnostic;

/// Deterministic, type-preserving IR rewrite exercised on `Profile::Release`
/// (without `HashMap`, staying strictly within the WASI-free sandbox):
/// - inlines private single-argument polymorphic identity helpers (`fun x -> x`),
///   substituting `type_params -> type_args` on the application result;
/// - folds constant integer additions (`Lit::Int(a) + Lit::Int(b)`) while
///   preserving expression types;
/// - drops now-unused private helpers.
fn optimize_if_release(program: &mut Program, request: &BuildRequest) {
    if request.target.profile != Profile::Release {
        return;
    }
    let own: BTreeSet<&str> = request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own)
        .map(|m| m.name.as_str())
        .collect();
    for module in &mut program.modules {
        if !own.contains(module.name.as_str()) {
            continue;
        }
        rewrite_module(module);
    }
}

fn rewrite_module(module: &mut Module) {
    let exported: BTreeSet<String> = module
        .exports
        .values
        .iter()
        .map(|e| e.name.clone())
        .collect();
    let mut identities: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in &module.items {
        if let ItemKind::Value {
            name,
            type_params,
            dicts,
            body,
            ..
        } = &item.kind
            && !exported.contains(name)
            && dicts.is_empty()
            && is_identity_lambda(body)
        {
            identities.insert(name.clone(), type_params.clone());
        }
    }
    let mod_name = module.name.clone();
    let mut interner = TypeInterner::new();
    for item in &mut module.items {
        if let ItemKind::Value { body, .. } = &mut item.kind {
            rewrite_expr(body, &mod_name, &identities, &mut interner);
        }
    }
    if identities.is_empty() {
        return;
    }
    let mut referenced = BTreeSet::new();
    for item in &module.items {
        item.each_body(&mut |body| collect_local_globals(body, &mod_name, &mut referenced));
    }
    module.items.retain(|item| match &item.kind {
        ItemKind::Value { name, .. } => !identities.contains_key(name) || referenced.contains(name),
        _ => true,
    });
}

fn is_identity_lambda(expr: &Expr) -> bool {
    let ExprKind::Lambda { params, body } = &expr.kind else {
        return false;
    };
    let [param] = params.as_slice() else {
        return false;
    };
    let Pat::Bind(param_name) = &param.pat else {
        return false;
    };
    matches!(&body.kind, ExprKind::Local(local) if local == param_name)
}

fn rewrite_expr(
    expr: &mut Expr,
    module: &str,
    identities: &BTreeMap<String, Vec<String>>,
    interner: &mut TypeInterner,
) {
    expr.each_child_mut(&mut |child| rewrite_expr(child, module, identities, interner));
    if let ExprKind::App { func, arg, .. } = &mut expr.kind
        && let ExprKind::Global {
            module: target_mod,
            name,
            type_args,
        } = &func.kind
        && target_mod == module
        && let Some(type_params) = identities.get(name)
    {
        let mut replacement = (**arg).clone();
        if replacement.ty.is_none() {
            replacement.ty.clone_from(&expr.ty);
        }
        if type_params.len() == type_args.len()
            && !type_params.is_empty()
            && let Some(ty) = &mut replacement.ty
        {
            let subst: Vec<(&str, &TypeRef)> = type_params
                .iter()
                .map(String::as_str)
                .zip(type_args.iter())
                .collect();
            *ty = ty.substitute(&subst, interner);
        }
        replacement.span = expr.span;
        *expr = replacement;
        return;
    }
    if let ExprKind::Binary(BinOp::Arith(Arith::Add, Num::Int), lhs, rhs) = &expr.kind
        && let (ExprKind::Lit(Lit::Int(a)), ExprKind::Lit(Lit::Int(b))) = (&lhs.kind, &rhs.kind)
        && let Some(sum) = a.checked_add(*b)
    {
        let ty = expr
            .ty
            .clone()
            .or_else(|| lhs.ty.clone())
            .or_else(|| rhs.ty.clone());
        *expr = Expr::with_ty(ExprKind::Lit(Lit::Int(sum)), expr.span, ty);
    }
}

fn collect_local_globals(expr: &Expr, module: &str, out: &mut BTreeSet<String>) {
    if let ExprKind::Global {
        module: m, name, ..
    } = &expr.kind
        && m == module
    {
        out.insert(name.clone());
    }
    expr.each_child(&mut |child| collect_local_globals(child, module, out));
}

pub(super) fn validate_program(
    program: &Program,
    request: &BuildRequest,
) -> Result<(), Vec<Diagnostic>> {
    let count = request.sources.len().max(program.files.len());
    let files: BTreeSet<FileId> = (0..count)
        .filter_map(|i| u32::try_from(i).ok().map(FileId::new))
        .collect();
    let own: BTreeSet<&str> = request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own)
        .map(|m| m.name.as_str())
        .collect();
    let problems = flick_ir::validate(program, &|m| !own.contains(m), &files);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(diagnostic(&problems.join("; "), None))
    }
}

pub(super) fn report_artifact(
    mut program: Program,
    request: &BuildRequest,
) -> Result<Artifact, Vec<Diagnostic>> {
    validate_program(&program, request)?;
    let optimized = request.target.profile == Profile::Release;
    optimize_if_release(&mut program, request);
    validate_program(&program, request)?;
    let own: BTreeSet<&str> = request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own)
        .map(|m| m.name.as_str())
        .collect();
    let mut modules = Map::new();
    for module in &program.modules {
        if own.contains(module.name.as_str()) {
            modules.insert(module.name.clone(), module_report(module));
        }
    }
    let report = json!({
        "ir_version": program.ir_version,
        "profile": if optimized { "release" } else { "debug" },
        "optimized": optimized,
        "modules": modules,
    });
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|e| diagnostic(&format!("report json failed: {e}"), None))?;
    Ok(Artifact {
        id: "typed-ir-report".into(),
        path: "typed-ir-report.json".into(),
        kind: ArtifactKind::Metadata,
        bytes,
    })
}

fn module_report(module: &Module) -> Value {
    let mut types = Map::new();
    for decl in &module.types {
        let mut ctors = Map::new();
        for c in &decl.constructors {
            let constraints: Vec<Value> = c
                .constraints
                .iter()
                .map(|con| {
                    json!({
                        "module": con.module,
                        "class": con.class,
                        "args": con.args.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    })
                })
                .collect();
            ctors.insert(
                c.name.clone(),
                json!({
                    "type_params": c.type_params,
                    "existentials": c.existentials,
                    "constraints": constraints,
                    "fields": c.fields.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "record": c.record,
                    "result_args": c.result_args.as_ref().map(|args| {
                        args.iter().map(ToString::to_string).collect::<Vec<_>>()
                    }),
                }),
            );
        }
        types.insert(
            decl.name.clone(),
            json!({
                "params": decl.params,
                "error": decl.error,
                "alias": decl.alias.as_ref().map(ToString::to_string),
                "constructors": ctors,
            }),
        );
    }

    let mut ctor_items = Map::new();
    let mut values = Map::new();
    for item in &module.items {
        match &item.kind {
            ItemKind::Constructor(c) => {
                ctor_items.insert(
                    c.name.clone(),
                    json!({
                        "ty": c.ty,
                        "arity": c.arity,
                        "dicts": c.dicts,
                        "fields": c.fields.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    }),
                );
            }
            ItemKind::Value {
                name,
                type_params,
                dicts,
                dict_constraints,
                ty,
                body,
            } => {
                let mut info = ValueInfo::default();
                collect_expr(body, &mut info);
                values.insert(
                    name.clone(),
                    json!({
                        "type_params": type_params,
                        "dicts": dicts,
                        "dict_constraints": dict_constraints.iter().map(|c| {
                            json!({
                                "module": c.module,
                                "class": c.class,
                                "args": c.args.iter().map(ToString::to_string).collect::<Vec<_>>(),
                            })
                        }).collect::<Vec<_>>(),
                        "ty": ty.as_ref().map(ToString::to_string),
                        "body_ty": body.ty.as_ref().map(ToString::to_string),
                        "params": info.params,
                        "lets": info.lets,
                        "vars": info.vars,
                        "arms": info.arms,
                        "globals": info.globals,
                        "con_uses": info.con_uses,
                        "strings": info.strings,
                        "ints": info.ints,
                    }),
                );
            }
            _ => {}
        }
    }

    json!({
        "types": types,
        "constructor_items": ctor_items,
        "values": values,
    })
}

#[derive(Default)]
struct ValueInfo {
    params: Vec<Value>,
    lets: Vec<Value>,
    vars: Vec<Value>,
    arms: Vec<Value>,
    globals: Vec<Value>,
    con_uses: Vec<Value>,
    strings: Vec<Value>,
    ints: Vec<Value>,
}

fn binders_json(binders: &[(String, flick_ir::TypeRef)]) -> Value {
    let mut map = Map::new();
    for (name, ty) in binders {
        map.insert(name.clone(), Value::String(ty.to_string()));
    }
    Value::Object(map)
}

fn collect_expr(expr: &Expr, out: &mut ValueInfo) {
    match &expr.kind {
        ExprKind::Lit(Lit::String(s)) => {
            out.strings.push(json!({
                "value": s,
                "ty": expr.ty.as_ref().map(ToString::to_string),
            }));
        }
        ExprKind::Lit(Lit::Int(n)) => {
            out.ints.push(json!({
                "value": n,
                "ty": expr.ty.as_ref().map(ToString::to_string),
            }));
        }
        ExprKind::Global {
            module,
            name,
            type_args,
        } => {
            out.globals.push(json!({
                "module": module,
                "name": name,
                "type_args": type_args.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "ty": expr.ty.as_ref().map(ToString::to_string),
            }));
        }
        ExprKind::Con(c) => {
            out.con_uses.push(json!({
                "module": c.module,
                "ty": c.ty,
                "name": c.name,
                "dicts": c.dicts,
                "fields": c.fields.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "type_args": c.type_args.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "expr_ty": expr.ty.as_ref().map(ToString::to_string),
            }));
        }
        ExprKind::Lambda { params, .. } => {
            for p in params {
                out.params.push(json!({
                    "ty": p.ty.as_ref().map(ToString::to_string),
                    "binder_types": binders_json(&p.binder_types),
                }));
            }
        }
        ExprKind::Block(stmts, _) => {
            for stmt in stmts {
                match stmt {
                    Stmt::Let { binder_types, .. } => {
                        out.lets.push(binders_json(binder_types));
                    }
                    Stmt::Var { name, ty, .. } => {
                        out.vars.push(json!({
                            "name": name,
                            "ty": ty.as_ref().map(ToString::to_string),
                        }));
                    }
                    _ => {}
                }
            }
        }
        ExprKind::Match(_, arms) => {
            for arm in arms {
                out.arms.push(binders_json(&arm.binder_types));
            }
        }
        _ => {}
    }
    expr.each_child(&mut |child| collect_expr(child, out));
}
