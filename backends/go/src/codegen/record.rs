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
use flick_ir::{Expr, ExprKind, IrType};
use flick_pretty::{Document, DocumentArena, docvec};

use super::Gen;
use super::arith::Native;
use super::go_type::{GoType, TypeDecls, Typed, Want};
use super::layout::{Code, call, list};
use super::once::canonical_key_cmp;
use super::shape::Types;

/// Structural layout of a closed record shape in canonical label order.
/// `Some(native)` stores the field unboxed in its Go scalar type; `None`
/// keeps `flickrt.Value`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct RecordShape {
    pub(super) fields: Vec<(String, Option<Native>)>,
}

enum FieldSlot {
    Native(Native),
    Value,
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
                FieldSlot::Native(n) => Some(n),
                FieldSlot::Value => None,
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
    /// (`Module.types`), unboxing declaration-site native scalar fields while
    /// keeping polymorphic / non-scalar fields as `flickrt.Value`.
    pub(super) fn for_ctor_record(lang: &Known, ty: &IrType) -> Option<RecordShape> {
        let IrType::Record { fields, tail: None } = ty else {
            return None;
        };
        if fields.is_empty() {
            return None;
        }
        let mut out: Vec<(String, Option<Native>)> = fields
            .iter()
            .map(|(label, field_ty)| (label.clone(), Native::of(lang, Some(field_ty))))
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
        let mut out: Vec<(String, Option<Native>)> =
            labels.iter().map(|l| (l.clone(), None)).collect();
        out.sort_by(|a, b| canonical_key_cmp(&a.0, &b.0));
        RecordShape { fields: out }
    }

