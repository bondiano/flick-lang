//! Operators. Int, Float, Bool and String values whose type the IR knows
//! stay Go's own `int32`, `float64`, `bool` and `string` through arithmetic,
//! comparisons and the locals they are bound to; anywhere else they are a
//! `flickrt.Value`, which Go boxes them into.

use flick_pretty::{Document, DocumentArena, docvec};

use super::Gen;
use super::layout::{Code, block, call, statement};
use crate::ir::{BinOp, Expr, ExprKind, Lit, TypeName};
use crate::numeric::{Arith, Num};
use crate::types::lang::{self, Lang};

/// A type whose values are Go's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Native {
    Int,
    Float,
    Bool,
    String,
}

impl Native {
    pub(super) fn go(self) -> &'static str {
        match self {
            Native::Int => "int32",
            Native::Float => "float64",
            Native::Bool => "bool",
            Native::String => "string",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Native::Int => "Int",
            Native::Float => "Float",
            Native::Bool => "Bool",
            Native::String => "String",
        }
    }

    fn of_num(num: Num) -> Option<Native> {
        match num {
            Num::Int => Some(Native::Int),
            Num::Float => Some(Native::Float),
            Num::BigInt => None,
        }
    }

    /// `v`, a `flickrt.Value`, as this type.
    pub(super) fn unbox<'a>(self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        call(a, docvec![a, "flickrt.As", self.name()], [v])
    }

    /// The type of values of type `ty`, when it is one of these.
    pub(super) fn of(ty: Option<&TypeName>) -> Option<Native> {
        let ty = ty?;
        [
            (Lang::Int, Native::Int),
            (Lang::Float, Native::Float),
            (Lang::Bool, Native::Bool),
            (Lang::String, Native::String),
        ]
        .into_iter()
        .find(|&(item, _)| lang::is(item, &ty.module, &ty.name))
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
}

impl<'a> Gen<'a> {
    /// The Go type the code of `e` has when it is not a `flickrt.Value`;
    /// none when it may be one.
    pub(super) fn native(&self, e: &Expr) -> Option<Native> {
        match &e.kind {
            ExprKind::Lit(Lit::Int(_)) => Some(Native::Int),
            // the other floats are variables of the runtime
            ExprKind::Lit(Lit::Float(x))
                if x.is_finite() && (*x != 0.0 || x.is_sign_positive()) =>
            {
                Some(Native::Float)
            }
            ExprKind::Lit(Lit::String(_)) => Some(Native::String),
            ExprKind::Local(name) => self.natives.get(name).copied(),
            ExprKind::Neg(num, _) | ExprKind::Binary(BinOp::Arith(_, num), _, _) => {
                Native::of_num(*num)
            }
            ExprKind::Lit(Lit::Bool(_))
            | ExprKind::Binary(
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Same
                | BinOp::NotSame
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge,
                _,
                _,
            ) => Some(Native::Bool),
            _ => None,
        }
    }

    /// `go`, the code of `e`, as a `want`.
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
        let known = |e: &Expr| self.native(e).or_else(|| Native::of(e.ty.as_deref()));
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
                (BinOp::Eq | BinOp::Same, _) => Some(Comparison::Infix(" == ")),
                (BinOp::NotEq | BinOp::NotSame, _) => Some(Comparison::Infix(" != ")),
                // JS orders strings by UTF-16 code units, and NaN by nothing
                (_, Native::String | Native::Bool) => None,
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
                }
                .into();
            }
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
        if rs.is_empty() {
            let go = if op == BinOp::And { " && " } else { " || " };
            let l = call(a, "flickrt.Bool", [l.doc]);
            let r = call(a, "flickrt.Bool", [rv.doc]);
            return docvec![a, "(", l, go, r, ")"].into();
        }
        let t = self.temp();
        let test = if op == BinOp::And {
            "flickrt.Bool"
        } else {
            "!flickrt.Bool"
        };
        rs.push(statement(a, docvec![a, t, " = ", rv]));
        self.emit(docvec![a, "var ", t, " flickrt.Value = ", l]);
        self.emit(docvec![
            a,
            "if ",
            call(a, test, [a.text(t)]),
            " ",
            block(a, rs)
        ]);
        self.code(t)
    }
}
