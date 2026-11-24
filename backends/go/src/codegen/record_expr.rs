//! Expression emission and shape registration for closed record structs.

use flick_ir::{Expr, ExprKind, IrType};
use flick_pretty::docvec;

use super::Gen;
use super::adt::AdtOps;
use super::arith::Native;
use super::go_type::{GoType, Typed, Want};
use super::layout::{Code, call, list};
use super::record::RecordShape;
use super::shape::Types;

impl<'a> Gen<'a> {
    pub(super) fn record_shape_of(&self, ty: Option<&IrType>) -> Option<RecordShape> {
        let decls = self.decls?;
        RecordShape::of(self.input.lang, decls, ty)
    }

    pub(super) fn register_shape(&mut self, shape: &RecordShape) {
        self.record_shapes.insert(shape.clone());
    }

    pub(super) fn register_go_type(&mut self, ty: &GoType) {
        match ty {
            GoType::Record(shape) => {
                self.record_shapes.insert(shape.clone());
            }
            GoType::Adt(shape) => {
                self.register_adt(shape);
            }
            GoType::Scalar(_) | GoType::Opt(_) => {}
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
        if label == "_tag"
            && let Some(GoType::Adt(ref shape)) = r.ty
        {
            self.register_adt_ops(shape, AdtOps::TAG);
            return Typed {
                code: call(a, a.alloc_str(&shape.tag_fn()), [r.code.doc]).into(),
                ty: Some(GoType::Scalar(Native::String)),
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
