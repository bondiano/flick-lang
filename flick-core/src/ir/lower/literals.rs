//! Literals in the lowering: a number takes the type the checker gave it and
//! keeps its value, not its spelling.

use super::Lower;
use crate::ir::Lit;
use crate::numeric::{self, Num};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;

impl Lower<'_> {
    /// Whether `node` is `-` of an `Int` literal, a literal itself:
    /// `-2147483648` is an `Int`, though `2147483648` is not.
    pub(super) fn negative_int(&self, node: &Node, children: &[&Node]) -> bool {
        node.has_token(K::Minus)
            && children.first().is_some_and(|c| {
                c.kind == K::Literal && self.typing.numeric.get(&c.range()) == Some(&Num::Int)
            })
    }

    pub(super) fn literal(&self, node: &Node) -> Lit {
        self.signed_literal(node, false)
    }

    /// The literal `node`, negated when `negative`; a number is of the type
    /// the checker gave it.
    pub(super) fn signed_literal(&self, node: &Node, negative: bool) -> Lit {
        let Some(token) = node.child_tokens().next() else {
            return Lit::Unit;
        };
        let text = token.text(self.text);
        match token.kind {
            K::Number => number(
                self.typing.numeric.get(&node.range()).copied(),
                text,
                negative,
            ),
            K::BigInt => number(Some(Num::BigInt), text, negative),
            K::String => Lit::String(literal::string_value(text)),
            K::TrueKw => Lit::Bool(true),
            K::FalseKw => Lit::Bool(false),
            _ => Lit::Unit,
        }
    }
}

/// A number token as a literal of `num`, `Float` when the checker gave none,
/// negated when `negative`. A value out of range is an error the checker
/// reported, so the module does not compile.
pub(super) fn number(num: Option<Num>, text: &str, negative: bool) -> Lit {
    let sign = if negative { -1 } else { 1 };
    match num {
        Some(Num::BigInt) => {
            let digits = literal::bigint_digits(text);
            Lit::BigInt(if negative {
                format!("-{digits}")
            } else {
                digits
            })
        }
        Some(Num::Int) => Lit::Int(
            numeric::integer_value(text)
                .and_then(|v| i32::try_from(sign * v).ok())
                .unwrap_or_default(),
        ),
        Some(Num::Float) | None => {
            let x = literal::number_value(text).unwrap_or_default();
            Lit::Float(if negative { -x } else { x })
        }
    }
}
