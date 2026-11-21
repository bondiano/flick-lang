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
    /// none names a `let mut` variable and one is a field's: it names
    /// nothing in scope (`{ id = 1; name = "Ada" }`), or, when the `if` has
    /// an `else` and so a value, a parameter, a pattern variable or a
    /// function, or itself (`{ id = id }`). It reports one.
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
            Some(_) if self.assigns_fields(block, node.has_token(K::ElseKw)) => around,
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
    /// variable, and one a field: a name not in scope or, with `valued`, a
    /// name no `let` binds, or the name itself. A plain `let` is left to
    /// `FLICK-T069`, which says how to make it a variable.
    pub(super) fn assigns_fields(&self, block: &Node, valued: bool) -> bool {
        let mut field = false;
        for statement in block.nodes() {
            let target = (statement.kind == K::AssignStmt)
                .then(|| statement.nodes().next())
                .flatten();
            let token = target.and_then(|t| t.child_tokens().find(|t| t.kind == K::Ident));
            let Some(token) = token else {
                return false;
            };
            let name = token.text(self.text);
            let itself = || {
                statement
                    .nodes()
                    .nth(1)
                    .is_some_and(|value| self.text_of(value) == name)
            };
            match self.cx.locals.iter().rev().find(|l| l.name == name) {
                Some(local) if local.binding == Binding::Mut => return false,
                Some(local) => field |= valued && (local.binding == Binding::Other || itself()),
                None => field = true,
            }
        }
        field
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

#[cfg(test)]
mod tests {
    use crate::types::lint_tests::assert_fix;

    /// Fields named after the parameters they take: the fix makes the
    /// braces the record they were written as.
    #[test]
    fn fields_named_after_parameters_are_put_in_parentheses() {
        let signature = "named : Int -> String -> Bool -> { id : Int; name : String }\n";
        assert_fix(
            &format!(
                "{signature}let named id name yes = if yes then {{ id = id; name = name }} else ({{ id = 0; name = \"\" }})\n"
            ),
            "FLICK-T074",
            &format!(
                "{signature}let named id name yes = if yes then ({{ id = id; name = name }}) else ({{ id = 0; name = \"\" }})\n"
            ),
        );
    }
}
