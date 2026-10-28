//! Conditions with a shorter form: T040 an `if` that returns its own
//! condition, T041 an `if not` with both branches, T042 an `if` alone in an
//! `else`, T043 a literal on the left of a comparison, T044 a value compared
//! with itself, T045 a condition known before the program runs and T046 a
//! `match` on a `Bool`.

use super::super::Checker;
use super::super::sugar_lints::unparen;
use super::single;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The comparison operators, each with the one that compares the other way.
const FLIPPED: [(K, &str, &str); 6] = [
    (K::EqEq, "==", "=="),
    (K::NotEq, "!=", "!="),
    (K::Lt, "<", ">"),
    (K::Le, "<=", ">="),
    (K::Gt, ">", "<"),
    (K::Ge, ">=", "<="),
];

impl Checker<'_> {
    pub(super) fn if_lints(&mut self, node: &Node) {
        let parts: Vec<&Node> = node.nodes().collect();
        let [condition, then, otherwise] = parts[..] else {
            return;
        };
        let at = self.span(node);
        if let Some(known) = self.known_bool(condition) {
            let (kept, word) = if known {
                (then, "true")
            } else {
                (otherwise, "false")
            };
            let edits = single(kept)
                .filter(|k| self.one_line(k))
                .map(|k| vec![self.replace(node, &self.operand(k))])
                .unwrap_or_default();
            self.lint(
                codes::T045,
                format!("this condition is always `{word}`"),
                self.span(condition),
                ("keep the branch that runs", edits, false),
            );
            return;
        }
        let (Some(then_value), Some(else_value)) = (single(then), single(otherwise)) else {
            return;
        };
        match (self.known_bool(then_value), self.known_bool(else_value)) {
            (Some(true), Some(false)) => {
                let replacement = self.source_of(condition).trim().to_owned();
                let replacement =
                    if super::atomic(unparen(condition)) || condition.kind == K::BinExpr {
                        replacement
                    } else {
                        format!("({replacement})")
                    };
                self.lint(
                    codes::T040,
                    "this `if` returns its condition".to_owned(),
                    at,
                    (
                        "write the condition",
                        vec![self.replace(node, &replacement)],
                        true,
                    ),
                );
                return;
            }
            (Some(false), Some(true)) => {
                let replacement = format!("not {}", self.operand(condition));
                self.lint(
                    codes::T040,
                    "this `if` returns the opposite of its condition".to_owned(),
                    at,
                    (
                        "write `not` of the condition",
                        vec![self.replace(node, &replacement)],
                        true,
                    ),
                );
                return;
            }
            _ => {}
        }
        self.negated(condition, then, otherwise);
        self.lonely_if(node, otherwise);
    }

    /// T041: `if not c then a else b` is `if c then b else a`.
    fn negated(&mut self, condition: &Node, then: &Node, otherwise: &Node) {
        let parts: Vec<&Node> = condition.nodes().collect();
        let (K::PrefixExpr, [negated], true) =
            (condition.kind, &parts[..], condition.has_token(K::NotKw))
        else {
            return;
        };
        // `else ()` is T028: without it the `if not` has nothing to swap
        let unit = single(otherwise).is_some_and(|n| unparen(n).kind == K::UnitExpr);
        if unit || otherwise.kind == K::IfExpr || !self.one_line(then) || !self.one_line(otherwise)
        {
            return;
        }
        let edits = vec![
            self.replace(condition, self.source_of(negated).trim()),
            self.replace(then, self.source_of(otherwise).trim()),
            self.replace(otherwise, self.source_of(then).trim()),
        ];
        self.lint(
            codes::T041,
            "an `if not` with an `else`: the condition without `not` reads directly".to_owned(),
            self.span(condition),
            ("drop `not` and swap the branches", edits, true),
        );
    }

    /// T042: an `else` whose only statement is an `if` is an `else if`.
    fn lonely_if(&mut self, node: &Node, otherwise: &Node) {
        if otherwise.kind != K::Block {
            return;
        }
        let Some(inner) = single(otherwise)
            .map(unparen)
            .filter(|n| n.kind == K::IfExpr)
        else {
            return;
        };
        let Some(keyword) = node.child_tokens().find(|t| t.kind == K::ElseKw) else {
            return;
        };
        let replacement = format!(" {}", self.source_of(inner).trim());
        let edit = self.edit(keyword.end, otherwise.range().1, &replacement);
        // an inner `if` over several lines keeps its own indentation
        let safe = self.one_line(inner);
        self.lint(
            codes::T042,
            "an `if` alone in an `else`: `else if` says it without nesting".to_owned(),
            self.span(inner),
            ("write `else if`", vec![edit], safe),
        );
    }

    /// T043 and T044: a comparison with a literal on the left, or of a
    /// value with itself.
    pub(super) fn comparison_lints(&mut self, node: &Node) {
        let Some((token, op, flipped)) = node.child_tokens().find_map(|t| {
            FLIPPED
                .iter()
                .find(|(k, ..)| *k == t.kind)
                .map(|&(_, o, f)| (t, o, f))
        }) else {
            return;
        };
        let parts: Vec<&Node> = node.nodes().collect();
        let [lhs, rhs] = parts[..] else {
            return;
        };
        let (left, right) = (self.source_of(lhs).trim(), self.source_of(rhs).trim());
        let same = matches!(unparen(lhs).kind, K::NameExpr | K::FieldExpr) && left == right;
        if same {
            let always = matches!(token.kind, K::EqEq | K::Le | K::Ge);
            self.lint(
                codes::T044,
                format!("`{left}` compared with itself is always `{always}`"),
                self.span(node),
                ("compare it with another value", Vec::new(), false),
            );
            return;
        }
        let literal = |n: &Node| unparen(n).kind == K::Literal;
        if literal(lhs) && !literal(rhs) && self.one_line(rhs) {
            let replacement = format!("{right} {flipped} {left}");
            self.lint(
                codes::T043,
                format!("the literal is on the left of `{op}`: the value compared reads first"),
                self.span(node),
                (
                    "put the literal on the right",
                    vec![self.replace(node, &replacement)],
                    true,
                ),
            );
        }
    }

    /// T046: `match b with | true -> a | false -> c` is an `if`.
    pub(super) fn bool_match(&mut self, node: &Node) {
        let mut parts = node.nodes();
        let Some(scrutinee) = parts.next() else {
            return;
        };
        let arms: Vec<&Node> = parts.filter(|n| n.kind == K::MatchArm).collect();
        let [first, second] = arms[..] else {
            return;
        };
        let (Some((a, x)), Some((b, y))) = (self.bool_arm(first), self.bool_arm(second)) else {
            return;
        };
        let (then, otherwise) = match (a, b) {
            (Some(true), Some(false) | None) => (x, y),
            (Some(false), Some(true) | None) => (y, x),
            _ => return,
        };
        let replacement = format!(
            "if {} then {} else {}",
            self.source_of(scrutinee).trim(),
            self.source_of(then).trim(),
            self.source_of(otherwise).trim()
        );
        self.lint(
            codes::T046,
            "a `match` on a `Bool`: `if` says it directly".to_owned(),
            self.span(node),
            (
                "write it as an `if`",
                vec![self.replace(node, &replacement)],
                true,
            ),
        );
    }

    /// The pattern of an arm without a guard, `true`, `false` or `_` as
    /// `None`, and its one-line body.
    fn bool_arm<'n>(&self, arm: &'n Node) -> Option<(Option<bool>, &'n Node)> {
        let mut parts = arm.nodes();
        let (pattern, body) = (parts.next()?, parts.next()?);
        if body.kind != K::Block || !self.one_line(body) {
            return None;
        }
        let body = single(body)?;
        let value = match pattern.kind {
            K::WildcardPat => None,
            K::LitPat if pattern.has_token(K::TrueKw) => Some(true),
            K::LitPat if pattern.has_token(K::FalseKw) => Some(false),
            _ => return None,
        };
        Some((value, body))
    }
}

#[cfg(test)]
#[path = "conditions_tests.rs"]
mod tests;
