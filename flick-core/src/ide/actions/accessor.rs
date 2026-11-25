//! Accessors and the functions they stand for: `fun x -> x.a.b` becomes
//! `.a.b` and back. Both read the same fields of their argument, typed as
//! the checker types the explicit function, so each rewrite keeps the
//! program's meaning; neither is applied unasked.

use super::super::{Ide, tree};
use super::{Action, edit};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Ide<'_> {
    /// `fun x -> x.a.b` as `.a.b`: offered when the function has one
    /// parameter, a bare name, its body is a path of fields written
    /// without spaces from that name, and no comment would be lost.
    pub(super) fn to_accessor(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let fun = *path.iter().rev().find(|n| n.kind == K::FunExpr)?;
        let mut params = fun.nodes().filter(|n| n.kind != K::Block);
        let (Some(param), None) = (params.next(), params.next()) else {
            return None;
        };
        let name = param
            .child_tokens()
            .find(|t| t.kind == K::Ident)?
            .text(text);
        let mut body = fun.nodes().find(|n| n.kind == K::Block)?.nodes();
        let (Some(read), None) = (body.next(), body.next()) else {
            return None;
        };
        let mut root = read;
        while root.kind == K::FieldExpr && root.child_tokens().any(|t| t.kind == K::Ident) {
            root = root.nodes().next()?;
        }
        let (start, end) = fun.range();
        let written = &text[start as usize..end as usize];
        let steps = &text[root.range().1 as usize..read.range().1 as usize];
        let continuous = steps.starts_with('.')
            && (steps.chars()).all(|c| matches!(c, '.' | '?' | '_' | '\'') || c.is_alphanumeric());
        if param.kind != K::VarPat
            || read.kind != K::FieldExpr
            || root.kind != K::NameExpr
            || root.text(text) != name
            || !continuous
            || written.contains("//")
            || written.contains("/*")
        {
            return None;
        }
        Some(Action {
            title: format!("use the accessor `{steps}`"),
            kind: "refactor.rewrite",
            edits: vec![edit(file, start, end, steps)],
        })
    }

    /// `.a.b` as `fun x -> x.a.b`, with a parameter no name in scope has,
    /// in parentheses where the function would take in what follows it.
    pub(super) fn to_function(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let at = path.iter().rposition(|n| n.kind == K::AccessorExpr)?;
        let accessor = path[at];
        let finished = (accessor.nodes()).all(|s| s.child_tokens().any(|t| t.kind == K::Ident));
        if !finished {
            return None;
        }
        let taken: Vec<String> = (self.complete_names(file, offset, None).into_iter())
            .map(|c| c.label)
            .collect();
        // one more candidate than names taken: one of them is free
        let name = (0..=taken.len())
            .map(|n| {
                if n == 0 {
                    "x".to_owned()
                } else {
                    format!("x{n}")
                }
            })
            .find(|n| !taken.contains(n))?;
        let (start, end) = accessor.range();
        let steps = &text[start as usize..end as usize];
        let function = format!("fun {name} -> {name}{steps}");
        // alone in parentheses or as a whole `let`'s value, nothing follows it
        let alone = match path.get(at.wrapping_sub(2)..at) {
            Some([_, parent]) if parent.kind == K::ParenExpr => true,
            Some([decl, block]) => {
                decl.kind == K::LetDecl && block.kind == K::Block && last(block, accessor)
            }
            _ => false,
        };
        Some(Action {
            title: format!("write `{steps}` as a function"),
            kind: "refactor.rewrite",
            edits: vec![edit(
                file,
                start,
                end,
                if alone {
                    function
                } else {
                    format!("({function})")
                },
            )],
        })
    }
}

/// Whether `node` is the only expression of `block`, which ends with it.
fn last(block: &Node, node: &Node) -> bool {
    let mut nodes = block.nodes();
    matches!((nodes.next(), nodes.next()), (Some(only), None) if only.range() == node.range())
}
