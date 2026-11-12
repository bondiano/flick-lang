//! A branch of an `if` in braces holds statements, as the body after `do`
//! does. A record written there before that is `FLICK-T074`, and its fix
//! puts the record in the parentheses it now needs.

use std::ptr;

use super::Checker;
use super::mutation::Binding;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Whether `block`, a branch of the `if` at `node`, is a record where
    /// braces hold statements: `{ r with f = v }`, or assignments of which
    /// none names a `let mut` variable and one names nothing in scope
    /// (`{ id = 1; name = "Ada" }`). It reports one.
    #[inline(never)]
    pub(super) fn record_in_braces(&mut self, node: &Node, block: &Node) -> bool {
        let around = braces_around(node, block);
        let mut statements = block.nodes();
        let braces = match statements.next() {
            // the parser keeps the old record update whole, braces included;
            // one in the braces of a branch is that block's value
            Some(one) if one.kind == K::RecordUpdate => {
                (around.is_none() && statements.next().is_none()).then(|| one.range())
            }
            Some(_) if self.assigns_unbound(block) => around,
            _ => None,
        };
        let Some((open, close)) = braces else {
            return false;
        };
        let at = |place: u32| Span::new(self.file, place, place);
        let edits = vec![
            Edit {
                span: at(open),
                replacement: "(".to_owned(),
            },
            Edit {
                span: at(close),
                replacement: ")".to_owned(),
            },
        ];
        let diagnostic = Diagnostic::error(
            codes::T074,
            "braces after `then` or `else` hold statements, so a record there goes in parentheses",
            Span::new(self.file, open, close),
        );
        (self.diagnostics)
            .push(diagnostic.with_safe_fix(|| ("put the record in parentheses", edits)));
        true
    }

    /// Whether every statement of `block` assigns, none a `let mut`
    /// variable, and one a name not in scope.
    fn assigns_unbound(&self, block: &Node) -> bool {
        let mut unbound = false;
        for statement in block.nodes() {
            let token = (statement.kind == K::AssignStmt)
                .then(|| statement.nodes().next())
                .flatten()
                .and_then(|target| target.child_tokens().find(|t| t.kind == K::Ident));
            let Some(token) = token else {
                return false;
            };
            let name = token.text(self.text);
            match self.cx.locals.iter().rev().find(|l| l.name == name) {
                Some(local) if local.binding == Binding::Mut => return false,
                Some(_) => {}
                None => unbound = true,
            }
        }
        unbound
    }
}

/// Where the `{` before `block` starts and the `}` after it ends, among
/// the children of `node`.
fn braces_around(node: &Node, block: &Node) -> Option<(u32, u32)> {
    let index = node
        .children
        .iter()
        .position(|child| matches!(child, Element::Node(n) if ptr::eq(n, block)))?;
    let token = |element: &Element| match element {
        Element::Token(t) if !t.kind.is_trivia() => Some(*t),
        _ => None,
    };
    let open = node.children[..index].iter().rev().find_map(token)?;
    let close = node.children[index + 1..].iter().find_map(token)?;
    (open.kind == K::LBrace && close.kind == K::RBrace).then_some((open.start, close.end))
}
