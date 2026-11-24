//! Emission of `adt_types.go`: Go value structs, `Box_A_*` / `As_A_*` erased
//! boundary adapters, and typed `Eq_A_*` / `EqualWith_A_*` helpers for every
//! by-value closed ADT reached by a program.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};

use super::adt::{AdtField, AdtOps, AdtShape, AdtSlot, AdtVariant};
use super::adt_ops_emit::{
    write_hash, write_ord, write_record_cmp, write_record_hash, write_record_show, write_show,
    write_tag,
};
use super::arith::Native;
use super::go_type::{OptPayload, OptRepr};
use super::record::RecordShape;
use super::{names, own};

/// Emits `adt_types.go` declaring every reached closed ADT value struct and
/// its boundary / derived helpers.
pub(super) fn file(
    shapes: &BTreeMap<AdtShape, AdtOps>,
    records: &own::Records,
    ctor_fields: &own::CtorFields,
) -> String {
    let mut go = String::from("package main\n\nimport \"flick.app/flickrt\"\n");
    let mut rec_ord: BTreeSet<RecordShape> = BTreeSet::new();
    let mut rec_hash: BTreeSet<RecordShape> = BTreeSet::new();
    let mut rec_show: BTreeSet<RecordShape> = BTreeSet::new();

    for (shape, ops) in shapes {
        if ops.has(AdtOps::ORD) || ops.has(AdtOps::HASH) || ops.has(AdtOps::SHOW) {
            for slot in &shape.slots {
                if let AdtSlot::Record(rec) = slot {
                    if ops.has(AdtOps::ORD) {
                        rec_ord.insert(rec.clone());
                    }
                    if ops.has(AdtOps::HASH) {
                        rec_hash.insert(rec.clone());
                    }
                    if ops.has(AdtOps::SHOW) {
                        rec_show.insert(rec.clone());
                    }
                }
            }
        }
        let _ = write_shape(&mut go, shape, *ops, records, ctor_fields);
    }

    for rec in &rec_ord {
        let _ = write_record_cmp(&mut go, rec);
    }
    for rec in &rec_hash {
        let _ = write_record_hash(&mut go, rec);
    }
    for rec in &rec_show {
        let _ = write_record_show(&mut go, rec);
    }
    go
}

fn own_held(
    shape: &AdtShape,
    variant: &AdtVariant,
    records: &own::Records,
    ctor_fields: &own::CtorFields,
) -> own::Held {
    if variant.fields.is_empty() {
        return own::Held::Zero;
    }
    if variant.fields.len() == 1
        && let Some(rec) = records.get(&(shape.module.clone(), variant.name.clone()))
    {
        return own::Held::Record(rec.clone());
    }
    let slots = ctor_fields
        .get(&(
            shape.module.clone(),
            shape.name.clone(),
            variant.name.clone(),
        ))
        .filter(|s| s.len() == variant.fields.len())
        .cloned()
        .unwrap_or_else(|| vec![None; variant.fields.len()]);
    own::Held::Fields(slots)
}

fn write_shape(
    go: &mut String,
    shape: &AdtShape,
    ops: AdtOps,
    records: &own::Records,
    ctor_fields: &own::CtorFields,
) -> fmt::Result {
    let ty = shape.go_type();
    if shape.slots.is_empty() && !shape.has_discriminant() {
        writeln!(go, "\ntype {ty} struct{{}}")?;
    } else {
        writeln!(go, "\ntype {ty} struct {{")?;
        for (i, slot) in shape.slots.iter().enumerate() {
            writeln!(go, "\tf{i} {}", slot.go_type())?;
        }
        if shape.has_discriminant() {
            writeln!(go, "\ttag uint8")?;
        }
        writeln!(go, "}}")?;
    }
    write_box(go, shape, &ty, records, ctor_fields)?;
    write_unbox(go, shape, &ty, records, ctor_fields)?;
    if !shape.is_go_comparable() {
        write_eq(go, shape, &ty, false)?;
    }
    if ops.has(AdtOps::EQ_WITH) {
        write_eq(go, shape, &ty, true)?;
    }
    if ops.has(AdtOps::ORD) {
        write_ord(go, shape, &ty)?;
    }
    if ops.has(AdtOps::HASH) {
        write_hash(go, shape, &ty)?;
    }
    if ops.has(AdtOps::SHOW) {
        write_show(go, shape, &ty)?;
    }
    if ops.has(AdtOps::TAG) {
        write_tag(go, shape, &ty)?;
    }
    Ok(())
}

