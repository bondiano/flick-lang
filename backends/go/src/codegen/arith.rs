//! Operators. Int, Float, Bool and String values whose type the IR knows
//! stay Go's own `int32`, `float64`, `bool` and `string` through arithmetic,
//! comparisons and the locals they are bound to; concrete `Option` values
//! compare without allocation via their typed layouts.

use flick_ir::{Arith, BinOp, Expr, IrType, Lang, Num};
use flick_pretty::{Document, DocumentArena, docvec};

use super::Gen;
use super::go_type::{GoType, OptRepr, Typed};
use super::layout::{block, call, statement};
use super::record::RecordShape;
use flick_backend_support::Known;

/// A type whose values are Go's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Native {
    Int,
    Float,
    Bool,
    String,
    Int8,
    Int16,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
}

impl Native {
    pub(super) fn go(self) -> &'static str {
        match self {
            Native::Int => "int32",
            Native::Float => "float64",
            Native::Bool => "bool",
            Native::String => "string",
            Native::Int8 => "int8",
            Native::Int16 => "int16",
            Native::Int64 => "int64",
            Native::UInt8 => "uint8",
            Native::UInt16 => "uint16",
            Native::UInt32 => "uint32",
            Native::UInt64 => "uint64",
            Native::Float32 => "float32",
        }
    }

    /// Every one of them.
    pub(super) const ALL: [Native; 12] = [
        Native::Int,
        Native::Float,
        Native::Bool,
        Native::String,
        Native::Int8,
        Native::Int16,
        Native::Int64,
        Native::UInt8,
        Native::UInt16,
        Native::UInt32,
        Native::UInt64,
        Native::Float32,
    ];

    /// The type Go names `go`.
    pub(super) fn named(go: &str) -> Option<Native> {
        Self::ALL.into_iter().find(|native| native.go() == go)
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Native::Int => "Int",
            Native::Float => "Float",
            Native::Bool => "Bool",
            Native::String => "String",
            Native::Int8 => "Int8",
            Native::Int16 => "Int16",
            Native::Int64 => "Int64",
            Native::UInt8 => "UInt8",
            Native::UInt16 => "UInt16",
            Native::UInt32 => "UInt32",
            Native::UInt64 => "UInt64",
            Native::Float32 => "Float32",
        }
    }

    /// Whether a `Lit::Float(x)` emits a native Go `float64(...)` literal
    /// rather than a runtime `flickrt.Value` variable (`NaN`, `Inf`, `NegZero`).
    pub(super) fn is_native_float_lit(x: f64) -> bool {
        x.is_finite() && (x != 0.0 || x.is_sign_positive())
    }

    /// Whether a `Lit::Float32(x)` emits a native Go `float32(...)` literal
    /// rather than a runtime `flickrt.Value` variable (`NaN32`, `Inf32`, `NegInf32`, `NegZero32`).
    pub(super) fn is_native_float32_lit(x: f32) -> bool {
        x.is_finite() && (x != 0.0 || x.is_sign_positive())
    }

    pub(super) fn of_num(num: Num) -> Option<Native> {
        match num {
            Num::Int => Some(Native::Int),
            Num::Float => Some(Native::Float),
            Num::Int8 => Some(Native::Int8),
            Num::Int16 => Some(Native::Int16),
            Num::Int64 => Some(Native::Int64),
            Num::UInt8 => Some(Native::UInt8),
            Num::UInt16 => Some(Native::UInt16),
            Num::UInt32 => Some(Native::UInt32),
            Num::UInt64 => Some(Native::UInt64),
            Num::Float32 => Some(Native::Float32),
            Num::BigInt => None,
        }
    }

    /// `v`, a `flickrt.Value`, as this type.
    pub(super) fn unbox<'a>(self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        call(a, docvec![a, "flickrt.As", self.name()], [v])
    }

    /// `v`, a `flickrt.Value`, as a constructor field of this type; `nil` maps
    /// to Go's zero value so runtime `tagOf` probing does not panic.
    pub(super) fn field_unbox<'a>(self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        call(a, docvec![a, "flickrt.Field", self.name()], [v])
    }

    /// The type of values of type `ty`, when it is one of these.
    pub(super) fn of(lang: &Known, ty: Option<&IrType>) -> Option<Native> {
        let (module, name) = ty?.as_bare_con()?;
        [
            (Lang::Int, Native::Int),
            (Lang::Float, Native::Float),
            (Lang::Bool, Native::Bool),
            (Lang::String, Native::String),
            (Lang::Int8, Native::Int8),
            (Lang::Int16, Native::Int16),
            (Lang::Int64, Native::Int64),
            (Lang::UInt8, Native::UInt8),
            (Lang::UInt16, Native::UInt16),
            (Lang::UInt32, Native::UInt32),
            (Lang::UInt64, Native::UInt64),
            (Lang::Float32, Native::Float32),
        ]
        .into_iter()
        .find(|&(item, _)| lang.is(item, module, name))
        .map(|(_, native)| native)
    }
}

