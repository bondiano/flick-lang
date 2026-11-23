//! Binary operators: `&&` and `||` short-circuit around statements, `==` by
//! the runtime's equality, and parentheses only where precedence needs them.

use flick_pretty::docvec;

use super::Gen;
use super::doc::{After, Before, Head, Js, MULTILINE, Stmt, block, call, items, stmt_flags};
use crate::ir::{BinOp, Expr, ExprKind};
use crate::numeric::{Arith, Num};
use flick_ir::Span;

impl<'d> Gen<'_, 'd> {
    /// `-inner`, kept in its target bit-width on fixed-width integer types;
    /// a signed `Int` literal is in range already.
    pub(super) fn neg(&mut self, num: Num, inner: &Expr) -> Js<'d> {
        let js = self.expr(inner);
        let a = self.a;
        let bigint_arg = |value: Js<'d>| {
            if self.options.typescript() {
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
            } else {
                value
            }
        };
        let negated = |value: Js<'d>| {
            Js::code(
                docvec![a, "-", value.at(a, Before::Other, After::Delimiter)],
                Head::Other,
                &[value],
            )
            .in_parens()
        };
        match num {
            Num::Int if !matches!(inner.kind, ExprKind::Lit(_)) => {
                let doc = docvec![a, "-", js.at(a, Before::Other, After::Other), " | 0"];
                Js::code(doc, Head::Other, &[js]).in_parens()
            }
            Num::Int8 => {
                let n = negated(js);
                let doc = docvec![a, n.at(a, Before::Other, After::Other), " << 24 >> 24"];
                Js::code(doc, Head::Other, &[n]).in_parens()
            }
            Num::Int16 => {
                let n = negated(js);
                let doc = docvec![a, n.at(a, Before::Other, After::Other), " << 16 >> 16"];
                Js::code(doc, Head::Other, &[n]).in_parens()
            }
            Num::UInt8 => {
                let n = negated(js);
                let doc = docvec![a, n.at(a, Before::Other, After::Other), " & 255"];
                Js::code(doc, Head::Other, &[n]).in_parens()
            }
            Num::UInt16 => {
                let n = negated(js);
                let doc = docvec![a, n.at(a, Before::Other, After::Other), " & 65535"];
                Js::code(doc, Head::Other, &[n]).in_parens()
            }
            Num::UInt32 => {
                let n = negated(js);
                let doc = docvec![a, n.at(a, Before::Other, After::Other), " >>> 0"];
                Js::code(doc, Head::Other, &[n]).in_parens()
            }
            Num::Int64 => {
                let n = negated(bigint_arg(js));
                call(a, self.leaf("BigInt.asIntN"), &[self.leaf("64"), n])
            }
            Num::UInt64 => {
                let n = negated(bigint_arg(js));
                call(a, self.leaf("BigInt.asUintN"), &[self.leaf("64"), n])
            }
            Num::Int | Num::Float | Num::Float32 | Num::BigInt => negated(js),
        }
    }

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
            BinOp::Arith(arith, num) => self.arith_binary(arith, num, l, r),
            _ => infix(l, op.symbol(), r, ""),
        }
    }

    fn arith_binary(&mut self, arith: Arith, num: Num, l: Js<'d>, r: Js<'d>) -> Js<'d> {
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
        let cast_bigint = |value: Js<'d>| {
            if self.options.typescript() {
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
            } else {
                value
            }
        };
        let wrapped_infix = |l: Js<'d>, symbol: &'d str, r: Js<'d>, wrap: &'d str| {
            let inner = infix(l, symbol, r, "");
            let doc = docvec![a, inner.at(a, Before::Other, After::Other), wrap];
            Js::code(doc, Head::Other, &[inner]).in_parens()
        };
        match (arith, num) {
            (Arith::Add | Arith::Sub, Num::Int) => infix(l, arith.symbol(), r, " | 0"),
            (Arith::Mul, Num::Int) => call(a, self.leaf("Math.imul"), &[l, r]),
            (Arith::Div, Num::Int) => self.runtime_call("divInt", l, r),
            (Arith::Rem, Num::Int) => self.runtime_call("remInt", l, r),
            (Arith::Pow, Num::Int) => self.runtime_call("powInt", l, r),
            (Arith::Add | Arith::Sub | Arith::Mul, Num::Int8) => {
                wrapped_infix(l, arith.symbol(), r, " << 24 >> 24")
            }
            (Arith::Div, Num::Int8) => self.runtime_call("divInt8", l, r),
            (Arith::Rem, Num::Int8) => self.runtime_call("remInt8", l, r),
            (Arith::Pow, Num::Int8) => self.runtime_call("powInt8", l, r),
            (Arith::Add | Arith::Sub | Arith::Mul, Num::Int16) => {
                wrapped_infix(l, arith.symbol(), r, " << 16 >> 16")
            }
            (Arith::Div, Num::Int16) => self.runtime_call("divInt16", l, r),
            (Arith::Rem, Num::Int16) => self.runtime_call("remInt16", l, r),
            (Arith::Pow, Num::Int16) => self.runtime_call("powInt16", l, r),
            (Arith::Add | Arith::Sub | Arith::Mul, Num::UInt8) => {
                wrapped_infix(l, arith.symbol(), r, " & 255")
            }
            (Arith::Add | Arith::Sub | Arith::Mul, Num::UInt16) => {
                wrapped_infix(l, arith.symbol(), r, " & 65535")
            }
            (Arith::Add | Arith::Sub, Num::UInt32) => wrapped_infix(l, arith.symbol(), r, " >>> 0"),
            (Arith::Mul, Num::UInt32) => {
                let prod = call(a, self.leaf("Math.imul"), &[l, r]);
                let doc = docvec![a, prod.at(a, Before::Other, After::Other), " >>> 0"];
                Js::code(doc, Head::Other, &[prod]).in_parens()
            }
            (Arith::Div, Num::UInt8 | Num::UInt16 | Num::UInt32) => {
                self.runtime_call("divUInt", l, r)
            }
            (Arith::Rem, Num::UInt8 | Num::UInt16 | Num::UInt32) => {
                self.runtime_call("remUInt", l, r)
            }
            (Arith::Pow, Num::UInt8) => self.runtime_call("powUInt8", l, r),
            (Arith::Pow, Num::UInt16) => self.runtime_call("powUInt16", l, r),
            (Arith::Pow, Num::UInt32) => self.runtime_call("powUInt32", l, r),
            (Arith::Add | Arith::Sub | Arith::Mul, Num::Int64) => {
                let expr = infix(cast_bigint(l), arith.symbol(), cast_bigint(r), "");
                call(a, self.leaf("BigInt.asIntN"), &[self.leaf("64"), expr])
            }
            (Arith::Div, Num::Int64) => self.runtime_call("divInt64", l, r),
            (Arith::Rem, Num::Int64) => self.runtime_call("remInt64", l, r),
            (Arith::Pow, Num::Int64) => self.runtime_call("powInt64", l, r),
            (Arith::Add | Arith::Sub | Arith::Mul, Num::UInt64) => {
                let expr = infix(cast_bigint(l), arith.symbol(), cast_bigint(r), "");
                call(a, self.leaf("BigInt.asUintN"), &[self.leaf("64"), expr])
            }
            (Arith::Div, Num::UInt64) => self.runtime_call("divUInt64", l, r),
            (Arith::Rem, Num::UInt64) => self.runtime_call("remUInt64", l, r),
            (Arith::Pow, Num::UInt64) => self.runtime_call("powUInt64", l, r),
            (_, Num::Float32) => {
                let expr = infix(l, arith.symbol(), r, "");
                call(a, self.leaf("Math.fround"), &[expr])
            }
            (Arith::Div, Num::BigInt) => self.runtime_call("divBigInt", l, r),
            (Arith::Rem, Num::BigInt) => self.runtime_call("remBigInt", l, r),
            (_, Num::BigInt) if self.options.typescript() => {
                infix(cast_bigint(l), arith.symbol(), cast_bigint(r), "")
            }
            (_, Num::Float | Num::BigInt) => infix(l, arith.symbol(), r, ""),
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
        // `l + r | 0` or masked/shifted fixed-width expressions
        BinOp::Arith(Arith::Add | Arith::Sub, Num::Int | Num::UInt32)
        | BinOp::Arith(
            Arith::Add | Arith::Sub | Arith::Mul,
            Num::Int8 | Num::Int16 | Num::UInt8 | Num::UInt16,
        )
        | BinOp::Arith(Arith::Mul, Num::UInt32) => 5,
        // a runtime, `Math`, or `BigInt.as*N` call
        BinOp::Arith(_, Num::Int | Num::Int64 | Num::UInt64 | Num::Float32)
        | BinOp::Arith(
            Arith::Div | Arith::Rem | Arith::Pow,
            Num::Int8 | Num::Int16 | Num::UInt8 | Num::UInt16 | Num::UInt32,
        )
        | BinOp::Arith(Arith::Div | Arith::Rem, Num::BigInt) => 17,
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
        BinOp::Arith(
            Arith::Add | Arith::Sub,
            Num::Int
            | Num::Int8
            | Num::Int16
            | Num::UInt8
            | Num::UInt16
            | Num::UInt32
            | Num::Int64
            | Num::UInt64
            | Num::Float32,
        ) => 11,
        BinOp::Arith(
            Arith::Mul,
            Num::Int8
            | Num::Int16
            | Num::UInt8
            | Num::UInt16
            | Num::Int64
            | Num::UInt64
            | Num::Float32,
        )
        | BinOp::Arith(Arith::Div | Arith::Rem, Num::Float32) => 12,
        BinOp::Arith(Arith::Pow, Num::Float32) => 13,
        _ => precedence(op),
    }
}
