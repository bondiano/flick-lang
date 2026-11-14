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

pub use flick_backend_support::consteval::Const;

pub fn eval(node: &Node, src: &str) -> Option<Const> {
    let mut nodes = node.nodes();
    match node.kind {
        K::Literal => literal_value(first_token(node)?, src),
        K::ParenExpr => eval(nodes.next()?, src),
        // a block whose only statement is an expression, e.g. a `let` body
        K::Block => {
            let only = nodes.next()?;
            (nodes.next().is_none() && only.kind != K::LetDecl && only.kind != K::Signature)
                .then(|| eval(only, src))?
        }
        K::PrefixExpr => prefix(first_token(node)?.kind, eval(nodes.next()?, src)?),
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

fn literal_value(token: Token, src: &str) -> Option<Const> {
    let text = token.text(src);
    Some(match token.kind {
        K::Number => Const::Number(literal::number_value(text)?),
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
