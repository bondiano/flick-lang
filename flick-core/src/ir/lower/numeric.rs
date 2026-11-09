//! Numbers in the lowering: arithmetic by its numeric type, and on a type
//! variable the methods of its numeric class.

use super::Lower;
use super::expr::with_dicts;
use super::literals::number;
use crate::ir::{Expr, ExprKind, Stage};
use crate::numeric::{self, Arith, Num};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Range;
use crate::types::lang::{self, Lang};

impl Lower<'_> {
    /// `lhs op rhs` on a type variable, by the methods of its `Numeric` or
    /// the comparisons of its `Ord`.
    pub(super) fn generic_binary(
        &mut self,
        key: Range,
        op: K,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Option<Expr> {
        let args = [Box::new(lhs.clone()), Box::new(rhs.clone())];
        if self.typing.generic_numeric.contains(&key) {
            let method = self.prelude_method(key, arith_method(op)?);
            return Some(apply(method, args, span));
        }
        if self.typing.generic_ordered.contains(&key) {
            let function = lang::item(ord_function(op)?);
            let global = ExprKind::Global {
                module: function.module.clone(),
                name: function.name.clone(),
            };
            let dicts = self.dictionaries(key);
            return Some(apply(
                with_dicts(Expr::new(global, span), dicts),
                args,
                span,
            ));
        }
        None
    }

    /// A number literal on a type variable, made by its class's method.
    pub(super) fn generic_literal(&mut self, node: &Node, span: Span) -> Expr {
        let text = node.child_tokens().next().map_or("", |t| t.text(self.text));
        let (make, num) = if numeric::is_integer_literal(text) {
            (Lang::FromInt, Num::Int)
        } else {
            (Lang::FromFloat, Num::Float)
        };
        let lit = Box::new(Expr::new(
            ExprKind::Lit(number(Some(num), text, false)),
            span,
        ));
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

/// The method of `Numeric` an operator on a type variable calls.
fn arith_method(op: K) -> Option<Lang> {
    Some(match op {
        K::Plus => Lang::Add,
        K::Minus => Lang::Sub,
        K::Star => Lang::Mul,
        K::Slash => Lang::Div,
        K::Percent => Lang::Rem,
        K::StarStar => Lang::Pow,
        K::Lt => Lang::Lt,
        K::Le => Lang::Le,
        K::Gt => Lang::Gt,
        K::Ge => Lang::Ge,
        _ => return None,
    })
}

/// The built-in function a comparison on a type variable with only
/// `Ord` calls.
fn ord_function(op: K) -> Option<Lang> {
    Some(match op {
        K::Lt => Lang::OrdLt,
        K::Le => Lang::OrdLe,
        K::Gt => Lang::OrdGt,
        K::Ge => Lang::OrdGe,
        _ => return None,
    })
}
