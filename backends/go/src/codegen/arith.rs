//! Operators. Int, Float, Bool and String values whose type the IR knows
//! stay Go's own `int32`, `float64`, `bool` and `string` through arithmetic,
//! comparisons and the locals they are bound to; anywhere else they are a
//! `flickrt.Value`, which Go boxes them into.

use flick_ir::Lang;
use flick_pretty::{Document, DocumentArena, docvec};

use super::Gen;
use super::layout::{Code, block, call, statement};
use flick_backend_support::Known;
use flick_ir::{Arith, Num};
use flick_ir::{Arm, BinOp, Expr, ExprKind, IrType, Lit};

/// A type whose values are Go's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Every one of them, in the order their numbers have in the shape of
    /// a function.
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

    fn of_num(num: Num) -> Option<Native> {
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
    /// The Go type the code of `e` has when it is not a `flickrt.Value`;
    /// none when it may be one.
    pub(super) fn native(&self, e: &Expr) -> Option<Native> {
        self.native_with(e, &[])
    }

    fn native_with(&self, e: &Expr, extra: &[(&str, Native)]) -> Option<Native> {
        match &e.kind {
            ExprKind::Lit(Lit::Int(_)) => Some(Native::Int),
            ExprKind::Lit(Lit::Int8(_)) => Some(Native::Int8),
            ExprKind::Lit(Lit::Int16(_)) => Some(Native::Int16),
            ExprKind::Lit(Lit::Int64(_)) => Some(Native::Int64),
            ExprKind::Lit(Lit::UInt8(_)) => Some(Native::UInt8),
            ExprKind::Lit(Lit::UInt16(_)) => Some(Native::UInt16),
            ExprKind::Lit(Lit::UInt32(_)) => Some(Native::UInt32),
            ExprKind::Lit(Lit::UInt64(_)) => Some(Native::UInt64),
            // the other floats are variables of the runtime
            ExprKind::Lit(Lit::Float(x)) if Native::is_native_float_lit(*x) => Some(Native::Float),
            ExprKind::Lit(Lit::Float32(x)) if Native::is_native_float32_lit(*x) => {
                Some(Native::Float32)
            }
            ExprKind::Lit(Lit::String(_)) => Some(Native::String),
            ExprKind::Local(name) | ExprKind::Var(name) => extra
                .iter()
                .rev()
                .find_map(|&(k, v)| (k == name).then_some(v))
                .or_else(|| self.natives.get(name).copied()),
            ExprKind::Neg(num, _) | ExprKind::Binary(BinOp::Arith(_, num), _, _) => {
                Native::of_num(*num)
            }
            ExprKind::Binary(BinOp::Concat, lhs, rhs) => (self.native_with(lhs, extra)
                == Some(Native::String)
                && self.native_with(rhs, extra) == Some(Native::String))
            .then_some(Native::String),
            ExprKind::Lit(Lit::Bool(_))
            | ExprKind::Not(_)
            | ExprKind::Binary(
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Same
                | BinOp::NotSame
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or,
                _,
                _,
            ) => Some(Native::Bool),
            // a call of a known function or scalar instance method that returns a Go type
            ExprKind::App { .. } => self.typed_result(e),
            // a choice whose every branch is of one Go type is of that type
            ExprKind::If(_, then, other) => self.agreed_with([&**then, &**other], extra),
            ExprKind::Match(_, arms) => self.match_native(arms, extra),
            _ => None,
        }
    }

    /// The Go type every one of `branches` has, when they have one.
    fn agreed_with<'e>(
        &self,
        branches: impl IntoIterator<Item = &'e Expr>,
        extra: &[(&str, Native)],
    ) -> Option<Native> {
        let mut branches = branches.into_iter();
        let first = self.native_with(branches.next()?, extra)?;
        branches
            .all(|b| self.native_with(b, extra) == Some(first))
            .then_some(first)
    }

    /// The Go type every arm of a `match` produces when its guaranteed-native
    /// pattern bindings are in scope.
    fn match_native(&self, arms: &[Arm], extra: &[(&str, Native)]) -> Option<Native> {
        let mut arms = arms.iter();
        let first_arm = arms.next()?;
        let arm_ty = |arm: &Arm| {
            let bound = self.pat_natives(&arm.pat);
            if bound.is_empty() {
                return self.native_with(&arm.body, extra);
            }
            let mut scoped = Vec::with_capacity(extra.len() + bound.len());
            scoped.extend_from_slice(extra);
            scoped.extend(bound);
            self.native_with(&arm.body, &scoped)
        };
        let first = arm_ty(first_arm)?;
        arms.all(|arm| arm_ty(arm) == Some(first)).then_some(first)
    }

    /// `go`, the code of `e`, as a `want`: code that already is one, as a
    /// condition Go holds as a `bool`, is not made a value to be read back.
    pub(super) fn unboxed(&self, e: &Expr, go: Code<'a>, want: Native) -> Code<'a> {
        if self.native(e) == Some(want) {
            go
        } else {
            want.unbox(self.a, go.doc).into()
        }
    }

    /// The operands of a scalar operator, as `want`s.
    fn operands(&mut self, lhs: &Expr, rhs: &Expr, want: Native) -> [Document<'a>; 2] {
        let go = self.ordered(&[lhs, rhs]);
        let [l, r] = <[Code<'a>; 2]>::try_from(go).expect("two operands");
        [
            self.unboxed(lhs, l, want).doc,
            self.unboxed(rhs, r, want).doc,
        ]
    }

    pub(super) fn neg(&mut self, num: Num, inner: &Expr) -> Code<'a> {
        let a = self.a;
        let v = self.expr(inner);
        match Native::of_num(num) {
            // a call, as Go's constant `-0.0` is zero
            Some(n) => {
                let v = self.unboxed(inner, v, n);
                call(a, docvec![a, "flickrt.Neg", n.name()], [v.doc]).into()
            }
            None => call(a, "flickrt.NegB", [v.doc]).into(),
        }
    }

    /// The type both operands of a comparison have, when it is one Go
    /// compares as Flick does.
    fn compared(&self, lhs: &Expr, rhs: &Expr) -> Option<Native> {
        let known = |e: &Expr| {
            self.native(e)
                .or_else(|| Native::of(self.input.lang, e.ty.as_deref()))
        };
        known(lhs).filter(|&n| known(rhs) == Some(n))
    }

    pub(super) fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Code<'a> {
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
            return call(a, f, operands).into();
        }
        if let Some(n) = self.compared(lhs, rhs) {
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
                return match go {
                    Comparison::Call(f) => call(a, f, operands),
                    Comparison::Infix(op) => {
                        let [l, r] = operands;
                        docvec![a, "(", l, op, r, ")"]
                    }
                    Comparison::Order(test) => {
                        let order = call(a, "flickrt.CompareStrings", operands);
                        docvec![a, "(", order, test, ")"]
                    }
                }
                .into();
            }
        }
        if op == BinOp::Concat
            && self.native(lhs) == Some(Native::String)
            && self.native(rhs) == Some(Native::String)
        {
            let operands = self.operands(lhs, rhs, Native::String);
            return call(a, "flickrt.ConcatString", operands).into();
        }
        let go = self.ordered(&[lhs, rhs]);
        let operands = [go[0].doc, go[1].doc];
        let f = match op {
            BinOp::Eq => "Equals",
            BinOp::NotEq => return call(a, "!flickrt.Equals", operands).into(),
            BinOp::Same => "Same",
            BinOp::NotSame => return call(a, "!flickrt.Same", operands).into(),
            BinOp::Lt => "Lt",
            BinOp::Le => "Le",
            BinOp::Gt => "Gt",
            BinOp::Ge => "Ge",
            BinOp::Concat => "Concat",
            BinOp::ListConcat => "ListConcat",
            BinOp::Arith(arith, _) => {
                let f = docvec![a, "flickrt.", arith_name(arith), "B"];
                return call(a, f, operands).into();
            }
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        call(a, docvec![a, "flickrt.", f], operands).into()
    }

    /// `&&` and `||`, which evaluate their right operand only when needed.
    fn logical(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Code<'a> {
        let a = self.a;
        let l = self.expr(lhs);
        let (mut rs, rv) = self.block(|g| g.expr(rhs));
        let (l, r) = (
            self.unboxed(lhs, l, Native::Bool).doc,
            self.unboxed(rhs, rv, Native::Bool).doc,
        );
        if rs.is_empty() {
            let go = if op == BinOp::And { " && " } else { " || " };
            return docvec![a, "(", l, go, r, ")"].into();
        }
        // either way the result is a Go `bool`, as `native` says of it
        let t = self.temp();
        let test = if op == BinOp::And { "" } else { "!" };
        rs.push(statement(a, docvec![a, t, " = ", r]));
        self.emit(docvec![a, "var ", t, " bool = ", l]);
        self.emit(docvec![a, "if ", test, t, " ", block(a, rs)]);
        self.code(t)
    }
}