fn write_box(
    go: &mut String,
    shape: &AdtShape,
    ty: &str,
    records: &own::Records,
    ctor_fields: &own::CtorFields,
) -> fmt::Result {
    let box_fn = shape.box_fn();
    writeln!(go, "\nfunc {box_fn}(v {ty}) flickrt.Value {{")?;
    if shape.has_discriminant() {
        writeln!(go, "\tswitch v.tag {{")?;
    }
    for (vi, variant) in shape.variants.iter().enumerate() {
        let indent = if shape.has_discriminant() {
            writeln!(go, "\tcase {vi}:")?;
            "\t\t"
        } else {
            "\t"
        };
        if shape.is_result {
            let helper = if variant.tag == "Success" {
                "flickrt.Ok"
            } else {
                "flickrt.Err"
            };
            let arg = variant_field_box_expr("v", &variant.fields[0]);
            writeln!(go, "{indent}return {helper}({arg})")?;
            continue;
        }
        let own_ty = names::own_type(&shape.module, &variant.name);
        match own_held(shape, variant, records, ctor_fields) {
            own::Held::Zero => {
                writeln!(go, "{indent}return flickrt.Value({own_ty}{{}})")?;
            }
            own::Held::Record(own_rec) => {
                let field = &variant.fields[0];
                let raw = field_read("v", field.pool);
                if matches!(&field.slot, AdtSlot::Record(r) if *r == own_rec) {
                    writeln!(go, "{indent}return {}({raw})", own::make(&own_ty))?;
                } else {
                    writeln!(go, "{indent}return {}({raw})", own::of(&own_ty))?;
                }
            }
            own::Held::Fields(own_slots) => {
                let args: Vec<String> = variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(k, field)| {
                        if own_slots[k].is_some() {
                            field_read("v", field.pool)
                        } else {
                            variant_field_box_expr("v", field)
                        }
                    })
                    .collect();
                writeln!(
                    go,
                    "{indent}return {}({})",
                    own::make(&own_ty),
                    args.join(", ")
                )?;
            }
        }
    }
    if shape.has_discriminant() {
        writeln!(
            go,
            "\t}}\n\tpanic(flickrt.Defect{{Message: \"invalid ADT tag\"}})"
        )?;
    }
    writeln!(go, "}}")
}

fn write_unbox(
    go: &mut String,
    shape: &AdtShape,
    ty: &str,
    records: &own::Records,
    ctor_fields: &own::CtorFields,
) -> fmt::Result {
    let as_fn = shape.unbox_fn();
    writeln!(
        go,
        "\nfunc {as_fn}(v flickrt.Value) {ty} {{\n\tif v == nil {{\n\t\treturn {ty}{{}}\n\t}}"
    )?;
    if shape.is_result {
        writeln!(go, "\tif d, ok := v.(*flickrt.Data); ok {{")?;
        write_tag_unbox_switch(go, shape, ty, "d.Tag", "d.Fields()", "\t\t")?;
        writeln!(go, "\t}}")?;
        writeln!(go, "\tc := v.(flickrt.Constructed)")?;
        write_tag_unbox_switch(go, shape, ty, "c.TagName()", "c.Fields()", "\t")?;
        return writeln!(
            go,
            "\tpanic(flickrt.Defect{{Message: \"unexpected Result tag\"}})\n}}"
        );
    }

    let uses_x = shape
        .variants
        .iter()
        .any(|v| v.fields.iter().any(|f| f.pool.is_some()));
    if uses_x {
        writeln!(go, "\tswitch x := v.(type) {{")?;
    } else {
        writeln!(go, "\tswitch v.(type) {{")?;
    }
    for (vi, variant) in shape.variants.iter().enumerate() {
        let own_ty = names::own_type(&shape.module, &variant.name);
        match own_held(shape, variant, records, ctor_fields) {
            own::Held::Zero => {
                writeln!(go, "\tcase {own_ty}:")?;
                writeln!(go, "\t\treturn {}", literal_init(shape, ty, vi, &[]))?;
            }
            own::Held::Record(own_rec) => {
                writeln!(go, "\tcase *{own_ty}:")?;
                let field = &variant.fields[0];
                let mut pairs = Vec::new();
                if let Some(pool) = field.pool {
                    let expr = if matches!(&field.slot, AdtSlot::Record(r) if *r == own_rec) {
                        "x.rec".to_owned()
                    } else {
                        slot_unbox_expr("x.rec", &field.slot)
                    };
                    pairs.push((pool, expr));
                }
                writeln!(go, "\t\treturn {}", literal_init(shape, ty, vi, &pairs))?;
            }
            own::Held::Fields(own_slots) => {
                writeln!(go, "\tcase *{own_ty}:")?;
                let has_scalars = own_slots.iter().any(Option::is_some);
                let mut pairs = Vec::new();
                for (k, field) in variant.fields.iter().enumerate() {
                    if let Some(pool) = field.pool {
                        let expr = if own_slots[k].is_some() {
                            format!("x.f{k}")
                        } else if has_scalars {
                            slot_unbox_expr(&format!("x.f{k}"), &field.slot)
                        } else {
                            slot_unbox_expr(&format!("x.f[{k}]"), &field.slot)
                        };
                        pairs.push((pool, expr));
                    }
                }
                writeln!(go, "\t\treturn {}", literal_init(shape, ty, vi, &pairs))?;
            }
        }
    }
    writeln!(go, "\t}}")?;
    writeln!(go, "\tc := v.(flickrt.Constructed)")?;
    write_tag_unbox_switch(go, shape, ty, "c.TagName()", "c.Fields()", "\t")?;
    writeln!(
        go,
        "\tpanic(flickrt.Defect{{Message: \"unexpected ADT tag\"}})\n}}"
    )
}