    /// Determines the Go field slot for `field_ty`, returning `FieldSlot::Erased`
    /// to reject the whole record if `field_ty` could unify or instantiate to a
    /// native scalar on one side and a non-scalar on another.
    fn field_slot(lang: &Known, decls: &TypeDecls, field_ty: &IrType) -> FieldSlot {
        match field_ty {
            IrType::Var(_) | IrType::App { .. } | IrType::Exists { .. } | IrType::Forall { .. } => {
                FieldSlot::Erased
            }
            IrType::Tuple(_) | IrType::Record { .. } | IrType::Fun { .. } => FieldSlot::Value,
            IrType::Named { module, name, .. } => {
                if let Some(native) = Native::of(lang, Some(field_ty)) {
                    return FieldSlot::Native(native);
                }
                if decls
                    .get(&(module.clone(), name.clone()))
                    .is_some_and(|decl| decl.alias.is_some())
                {
                    return FieldSlot::Erased;
                }
                FieldSlot::Value
            }
        }
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

    fn shape_hash(&self) -> u64 {
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
            feed(&[0xff, slot_tag(*slot)]);
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
        self.fields.get(i)?.1.map(GoType::Scalar)
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
    /// equality on this struct (every field is a non-float native scalar).
    pub(super) fn is_go_comparable(&self) -> bool {
        self.fields.iter().all(|(_, slot)| {
            matches!(
                slot,
                Some(
                    Native::Int
                        | Native::Int8
                        | Native::Int16
                        | Native::Int64
                        | Native::UInt8
                        | Native::UInt16
                        | Native::UInt32
                        | Native::UInt64
                        | Native::Bool
                        | Native::String
                )
            )
        })
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
pub(super) fn equal_with_expr(lhs: &str, rhs: &str, slot: Option<Native>) -> String {
    match slot {
        Some(Native::Float) => format!("flickrt.EqualFloat({lhs}, {rhs}, strictZero)"),
        Some(Native::Float32) => {
            format!("flickrt.EqualFloat(float64({lhs}), float64({rhs}), strictZero)")
        }
        Some(
            Native::Int
            | Native::Int8
            | Native::Int16
            | Native::Int64
            | Native::UInt8
            | Native::UInt16
            | Native::UInt32
            | Native::UInt64
            | Native::Bool
            | Native::String,
        ) => format!("{lhs} == {rhs}"),
        None => format!("flickrt.EqualWith({lhs}, {rhs}, strictZero)"),
    }
}

impl<'a> Gen<'a> {
    pub(super) fn record_shape_of(&self, ty: Option<&IrType>) -> Option<RecordShape> {
        let decls = self.decls?;
        RecordShape::of(self.input.lang, decls, ty)
    }

    pub(super) fn register_shape(&mut self, shape: &RecordShape) {
        self.record_shapes.insert(shape.clone());
    }

    pub(super) fn register_go_type(&mut self, ty: &GoType) {
        if let GoType::Record(shape) = ty {
            self.record_shapes.insert(shape.clone());
        }
    }

    pub(super) fn register_types(&mut self, types: &Types) {
        if let Some(gt) = types.result() {
            self.register_go_type(gt);
        }
        for i in 0..types.param_len() {
            if let Some(gt) = types.param(i) {
                self.register_go_type(gt);
            }
        }
    }

    /// Emits a record literal: a closed record struct when the shape is known,
    /// preserving exact source-order evaluation of field expressions.
    pub(super) fn record_typed(
        &mut self,
        e: &Expr,
        fields: &[(String, Expr)],
        want: Want<'_>,
    ) -> Typed<'a> {
        let a = self.a;
        let target = match want {
            Want::Exact(GoType::Record(shape)) if shape.matches_literal(fields) => {
                Some(shape.clone())
            }
            _ => self
                .record_shape_of(e.ty.as_deref())
                .filter(|shape| shape.matches_literal(fields)),
        };
        let Some(shape) = target else {
            return Typed {
                code: self.record_erased(None, fields),
                ty: None,
            };
        };
        self.register_shape(&shape);
        let ty = a.alloc_str(&shape.go_type());
        if fields.is_empty() {
            return Typed {
                code: Code {
                    doc: docvec![a, ty, "{}"],
                    atomic: true,
                },
                ty: Some(GoType::Record(shape)),
            };
        }
        let places: Vec<usize> = fields
            .iter()
            .map(|(label, _)| shape.index_of(label).expect("checked by matches_literal"))
            .collect();
        let wants: Vec<Option<GoType>> = places.iter().map(|&p| shape.field_go_type(p)).collect();
        let exprs: Vec<&Expr> = fields.iter().map(|(_, v)| v).collect();
        let go = self.ordered_into(&exprs, &wants);
        // Keyed composite literal in source order: Go evaluates elements left-to-right,
        // and `ordered_into` has already spilled around statements and `Var` hazards.
        let items = places
            .into_iter()
            .zip(go)
            .map(|(place, v)| docvec![a, "f", place, ": ", v.doc]);
        Typed {
            code: list(a, docvec![a, ty, "{"], items, "}").into(),
            ty: Some(GoType::Record(shape)),
        }
    }

    /// Emits `{ base with … }`: stack value copy + direct field assignments when
    /// the record's closed shape is known, preserving source evaluation order.
    pub(super) fn update_typed(
        &mut self,
        e: &Expr,
        base: &Expr,
        fields: &[(String, Expr)],
        want: Want<'_>,
    ) -> Typed<'a> {
        let a = self.a;
        let target = match want {
            Want::Exact(GoType::Record(shape)) if shape.matches_update(fields) => {
                Some(shape.clone())
            }
            _ => self
                .record_shape_of(e.ty.as_deref())
                .or_else(|| match self.predict_go_type(base, &[]) {
                    Some(GoType::Record(shape)) => Some(shape),
                    _ => None,
                })
                .filter(|shape| shape.matches_update(fields)),
        };
        let Some(shape) = target else {
            return Typed {
                code: self.record_erased(Some(base), fields),
                ty: None,
            };
        };
        self.register_shape(&shape);
        let places: Vec<usize> = fields
            .iter()
            .map(|(label, _)| shape.index_of(label).expect("checked by matches_update"))
            .collect();
        let mut all: Vec<&Expr> = Vec::with_capacity(fields.len() + 1);
        all.push(base);
        all.extend(fields.iter().map(|(_, v)| v));
        let mut wants: Vec<Option<GoType>> = Vec::with_capacity(fields.len() + 1);
        wants.push(Some(GoType::Record(shape.clone())));
        wants.extend(places.iter().map(|&p| shape.field_go_type(p)));
        let mut go = self.ordered_into(&all, &wants).into_iter();
        let base_code = go.next().expect("base expression");
        if fields.is_empty() {
            return Typed {
                code: base_code,
                ty: Some(GoType::Record(shape)),
            };
        }
        let t = self.temp();
        let ty = a.alloc_str(&shape.go_type());
        self.emit(docvec![a, "var ", t, " ", ty, " = ", base_code]);
        for (place, v) in places.into_iter().zip(go) {
            self.emit(docvec![a, t, ".f", place, " = ", v]);
        }
        Typed {
            code: self.code(t),
            ty: Some(GoType::Record(shape)),
        }
    }

    /// Emits `record.label`: direct struct field access when `record` has a known
    /// closed record Go type; `flickrt.Field` / `flickrt.TagField` otherwise.
    pub(super) fn field_typed(&mut self, record: &Expr, label: &str) -> Typed<'a> {
        let a = self.a;
        let r = self.expr_typed(record);
        if let Some(GoType::Record(ref shape)) = r.ty
            && let Some(place) = shape.index_of(label)
        {
            self.register_shape(shape);
            // Parenthesize composite literals (including through blocks and empty
            // updates) so `if (let a = true; { x = a }).x` emits valid Go
            // `if (R_x_...{f0: l_a}).f0 {`.
            let recv = if yields_composite_literal(record) {
                docvec![a, "(", r.code.doc, ")"]
            } else {
                r.code.doc
            };
            return Typed {
                code: Code {
                    doc: docvec![a, recv, ".f", place],
                    atomic: r.code.atomic,
                },
                ty: shape.field_go_type(place),
            };
        }
        let v = self.to_value(r);
        let code = if label == "_tag" {
            call(a, "flickrt.TagField", [v.doc]).into()
        } else {
            call(a, "flickrt.Field", [v.doc, self.string(label)]).into()
        };
        Typed { code, ty: None }
    }
}

/// Whether `e` can evaluate directly to a Go composite literal `R_...{...}`
/// without being spilled into a temporary variable.
fn yields_composite_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Record(_) => true,
        ExprKind::Block(_, result) => yields_composite_literal(result),
        ExprKind::Update(base, fields) if fields.is_empty() => yields_composite_literal(base),
        _ => false,
    }
}
