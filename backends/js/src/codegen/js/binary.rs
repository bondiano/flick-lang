//! Binary operators: `&&` and `||` short-circuit around statements, `==` by
//! the runtime's equality, and parentheses only where precedence needs them.

use flick_pretty::docvec;

use super::Gen;
use super::doc::{After, Before, Head, Js, MULTILINE, Stmt, block, call, items, stmt_flags};
use crate::ir::{BinOp, Expr, ExprKind};
use crate::numeric::{Arith, Num};
use flick_ir::Span;

impl<'d> Gen<'_, 'd> {
    pub(super) fn binary(&mut self, span: Span, op: BinOp, lhs: &Expr, rhs: &Expr) -> Js<'d> {
        // an operand that binds tighter needs no parentheses; on the right an
        // equal one keeps them, as `a + (b + c)` may round otherwise, unless
        // the operator is associative: `&&`, `||`, string `++`, or right `**`
        let p = operand_precedence(op);
        let binds = |e: &Expr, right: bool| match &e.kind {
            ExprKind::Binary(inner, ..) => {
                let q = precedence(*inner);
                let associative =
                    *inner == op && matches!(op, BinOp::And | BinOp::Or | BinOp::Concat);
                let pow = matches!(op, BinOp::Arith(Arith::Pow, _));
                q > p || (q == p && (associative || right == pow))
            }
            _ => false,
        };
        let operand = |js: Js<'d>, e: &Expr, right: bool| Js {
            paren: js.paren && !binds(e, right),
            ..js
        };
        let a = self.a;
        let infix = |l: Js<'d>, symbol: &'d str, r: Js<'d>, suffix: &'d str| {
            let doc = docvec![
                a,
                l.at(a, Before::Other, After::Other),
                " ",
                symbol,
                " ",
                r.at(a, Before::Other, After::Other),
                suffix
            ];
            l.starting(doc, &[l, r]).in_parens()
        };
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = operand(self.expr(lhs), lhs, false);
            let (rs, rv) = self.block(|g| g.expr(rhs));
            let rv = operand(rv, rhs, true);
            let symbol = if op == BinOp::And { "&&" } else { "||" };
            if rs.is_empty() {
                return infix(l, symbol, rv, "");
            }
            return self.short_branch(l, rv, rs, op);
        }
        let mut js = self.ordered(&[lhs, rhs]);
        let r = operand(js.pop().expect("two operands"), rhs, true);
        let l = operand(js.pop().expect("two operands"), lhs, false);
        let l = l.marked_by(a, mark!(self, span));
        match op {
            BinOp::Eq => {
                let equals = self.rt("equals");
                call(a, equals, &[l, r])
            }
            BinOp::NotEq => {
                let equals = self.rt("equals");
                let equal = call(a, equals, &[l, r]);
                let doc = docvec![a, "!", equal.at(a, Before::Other, After::Other)];
                Js::code(doc, Head::Other, &[equal])
            }
            BinOp::ListConcat => {
                let doc = docvec![
                    a,
                    "(",
                    l.at(a, Before::Delimiter, After::Delimiter),
                    ").concat",
                    items(a, "(", &[r], ")")
                ];
                Js::code(doc, l.after_paren(), &[l, r]).call()
            }
            BinOp::Concat => infix(l, "+", r, ""),
            BinOp::Arith(arith @ (Arith::Add | Arith::Sub), Num::Int) => {
                infix(l, arith.symbol(), r, " | 0")
            }
            BinOp::Arith(Arith::Mul, Num::Int) => call(a, self.leaf("Math.imul"), &[l, r]),
            BinOp::Arith(Arith::Div, Num::Int) => self.runtime_call("divInt", l, r),
            BinOp::Arith(Arith::Rem, Num::Int) => self.runtime_call("remInt", l, r),
            BinOp::Arith(Arith::Pow, Num::Int) => self.runtime_call("powInt", l, r),
            BinOp::Arith(Arith::Div, Num::BigInt) => self.runtime_call("divBigInt", l, r),
            BinOp::Arith(Arith::Rem, Num::BigInt) => self.runtime_call("remBigInt", l, r),
            BinOp::Arith(_, Num::BigInt) if self.options.typescript() => {
                let cast = |value: Js<'d>| {
                    Js::code(
                        docvec![
                            a,
                            value.at(a, Before::Delimiter, After::Other),
                            " as bigint"
                        ],
                        value.lead(),
                        &[value],
                    )
                    .in_parens()
                };
                infix(cast(l), op.symbol(), cast(r), "")
            }
            _ => infix(l, op.symbol(), r, ""),
        }
    }

    fn short_branch(&mut self, l: Js<'d>, rv: Js<'d>, mut rs: Vec<Stmt<'d>>, op: BinOp) -> Js<'d> {
        let a = self.a;
        let t = self.temp();
        let test = if op == BinOp::And {
            t.clone()
        } else {
            format!("!{t}")
        };
        let init = docvec![
            a,
            "let ",
            a.alloc_str(&t),
            " = ",
            l.at(a, Before::Assign, After::Semicolon),
            ";"
        ];
        let assign = docvec![
            a,
            a.alloc_str(&t),
            " = ",
            rv.at(a, Before::Assign, After::Semicolon),
            ";"
        ];
        rs.push(Stmt::new(assign, rv.flags));
        let doc = docvec![a, init, a.line(), "if (", test, ") ", block(a, &rs)];
        self.buf
            .push(Stmt::new(doc, l.flags | stmt_flags(&rs) | MULTILINE));
        self.leaf(&t)
    }

    fn runtime_call(&mut self, name: &str, l: Js<'d>, r: Js<'d>) -> Js<'d> {
        let f = self.rt(name);
        call(self.a, f, &[l, r])
    }
}

/// The JS precedence of an operator as emitted; a call form binds tightest.
#[allow(
    clippy::match_same_arms,
    reason = "the forms of `Int` come before the JS operators"
)]
fn precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 3,
        BinOp::And => 4,
        BinOp::Same | BinOp::NotSame => 8,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 9,
        // `l + r | 0`
        BinOp::Arith(Arith::Add | Arith::Sub, Num::Int) => 5,
        // a runtime or `Math` call
        BinOp::Arith(_, Num::Int) | BinOp::Arith(Arith::Div | Arith::Rem, Num::BigInt) => 17,
        BinOp::Concat | BinOp::Arith(Arith::Add | Arith::Sub, _) => 11,
        BinOp::Arith(Arith::Mul | Arith::Div | Arith::Rem, _) => 12,
        BinOp::Arith(Arith::Pow, _) => 13,
        BinOp::Eq | BinOp::NotEq | BinOp::ListConcat => 17,
    }
}

/// The precedence an operand of `op` must exceed to need no parentheses:
/// that of the JS operator it is written beside.
fn operand_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Arith(Arith::Add | Arith::Sub, Num::Int) => 11,
        _ => precedence(op),
    }
}