fn write_tag_unbox_switch(
    go: &mut String,
    shape: &AdtShape,
    ty: &str,
    tag_expr: &str,
    fields_expr: &str,
    indent: &str,
) -> fmt::Result {
    let any_fields = shape
        .variants
        .iter()
        .any(|v| v.fields.iter().any(|f| f.pool.is_some()));
    writeln!(go, "{indent}switch {tag_expr} {{")?;
    for (vi, variant) in shape.variants.iter().enumerate() {
        let q = names::string(&variant.tag);
        writeln!(go, "{indent}case {q}:")?;
        let has_pooled = variant.fields.iter().any(|f| f.pool.is_some());
        let mut pairs = Vec::new();
        if has_pooled && any_fields {
            writeln!(go, "{indent}\tfs := {fields_expr}")?;
            for (k, field) in variant.fields.iter().enumerate() {
                if let Some(pool) = field.pool {
                    pairs.push((pool, slot_unbox_expr(&format!("fs[{k}]"), &field.slot)));
                }
            }
        }
        writeln!(
            go,
            "{indent}\treturn {}",
            literal_init(shape, ty, vi, &pairs)
        )?;
    }
    writeln!(go, "{indent}}}")
}

fn literal_init(shape: &AdtShape, ty: &str, vi: usize, fields: &[(usize, String)]) -> String {
    let mut parts = Vec::with_capacity(fields.len() + 1);
    if shape.has_discriminant() && (vi != 0 || !fields.is_empty()) {
        parts.push(format!("tag: {vi}"));
    }
    for (pool, expr) in fields {
        parts.push(format!("f{pool}: {expr}"));
    }
    if parts.is_empty() {
        format!("{ty}{{}}")
    } else {
        format!("{ty}{{{}}}", parts.join(", "))
    }
}