fn arith_name(arith: Arith) -> &'static str {
    match arith {
        Arith::Add => "Add",
        Arith::Sub => "Sub",
        Arith::Mul => "Mul",
        Arith::Div => "Div",
        Arith::Rem => "Rem",
        Arith::Pow => "Pow",
    }
}

/// How Go compares two scalars of the same type.
enum Comparison {
    /// A function of the runtime.
    Call(&'static str),
    /// An operator.
    Infix(&'static str),
    /// A test of the order the runtime gives two strings.
    Order(&'static str),
}

impl<'a> Gen<'a> {
    /// The operands of a scalar operator, as `want`s.
    fn operands(&mut self, lhs: &Expr, rhs: &Expr, want: Native) -> [Document<'a>; 2] {
        let gt = Some(GoType::Scalar(want));
        let go = self.ordered_into(&[lhs, rhs], &[gt.clone(), gt]);
        [go[0].doc, go[1].doc]
    }

    pub(super) fn neg(&mut self, num: Num, inner: &Expr) -> Typed<'a> {
        let a = self.a;
        if let Some(n) = Native::of_num(num) {
            // a call, as Go's constant `-0.0` is zero
            let v = self.expr_into(inner, Some(&GoType::Scalar(n)));
            Typed {
                code: call(a, docvec![a, "flickrt.Neg", n.name()], [v.doc]).into(),
                ty: Some(GoType::Scalar(n)),
            }
        } else {
            let v = self.expr(inner);
            Typed {
                code: call(a, "flickrt.NegB", [v.doc]).into(),
                ty: None,
            }
        }
    }

    /// The scalar type both operands of a comparison have, when Go compares it
    /// as Flick does.
    fn compared_scalar(&self, lhs: &Expr, rhs: &Expr) -> Option<Native> {
        let known = |e: &Expr| match self.predict_go_type(e, &[]) {
            Some(GoType::Scalar(n)) => Some(n),
            _ => Native::of(self.input.lang, e.ty.as_deref()),
        };
        known(lhs).filter(|&n| known(rhs) == Some(n))
    }

    /// The typed `Option` representation both operands of `==` / `!=` share.
    fn compared_opt(&self, lhs: &Expr, rhs: &Expr) -> Option<OptRepr> {
        let known = |e: &Expr| match self.predict_go_type(e, &[]) {
            Some(GoType::Opt(r)) => Some(r),
            _ => self.opt_repr_of(e.ty.as_deref()),
        };
        known(lhs).filter(|r| known(rhs).as_ref() == Some(r))
    }

    fn compare_opt(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, repr: &OptRepr) -> Typed<'a> {
        let a = self.a;
        let want = Some(GoType::Opt(repr.clone()));
        let go = self.ordered_into(&[lhs, rhs], &[want.clone(), want]);
        let eq = repr.eq_doc(a, go[0].doc, go[1].doc);
        let doc = if op == BinOp::NotEq {
            docvec![a, "!", eq]
        } else {
            eq
        };
        Typed {
            code: doc.into(),
            ty: Some(GoType::Scalar(Native::Bool)),
        }
    }

    /// The closed record shape both operands of `==` / `!=` share.
    fn compared_record(&self, lhs: &Expr, rhs: &Expr) -> Option<RecordShape> {
        let known = |e: &Expr| match self.predict_go_type(e, &[]) {
            Some(GoType::Record(s)) => Some(s),
            _ => self.record_shape_of(e.ty.as_deref()),
        };
        known(lhs).filter(|s| known(rhs).as_ref() == Some(s))
    }

    fn compare_record(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        shape: &RecordShape,
    ) -> Typed<'a> {
        let a = self.a;
        self.register_shape(shape);
        let want = Some(GoType::Record(shape.clone()));
        let go = self.ordered_into(&[lhs, rhs], &[want.clone(), want]);
        let eq = shape.eq_doc(a, go[0].doc, go[1].doc);
        let doc = if op == BinOp::NotEq {
            docvec![a, "!", eq]
        } else {
            eq
        };
        Typed {
            code: doc.into(),
            ty: Some(GoType::Scalar(Native::Bool)),
        }
    }

