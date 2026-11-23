//! Compile-time evaluation of expressions built only from literals, as
//! `ReScript` does: `60 * 60 * 24`, `"a" ++ "b"`, `not false`. The result is the
//! value the expression has at run time, with JS semantics bit for bit:
//! IEEE 754 arithmetic and remainder, `SameValueZero` for `==` on `Float`,
//! UTF-16 order for `<` on `String`.
//!
//! `None` means "evaluate at run time": the expression is not made of literals
//! only, mixes types (the checker reports that), or its result is not certain
//! to match JS exactly (e.g. `**` with a non-integer result) or would throw
//! (`1n / 0n` is a run-time `RangeError`, a defect in Effect).

use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use flick_backend_support::consteval::{self, Op};
use flick_ir::Num;

pub use flick_backend_support::consteval::Const;

pub fn eval(node: &Node, src: &str) -> Option<Const> {
    let mut nodes = node.nodes();
    match node.kind {
        K::Literal => literal_value(first_token(node)?, false, src),
        K::ParenExpr => eval(nodes.next()?, src),
        // a block whose only statement is an expression, e.g. a `let` body
        K::Block => {
            let only = nodes.next()?;
            (nodes.next().is_none() && only.kind != K::LetDecl && only.kind != K::Signature)
                .then(|| eval(only, src))?
        }
        K::PrefixExpr => {
            let op = first_token(node)?.kind;
            let operand = nodes.next()?;
            if op == K::Minus
                && operand.kind == K::Literal
                && let Some(tok) = first_token(operand)
                && tok.kind == K::Number
                && literal::split_number_suffix(tok.text(src)).1.is_some()
            {
                return literal_value(tok, true, src);
            }
            prefix(op, eval(operand, src)?)
        }
        K::BinExpr => {
            let lhs = eval(nodes.next()?, src)?;
            let rhs = eval(nodes.next()?, src)?;
            binary(operator(node)?, lhs, rhs)
        }
        K::IfExpr => {
            let Const::Bool(condition) = eval(nodes.next()?, src)? else {
                return None;
            };
            let (then, otherwise) = (nodes.next()?, nodes.next()?);
            eval(if condition { then } else { otherwise }, src)
        }
        _ => None,
    }
}

fn first_token(node: &Node) -> Option<Token> {
    node.children.iter().find_map(|c| match c {
        Element::Token(t) if !t.kind.is_trivia() => Some(*t),
        _ => None,
    })
}

/// The operator token of a binary expression (the token between its operands).
fn operator(node: &Node) -> Option<K> {
    node.children.iter().find_map(|c| match c {
        Element::Token(t) if t.kind.is_binary_op() => Some(t.kind),
        _ => None,
    })
}

fn literal_value(token: Token, negative: bool, src: &str) -> Option<Const> {
    let text = token.text(src);
    let sign = if negative { -1_i128 } else { 1_i128 };
    Some(match token.kind {
        K::Number => match literal::split_number_suffix(text).1 {
            Some(Num::Int8) => {
                let v = sign * literal::integer_value(text)?;
                Const::Int8(i8::try_from(v).ok()?)
            }
            Some(Num::Int16) => {
                let v = sign * literal::integer_value(text)?;
                Const::Int16(i16::try_from(v).ok()?)
            }
            Some(Num::Int) => {
                let v = sign * literal::integer_value(text)?;
                Const::Number(f64::from(i32::try_from(v).ok()?))
            }
            Some(Num::Int64) => {
                let v = sign * literal::integer_value(text)?;
                Const::Int64(i64::try_from(v).ok()?)
            }
            Some(Num::UInt8) => {
                let v = sign * literal::integer_value(text)?;
                Const::UInt8(u8::try_from(v).ok()?)
            }
            Some(Num::UInt16) => {
                let v = sign * literal::integer_value(text)?;
                Const::UInt16(u16::try_from(v).ok()?)
            }
            Some(Num::UInt32) => {
                let v = sign * literal::integer_value(text)?;
                Const::UInt32(u32::try_from(v).ok()?)
            }
            Some(Num::UInt64) => {
                let v = sign * literal::integer_value(text)?;
                Const::UInt64(u64::try_from(v).ok()?)
            }
            Some(Num::Float32) => {
                let x = literal::float32_value(text)?;
                Const::Float32(if negative { -x } else { x })
            }
            None | Some(Num::Float | Num::BigInt) => {
                let x = literal::number_value(text)?;
                Const::Number(if negative { -x } else { x })
            }
        },
        K::BigInt => Const::BigInt(literal::bigint_value(text)?),
        K::String => Const::String(literal::string_value(text)),
        K::TrueKw => Const::Bool(true),
        K::FalseKw => Const::Bool(false),
        _ => return None,
    })
}

pub(crate) fn prefix(op: K, value: Const) -> Option<Const> {
    consteval::prefix(operation(op)?, value)
}

pub(crate) fn binary(op: K, lhs: Const, rhs: Const) -> Option<Const> {
    consteval::binary(operation(op)?, lhs, rhs)
}

fn operation(op: K) -> Option<Op> {
    Some(match op {
        K::Minus => Op::Minus,
        K::NotKw => Op::NotKw,
        K::Plus => Op::Plus,
        K::Star => Op::Star,
        K::Slash => Op::Slash,
        K::Percent => Op::Percent,
        K::StarStar => Op::StarStar,
        K::EqEq => Op::EqEq,
        K::NotEq => Op::NotEq,
        K::Lt => Op::Lt,
        K::Le => Op::Le,
        K::Gt => Op::Gt,
        K::Ge => Op::Ge,
        K::PlusPlus => Op::PlusPlus,
        K::AndAnd => Op::AndAnd,
        K::OrOr => Op::OrOr,
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