fn write_eq(go: &mut String, shape: &AdtShape, ty: &str, strict_param: bool) -> fmt::Result {
    let (fn_name, params) = if strict_param {
        (shape.equal_with_fn(), format!("a, b {ty}, strictZero bool"))
    } else {
        (shape.eq_fn(), format!("a, b {ty}"))
    };
    writeln!(go, "\nfunc {fn_name}({params}) bool {{")?;
    if shape.has_discriminant() {
        writeln!(
            go,
            "\tif a.tag != b.tag {{\n\t\treturn false\n\t}}\n\tswitch a.tag {{"
        )?;
    }
    for (vi, variant) in shape.variants.iter().enumerate() {
        let indent = if shape.has_discriminant() {
            writeln!(go, "\tcase {vi}:")?;
            "\t\t"
        } else {
            "\t"
        };
        let parts: Vec<String> = variant
            .fields
            .iter()
            .filter_map(|f| {
                let pool = f.pool?;
                let lhs = format!("a.f{pool}");
                let rhs = format!("b.f{pool}");
                Some(if strict_param {
                    slot_equal_with_expr(&lhs, &rhs, &f.slot)
                } else {
                    slot_eq_expr(&lhs, &rhs, &f.slot)
                })
            })
            .collect();
        let cond = if parts.is_empty() {
            "true".to_owned()
        } else {
            parts.join(" && ")
        };
        writeln!(go, "{indent}return {cond}")?;
    }
    if shape.has_discriminant() {
        writeln!(go, "\t}}\n\treturn false")?;
    }
    writeln!(go, "}}")
}

fn field_read(var: &str, pool: Option<usize>) -> String {
    match pool {
        Some(idx) => format!("{var}.f{idx}"),
        None => "flickrt.Unit".to_owned(),
    }
}

fn variant_field_box_expr(var: &str, field: &AdtField) -> String {
    match field.pool {
        Some(idx) => slot_box_expr(&format!("{var}.f{idx}"), &field.slot),
        None => "flickrt.Unit".to_owned(),
    }
}

fn slot_box_expr(expr: &str, slot: &AdtSlot) -> String {
    match slot {
        AdtSlot::Scalar(_) | AdtSlot::Record(_) => expr.to_owned(),
        AdtSlot::Unit => "flickrt.Unit".to_owned(),
        AdtSlot::Adt(a) => format!("{}({expr})", a.box_fn()),
        AdtSlot::Opt(r) => opt_box_expr(expr, r),
    }
}

fn opt_box_expr(expr: &str, repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => format!("flickrt.BoxOptBool({expr})"),
        OptRepr::Flag(OptPayload::Int) => format!("flickrt.BoxOptInt({expr})"),
        OptRepr::Flag(OptPayload::Float) => format!("flickrt.BoxOptFloat({expr})"),
        OptRepr::Flag(OptPayload::String) => format!("flickrt.BoxOptString({expr})"),
        OptRepr::Flag(OptPayload::Nested(inner)) => {
            format!("flickrt.BoxOpt({expr}, {})", opt_box_fn(inner))
        }
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn opt_box_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => "flickrt.BoxOptBool".to_owned(),
        OptRepr::Flag(OptPayload::Int) => "flickrt.BoxOptInt".to_owned(),
        OptRepr::Flag(OptPayload::Float) => "flickrt.BoxOptFloat".to_owned(),
        OptRepr::Flag(OptPayload::String) => "flickrt.BoxOptString".to_owned(),
        OptRepr::Flag(OptPayload::Nested(_)) => format!(
            "func(v {}) flickrt.Value {{ return {} }}",
            repr.go_type(),
            opt_box_expr("v", repr)
        ),
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn slot_unbox_expr(expr: &str, slot: &AdtSlot) -> String {
    match slot {
        AdtSlot::Scalar(n) => format!("flickrt.Field{}({expr})", n.name()),
        AdtSlot::Record(r) => format!("{}({expr})", r.unbox_fn()),
        AdtSlot::Adt(a) => format!("{}({expr})", a.unbox_fn()),
        AdtSlot::Opt(r) => opt_unbox_expr(expr, r),
        AdtSlot::Unit => "flickrt.UnitType{}".to_owned(),
    }
}

fn opt_unbox_expr(expr: &str, repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => format!("flickrt.FieldOptBool({expr})"),
        OptRepr::Flag(OptPayload::Int) => format!("flickrt.FieldOptInt({expr})"),
        OptRepr::Flag(OptPayload::Float) => format!("flickrt.FieldOptFloat({expr})"),
        OptRepr::Flag(OptPayload::String) => format!("flickrt.FieldOptString({expr})"),
        OptRepr::Flag(OptPayload::Nested(inner)) => {
            format!("flickrt.FieldOpt({expr}, {})", opt_unbox_fn(inner))
        }
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn opt_unbox_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => "flickrt.FieldOptBool".to_owned(),
        OptRepr::Flag(OptPayload::Int) => "flickrt.FieldOptInt".to_owned(),
        OptRepr::Flag(OptPayload::Float) => "flickrt.FieldOptFloat".to_owned(),
        OptRepr::Flag(OptPayload::String) => "flickrt.FieldOptString".to_owned(),
        OptRepr::Flag(OptPayload::Nested(_)) => format!(
            "func(v flickrt.Value) {} {{ return {} }}",
            repr.go_type(),
            opt_unbox_expr("v", repr)
        ),
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn slot_eq_expr(lhs: &str, rhs: &str, slot: &AdtSlot) -> String {
    if slot.is_go_comparable() {
        return format!("{lhs} == {rhs}");
    }
    match slot {
        AdtSlot::Scalar(Native::Float) => format!("flickrt.EqFloat({lhs}, {rhs})"),
        AdtSlot::Scalar(Native::Float32) => format!("flickrt.EqFloat32({lhs}, {rhs})"),
        AdtSlot::Record(r) => format!("{}({lhs}, {rhs})", r.eq_fn()),
        AdtSlot::Adt(a) => format!("{}({lhs}, {rhs})", a.eq_fn()),
        AdtSlot::Opt(r) => opt_eq_expr(lhs, rhs, r),
        _ => format!("{lhs} == {rhs}"),
    }
}

fn opt_eq_expr(lhs: &str, rhs: &str, repr: &OptRepr) -> String {
    if repr.is_go_comparable() {
        return format!("{lhs} == {rhs}");
    }
    match repr {
        OptRepr::Flag(OptPayload::Float) => format!("flickrt.EqOptFloat({lhs}, {rhs})"),
        OptRepr::Flag(OptPayload::Nested(inner)) => {
            format!("flickrt.EqOpt({lhs}, {rhs}, {})", opt_eq_fn(inner))
        }
        _ => unreachable!(),
    }
}

fn opt_eq_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::Flag(OptPayload::Float) => "flickrt.EqOptFloat".to_owned(),
        _ => format!(
            "func(a, b {}) bool {{ return {} }}",
            repr.go_type(),
            opt_eq_expr("a", "b", repr)
        ),
    }
}

