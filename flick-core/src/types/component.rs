//! The type of a `component`: a `Ui.View` of its props. Without `setup` it
//! is `Ui.view (fun props -> render)`; with one, `Ui.stateful (fun props ->
//! setup; fun props -> render)`, whose setup runs once per instance in the
//! instance's region `s` and lends what it makes to every render.

use super::Checker;
use super::ty::Type;
use crate::iface::Namespace;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

/// The props pattern, the block of `setup` and the block of `render` of a
/// `ComponentExpr`.
pub struct Parts<'n> {
    pub props: Option<&'n Node>,
    pub setup: Option<&'n Node>,
    pub render: Option<&'n Node>,
}

pub fn parts(node: &Node) -> Parts<'_> {
    let props = node.nodes().find(|n| super::is_pattern(n.kind));
    let mut setup = None;
    let mut render = None;
    let mut section = None;
    for child in &node.children {
        match child {
            Element::Token(t) if matches!(t.kind, K::SetupKw | K::RenderKw) => {
                section = Some(t.kind);
            }
            Element::Node(n) if n.kind == K::Block => match section.take() {
                Some(K::SetupKw) => setup = Some(n),
                Some(K::RenderKw) => render = Some(n),
                _ => {}
            },
            _ => {}
        }
    }
    Parts {
        props,
        setup,
        render,
    }
}

impl Checker<'_> {
    pub(super) fn component(&mut self, node: &Node) -> Type {
        let Parts {
            props,
            setup,
            render,
        } = parts(node);
        let Some(render) = render else {
            return Type::Error;
        };
        let keyword = |kind| node.child_tokens().find(|t| t.kind == kind);
        let span = keyword(K::RenderKw).map_or_else(|| self.span(node), |t| self.token_span(t));
        let maker = if setup.is_some() { "stateful" } else { "view" };
        let maker = self.qualified("Ui", Namespace::Value, maker, span);
        let Type::Fun { param, result, .. } = self.whnf(&maker) else {
            return Type::Error;
        };
        let props: Vec<&Node> = props.into_iter().collect();
        let mut render_ty = None;
        let mut renders = |c: &mut Self, expected: Option<&Type>| {
            let ty = c.function(&props, Some(render), expected, render);
            render_ty = Some(ty.clone());
            ty
        };
        let instance = match setup {
            None => self.function(&props, Some(render), Some(&param), node),
            Some(setup) => {
                let params: Vec<Option<&Node>> = props.iter().copied().map(Some).collect();
                let body = |c: &mut Self, rest: Option<Type>| {
                    c.block_then(setup, rest.as_ref(), Some(&mut renders))
                };
                self.function_with(&params, body, Some(&param), node)
            }
        };
        if let Some(t) = keyword(K::SetupKw) {
            self.note_type(self.token_span(t), &instance);
        }
        if let (Some(t), Some(ty)) = (keyword(K::RenderKw), render_ty.or(Some(instance))) {
            self.note_type(self.token_span(t), &ty);
        }
        *result
    }
}
