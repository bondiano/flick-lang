//! Target-independent bridge from [`TypeLayout`] (#742) to shared memory-IR
//! representations ([`ValueRep`] and [`CellLayout`], #744).
//!
//! Derives aggregate vs. heap-cell selection and typed-cell layout compatibility
//! descriptors directly from computed [`TypeLayout`]s without hand-maintained
//! type-name lists.

use flick_ir::IrType;
use flick_ir::memory::{CellLayout, ValueRep};

use super::{FieldLayout, FieldStorage, Primitive, TypeLayout, TypeRepr, VariantLayout};

/// Selects whether a monomorphic [`TypeLayout`] lowers as an unboxed by-value
/// aggregate (`ValueRep::Aggregate`) or as a heap-allocated / immediate cell
/// (`ValueRep::Cell`).
///
/// - Non-recursive ADTs with no pointer-indirected cycle fields and no held
///   GADT dictionaries lower as `ValueRep::Aggregate` (inline by value; boxed
///   descendants such as `String` or `List` remain counted objects owned
///   inline inside the aggregate).
/// - Non-empty tuples lower as `ValueRep::Aggregate`.
/// - Recursive ADTs (whose `#742` indirection pass selected at least one
///   `FieldStorage::Pointer` cycle/growing field), GADT types carrying runtime
///   dictionaries, and primitives/handles lower as `ValueRep::Cell` (with
///   derived [`CellLayout`] descriptors for typed-cell `Reuse` compatibility).
pub fn value_rep_of(layout: &TypeLayout) -> ValueRep {
    match &layout.repr {
        TypeRepr::Data(data) => {
            let has_dicts = data.variants.iter().any(|v| !v.dicts.is_empty());
            let has_indirection = !data.pointer_fields.is_empty();
            if !has_dicts && !has_indirection && !data.variants.is_empty() {
                ValueRep::Aggregate
            } else {
                ValueRep::Cell
            }
        }
        TypeRepr::Tuple(tuple) if !tuple.fields.is_empty() => ValueRep::Aggregate,
        TypeRepr::Tuple(_) | TypeRepr::Record(_) | TypeRepr::Primitive(_) => ValueRep::Cell,
    }
}

/// Derives an abstract [`CellLayout`] descriptor from a computed [`TypeLayout`].
///
/// Two types with the same logical field count but different sizes, alignments,
/// or inline-vs-boxed field ownership patterns produce distinct [`CellLayout`]s,
/// so typed-cell FBIP `Reuse` checks compatibility cleanly.
pub fn cell_layout_of(layout: &TypeLayout) -> CellLayout {
    let align = u32::try_from(layout.align).unwrap_or(u32::MAX);
    match &layout.repr {
        TypeRepr::Data(data) => {
            let capacity = data
                .variants
                .iter()
                .map(|v| v.fields.len() + v.dicts.len())
                .max()
                .unwrap_or(0);
            let variants_sig: Vec<String> = data
                .variants
                .iter()
                .map(variant_ownership_signature)
                .collect();
            CellLayout {
                capacity,
                align,
                ownership: format!("size={}:[{}]", layout.size, variants_sig.join("|")),
            }
        }
        TypeRepr::Tuple(tuple) => {
            let fields_sig: Vec<String> = tuple.fields.iter().map(field_ownership_tag).collect();
            CellLayout {
                capacity: tuple.fields.len(),
                align,
                ownership: format!("size={}:({})", layout.size, fields_sig.join(",")),
            }
        }
        TypeRepr::Record(record) => {
            let fields_sig: Vec<String> = record
                .fields
                .iter()
                .map(|rf| field_ownership_tag(&rf.field))
                .collect();
            CellLayout {
                capacity: record.fields.len(),
                align,
                ownership: format!("size={}:{{{}}}", layout.size, fields_sig.join(",")),
            }
        }
        TypeRepr::Primitive(prim) => CellLayout {
            capacity: 0,
            align,
            ownership: format!("size={}:{}", layout.size, primitive_ownership_tag(*prim)),
        },
    }
}

/// Derives an exact [`CellLayout`] for one constructor variant of a
/// [`TypeLayout`], matching the constructor's physical `fields.len() + dicts.len()`.
pub fn variant_cell_layout_of(layout: &TypeLayout, variant: &VariantLayout) -> CellLayout {
    let align = u32::try_from(layout.align).unwrap_or(u32::MAX);
    CellLayout {
        capacity: variant.fields.len() + variant.dicts.len(),
        align,
        ownership: format!(
            "size={}:{}",
            layout.size,
            variant_ownership_signature(variant)
        ),
    }
}

fn variant_ownership_signature(variant: &VariantLayout) -> String {
    let mut parts: Vec<String> = variant.fields.iter().map(field_ownership_tag).collect();
    for _ in 0..variant.dicts.len() {
        parts.push("dict".to_owned());
    }
    parts.join(",")
}

fn field_ownership_tag(field: &FieldLayout) -> String {
    if field.storage == FieldStorage::Pointer {
        return "ptr".to_owned();
    }
    match &*field.ty {
        IrType::Named { module, name, .. } if module == "Prelude" => match name.as_str() {
            "Unit" | "Never" | "Bool" | "Int" | "Float" | "Int8" | "Int16" | "Int64" | "UInt8"
            | "UInt16" | "UInt32" | "UInt64" | "Float32" => {
                format!("imm{}", field.size)
            }
            _ => "boxed".to_owned(),
        },
        IrType::Named { module, name, .. } if module == "Bytes" && name == "Bytes" => {
            "boxed".to_owned()
        }
        IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. } => "boxed".to_owned(),
        _ => format!("inline{}", field.size),
    }
}

const fn primitive_ownership_tag(prim: Primitive) -> &'static str {
    match prim {
        Primitive::Unit
        | Primitive::Never
        | Primitive::Bool
        | Primitive::Int
        | Primitive::Float
        | Primitive::Int8
        | Primitive::Int16
        | Primitive::Int64
        | Primitive::UInt8
        | Primitive::UInt16
        | Primitive::UInt32
        | Primitive::UInt64
        | Primitive::Float32 => "imm",
        Primitive::Pointer
        | Primitive::Closure
        | Primitive::String
        | Primitive::BigInt
        | Primitive::Bytes => "boxed",
    }
}
