//! Numbers in the lowering: arithmetic by its numeric type, and on a type
//! variable the methods of its numeric class.

use super::Lower;
use crate::ir::{Expr, ExprKind, Stage};
use crate::numeric::{self, Arith, Num};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Range;

impl Lower<'_> {
    /// A number literal on a type variable, made by its class's method.
    pub(super) fn generic_literal(&mut self, node: &Node, span: Span) -> Expr {
        let text = node.child_tokens().next().map_or("", |t| t.text(self.text));
        let make = if numeric::is_integer_literal(text) {
            "from_int"
        } else {
            "from_float"
        };
        let lit = Box::new(Expr::new(ExprKind::Lit(self.literal(node)), span));
        let make = self.prelude_method(node.range(), make);
        apply(make, [lit], span)
    }

    /// The numeric type of the arithmetic at `key`.
    pub(super) fn num(&self, key: Range) -> Num {
        // an operator the checker rejected keeps JS arithmetic; the module does not compile
        self.typing.numeric.get(&key).copied().unwrap_or(Num::Float)
    }
}

/// `func` applied to `args` one at a time, at once.
pub(super) fn apply(func: Expr, args: impl IntoIterator<Item = Box<Expr>>, span: Span) -> Expr {
    args.into_iter().fold(func, |func, arg| {
        let app = ExprKind::App {
            func: Box::new(func),
            arg,
            stage: Stage::Pure,
            arg_first: false,
        };
        Expr::new(app, span)
    })
}

/// The arithmetic of an operator token.
pub(super) fn arith_of(op: K) -> Option<Arith> {
    Some(match op {
        K::Plus => Arith::Add,
        K::Minus => Arith::Sub,
        K::Star => Arith::Mul,
        K::Slash => Arith::Div,
        K::Percent => Arith::Rem,
        K::StarStar => Arith::Pow,
        _ => return None,
    })
}

/// The method of `Numeric` an arithmetic operator on a type variable calls.
pub(super) fn arith_method(op: K) -> Option<&'static str> {
    Some(match op {
        K::Plus => "add",
        K::Minus => "sub",
        K::Star => "mul",
        K::Slash => "div",
        K::Percent => "rem",
        K::StarStar => "pow",
        _ => return None,
    })
}
