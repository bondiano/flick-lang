//! Emission of `record_types.go`: Go struct declarations, `flickrt.RecordView`
//! and `flickrt.RecordEqual` methods, typed `Eq_R_*` equality, and `As_R_*`
//! runtime-boundary adapters for every closed record shape in a program.

use std::collections::BTreeSet;
use std::fmt::{self, Write};

use super::arith::Native;
use super::names;
use super::record::{RecordShape, RecordSlot, equal_with_expr};

fn eq_expr(lhs: &str, rhs: &str, slot: &RecordSlot) -> String {
    match slot {
        RecordSlot::Scalar(Native::Float) => format!("flickrt.EqFloat({lhs}, {rhs})"),
        RecordSlot::Scalar(Native::Float32) => format!("flickrt.EqFloat32({lhs}, {rhs})"),
        RecordSlot::Scalar(_) => format!("{lhs} == {rhs}"),
        RecordSlot::Adt(a) if a.is_go_comparable() => format!("{lhs} == {rhs}"),
        RecordSlot::Adt(a) => format!("{}({lhs}, {rhs})", a.eq_fn()),
        RecordSlot::Value => format!("flickrt.Equals({lhs}, {rhs})"),
    }
}

fn view_field_expr(i: usize, slot: &RecordSlot) -> String {
    match slot {
        RecordSlot::Adt(a) => format!("{}(r.f{i})", a.box_fn()),
        RecordSlot::Scalar(_) | RecordSlot::Value => format!("r.f{i}"),
    }
}

/// Emits `record_types.go` declaring every closed record shape reached by the program.
pub(super) fn file(shapes: &BTreeSet<RecordShape>) -> String {
    let mut go = String::from("package main\n\nimport \"flick.app/flickrt\"\n");
    for shape in shapes {
        let _ = write_shape(&mut go, shape);
    }
    go
}

fn write_shape(go: &mut String, shape: &RecordShape) -> fmt::Result {
    let ty = shape.go_type();
    let keys_var = format!("k_{ty}");
    let as_fn = shape.unbox_fn();
    if shape.fields.is_empty() {
        return write!(
            go,
            "\ntype {ty} struct{{}}\n\n\
             var {keys_var} = []string{{}}\n\n\
             func ({ty}) Get(string) flickrt.Value {{ return nil }}\n\n\
             func ({ty}) Lookup(string) (flickrt.Value, bool) {{ return nil, false }}\n\n\
             func ({ty}) Keys() []string {{ return {keys_var} }}\n\n\
             func ({ty}) Len() int {{ return 0 }}\n\n\
             func ({ty}) At(int) flickrt.Value {{ return nil }}\n\n\
             func ({ty}) EqualRecord(other flickrt.Value, _ bool) (bool, bool) {{\n\
             \t_, ok := other.({ty})\n\treturn ok, ok\n}}\n\n\
             func {as_fn}(v flickrt.Value) {ty} {{\n\
             \tif r, ok := v.({ty}); ok {{\n\t\treturn r\n\t}}\n\
             \tif v != nil {{\n\t\t_ = flickrt.AsRecord(v)\n\t}}\n\
             \treturn {ty}{{}}\n}}\n",
        );
    }

    let mut order: Vec<usize> = (0..shape.fields.len()).collect();
    order.sort_by_key(|&i| (shape.fields[i].1.layout_rank(), i));
    writeln!(go, "\ntype {ty} struct {{")?;
    for i in order {
        let go_ty = shape.fields[i].1.go_type();
        writeln!(go, "\tf{i} {go_ty}")?;
    }
    writeln!(go, "}}")?;

    let quoted: Vec<String> = shape.fields.iter().map(|(l, _)| names::string(l)).collect();
    writeln!(go, "\nvar {keys_var} = []string{{{}}}", quoted.join(", "))?;
    write_view_methods(go, shape, &ty, &keys_var, &quoted)?;
    write_eq_and_adapter(go, shape, &ty, &as_fn, &quoted)
}

