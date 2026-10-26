//! The type of a `component`: a `Ui.View` of its props. Its body is a block:
//! one whose only statement is an expression is `Ui.view (fun props ->
//! body)`; otherwise the statements are the setup of an instance and the
//! last expression its render, `Ui.stateful (fun props -> setup; fun props
//! -> render)`. The setup runs once per instance in the instance's region
//! `s` and lends what it makes to every render.

use super::Checker;
use super::ty::Type;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The props pattern, the body block and its last statement, the render,
/// of a `ComponentExpr`; the body has a setup when it has more statements.
pub struct Parts<'n> {
    pub props: Option<&'n Node>,
    pub body: Option<&'n Node>,
    pub render: Option<&'n Node>,
}

impl Parts<'_> {
    pub fn has_setup(&self) -> bool {
        self.body.is_some_and(|b| b.nodes().count() > 1)
    }
}

pub fn parts(node: &Node) -> Parts<'_> {
    let props = node.nodes().find(|n| super::is_pattern(n.kind));
    let body = node.nodes().find(|n| n.kind == K::Block);
    let render = body.and_then(|b| b.nodes().last());
    Parts {
        props,
        body,
        render,
    }
}

/// Where an editor finds the type of a component's instance: an empty range
/// at its start, which no expression has.
pub fn instance_key(node: &Node) -> (u32, u32) {
    (node.range().0, node.range().0)
}

impl Checker<'_> {
    pub(super) fn component(&mut self, node: &Node) -> Type {
        let parts = parts(node);
        let (Some(body), Some(render)) = (parts.body, parts.render) else {
            return Type::Error;
        };
        let setup = parts.has_setup();
        // the maker is named at `=`, outside the render a maker in which is U006
        let span = node
            .child_tokens()
            .find(|t| t.kind == K::Eq)
            .map_or_else(|| self.span(node), |t| self.token_span(t));
        let maker = if setup { "stateful" } else { "view" };
        let maker = self.qualified("Ui", Namespace::Value, maker, span);
        let Type::Fun { param, result, .. } = self.whnf(&maker) else {
            return Type::Error;
        };
        let props: Vec<&Node> = parts.props.into_iter().collect();
        let instance = if setup {
            let params: Vec<Option<&Node>> = props.iter().copied().map(Some).collect();
            let mut renders = |c: &mut Self, expected: Option<&Type>| {
                c.function(&props, Some(render), expected, render)
            };
            let body = |c: &mut Self, rest: Option<Type>| {
                c.block_then(body, rest.as_ref(), Some(&mut renders))
            };
            self.function_with(&params, body, Some(&param), node)
        } else {
            self.function(&props, Some(body), Some(&param), node)
        };
        let (start, end) = instance_key(node);
        self.note_type(Span::new(self.file, start, end), &instance);
        *result
    }
}
