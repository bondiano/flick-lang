//! Deterministic JSON report generation for `layout-specialize` mode.

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{Artifact, ArtifactKind, BuildRequest, Diagnostic, ModuleRole};
use flick_backend_support::layout::{
    Discriminant, FieldLayout, FieldStorage, LayoutEngine, Niche, PrimitiveTable, TypeLayout,
    TypeRepr,
};
use flick_backend_support::specialize::{
    DefMode, DiagnosticKind, FallbackReason, SpecExpr, SpecExprKind, SpecStmt, SpecializedProgram,
};
use flick_ir::{IrType, Program, TypeRef};
use serde_json::{Map, Value, json};

use crate::diagnostic;

const fn fallback_reason_str(reason: FallbackReason) -> &'static str {
    match reason {
        FallbackReason::PolymorphicRecursion => "polymorphic_recursion",
        FallbackReason::HigherRankCallback => "higher_rank_callback",
        FallbackReason::PackedOrExistential => "packed_or_existential",
        FallbackReason::GadtDictionary => "gadt_dictionary",
        FallbackReason::PolymorphicForeign => "polymorphic_foreign",
        FallbackReason::OpenRowPolymorphic => "open_row_polymorphic",
    }
}

fn collect_report_layouts(
    program: &Program,
    specialized: &SpecializedProgram,
    request: &BuildRequest,
) -> Result<BTreeMap<String, Value>, Vec<Diagnostic>> {
    let mut engine = LayoutEngine::new(program, PrimitiveTable::target_64())
        .map_err(|e| diagnostic(&e.to_string(), None))?;
    let mut all_layouts: BTreeMap<TypeRef, TypeLayout> = specialized.layouts.clone();

    let own: BTreeSet<&str> = request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own)
        .map(|m| m.name.as_str())
        .collect();

    let mut extra_types = Vec::new();
    for module in &program.modules {
        if !own.contains(module.name.as_str()) {
            continue;
        }
        for decl in &module.types {
            if decl.params.is_empty() {
                extra_types.push(TypeRef::new(IrType::Named {
                    module: module.name.clone(),
                    name: decl.name.clone(),
                    args: vec![],
                }));
            }
        }
    }
    let ty_int = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Int".into(),
        args: vec![],
    });
    let ty_float = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Float".into(),
        args: vec![],
    });
    extra_types.push(TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "List".into(),
        args: vec![ty_int],
    }));
    extra_types.push(TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![ty_float],
    }));

    for ty in extra_types {
        if !all_layouts.contains_key(&ty)
            && let Ok(layout) = engine.layout(&ty)
        {
            all_layouts.insert(ty, layout);
        }
    }

    let mut out = BTreeMap::new();
    for (ty, layout) in &all_layouts {
        out.insert(ty.to_string(), layout_to_json(layout));
    }
    Ok(out)
}

fn niche_to_json(niche: Option<Niche>) -> Value {
    match niche {
        None => Value::Null,
        Some(n) => json!({
            "offset": n.offset,
            "width": n.width,
            "start": n.start,
            "end": n.end,
        }),
    }
}

fn field_to_json(field: &FieldLayout) -> Value {
    json!({
        "offset": field.offset,
        "size": field.size,
        "align": field.align,
        "storage": match field.storage {
            FieldStorage::ByValue => "by_value",
            FieldStorage::Pointer => "pointer",
        },
        "ty": field.ty.to_string(),
    })
}

fn layout_to_json(layout: &TypeLayout) -> Value {
    let mut obj = Map::new();
    obj.insert("size".into(), json!(layout.size));
    obj.insert("align".into(), json!(layout.align));
    obj.insert("niche".into(), niche_to_json(layout.niche));
    match &layout.repr {
        TypeRepr::Primitive(prim) => {
            obj.insert("kind".into(), json!("primitive"));
            obj.insert("primitive".into(), json!(format!("{prim:?}")));
        }
        TypeRepr::Data(data) => {
            obj.insert("kind".into(), json!("data"));
            let disc = match &data.discriminant {
                Discriminant::None => json!({ "kind": "none" }),
                Discriminant::Tag { offset, width } => json!({
                    "kind": "tag",
                    "offset": offset,
                    "width": width,
                }),
                Discriminant::Niche {
                    offset,
                    width,
                    data_variant,
                    niche_start,
                    niche_end,
                } => json!({
                    "kind": "niche",
                    "offset": offset,
                    "width": width,
                    "data_variant": data_variant,
                    "niche_start": niche_start,
                    "niche_end": niche_end,
                }),
            };
            obj.insert("discriminant".into(), disc);
            obj.insert(
                "pointer_fields".into(),
                json!(data.pointer_fields.iter().collect::<Vec<_>>()),
            );
            let variants: Vec<Value> = data
                .variants
                .iter()
                .map(|v| {
                    json!({
                        "name": v.name,
                        "tag": v.tag,
                        "dicts": v.dicts.len(),
                        "fields": v.fields.iter().map(field_to_json).collect::<Vec<_>>(),
                    })
                })
                .collect();
            obj.insert("variants".into(), Value::Array(variants));
        }
        TypeRepr::Record(record) => {
            obj.insert("kind".into(), json!("record"));
            let fields: Vec<Value> = record
                .fields
                .iter()
                .map(|rf| {
                    let mut m = Map::new();
                    m.insert("label".into(), json!(rf.label));
                    m.insert("offset".into(), json!(rf.field.offset));
                    m.insert("size".into(), json!(rf.field.size));
                    m.insert("align".into(), json!(rf.field.align));
                    m.insert(
                        "storage".into(),
                        json!(match rf.field.storage {
                            FieldStorage::ByValue => "by_value",
                            FieldStorage::Pointer => "pointer",
                        }),
                    );
                    m.insert("ty".into(), json!(rf.field.ty.to_string()));
                    Value::Object(m)
                })
                .collect();
            obj.insert("fields".into(), Value::Array(fields));
        }
        TypeRepr::Tuple(tuple) => {
            obj.insert("kind".into(), json!("tuple"));
            obj.insert(
                "fields".into(),
                Value::Array(tuple.fields.iter().map(field_to_json).collect()),
            );
        }
    }
    Value::Object(obj)
}

