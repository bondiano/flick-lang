//! Emission of on-demand derived `Cmp_A_*` / `Ord_A_*` / `Hash_A_*` /
//! `Show_A_*` / `Tag_A_*` helpers and nested record helpers for `adt_types.go`.

use std::fmt::{self, Write};

use super::adt::{AdtShape, AdtSlot};
use super::arith::Native;
use super::go_type::{OptPayload, OptRepr};
use super::names;
use super::record::{RecordShape, RecordSlot};

pub(super) fn write_ord(go: &mut String, shape: &AdtShape, ty: &str) -> fmt::Result {
    let cmp_fn = shape.cmp_fn();
    let ord_fn = shape.ord_fn();
    writeln!(go, "\nfunc {cmp_fn}(a, b {ty}) int {{")?;
    if shape.has_discriminant() {
        writeln!(
            go,
            "\tif a.tag < b.tag {{\n\t\treturn -1\n\t}}\n\tif a.tag > b.tag {{\n\t\treturn 1\n\t}}"
        )?;
        let any_fields = shape
            .variants
            .iter()
            .any(|v| v.fields.iter().any(|f| f.pool.is_some()));
        if !any_fields {
            return writeln!(
                go,
                "\treturn 0\n}}\n\nfunc {ord_fn}(a, b {ty}) flickrt.Value {{\n\treturn flickrt.Ordering({cmp_fn}(a, b))\n}}"
            );
        }
        writeln!(go, "\tswitch a.tag {{")?;
    }
    for (vi, variant) in shape.variants.iter().enumerate() {
        let indent = if shape.has_discriminant() {
            writeln!(go, "\tcase {vi}:")?;
            "\t\t"
        } else {
            "\t"
        };
        for f in &variant.fields {
            if let Some(pool) = f.pool {
                let expr = slot_cmp_expr(&format!("a.f{pool}"), &format!("b.f{pool}"), &f.slot);
                writeln!(
                    go,
                    "{indent}if c := {expr}; c != 0 {{\n{indent}\treturn c\n{indent}}}"
                )?;
            }
        }
        writeln!(go, "{indent}return 0")?;
    }
    if shape.has_discriminant() {
        writeln!(go, "\t}}\n\treturn 0")?;
    }
    writeln!(
        go,
        "}}\n\nfunc {ord_fn}(a, b {ty}) flickrt.Value {{\n\treturn flickrt.Ordering({cmp_fn}(a, b))\n}}"
    )
}

pub(super) fn write_hash(go: &mut String, shape: &AdtShape, ty: &str) -> fmt::Result {
    let hash_fn = shape.hash_fn();
    writeln!(go, "\nfunc {hash_fn}(v {ty}) int32 {{")?;
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
            let f = &variant.fields[0];
            let h = field_hash_expr("v", f.pool, &f.slot);
            if variant.tag == "Success" {
                writeln!(go, "{indent}return {h}")?;
            } else {
                writeln!(go, "{indent}return int32(1) + {h}")?;
            }
            continue;
        }
        if variant.fields.is_empty() {
            writeln!(go, "{indent}return int32({vi})")?;
            continue;
        }
        writeln!(go, "{indent}h := int32({vi})")?;
        for f in &variant.fields {
            let h = field_hash_expr("v", f.pool, &f.slot);
            writeln!(go, "{indent}h = flickrt.CombineHash(h, {h})")?;
        }
        writeln!(go, "{indent}return h")?;
    }
    if shape.has_discriminant() {
        writeln!(go, "\t}}\n\treturn 0")?;
    }
    writeln!(go, "}}")
}

pub(super) fn write_show(go: &mut String, shape: &AdtShape, ty: &str) -> fmt::Result {
    let show_fn = shape.show_fn();
    writeln!(go, "\nfunc {show_fn}(v {ty}) string {{")?;
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
        // Flattened inline submodule constructors carry a `Sub$` prefix;
        // derived `Show` displays only the unqualified constructor name.
        let display_name = variant.name.rsplit('$').next().unwrap_or(&variant.name);
        let mut expr = names::string(display_name);
        for f in &variant.fields {
            let shown = field_show_expr("v", f.pool, &f.slot);
            let _ = write!(expr, " + \" \" + flickrt.ShowArg({shown})");
        }
        writeln!(go, "{indent}return {expr}")?;
    }
    if shape.has_discriminant() {
        writeln!(go, "\t}}\n\treturn \"\"")?;
    }
    writeln!(go, "}}")
}

