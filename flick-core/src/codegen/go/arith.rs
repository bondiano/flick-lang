//! Operators. Int, Float, Bool and String values whose type the IR knows
//! stay Go's own `int32`, `float64`, `bool` and `string` through arithmetic,
//! comparisons and the locals they are bound to; anywhere else they are a
//! `flickrt.Value`, which Go boxes them into.

use super::{Gen, indent};
use crate::ir::{BinOp, Expr, ExprKind, Lit};
use crate::numeric::{Arith, Num};
use crate::types::{PRELUDE, Type};

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
    pub(super) fn unbox(self, v: &str) -> String {
        format!("flickrt.As{}({v})", self.name())
    }

    /// The type of values of type `ty`, when it is one of these.
    pub(super) fn of(ty: Option<&Type>) -> Option<Native> {
        let Some(Type::Con { module, name }) = ty else {
            return None;
        };
        if module != PRELUDE {
            return None;
        }
        match name.as_str() {
            "Int" => Some(Native::Int),
            "Float" => Some(Native::Float),
            "Bool" => Some(Native::Bool),
            "String" => Some(Native::String),
            _ => None,
        }
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

impl Gen<'_> {
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
    pub(super) fn unboxed(&self, e: &Expr, go: String, want: Native) -> String {
        if self.native(e) == Some(want) {
            go
        } else {
            want.unbox(&go)
        }
    }

    /// The operands of a scalar operator, as `want`s.
    fn operands(&mut self, lhs: &Expr, rhs: &Expr, want: Native) -> (String, String) {
        let go = self.ordered(&[lhs, rhs]);
        let [l, r] = <[String; 2]>::try_from(go).expect("two operands");
        (self.unboxed(lhs, l, want), self.unboxed(rhs, r, want))
    }

    pub(super) fn neg(&mut self, num: Num, inner: &Expr) -> String {
        let v = self.expr(inner);
        match Native::of_num(num) {
            // a call, as Go's constant `-0.0` is zero
            Some(n) => format!("flickrt.Neg{}({})", n.name(), self.unboxed(inner, v, n)),
            None => format!("flickrt.NegB({v})"),
        }
    }

    /// The type both operands of a comparison have, when it is one Go
    /// compares as Flick does.
    fn compared(&self, lhs: &Expr, rhs: &Expr) -> Option<Native> {
        let known = |e: &Expr| self.native(e).or_else(|| Native::of(e.ty.as_deref()));
        known(lhs).filter(|&n| known(rhs) == Some(n))
    }

    pub(super) fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> String {
        if matches!(op, BinOp::And | BinOp::Or) {
            return self.logical(op, lhs, rhs);
        }
        if let BinOp::Arith(arith, num) = op
            && let Some(n) = Native::of_num(num)
        {
            // through the runtime's functions, so that operands Go knows
            // are constants are not folded with Go's rules
            let (l, r) = self.operands(lhs, rhs, n);
            return format!("flickrt.{}{}({l}, {r})", arith_name(arith), n.name());
        }
        if let Some(n) = self.compared(lhs, rhs) {
            let go = match (op, n) {
                // `==` holds of NaN and NaN, `===` does not
                (BinOp::Eq, Native::Float) => Some("flickrt.EqFloat({l}, {r})"),
                (BinOp::NotEq, Native::Float) => Some("!flickrt.EqFloat({l}, {r})"),
                (BinOp::Eq | BinOp::Same, _) => Some("({l} == {r})"),
                (BinOp::NotEq | BinOp::NotSame, _) => Some("({l} != {r})"),
                // JS orders strings by UTF-16 code units, and NaN by nothing
                (_, Native::String | Native::Bool) => None,
                (BinOp::Lt, _) => Some("({l} < {r})"),
                (BinOp::Le, _) => Some("({l} <= {r})"),
                (BinOp::Gt, _) => Some("({l} > {r})"),
                (BinOp::Ge, _) => Some("({l} >= {r})"),
                _ => None,
            };
            if let Some(go) = go {
                let (l, r) = self.operands(lhs, rhs, n);
                return go.replace("{l}", &l).replace("{r}", &r);
            }
        }
        let go = self.ordered(&[lhs, rhs]);
        let (l, r) = (&go[0], &go[1]);
        let f = match op {
            BinOp::Eq => "Equals",
            BinOp::NotEq => return format!("!flickrt.Equals({l}, {r})"),
            BinOp::Same => "Same",
            BinOp::NotSame => return format!("!flickrt.Same({l}, {r})"),
            BinOp::Lt => "Lt",
            BinOp::Le => "Le",
            BinOp::Gt => "Gt",
            BinOp::Ge => "Ge",
            BinOp::Concat => "Concat",
            BinOp::ListConcat => "ListConcat",
            BinOp::Arith(arith, _) => return format!("flickrt.{}B({l}, {r})", arith_name(arith)),
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        format!("flickrt.{f}({l}, {r})")
    }

    /// `&&` and `||`, which evaluate their right operand only when needed.
    fn logical(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> String {
        let l = self.expr(lhs);
        let (rs, rv) = self.block(|g| g.expr(rhs));
        let go = if op == BinOp::And { "&&" } else { "||" };
        if rs.is_empty() {
            return format!("(flickrt.Bool({l}) {go} flickrt.Bool({rv}))");
        }
        let t = self.temp();
        let test = if op == BinOp::And {
            format!("flickrt.Bool({t})")
        } else {
            format!("!flickrt.Bool({t})")
        };
        self.buf.push(format!(
            "var {t} flickrt.Value = {l}\nif {test} {{\n{}\t{t} = {rv}\n}}",
            indent(&rs)
        ));
        t
    }
}