fn slot_equal_with_expr(lhs: &str, rhs: &str, slot: &AdtSlot) -> String {
    if slot.is_go_comparable() {
        return format!("{lhs} == {rhs}");
    }
    match slot {
        AdtSlot::Scalar(Native::Float) => format!("flickrt.EqualFloat({lhs}, {rhs}, strictZero)"),
        AdtSlot::Scalar(Native::Float32) => {
            format!("flickrt.EqualFloat32({lhs}, {rhs}, strictZero)")
        }
        AdtSlot::Record(r) => {
            let parts: Vec<String> = r
                .fields
                .iter()
                .enumerate()
                .map(|(i, (_, fslot))| {
                    super::record::equal_with_expr(
                        &format!("{lhs}.f{i}"),
                        &format!("{rhs}.f{i}"),
                        fslot,
                    )
                })
                .collect();
            if parts.is_empty() {
                "true".to_owned()
            } else {
                format!("({})", parts.join(" && "))
            }
        }
        AdtSlot::Adt(a) => format!("{}({lhs}, {rhs}, strictZero)", a.equal_with_fn()),
        AdtSlot::Opt(r) => opt_equal_with_expr(lhs, rhs, r),
        _ => format!("{lhs} == {rhs}"),
    }
}

fn opt_equal_with_expr(lhs: &str, rhs: &str, repr: &OptRepr) -> String {
    if repr.is_go_comparable() {
        return format!("{lhs} == {rhs}");
    }
    match repr {
        OptRepr::Flag(OptPayload::Float) => {
            format!("flickrt.EqualWithOptFloat({lhs}, {rhs}, strictZero)")
        }
        OptRepr::Flag(OptPayload::Nested(inner)) => format!(
            "flickrt.EqualWithOpt({lhs}, {rhs}, {}, strictZero)",
            opt_equal_with_fn(inner)
        ),
        _ => unreachable!(),
    }
}

fn opt_equal_with_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::Flag(OptPayload::Float) => "flickrt.EqualWithOptFloat".to_owned(),
        _ => format!(
            "func(a, b {}, strictZero bool) bool {{ return {} }}",
            repr.go_type(),
            opt_equal_with_expr("a", "b", repr)
        ),
    }
}