    pub(super) fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Typed<'a> {
        let a = self.a;
        if matches!(op, BinOp::And | BinOp::Or) {
            return self.logical(op, lhs, rhs);
        }
        if let BinOp::Arith(arith, num) = op
            && let Some(n) = Native::of_num(num)
        {
            // through the runtime's functions, so that operands Go knows
            // are constants are not folded with Go's rules
            let operands = self.operands(lhs, rhs, n);
            let f = docvec![a, "flickrt.", arith_name(arith), n.name()];
            return Typed {
                code: call(a, f, operands).into(),
                ty: Some(GoType::Scalar(n)),
            };
        }
        if let Some(n) = self.compared_scalar(lhs, rhs) {
            let go = match (op, n) {
                // `==` holds of NaN and NaN, `===` does not
                (BinOp::Eq, Native::Float) => Some(Comparison::Call("flickrt.EqFloat")),
                (BinOp::NotEq, Native::Float) => Some(Comparison::Call("!flickrt.EqFloat")),
                (BinOp::Eq, Native::Float32) => Some(Comparison::Call("flickrt.EqFloat32")),
                (BinOp::NotEq, Native::Float32) => Some(Comparison::Call("!flickrt.EqFloat32")),
                (BinOp::Eq | BinOp::Same, _) => Some(Comparison::Infix(" == ")),
                (BinOp::NotEq | BinOp::NotSame, _) => Some(Comparison::Infix(" != ")),
                (_, Native::Bool) => None,
                // JS orders strings by UTF-16 code units, as the runtime does
                (BinOp::Lt, Native::String) => Some(Comparison::Order(" < 0")),
                (BinOp::Le, Native::String) => Some(Comparison::Order(" <= 0")),
                (BinOp::Gt, Native::String) => Some(Comparison::Order(" > 0")),
                (BinOp::Ge, Native::String) => Some(Comparison::Order(" >= 0")),
                (BinOp::Lt, _) => Some(Comparison::Infix(" < ")),
                (BinOp::Le, _) => Some(Comparison::Infix(" <= ")),
                (BinOp::Gt, _) => Some(Comparison::Infix(" > ")),
                (BinOp::Ge, _) => Some(Comparison::Infix(" >= ")),
                _ => None,
            };
            if let Some(go) = go {
                let operands = self.operands(lhs, rhs, n);
                let doc = match go {
                    Comparison::Call(f) => call(a, f, operands),
                    Comparison::Infix(op) => {
                        let [l, r] = operands;
                        docvec![a, "(", l, op, r, ")"]
                    }
                    Comparison::Order(test) => {
                        let order = call(a, "flickrt.CompareStrings", operands);
                        docvec![a, "(", order, test, ")"]
                    }
                };
                return Typed {
                    code: doc.into(),
                    ty: Some(GoType::Scalar(Native::Bool)),
                };
            }
        }
        if matches!(op, BinOp::Eq | BinOp::NotEq)
            && let Some(repr) = self.compared_opt(lhs, rhs)
        {
            return self.compare_opt(op, lhs, rhs, &repr);
        }
        if matches!(op, BinOp::Eq | BinOp::NotEq)
            && let Some(shape) = self.compared_record(lhs, rhs)
        {
            return self.compare_record(op, lhs, rhs, &shape);
        }
        if op == BinOp::Concat
            && self.predict_go_type(lhs, &[]) == Some(GoType::Scalar(Native::String))
            && self.predict_go_type(rhs, &[]) == Some(GoType::Scalar(Native::String))
        {
            let operands = self.operands(lhs, rhs, Native::String);
            return Typed {
                code: call(a, "flickrt.ConcatString", operands).into(),
                ty: Some(GoType::Scalar(Native::String)),
            };
        }
        let go = self.ordered(&[lhs, rhs]);
        let operands = [go[0].doc, go[1].doc];
        let (doc, is_bool) = match op {
            BinOp::Eq => (call(a, "flickrt.Equals", operands), true),
            BinOp::NotEq => (call(a, "!flickrt.Equals", operands), true),
            BinOp::Same => (call(a, "flickrt.Same", operands), true),
            BinOp::NotSame => (call(a, "!flickrt.Same", operands), true),
            BinOp::Lt => (call(a, "flickrt.Lt", operands), true),
            BinOp::Le => (call(a, "flickrt.Le", operands), true),
            BinOp::Gt => (call(a, "flickrt.Gt", operands), true),
            BinOp::Ge => (call(a, "flickrt.Ge", operands), true),
            BinOp::Concat => (call(a, "flickrt.Concat", operands), false),
            BinOp::ListConcat => (call(a, "flickrt.ListConcat", operands), false),
            BinOp::Arith(arith, _) => {
                let f = docvec![a, "flickrt.", arith_name(arith), "B"];
                (call(a, f, operands), false)
            }
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        Typed {
            code: doc.into(),
            ty: is_bool.then_some(GoType::Scalar(Native::Bool)),
        }
    }

    /// `&&` and `||`, which evaluate their right operand only when needed.
    fn logical(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Typed<'a> {
        let a = self.a;
        let l = self.expr_bool(lhs).doc;
        let (mut rs, r) = self.block(|g| g.expr_bool(rhs).doc);
        if rs.is_empty() {
            let go = if op == BinOp::And { " && " } else { " || " };
            return Typed {
                code: docvec![a, "(", l, go, r, ")"].into(),
                ty: Some(GoType::Scalar(Native::Bool)),
            };
        }
        let t = self.temp();
        let test = if op == BinOp::And { "" } else { "!" };
        rs.push(statement(a, docvec![a, t, " = ", r]));
        self.emit(docvec![a, "var ", t, " bool = ", l]);
        self.emit(docvec![a, "if ", test, t, " ", block(a, rs)]);
        Typed {
            code: self.code(t),
            ty: Some(GoType::Scalar(Native::Bool)),
        }
    }
}
