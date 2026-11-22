//! Document highlights: control-flow highlighting for `let ... else` (showing
//! the branch keywords, fallback, and the exact block remainder skipped on
//! mismatch) and same-file symbol occurrences.

use super::{Ide, Symbol, tree};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// Highlight kind, matching LSP `DocumentHighlightKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Text = 1,
    Read = 2,
    Write = 3,
}

/// A highlighted UTF-8 byte range in the current file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    pub start: u32,
    pub end: u32,
    pub kind: HighlightKind,
}

impl Ide<'_> {
    /// Highlights for the cursor at `offset` in `file`.
    ///
    /// On the `let` or `else` keyword of `let ... else`, highlights the
    /// keywords, the fallback block, and the exact remainder of the nearest
    /// enclosing block that is skipped on mismatch. Otherwise highlights all
    /// same-file occurrences of the symbol at `offset`.
    pub fn document_highlights(&self, file: FileId, offset: u32) -> Vec<Highlight> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        if let Some(flow) = let_else_highlights(&m.tree, offset) {
            return flow;
        }
        let Some((symbol, _)) = self.symbol_at(file, offset) else {
            return Vec::new();
        };
        let binder = match &symbol {
            Symbol::Local { binder, .. } => Some(*binder),
            _ => None,
        };
        self.references(&symbol)
            .into_iter()
            .filter(|(f, _)| *f == file)
            .map(|(_, (start, end))| Highlight {
                start,
                end,
                kind: if binder == Some((start, end)) {
                    HighlightKind::Write
                } else {
                    HighlightKind::Read
                },
            })
            .collect()
    }
}

/// Control-flow highlights when `offset` is on `let` or `else` of a `let-else`.
fn let_else_highlights(tree: &Node, offset: u32) -> Option<Vec<Highlight>> {
    let path = tree::path_to(tree, offset);
    let i = path
        .iter()
        .rposition(|n| n.kind == K::LetDecl && n.has_token(K::ElseKw))?;
    let decl = path[i];
    let let_kw = decl.child_tokens().find(|t| t.kind == K::LetKw)?;
    let else_kw = decl.child_tokens().find(|t| t.kind == K::ElseKw)?;
    let on_let = let_kw.start <= offset && offset <= let_kw.end;
    let on_else = else_kw.start <= offset && offset <= else_kw.end;
    if !on_let && !on_else {
        return None;
    }
    let mut out = vec![
        Highlight {
            start: let_kw.start,
            end: let_kw.end,
            kind: HighlightKind::Write,
        },
        Highlight {
            start: else_kw.start,
            end: else_kw.end,
            kind: HighlightKind::Write,
        },
    ];
    if let Some(fallback) = decl
        .nodes()
        .find(|n| n.kind == K::Block && n.range().0 >= else_kw.end)
    {
        let (start, end) = fallback.range();
        if start < end {
            out.push(Highlight {
                start,
                end,
                kind: HighlightKind::Read,
            });
        }
    }
    // Highlight the exact remainder of the nearest enclosing block that is skipped
    // when the pattern does not match.
    if i > 0 && path[i - 1].kind == K::Block {
        let siblings: Vec<&Node> = path[i - 1].nodes().collect();
        if let Some(pos) = siblings.iter().position(|n| n.range() == decl.range()) {
            let rest = &siblings[pos + 1..];
            if let (Some(first), Some(last)) = (rest.first(), rest.last()) {
                out.push(Highlight {
                    start: first.range().0,
                    end: last.range().1,
                    kind: HighlightKind::Text,
                });
            }
        }
    }
    Some(out)
}
