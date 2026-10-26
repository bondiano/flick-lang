//! The type of a `component`: a `Ui.View` of its props.

use super::Checker;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// `component name (props : P) = render e` is `Ui.view (fun props -> e)`;
    /// a `setup` section waits for the types of C7c.
    pub(super) fn component(&mut self, node: &Node) -> Type {
        let mut words = node
            .child_tokens()
            .filter(|t| matches!(t.kind, K::SetupKw | K::RenderKw));
        let Some(word) = words.next() else {
            return Type::Error;
        };
        let span = self.token_span(word);
        if word.kind == K::SetupKw {
            self.error(
                codes::S002,
                span,
                "a component's `setup` is not supported yet: keep its state in top-level atoms",
            );
            return Type::Error;
        }
        let view = self.qualified("Ui", Namespace::Value, "view", span);
        let Type::Fun { param, result, .. } = self.whnf(&view) else {
            return Type::Error;
        };
        self.lambda(node, Some(&param));
        *result
    }
}
