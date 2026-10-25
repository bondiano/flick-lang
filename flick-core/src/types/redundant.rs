//! FLICK-T021: a `let` annotation of the very type its value synthesizes
//! on its own, as in `let kinds : List Kind = [Kind.a, Kind.b]`, and
//! FLICK-T028: an `else ()` an `if` has without it.

use super::Checker;
use super::bindings::LetParts;
use super::env::Form;
use super::ty::Type;
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// [`Checker::check`], and whether `node` alone already synthesizes
    /// exactly `expected`, with no unknowns left. Only forms that `check`
    /// types as `infer` would are followed; any other form is just checked.
    pub fn check_exact(&mut self, node: &Node, expected: &Type) -> bool {
        let target = self.whnf(expected);
        let children: Vec<&Node> = node.nodes().collect();
        let exact = match (node.kind, &target) {
            (_, Type::Forall { .. }) => {
                self.check(node, expected);
                return false;
            }
            (K::ParenExpr, _) => self.check_exact(children[0], expected),
            // a block of one expression: what `block` would check
            (K::Block, _)
                if children.len() == 1
                    && !matches!(children[0].kind, K::Signature | K::LetDecl) =>
            {
                self.check_exact(children[0], expected)
            }
            (K::TupleExpr, Type::Tuple(items)) if items.len() == children.len() => {
                children.iter().zip(items).fold(true, |all, (child, item)| {
                    self.check_exact(child, item) & all
                })
            }
            (K::ListExpr, _) if !children.is_empty() && self.list_item(&target).is_some() => {
                let item = self.list_item(&target).expect("checked");
                children
                    .iter()
                    .fold(true, |all, child| self.check_exact(child, &item) & all)
            }
            (
                K::Block
                | K::FunExpr
                | K::IfExpr
                | K::MatchExpr
                | K::TryExpr
                | K::ProvideExpr
                | K::ScopeExpr
                | K::CodecExpr
                | K::SqlExpr
                | K::TupleExpr
                | K::ListExpr
                | K::RecordExpr
                | K::TaskExpr,
                _,
            ) => {
                self.check(node, expected);
                return false;
            }
            _ => {
                let actual = self.infer(node);
                let actual = self.zonk(&actual);
                let exact = actual == self.zonk(expected)
                    && !actual.contains(&mut |t| matches!(t, Type::Meta(_) | Type::Error));
                self.expect(&actual, expected, self.span(node));
                exact
            }
        };
        self.note_head(node, expected);
        exact
    }

    /// The warning for `parts.header`, with the edit that deletes `: T`;
    /// none when the header names an alias, a name the value's type lacks.
    pub fn redundant_annotation(&mut self, decl: &Node, parts: &LetParts, header: &Node) {
        let (start, end) = header.range();
        let alias = self
            .typing
            .type_names
            .iter()
            .any(|(&(s, e), (module, name))| {
                start <= s
                    && e <= end
                    && self
                        .env
                        .type_def(module, name)
                        .is_some_and(|d| matches!(d.form, Form::Alias { .. }))
            });
        if alias {
            return;
        }
        let (Some(pattern), Some(colon)) = (
            parts.pattern,
            decl.child_tokens().find(|t| t.kind == K::Colon),
        ) else {
            return;
        };
        let end = header.range().1;
        let written = self.text_of(header);
        self.diagnostics.push(
            Diagnostic::new(
                codes::T021,
                Severity::Warning,
                format!("the value already has type `{written}`; the annotation can be inferred"),
            )
            .with_span(Span::new(self.file, colon.start, end))
            .with_safe_fix(
                "remove the annotation",
                vec![Edit {
                    span: Span::new(self.file, pattern.range().1, end),
                    replacement: String::new(),
                }],
            ),
        );
    }

    /// The warning for an `else ()`, with the edit that deletes it; none when
    /// a comment sits between the branches.
    pub fn redundant_else(&mut self, node: &Node, children: &[&Node], otherwise: &Node) {
        let unit = otherwise.kind == K::Block
            && matches!(otherwise.nodes().collect::<Vec<_>>()[..], [n] if n.kind == K::UnitExpr);
        let (Some(then), Some(keyword)) = (
            children.get(1).filter(|_| unit),
            node.child_tokens().find(|t| t.kind == K::ElseKw),
        ) else {
            return;
        };
        let (start, end) = (then.range().1, otherwise.range().1);
        if self.text[start as usize..end as usize].contains("//") {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                codes::T028,
                Severity::Warning,
                "`else ()` can be left out: an `if` without `else` is `Unit`",
            )
            .with_span(Span::new(self.file, keyword.start, end))
            .with_safe_fix(
                "remove `else ()`",
                vec![Edit {
                    span: Span::new(self.file, start, end),
                    replacement: String::new(),
                }],
            ),
        );
    }
}
