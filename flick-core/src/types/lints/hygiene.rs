//! Expressions with a shorter form: T052 a `fun` that only passes its
//! parameter on (`fun x -> f x` is `f`) and T053 two string literals
//! joined with `++`.

use std::iter;

use super::super::Checker;
use super::super::sugar_lints::unparen;
use super::super::typing::Ref;
use super::single;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    pub(super) fn hygiene_lints(&mut self, node: &Node) {
        match node.kind {
            // the `fun` and the parentheses around it go together
            K::ParenExpr => {
                if let Some(f) = node.nodes().next().filter(|f| f.kind == K::FunExpr) {
                    self.eta(node, f);
                }
            }
            K::Block => {
                if let Some(f) = single(node).filter(|f| f.kind == K::FunExpr) {
                    self.eta(f, f);
                }
            }
            K::BinExpr if node.has_token(K::PlusPlus) => self.joined_literals(node),
            _ => {}
        }
    }

    /// T052: `fun x -> f a x` is `f a`, where `x` occurs nowhere else;
    /// `node` is the `fun` or the parentheses around it.
    fn eta(&mut self, node: &Node, fun: &Node) {
        let parts: Vec<&Node> = fun.nodes().collect();
        let [param, body] = parts[..] else {
            return;
        };
        if param.kind != K::VarPat {
            return;
        }
        let Some(call) = single(body).map(unparen).filter(|c| c.kind == K::AppExpr) else {
            return;
        };
        let args: Vec<&Node> = call.nodes().collect();
        let [head, rest @ .., last] = &args[..] else {
            return;
        };
        let x = self.source_of(param).trim();
        let passes = unparen(last).kind == K::NameExpr && self.source_of(unparen(last)).trim() == x;
        // a constructor is not a function value, and `x` must not be used again
        let constructor = unparen(head).has_token(K::Con) && unparen(head).kind == K::NameExpr;
        let mentioned = iter::once(*head).chain(rest.iter().copied()).any(|n| {
            n.tokens()
                .iter()
                .any(|t| t.kind == K::Ident && t.text(self.text) == x)
        });
        // a class method finds its instance from the type at the call
        let method = matches!(
            self.typing.names.get(&unparen(head).range()),
            Some(Ref::Method { .. })
        );
        if !passes || constructor || mentioned || method {
            return;
        }
        let function = if rest.is_empty() {
            self.source_of(head).trim().to_owned()
        } else {
            let (start, _) = head.range();
            let (_, end) = rest[rest.len() - 1].range();
            let applied = &self.text[start as usize..end as usize];
            if node.kind == K::ParenExpr {
                format!("({applied})")
            } else {
                applied.to_owned()
            }
        };
        // `f a` runs when the `fun` is made rather than when it is called
        let safe = rest.is_empty() && matches!(unparen(head).kind, K::NameExpr | K::FieldExpr);
        self.lint(
            codes::T052,
            format!("a `fun` that only passes `{x}` on: `{function}` is the same function"),
            self.span(node),
            (
                "write the function itself",
                vec![self.replace(node, &function)],
                safe,
            ),
        );
    }

    /// T053: `"a" ++ "b"` is `"ab"`.
    fn joined_literals(&mut self, node: &Node) {
        let parts: Vec<&Node> = node.nodes().collect();
        let [lhs, rhs] = parts[..] else {
            return;
        };
        let plain = |n: &Node| {
            let s = self.source_of(n).trim();
            n.kind == K::Literal
                && s.len() >= 2
                && s.starts_with('"')
                && !s.starts_with("\"\"\"")
                && !s.contains('\n')
        };
        // literals on lines of their own wrap a long text
        if !plain(lhs) || !plain(rhs) || !self.one_line(node) {
            return;
        }
        let (left, right) = (self.source_of(lhs).trim(), self.source_of(rhs).trim());
        let joined = format!("{}{}", &left[..left.len() - 1], &right[1..]);
        self.lint(
            codes::T053,
            "two string literals joined with `++`: one literal says it".to_owned(),
            self.span(node),
            ("write one literal", vec![self.replace(node, &joined)], true),
        );
    }
}

#[cfg(test)]
#[path = "hygiene_tests.rs"]
mod tests;
