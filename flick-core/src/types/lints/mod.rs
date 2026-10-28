//! Warnings about the shape of code: the size and branching of a function
//! ([`structure`]), conditions that say more than they need
//! ([`conditions`]), list and option code a library function says
//! directly ([`collections`]), expressions with a shorter form
//! ([`hygiene`]) and names and arms that hide something ([`scope`]). They
//! run once the module is solved, beside `sugar_lints`.

mod collections;
mod conditions;
mod hygiene;
mod scope;
mod structure;

use super::Checker;
use crate::consteval::{self, Const};
use crate::diagnostics::{Code, Diagnostic, Edit, Severity};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The lints of `node` itself; the caller walks its children.
    pub(super) fn shape_lints(&mut self, node: &Node) {
        if self.builtin {
            return;
        }
        match node.kind {
            K::SourceFile => self.scope_lints(node),
            K::LetDecl => self.structure(node),
            K::IfExpr => self.if_lints(node),
            K::BinExpr => self.comparison_lints(node),
            K::MatchExpr => self.bool_match(node),
            _ => {}
        }
        self.collection_lints(node);
        self.hygiene_lints(node);
    }

    /// A warning with the fix `fix` of `edits`, safe when `safe`.
    fn lint(&mut self, code: Code, message: String, at: Span, fix: (&str, Vec<Edit>, bool)) {
        let warning = Diagnostic::new(code, Severity::Warning, message).with_span(at);
        let (help, edits, safe) = fix;
        self.diagnostics.push(if safe {
            warning.with_safe_fix(help, edits)
        } else {
            warning.with_fix(help, edits)
        });
    }

    /// The edit that writes `replacement` over `node`.
    fn replace(&self, node: &Node, replacement: &str) -> Edit {
        let (start, end) = node.range();
        self.edit(start, end, replacement)
    }

    /// `node` as written, in parentheses unless it is one operand already.
    fn operand(&self, node: &Node) -> String {
        let source = self.source_of(node).trim();
        if atomic(node) {
            source.to_owned()
        } else {
            format!("({source})")
        }
    }

    /// The value of a condition known before the program runs.
    fn known_bool(&self, node: &Node) -> Option<bool> {
        match consteval::eval(node, self.text)? {
            Const::Bool(b) => Some(b),
            _ => None,
        }
    }

    fn one_line(&self, node: &Node) -> bool {
        !self.source_of(node).trim().contains('\n')
    }
}

/// The only statement of a block, when it is an expression.
fn single(node: &Node) -> Option<&Node> {
    if node.kind != K::Block {
        return Some(node);
    }
    let mut statements = node.nodes();
    let only = statements.next()?;
    (statements.next().is_none() && !matches!(only.kind, K::LetDecl | K::Signature)).then_some(only)
}

/// Whether `node` stays one operand wherever it is written.
fn atomic(node: &Node) -> bool {
    matches!(
        node.kind,
        K::NameExpr
            | K::FieldExpr
            | K::ParenExpr
            | K::Literal
            | K::TupleExpr
            | K::UnitExpr
            | K::ListExpr
            | K::RecordExpr
            | K::AppExpr
    )
}
