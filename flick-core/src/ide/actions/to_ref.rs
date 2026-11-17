//! "Keep `x` in a `Ref`": a `let mut` variable that a function or a `task`
//! uses (`FLICK-G005`) becomes a cell they can share. Its reads become
//! `Ref.get x` and its assignments `Ref.set x v`. Each `Ref` operation is a
//! runtime call, so the rewrite is offered, never applied by `--fix`.

use super::super::{Ide, Symbol, tree};
use super::{Action, edit};
use crate::diagnostics::Edit;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// Expressions an application takes as an argument without parentheses.
const ATOMIC: [K; 6] = [
    K::Literal,
    K::NameExpr,
    K::ParenExpr,
    K::UnitExpr,
    K::ListExpr,
    K::RecordExpr,
];

/// Where `Ref.get x` may stand for `x` without parentheses.
const LOOSE: [K; 5] = [
    K::Block,
    K::ParenExpr,
    K::BinExpr,
    K::TupleExpr,
    K::ListExpr,
];

impl Ide<'_> {
    pub(super) fn to_ref(&self, file: FileId, offset: u32) -> Option<Action> {
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
        let pattern = decl.nodes().find(|n| n.kind == K::VarPat)?;
        let value = decl.nodes().find(|n| n.kind == K::Block)?;
        if !decl.has_token(K::MutKw) || pattern.range() != binder || d == 0 {
            return None;
        }
        let uses: Vec<(u32, u32)> = (self.references(&symbol).into_iter())
            .map(|(_, r)| r)
            .filter(|r| *r != binder)
            .collect();
        // offered where it is needed: a use runs apart from the function
        let apart = |at: u32| {
            let p = tree::path_to(&m.tree, at);
            (p.iter().skip(d)).any(|n| {
                matches!(n.kind, K::FunExpr | K::TaskExpr)
                    || (n.kind == K::LetDecl && n.nodes().any(|c| c.kind == K::Params))
            })
        };
        if !uses.iter().any(|&(s, _)| apart(s)) {
            return None;
        }
        let mut edits = Vec::new();
        let mut_kw = decl.child_tokens().find(|t| t.kind == K::MutKw)?;
        let rest = &text[mut_kw.end as usize..];
        let spaces = super::to_u32(rest.len() - rest.trim_start().len());
        edits.push(edit(file, mut_kw.start, mut_kw.end + spaces, ""));
        // the type a header or a signature line gives becomes a cell's
        let header = decl
            .nodes()
            .find(|n| n.range().0 > pattern.range().1 && n.range().1 < value.range().0);
        let signature = path[d - 1].nodes().find(|n| {
            n.kind == K::Signature
                && n.range().1 < decl.range().0
                && n.child_tokens()
                    .next()
                    .is_some_and(|t| t.text(text) == name)
        });
        for ty in header
            .into_iter()
            .chain(signature.and_then(|s| s.nodes().last()))
        {
            let (start, end) = ty.range();
            let shown = one_line(text, ty)?;
            edits.extend(applied(file, "Ref", start, end, !shown.contains(' ')));
        }
        let initial = only_expression(value)?;
        let (start, end) = initial.range();
        one_line(text, initial)?;
        edits.extend(applied(
            file,
            "Ref.make",
            start,
            end,
            ATOMIC.contains(&initial.kind),
        ));
        for &(start, end) in &uses {
            let at = tree::path_to(&m.tree, start);
            let i = at.iter().rposition(|n| n.range() == (start, end))?;
            let parent = at[i.checked_sub(1)?];
            if parent.kind == K::AssignStmt {
                let assigned = only_expression(parent.nodes().find(|n| n.kind == K::Block)?)?;
                one_line(text, assigned)?;
                let (from, to) = assigned.range();
                // a read of the variable itself becomes an application
                let atomic = ATOMIC.contains(&assigned.kind) && !uses.contains(&(from, to));
                let mut set = applied(file, &format!("Ref.set {name}"), from, to, atomic);
                set[0].span.start = start;
                edits.extend(set);
            } else if LOOSE.contains(&parent.kind) {
                edits.push(edit(file, start, end, format!("Ref.get {name}")));
            } else {
                edits.push(edit(file, start, end, format!("(Ref.get {name})")));
            }
        }
        Some(Action {
            title: format!("keep `{name}` in a `Ref`"),
            kind: "refactor.rewrite",
            edits: separate(edits)?,
        })
    }
}

/// The text of `node`, when it is on one line: parentheses around more
/// lines could move them out of their layout.
fn one_line<'t>(text: &'t str, node: &Node) -> Option<&'t str> {
    let (start, end) = node.range();
    Some(&text[start as usize..end as usize]).filter(|shown| !shown.contains('\n'))
}

/// The edits applying `f` to the code at `start..end`, leaving that code as
/// it is: `f ` before it, or `f (` before and `)` after.
fn applied(file: FileId, f: &str, start: u32, end: u32, atomic: bool) -> Vec<Edit> {
    if atomic {
        vec![edit(file, start, start, format!("{f} "))]
    } else {
        vec![
            edit(file, start, start, format!("{f} (")),
            edit(file, end, end, ")"),
        ]
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

/// The edits in order, or none when two overlap or start at one place,
/// where their order would be the editor's to choose.
fn separate(mut edits: Vec<Edit>) -> Option<Vec<Edit>> {
    edits.sort_by_key(|e| e.span.start);
    let overlapping = (edits.windows(2))
        .any(|w| w[0].span.end > w[1].span.start || w[0].span.start == w[1].span.start);
    (!overlapping).then_some(edits)
}
