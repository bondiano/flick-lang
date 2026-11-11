//! "Extract a variable" from a selected expression, and "inline" a local
//! `let` into its uses.

use super::super::{Ide, Symbol, locals, tree};
use super::{Action, edit, indent_at, line_start, starts_line};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// Expressions that need no parentheses wherever they go.
const ATOMIC: [K; 9] = [
    K::Literal,
    K::NameExpr,
    K::ParenExpr,
    K::TupleExpr,
    K::UnitExpr,
    K::ListExpr,
    K::RecordExpr,
    K::RecordUpdate,
    K::FieldExpr,
];

/// Expressions worth a name of their own.
const EXTRACTABLE: [K; 6] = [
    K::AppExpr,
    K::BinExpr,
    K::PrefixExpr,
    K::FieldExpr,
    K::TupleExpr,
    K::ListExpr,
];

impl Ide<'_> {
    pub(super) fn extract(&self, file: FileId, start: u32, end: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let selected = &text[start as usize..end as usize];
        let start = start + super::to_u32(selected.len() - selected.trim_start().len());
        let end = end - super::to_u32(selected.len() - selected.trim_end().len());
        if start >= end {
            return None;
        }
        let node = tree::find(&m.tree, (start, end))?;
        let mut node = node;
        while let Some(inner) = node.nodes().find(|n| n.range() == (start, end)) {
            node = inner;
        }
        // moved before its statement, a read of a variable could pass an assignment
        if !EXTRACTABLE.contains(&node.kind) || self.uses_variable(file, node) {
            return None;
        }
        // the statement of the innermost block around it, on a line of its own
        let path = tree::path_to(&m.tree, start);
        let b = path
            .iter()
            .rposition(|n| n.kind == K::Block && n.range().0 <= start)?;
        let statement = *path.get(b + 1)?;
        if statement.range() == node.range() || !starts_line(text, statement.range().0) {
            return None;
        }
        let taken: Vec<String> = locals::in_scope(&m.tree, start, text)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        let name = (1..=taken.len() + 1)
            .map(|i| {
                if i == 1 {
                    "value".to_owned()
                } else {
                    format!("value{i}")
                }
            })
            .find(|n| !taken.contains(n))?;
        // `(e)` becomes the name, not `(name)`
        let (start, end) = match path
            .iter()
            .rev()
            .find(|n| n.kind == K::ParenExpr && n.range().0 < start)
        {
            Some(paren) if paren.nodes().map(Node::range).eq([node.range()]) => paren.range(),
            _ => (start, end),
        };
        let at = statement.range().0;
        let indent = indent_at(text, at);
        Some(Action {
            title: format!("extract `{name}`"),
            kind: "refactor.extract",
            edits: vec![
                edit(
                    file,
                    at,
                    at,
                    format!("let {name} = {}\n{indent}", node.text(text)),
                ),
                edit(file, start, end, name),
            ],
        })
    }

    pub(super) fn inline(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let (symbol, _) = self.symbol_at(file, offset)?;
        let Symbol::Local {
            binder, ref name, ..
        } = symbol
        else {
            return None;
        };
        let path = tree::path_to(&m.tree, binder.0);
        let d = path.iter().rposition(|n| n.kind == K::LetDecl)?;
        let decl = path[d];
        if d == 0 || path[d - 1].kind != K::Block || !starts_line(text, decl.range().0) {
            return None;
        }
        let pattern = decl.nodes().find(|n| n.kind == K::VarPat)?;
        if pattern.range() != binder
            || decl.nodes().any(|n| n.kind == K::Params)
            || decl.has_token(K::MutKw)
        {
            return None;
        }
        let value = only_expression(decl.nodes().find(|n| n.kind == K::Block)?)?;
        // at a later use, a variable it reads may hold another value
        if self.uses_variable(file, value) {
            return None;
        }
        let shown = value.text(text);
        if shown.contains('\n') {
            return None;
        }
        let replacement = if ATOMIC.contains(&value.kind) {
            shown
        } else {
            format!("({shown})")
        };
        let uses: Vec<(u32, u32)> = self
            .references(&symbol)
            .into_iter()
            .map(|(_, r)| r)
            .filter(|r| *r != binder)
            .collect();
        if uses.is_empty() {
            return None;
        }
        let start = line_start(text, decl.range().0);
        let line_end = text[decl.range().1 as usize..]
            .find('\n')
            .map_or(text.len(), |i| decl.range().1 as usize + i + 1);
        let mut edits = vec![edit(file, start, super::to_u32(line_end), "")];
        edits.extend(
            uses.iter()
                .map(|&(s, e)| edit(file, s, e, replacement.clone())),
        );
        Some(Action {
            title: format!("inline `{name}`"),
            kind: "refactor.inline",
            edits,
        })
    }
}

impl Ide<'_> {
    /// Whether `node` reads or assigns a `let mut` variable.
    fn uses_variable(&self, file: FileId, node: &Node) -> bool {
        if node.kind == K::AssignStmt {
            return true;
        }
        if node.kind == K::NameExpr
            && let Some((Symbol::Local { binder, .. }, _)) = self.symbol_at(file, node.range().0)
            && let Some(m) = self.module(file)
            && let Some(decl) =
                (tree::path_to(&m.tree, binder.0).into_iter().rev()).find(|n| n.kind == K::LetDecl)
            && decl.has_token(K::MutKw)
        {
            return true;
        }
        node.nodes().any(|n| self.uses_variable(file, n))
    }
}

/// The one expression of a block without statements.
fn only_expression(block: &Node) -> Option<&Node> {
    let mut nodes = block.nodes();
    match (nodes.next(), nodes.next()) {
        (Some(e), None) if e.kind != K::LetDecl => Some(e),
        _ => None,
    }
}