pub(super) fn write_tag(go: &mut String, shape: &AdtShape, ty: &str) -> fmt::Result {
    let tag_fn = shape.tag_fn();
    writeln!(go, "\nfunc {tag_fn}(v {ty}) string {{")?;
    if !shape.has_discriminant() {
        return writeln!(go, "\treturn {}\n}}", names::string(&shape.variants[0].tag));
    }
    writeln!(go, "\tswitch v.tag {{")?;
    for (vi, variant) in shape.variants.iter().enumerate() {
        writeln!(
            go,
            "\tcase {vi}:\n\t\treturn {}",
            names::string(&variant.tag)
        )?;
    }
    writeln!(go, "\t}}\n\treturn \"\"\n}}")
}

fn record_utf16_order(rec: &RecordShape) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rec.fields.len()).collect();
    order.sort_by(|&a, &b| {
        rec.fields[a]
            .0
            .encode_utf16()
            .cmp(rec.fields[b].0.encode_utf16())
    });
    order
}

pub(super) fn write_record_cmp(go: &mut String, rec: &RecordShape) -> fmt::Result {
    let ty = rec.go_type();
    writeln!(go, "\nfunc Cmp_{ty}(a, b {ty}) int {{")?;
    for i in record_utf16_order(rec) {
        let cmp = match &rec.fields[i].1 {
            RecordSlot::Scalar(n) => scalar_cmp_expr(&format!("a.f{i}"), &format!("b.f{i}"), *n),
            RecordSlot::Adt(a) => format!("{}(a.f{i}, b.f{i})", a.cmp_fn()),
            RecordSlot::Value => unreachable!("AdtSlot::Record has no Value slots"),
        };
        writeln!(go, "\tif c := {cmp}; c != 0 {{\n\t\treturn c\n\t}}")?;
    }
    writeln!(go, "\treturn 0\n}}")
}

pub(super) fn write_record_hash(go: &mut String, rec: &RecordShape) -> fmt::Result {
    let ty = rec.go_type();
    writeln!(go, "\nfunc Hash_{ty}(v {ty}) int32 {{\n\th := int32(11)")?;
    for i in record_utf16_order(rec) {
        let h = match &rec.fields[i].1 {
            RecordSlot::Scalar(n) => format!("flickrt.Hash{}(v.f{i})", n.name()),
            RecordSlot::Adt(a) => format!("{}(v.f{i})", a.hash_fn()),
            RecordSlot::Value => unreachable!("AdtSlot::Record has no Value slots"),
        };
        writeln!(go, "\th = flickrt.CombineHash(h, {h})")?;
    }
    writeln!(go, "\treturn h\n}}")
}

pub(super) fn write_record_show(go: &mut String, rec: &RecordShape) -> fmt::Result {
    let ty = rec.go_type();
    writeln!(go, "\nfunc Show_{ty}(v {ty}) string {{")?;
    let order = record_utf16_order(rec);
    if order.is_empty() {
        return writeln!(go, "\treturn \"{{  }}\"\n}}");
    }
    let mut parts = Vec::with_capacity(order.len());
    for i in order {
        let label = &rec.fields[i].0;
        let shown = match &rec.fields[i].1 {
            RecordSlot::Scalar(n) => format!("flickrt.Show{}(v.f{i})", n.name()),
            RecordSlot::Adt(a) => format!("{}(v.f{i})", a.show_fn()),
            RecordSlot::Value => unreachable!("AdtSlot::Record has no Value slots"),
        };
        parts.push(format!(
            "{} + {shown}",
            names::string(&format!("{label} = "))
        ));
    }
    writeln!(
        go,
        "\treturn \"{{ \" + {} + \" }}\"\n}}",
        parts.join(" + \", \" + ")
    )
}

fn scalar_cmp_expr(lhs: &str, rhs: &str, n: Native) -> String {
    format!("flickrt.Cmp{}({lhs}, {rhs})", n.name())
}

fn slot_cmp_expr(lhs: &str, rhs: &str, slot: &AdtSlot) -> String {
    match slot {
        AdtSlot::Scalar(n) => scalar_cmp_expr(lhs, rhs, *n),
        AdtSlot::Record(r) => format!("Cmp_{}({lhs}, {rhs})", r.go_type()),
        AdtSlot::Adt(a) => format!("{}({lhs}, {rhs})", a.cmp_fn()),
        AdtSlot::Opt(r) => opt_cmp_expr(lhs, rhs, r),
        AdtSlot::Unit => "0".to_owned(),
    }
}