#[allow(clippy::too_many_lines)]
fn count_boxed_cells_in_expr(expr: &SpecExpr) -> usize {
    match &expr.kind {
        SpecExprKind::Box { expr: inner, .. } | SpecExprKind::Unbox { expr: inner, .. } => {
            1 + count_boxed_cells_in_expr(inner)
        }
        SpecExprKind::FieldOffset { record, .. } | SpecExprKind::FieldByLabel { record, .. } => {
            count_boxed_cells_in_expr(record)
        }
        SpecExprKind::UpdateOffset { record, fields } => {
            count_boxed_cells_in_expr(record)
                + fields
                    .iter()
                    .map(|(_, _, _, v)| count_boxed_cells_in_expr(v))
                    .sum::<usize>()
        }
        SpecExprKind::UpdateByLabel { record, fields } => {
            count_boxed_cells_in_expr(record)
                + fields
                    .iter()
                    .map(|(_, v)| count_boxed_cells_in_expr(v))
                    .sum::<usize>()
        }
        SpecExprKind::App { func, arg, .. }
        | SpecExprKind::Binary(_, func, arg)
        | SpecExprKind::WithOwnerEvidence(func, arg) => {
            count_boxed_cells_in_expr(func) + count_boxed_cells_in_expr(arg)
        }
        SpecExprKind::Lambda { body, .. }
        | SpecExprKind::Abstract(_, body)
        | SpecExprKind::Dicts(body, _)
        | SpecExprKind::Not(body)
        | SpecExprKind::Unwrap(body)
        | SpecExprKind::Neg(_, body)
        | SpecExprKind::Ignore(body)
        | SpecExprKind::Await(body)
        | SpecExprKind::Task(body)
        | SpecExprKind::Within(_, body)
        | SpecExprKind::Lift(body, _)
        | SpecExprKind::Yield(body) => count_boxed_cells_in_expr(body),
        SpecExprKind::Block(stmts, tail) => {
            stmts
                .iter()
                .map(|s| match s {
                    SpecStmt::Let { value, .. }
                    | SpecStmt::Var { value, .. }
                    | SpecStmt::Assign(_, value)
                    | SpecStmt::Fun { value, .. }
                    | SpecStmt::Expr(value) => count_boxed_cells_in_expr(value),
                })
                .sum::<usize>()
                + count_boxed_cells_in_expr(tail)
        }
        SpecExprKind::If(c, t, e) => {
            count_boxed_cells_in_expr(c)
                + count_boxed_cells_in_expr(t)
                + count_boxed_cells_in_expr(e)
        }
        SpecExprKind::Match(s, arms) => {
            count_boxed_cells_in_expr(s)
                + arms
                    .iter()
                    .map(|a| {
                        a.guard.as_ref().map_or(0, count_boxed_cells_in_expr)
                            + count_boxed_cells_in_expr(&a.body)
                    })
                    .sum::<usize>()
        }
        SpecExprKind::Tuple(items) | SpecExprKind::List(items) => {
            items.iter().map(count_boxed_cells_in_expr).sum()
        }
        SpecExprKind::Record { fields, .. } | SpecExprKind::Dictionary(fields) => fields
            .iter()
            .map(|(_, v)| count_boxed_cells_in_expr(v))
            .sum(),
        SpecExprKind::Generate { body, limit, .. }
        | SpecExprKind::ControlLoop {
            body,
            condition: limit,
            ..
        } => {
            count_boxed_cells_in_expr(body) + limit.as_deref().map_or(0, count_boxed_cells_in_expr)
        }
        SpecExprKind::Each {
            stop, source, body, ..
        } => {
            stop.as_deref().map_or(0, count_boxed_cells_in_expr)
                + count_boxed_cells_in_expr(source)
                + count_boxed_cells_in_expr(body)
        }
        SpecExprKind::Loop { condition, body } => {
            count_boxed_cells_in_expr(condition) + count_boxed_cells_in_expr(body)
        }
        SpecExprKind::LoopExit { value, .. } => {
            value.as_deref().map_or(0, count_boxed_cells_in_expr)
        }
        SpecExprKind::Handle(h) => {
            h.effects
                .iter()
                .flat_map(|e| &e.operations)
                .map(|o| count_boxed_cells_in_expr(&o.function))
                .sum::<usize>()
                + count_boxed_cells_in_expr(&h.body)
                + h.ret.as_ref().map_or(0, count_boxed_cells_in_expr)
                + h.errors
                    .iter()
                    .map(|a| {
                        a.guard.as_ref().map_or(0, count_boxed_cells_in_expr)
                            + count_boxed_cells_in_expr(&a.body)
                    })
                    .sum::<usize>()
        }
        SpecExprKind::CallDirect { .. }
        | SpecExprKind::CallFallback { .. }
        | SpecExprKind::CallForeign { .. }
        | SpecExprKind::MethodCall { .. }
        | SpecExprKind::Construct { .. }
        | SpecExprKind::Lit(_)
        | SpecExprKind::Local(_)
        | SpecExprKind::Var(_)
        | SpecExprKind::Perform { .. }
        | SpecExprKind::ServiceTag(_)
        | SpecExprKind::Label(_)
        | SpecExprKind::ReadOwnerEvidence => 0,
    }
}

