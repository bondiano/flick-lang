//! Closed record shapes as dedicated Go structs carried by value.
//!
//! Each closed record shape whose fields have substitution- and agreement-
//! invariant Go representations gets one Go struct `R_...` with fields in
//! canonical label order (`canonical_key_cmp`) and scalar fields stored in
//! Go's native scalar types (packed pointer-first to minimize GC `ptrdata`
//! and alignment padding). Value-receiver methods implement `flickrt.RecordView`
//! and `flickrt.RecordEqual`, so a struct value crosses into `flickrt.Value`
//! directly (at most 1 Go interface-boxing allocation) while typed calls,
//! locals, updates and field reads stay unboxed on the stack (0 allocations).

use std::fmt::Write;

use flick_backend_support::Known;
use flick_ir::{Expr, IrType, Lang};
use flick_pretty::{Document, DocumentArena, docvec};

use super::adt::{AdtShape, align_up, native_size_align};
use super::arith::Native;
use super::go_type::{GoType, TypeDecls};
use super::layout::call;
use super::once::canonical_key_cmp;

/// The Go storage slot of one field in a closed record shape.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum RecordSlot {
    Value,
    Scalar(Native),
    Adt(AdtShape),
}

impl RecordSlot {
    pub(super) fn go_type(&self) -> String {
        match self {
            RecordSlot::Value => "flickrt.Value".to_owned(),
            RecordSlot::Scalar(n) => n.go().to_owned(),
            RecordSlot::Adt(a) => a.go_type(),
        }
    }

    pub(super) fn to_go_type(&self) -> Option<GoType> {
        match self {
            RecordSlot::Value => None,
            RecordSlot::Scalar(n) => Some(GoType::Scalar(*n)),
            RecordSlot::Adt(a) => Some(GoType::Adt(a.clone())),
        }
    }

    pub(super) fn has_pointers(&self) -> bool {
        match self {
            RecordSlot::Value | RecordSlot::Scalar(Native::String) => true,
            RecordSlot::Scalar(_) => false,
            RecordSlot::Adt(a) => a.has_pointers(),
        }
    }

    pub(super) fn size_align(&self) -> (usize, usize) {
        match self {
            RecordSlot::Value => (16, 8),
            RecordSlot::Scalar(n) => native_size_align(*n),
            RecordSlot::Adt(a) => a.size_align(),
        }
    }

    pub(super) fn layout_rank(&self) -> u8 {
        match self {
            RecordSlot::Value => 0,
            RecordSlot::Scalar(n) => layout_rank(Some(*n)),
            RecordSlot::Adt(a) => {
                if a.has_pointers() {
                    1
                } else {
                    match a.size_align().1 {
                        8 => 2,
                        4 => 3,
                        2 => 4,
                        _ => 5,
                    }
                }
            }
        }
    }

    pub(super) fn is_go_comparable(&self) -> bool {
        match self {
            RecordSlot::Value | RecordSlot::Scalar(Native::Float | Native::Float32) => false,
            RecordSlot::Scalar(_) => true,
            RecordSlot::Adt(a) => a.is_go_comparable(),
        }
    }
}

/// Structural layout of a closed record shape in canonical label order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct RecordShape {
    pub(super) fields: Vec<(String, RecordSlot)>,
}

enum FieldSlot {
    Slot(RecordSlot),
    Erased,
}

impl RecordShape {
    /// Computes the closed record shape for `ty` when `ty` is a closed
    /// `IrType::Record` whose every field has a substitution- and
    /// agreement-invariant Go slot.
    pub(super) fn of(lang: &Known, decls: &TypeDecls, ty: Option<&IrType>) -> Option<RecordShape> {
        let IrType::Record { fields, tail: None } = ty? else {
            return None;
        };
        let mut out = Vec::with_capacity(fields.len());
        for (label, field_ty) in fields {
            let slot = match Self::field_slot(lang, decls, field_ty) {
                FieldSlot::Slot(s) => s,
                FieldSlot::Erased => return None,
            };
            out.push((label.clone(), slot));
        }
        out.sort_by(|a, b| canonical_key_cmp(&a.0, &b.0));
        if out.windows(2).any(|w| w[0].0 == w[1].0) {
            return None;
        }
        Some(RecordShape { fields: out })
    }