fn opt_cmp_expr(lhs: &str, rhs: &str, repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => format!("flickrt.CmpOptBool({lhs}, {rhs})"),
        OptRepr::Flag(OptPayload::Int) => format!("flickrt.CmpOpt({lhs}, {rhs}, flickrt.CmpInt)"),
        OptRepr::Flag(OptPayload::Float) => {
            format!("flickrt.CmpOpt({lhs}, {rhs}, flickrt.CmpFloat)")
        }
        OptRepr::Flag(OptPayload::String) => {
            format!("flickrt.CmpOpt({lhs}, {rhs}, flickrt.CmpString)")
        }
        OptRepr::Flag(OptPayload::Nested(inner)) => {
            format!("flickrt.CmpOpt({lhs}, {rhs}, {})", opt_cmp_fn(inner))
        }
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn opt_cmp_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => "flickrt.CmpOptBool".to_owned(),
        _ => format!(
            "func(a, b {}) int {{ return {} }}",
            repr.go_type(),
            opt_cmp_expr("a", "b", repr)
        ),
    }
}

fn field_hash_expr(var: &str, pool: Option<usize>, slot: &AdtSlot) -> String {
    let Some(idx) = pool else {
        return "int32(0)".to_owned();
    };
    let expr = format!("{var}.f{idx}");
    match slot {
        AdtSlot::Scalar(n) => format!("flickrt.Hash{}({expr})", n.name()),
        AdtSlot::Record(r) => format!("Hash_{}({expr})", r.go_type()),
        AdtSlot::Adt(a) => format!("{}({expr})", a.hash_fn()),
        AdtSlot::Opt(r) => opt_hash_expr(&expr, r),
        AdtSlot::Unit => "int32(0)".to_owned(),
    }
}

fn opt_hash_expr(expr: &str, repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => format!("flickrt.HashOptBool({expr})"),
        OptRepr::Flag(OptPayload::Int) => format!("flickrt.HashOpt({expr}, flickrt.HashInt)"),
        OptRepr::Flag(OptPayload::Float) => format!("flickrt.HashOpt({expr}, flickrt.HashFloat)"),
        OptRepr::Flag(OptPayload::String) => {
            format!("flickrt.HashOpt({expr}, flickrt.HashString)")
        }
        OptRepr::Flag(OptPayload::Nested(inner)) => {
            format!("flickrt.HashOpt({expr}, {})", opt_hash_fn(inner))
        }
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn opt_hash_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => "flickrt.HashOptBool".to_owned(),
        _ => format!(
            "func(v {}) int32 {{ return {} }}",
            repr.go_type(),
            opt_hash_expr("v", repr)
        ),
    }
}

fn field_show_expr(var: &str, pool: Option<usize>, slot: &AdtSlot) -> String {
    let Some(idx) = pool else {
        return "\"()\"".to_owned();
    };
    let expr = format!("{var}.f{idx}");
    match slot {
        AdtSlot::Scalar(n) => format!("flickrt.Show{}({expr})", n.name()),
        AdtSlot::Record(r) => format!("Show_{}({expr})", r.go_type()),
        AdtSlot::Adt(a) => format!("{}({expr})", a.show_fn()),
        AdtSlot::Opt(r) => opt_show_expr(&expr, r),
        AdtSlot::Unit => "\"()\"".to_owned(),
    }
}

fn opt_show_expr(expr: &str, repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => format!("flickrt.ShowOptBool({expr})"),
        OptRepr::Flag(OptPayload::Int) => format!("flickrt.ShowOpt({expr}, flickrt.ShowInt)"),
        OptRepr::Flag(OptPayload::Float) => format!("flickrt.ShowOpt({expr}, flickrt.ShowFloat)"),
        OptRepr::Flag(OptPayload::String) => {
            format!("flickrt.ShowOpt({expr}, flickrt.ShowString)")
        }
        OptRepr::Flag(OptPayload::Nested(inner)) => {
            format!("flickrt.ShowOpt({expr}, {})", opt_show_fn(inner))
        }
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => unreachable!(),
    }
}

fn opt_show_fn(repr: &OptRepr) -> String {
    match repr {
        OptRepr::BoolSentinel => "flickrt.ShowOptBool".to_owned(),
        _ => format!(
            "func(v {}) string {{ return {} }}",
            repr.go_type(),
            opt_show_expr("v", repr)
        ),
    }
}
