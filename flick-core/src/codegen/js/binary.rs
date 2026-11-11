//! Binary operators: `&&` and `||` short-circuit around statements, `==` by
//! the runtime's equality, and parentheses only where precedence needs them.

use super::{Gen, indent, tidy};
use crate::ir::{BinOp, Expr, ExprKind};
use crate::numeric::{Arith, Num};
use crate::source::Span;

impl Gen<'_> {
    pub(super) fn binary(&mut self, span: Span, op: BinOp, lhs: &Expr, rhs: &Expr) -> String {
        // an operand that binds tighter needs no parentheses; on the right an
        // equal one keeps them, as `a + (b + c)` may round otherwise, unless
        // the operator is associative: `&&`, `||`, string `++`, or right `**`
        let p = tidy::operand_precedence(op);
        let binds = |e: &Expr, right: bool| match &e.kind {
            ExprKind::Binary(inner, ..) => {
                let q = tidy::precedence(*inner);
                let associative =
                    *inner == op && matches!(op, BinOp::And | BinOp::Or | BinOp::Concat);
                let pow = matches!(op, BinOp::Arith(Arith::Pow, _));
                q > p || (q == p && (associative || right == pow))
            }
            _ => false,
        };
        let operand = |js: String, e: &Expr, right: bool| {
            if binds(e, right) {
                tidy::unwrap(&js)
            } else {
                js
            }
        };
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = operand(self.expr(lhs), lhs, false);
            let (rs, rv) = self.block(|g| g.expr(rhs));
            let rv = operand(rv, rhs, true);
            let js = if op == BinOp::And { "&&" } else { "||" };
            if rs.is_empty() {
                return format!("({l} {js} {rv})");
            }
            let t = self.temp();
            let test = if op == BinOp::And {
                t.clone()
            } else {
                format!("!{t}")
            };
            self.buf.push(format!(
                "let {t} = {l};\nif ({test}) {{\n{}  {t} = {rv};\n}}",
                indent(&rs)
            ));
            return t;
        }
        let mut js = self.ordered(&[lhs, rhs]);
        let r = operand(js.pop().unwrap_or_default(), rhs, true);
        let l = operand(js.pop().unwrap_or_default(), lhs, false);
        #[cfg(feature = "native-project")]
        let l = format!("{}{l}", self.mark(span));
        #[cfg(not(feature = "native-project"))]
        let _ = span;
        let (l, r) = (&l, &r);
        match op {
            BinOp::Eq => format!("{}({l}, {r})", self.rt("equals")),
            BinOp::NotEq => format!("!{}({l}, {r})", self.rt("equals")),
            BinOp::ListConcat => format!("({l}).concat({r})"),
            BinOp::Concat => format!("({l} + {r})"),
            BinOp::Arith(arith @ (Arith::Add | Arith::Sub), Num::Int) => {
                format!("({l} {} {r} | 0)", arith.symbol())
            }
            BinOp::Arith(Arith::Mul, Num::Int) => format!("Math.imul({l}, {r})"),
            BinOp::Arith(Arith::Div, Num::Int) => format!("{}({l}, {r})", self.rt("divInt")),
            BinOp::Arith(Arith::Rem, Num::Int) => format!("{}({l}, {r})", self.rt("remInt")),
            BinOp::Arith(Arith::Pow, Num::Int) => format!("{}({l}, {r})", self.rt("powInt")),
            BinOp::Arith(Arith::Div, Num::BigInt) => format!("{}({l}, {r})", self.rt("divBigInt")),
            BinOp::Arith(Arith::Rem, Num::BigInt) => format!("{}({l}, {r})", self.rt("remBigInt")),
            // TypeScript types `any * any` as a number, which no `bigint` meets
            BinOp::Arith(_, Num::BigInt) if self.options.typescript() => {
                format!("(({l} as bigint) {} ({r} as bigint))", op.symbol())
            }
            _ => format!("({l} {} {r})", op.symbol()),
        }
    }
}