    /// Computes the record shape of a constructor declared over a closed record
    /// (`Module.types`), unboxing declaration-site scalars and eligible closed
    /// ADTs while keeping polymorphic / other fields as `flickrt.Value`.
    pub(super) fn for_ctor_record(
        lang: &Known,
        decls: &TypeDecls,
        ty: &IrType,
    ) -> Option<RecordShape> {
        let IrType::Record { fields, tail: None } = ty else {
            return None;
        };
        if fields.is_empty() {
            return None;
        }
        let mut out: Vec<(String, RecordSlot)> = fields
            .iter()
            .map(|(label, field_ty)| {
                let slot = if let Some(n) = Native::of(lang, Some(field_ty)) {
                    RecordSlot::Scalar(n)
                } else if let Some(adt) = AdtShape::of(lang, decls, Some(field_ty)) {
                    RecordSlot::Adt(adt)
                } else {
                    RecordSlot::Value
                };
                (label.clone(), slot)
            })
            .collect();
        out.sort_by(|a, b| canonical_key_cmp(&a.0, &b.0));
        if out.windows(2).any(|w| w[0].0 == w[1].0) {
            return None;
        }
        Some(RecordShape { fields: out })
    }

    /// Fallback record shape from canonicalized label names (all `flickrt.Value`
    /// slots), used when `Module.types` is absent in synthetic IR.
    pub(super) fn from_labels(labels: &[String]) -> RecordShape {
        let mut out: Vec<(String, RecordSlot)> = labels
            .iter()
            .map(|l| (l.clone(), RecordSlot::Value))
            .collect();
        out.sort_by(|a, b| canonical_key_cmp(&a.0, &b.0));
        RecordShape { fields: out }
    }

    /// Determines the Go field slot for `field_ty`, returning `FieldSlot::Erased`
    /// to reject the whole record if `field_ty` could unify or instantiate to a
    /// native scalar or by-value ADT on one side and a different slot on another.
    fn field_slot(lang: &Known, decls: &TypeDecls, field_ty: &IrType) -> FieldSlot {
        match field_ty {
            IrType::Var(_) | IrType::App { .. } | IrType::Exists { .. } | IrType::Forall { .. } => {
                FieldSlot::Erased
            }
            IrType::Tuple(_) | IrType::Record { .. } | IrType::Fun { .. } => {
                FieldSlot::Slot(RecordSlot::Value)
            }
            IrType::Named { module, name, .. } => {
                if let Some(native) = Native::of(lang, Some(field_ty)) {
                    return FieldSlot::Slot(RecordSlot::Scalar(native));
                }
                if lang.is(Lang::Errors, module, name)
                    || lang.is(Lang::ServiceImpl, module, name)
                    || lang.is(Lang::Field, module, name)
                    || decls
                        .get(&(module.clone(), name.clone()))
                        .is_some_and(|info| info.decl.alias.is_some())
                {
                    return FieldSlot::Erased;
                }
                if let Some(adt) = AdtShape::of(lang, decls, Some(field_ty)) {
                    return FieldSlot::Slot(RecordSlot::Adt(adt));
                }
                if AdtShape::could_be_adt_under_subst(lang, decls, field_ty) {
                    return FieldSlot::Erased;
                }
                FieldSlot::Slot(RecordSlot::Value)
            }
        }
    }

    pub(super) fn has_value_slots(&self) -> bool {
        self.fields
            .iter()
            .any(|(_, s)| matches!(s, RecordSlot::Value))
    }

    pub(super) fn has_pointers(&self) -> bool {
        self.fields.iter().any(|(_, s)| s.has_pointers())
    }

    pub(super) fn size_align(&self) -> (usize, usize) {
        let mut order: Vec<usize> = (0..self.fields.len()).collect();
        order.sort_by_key(|&i| (self.fields[i].1.layout_rank(), i));
        let mut offset = 0usize;
        let mut max_align = 1usize;
        for i in order {
            let (sz, al) = self.fields[i].1.size_align();
            max_align = max_align.max(al);
            offset = align_up(offset, al) + sz;
        }
        (align_up(offset, max_align), max_align)
    }

    /// Deterministic Go struct type name for this structural shape.
    pub(super) fn go_type(&self) -> String {
        if self.fields.is_empty() {
            return "R_empty".to_owned();
        }
        let mut prefix = String::new();
        for (label, _) in self.fields.iter().take(3) {
            let clean: String = label
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .take(12)
                .collect();
            if !clean.is_empty() {
                if !prefix.is_empty() {
                    prefix.push('_');
                }
                prefix.push_str(&clean);
            }
        }
        if self.fields.len() > 3 {
            if !prefix.is_empty() {
                prefix.push('_');
            }
            let _ = write!(prefix, "n{}", self.fields.len());
        }
        let hash = self.shape_hash();
        if prefix.is_empty() {
            format!("R_{hash:016x}")
        } else {
            format!("R_{prefix}_{hash:016x}")
        }
    }