fn write_view_methods(
    go: &mut String,
    shape: &RecordShape,
    ty: &str,
    keys_var: &str,
    quoted: &[String],
) -> fmt::Result {
    writeln!(
        go,
        "\nfunc (r {ty}) Get(k string) flickrt.Value {{\n\tswitch k {{"
    )?;
    for (i, q) in quoted.iter().enumerate() {
        let val = view_field_expr(i, &shape.fields[i].1);
        writeln!(go, "\tcase {q}:\n\t\treturn {val}")?;
    }
    writeln!(go, "\t}}\n\treturn nil\n}}")?;

    writeln!(
        go,
        "\nfunc (r {ty}) Lookup(k string) (flickrt.Value, bool) {{\n\tswitch k {{"
    )?;
    for (i, q) in quoted.iter().enumerate() {
        let val = view_field_expr(i, &shape.fields[i].1);
        writeln!(go, "\tcase {q}:\n\t\treturn {val}, true")?;
    }
    writeln!(go, "\t}}\n\treturn nil, false\n}}")?;

    writeln!(go, "\nfunc ({ty}) Keys() []string {{ return {keys_var} }}")?;
    writeln!(go, "\nfunc ({ty}) Len() int {{ return {} }}", quoted.len())?;

    writeln!(
        go,
        "\nfunc (r {ty}) At(i int) flickrt.Value {{\n\tswitch i {{"
    )?;
    for i in 0..quoted.len() {
        let val = view_field_expr(i, &shape.fields[i].1);
        writeln!(go, "\tcase {i}:\n\t\treturn {val}")?;
    }
    writeln!(go, "\t}}\n\treturn nil\n}}")
}

fn write_eq_and_adapter(
    go: &mut String,
    shape: &RecordShape,
    ty: &str,
    as_fn: &str,
    quoted: &[String],
) -> fmt::Result {
    let eq_parts: Vec<String> = shape
        .fields
        .iter()
        .enumerate()
        .map(|(i, (_, slot))| equal_with_expr(&format!("r.f{i}"), &format!("q.f{i}"), slot))
        .collect();
    writeln!(
        go,
        "\nfunc (r {ty}) EqualRecord(other flickrt.Value, strictZero bool) (bool, bool) {{\n\
         \tq, ok := other.({ty})\n\
         \tif !ok {{\n\t\treturn false, false\n\t}}\n\
         \treturn {}, true\n}}",
        eq_parts.join(" && ")
    )?;

    if !shape.is_go_comparable() {
        let eq_fn = shape.eq_fn();
        let parts: Vec<String> = shape
            .fields
            .iter()
            .enumerate()
            .map(|(i, (_, slot))| eq_expr(&format!("a.f{i}"), &format!("b.f{i}"), slot))
            .collect();
        writeln!(
            go,
            "\nfunc {eq_fn}(a, b {ty}) bool {{\n\treturn {}\n}}",
            parts.join(" && ")
        )?;
    }

    writeln!(
        go,
        "\nfunc {as_fn}(v flickrt.Value) {ty} {{\n\
         \tif r, ok := v.({ty}); ok {{\n\t\treturn r\n\t}}\n\
         \tif v == nil {{\n\t\treturn {ty}{{}}\n\t}}\n\
         \trv := flickrt.AsRecord(v)\n\
         \treturn {ty}{{"
    )?;
    for (i, ((_, slot), q)) in shape.fields.iter().zip(quoted).enumerate() {
        match slot {
            RecordSlot::Scalar(native) => {
                writeln!(go, "\t\tf{i}: flickrt.Field{}(rv.Get({q})),", native.name())?;
            }
            RecordSlot::Adt(adt) => writeln!(go, "\t\tf{i}: {}(rv.Get({q})),", adt.unbox_fn())?,
            RecordSlot::Value => writeln!(go, "\t\tf{i}: rv.Get({q}),")?,
        }
    }
    writeln!(go, "\t}}\n}}")
}