pub(super) fn report_artifact(
    program: &Program,
    specialized: &SpecializedProgram,
    request: &BuildRequest,
) -> Result<Artifact, Vec<Diagnostic>> {
    let layouts = collect_report_layouts(program, specialized, request)?;

    let mut fallback_reasons: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    for def in specialized.definitions.values() {
        if let DefMode::Fallback(reason) = def.mode {
            let key = format!("{}.{}", def.source.0, def.source.1);
            fallback_reasons
                .entry(key)
                .or_default()
                .insert(fallback_reason_str(reason));
        }
    }
    for f in specialized.foreigns.values() {
        if let Some(reason) = f.reason {
            let key = format!("{}.{}", f.module, f.name);
            fallback_reasons
                .entry(key)
                .or_default()
                .insert(fallback_reason_str(reason));
        }
    }
    for d in &specialized.diagnostics {
        if let DiagnosticKind::FallbackSelected(reason) = d.kind {
            let key = format!("{}.{}", d.def.0, d.def.1);
            fallback_reasons
                .entry(key)
                .or_default()
                .insert(fallback_reason_str(reason));
        }
    }

    let mut classifications = Map::new();
    for (key, reasons) in &fallback_reasons {
        let list: Vec<&str> = reasons.iter().copied().collect();
        classifications.insert(key.clone(), json!({ "fallback": list }));
    }

    let mut specializations = Vec::new();
    let mut per_fn_boxed = Map::new();
    for (sym, def) in &specialized.definitions {
        let boxed = count_boxed_cells_in_expr(&def.body);
        per_fn_boxed.insert(sym.as_str().to_owned(), json!(boxed));
        if matches!(def.mode, DefMode::Specialized) {
            specializations.push(json!({
                "symbol": sym.as_str(),
                "module": def.source.0,
                "name": def.source.1,
                "type_args": def.type_args.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "runtime_dicts": def.runtime_dicts,
            }));
        }
    }

    let instances_per_def: Map<String, Value> = specialized
        .stats
        .instances_per_def
        .iter()
        .map(|((m, n), c)| (format!("{m}.{n}"), json!(c)))
        .collect();

    let report = json!({
        "ir_version": program.ir_version,
        "mode": "whole-program",
        "profile": format!("{:?}", request.target.profile).to_lowercase(),
        "layouts": layouts,
        "stats": {
            "specialized_instances": specialized.stats.specialized_instances,
            "fallback_definitions": specialized.stats.fallback_definitions,
            "polymorphic_foreigns": specialized.stats.polymorphic_foreigns,
            "box_count": specialized.stats.box_count,
            "unbox_count": specialized.stats.unbox_count,
            "boxed_cells": specialized.stats.boxed_cells,
            "direct_dict_calls": specialized.stats.direct_dict_calls,
            "runtime_dict_calls": specialized.stats.runtime_dict_calls,
            "offset_field_reads": specialized.stats.offset_field_reads,
            "label_field_reads": specialized.stats.label_field_reads,
            "estimated_output_bytes": specialized.stats.estimated_output_bytes,
            "per_function_boxed_cells": per_fn_boxed,
            "instances_per_def": instances_per_def,
        },
        "classifications": classifications,
        "specializations": specializations,
    });

    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|e| diagnostic(&format!("layout-specialize report json failed: {e}"), None))?;
    Ok(Artifact {
        id: "layout-specialize-report".into(),
        path: "layout-specialize-report.json".into(),
        kind: ArtifactKind::Metadata,
        bytes,
    })
}