    pub(super) fn shape_hash(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0100_0000_01b3;
        let mut h = OFFSET;
        let mut feed = |bytes: &[u8]| {
            for &b in bytes {
                h ^= u64::from(b);
                h = h.wrapping_mul(PRIME);
            }
        };
        feed(&(self.fields.len() as u64).to_le_bytes());
        for (label, slot) in &self.fields {
            feed(&(label.len() as u64).to_le_bytes());
            feed(label.as_bytes());
            feed(&[0xff]);
            match slot {
                RecordSlot::Value => feed(&[0]),
                RecordSlot::Scalar(n) => feed(&[1, slot_tag(Some(*n))]),
                RecordSlot::Adt(a) => {
                    feed(&[2]);
                    feed(&a.shape_hash().to_le_bytes());
                }
            }
        }
        h
    }

    pub(super) fn unbox_fn(&self) -> String {
        format!("As_{}", self.go_type())
    }

    pub(super) fn eq_fn(&self) -> String {
        format!("Eq_{}", self.go_type())
    }

    pub(super) fn index_of(&self, label: &str) -> Option<usize> {
        self.fields.iter().position(|(l, _)| l == label)
    }

    pub(super) fn field_go_type(&self, i: usize) -> Option<GoType> {
        self.fields.get(i)?.1.to_go_type()
    }

    pub(super) fn matches_literal(&self, fields: &[(String, Expr)]) -> bool {
        if fields.len() != self.fields.len() {
            return false;
        }
        let mut seen = vec![false; self.fields.len()];
        for (label, _) in fields {
            let Some(i) = self.index_of(label) else {
                return false;
            };
            if seen[i] {
                return false;
            }
            seen[i] = true;
        }
        true
    }

    pub(super) fn matches_update(&self, fields: &[(String, Expr)]) -> bool {
        fields
            .iter()
            .all(|(label, _)| self.index_of(label).is_some())
    }

    /// Whether Go's built-in `==` matches Flick's `SameValueZero` structural
    /// equality on this struct.
    pub(super) fn is_go_comparable(&self) -> bool {
        self.fields.iter().all(|(_, slot)| slot.is_go_comparable())
    }

    pub(super) fn unbox_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        call(a, a.alloc_str(&self.unbox_fn()), [v])
    }

    pub(super) fn eq_doc<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        lhs: Document<'a>,
        rhs: Document<'a>,
    ) -> Document<'a> {
        if self.is_go_comparable() {
            docvec![a, "(", lhs, " == ", rhs, ")"]
        } else {
            call(a, a.alloc_str(&self.eq_fn()), [lhs, rhs])
        }
    }
}

fn slot_tag(slot: Option<Native>) -> u8 {
    match slot {
        None => 0,
        Some(Native::Int) => 1,
        Some(Native::Float) => 2,
        Some(Native::Bool) => 3,
        Some(Native::String) => 4,
        Some(Native::Int8) => 5,
        Some(Native::Int16) => 6,
        Some(Native::Int64) => 7,
        Some(Native::UInt8) => 8,
        Some(Native::UInt16) => 9,
        Some(Native::UInt32) => 10,
        Some(Native::UInt64) => 11,
        Some(Native::Float32) => 12,
    }
}

/// Struct field packing order: pointer-bearing fields first to minimize Go GC
/// `ptrdata`, then scalar fields by descending alignment (`8`, `4`, `2`, `1` bytes).
pub(super) fn layout_rank(slot: Option<Native>) -> u8 {
    match slot {
        None => 0,
        Some(Native::String) => 1,
        Some(Native::Float | Native::Int64 | Native::UInt64) => 2,
        Some(Native::Int | Native::UInt32 | Native::Float32) => 3,
        Some(Native::Int16 | Native::UInt16) => 4,
        Some(Native::Int8 | Native::UInt8 | Native::Bool) => 5,
    }
}

/// Go expression comparing two fields with `strictZero` (used in `EqualRecord`
/// and `EqualConstructed`).
pub(super) fn equal_with_expr(lhs: &str, rhs: &str, slot: &RecordSlot) -> String {
    match slot {
        RecordSlot::Scalar(Native::Float) => {
            format!("flickrt.EqualFloat({lhs}, {rhs}, strictZero)")
        }
        RecordSlot::Scalar(Native::Float32) => {
            format!("flickrt.EqualFloat32({lhs}, {rhs}, strictZero)")
        }
        RecordSlot::Scalar(_) => format!("{lhs} == {rhs}"),
        RecordSlot::Adt(a) if a.is_go_comparable() => format!("{lhs} == {rhs}"),
        RecordSlot::Adt(a) => format!("{}({lhs}, {rhs}, strictZero)", a.equal_with_fn()),
        RecordSlot::Value => format!("flickrt.EqualWith({lhs}, {rhs}, strictZero)"),
    }
}
