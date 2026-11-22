//! Pipes, compositions and `orelse`: the type they give and what they do.

use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Ide<'_> {
    /// `|>`, `<|`, `orelse`, `>>` or `<<` at `offset`: the type the checker
    /// gave there, what the operator does, and the parsed grouping when
    /// `orelse` is chained or mixed with pipelines.
    pub(super) fn operator_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let path = tree::path_to(&m.tree, offset);
        let parent = *path.last()?;
        let op = parent.child_tokens().find(|t| {
            t.start <= offset
                && offset <= t.end
                && matches!(
                    t.kind,
                    K::PipeGt | K::LtPipe | K::OrElse | K::GtGt | K::LtLt
                )
        })?;
        let at = (op.start, op.end);
        let ty = m.typing.types.get(&at)?;
        let (code, note) = match op.kind {
            K::PipeGt => (
                "x |> f",
                "Applies `f` to `x`, evaluating `x` first: `x |> f` is `f x`. \
                 The type is that of the value after this step.",
            ),
            K::LtPipe => (
                "f <| x",
                "Applies `f` to everything on its right, evaluating `f` first: \
                 `f a <| x` is the call `f a x`.",
            ),
            K::OrElse => (
                "option orelse fallback",
                "Evaluates `option` once; returns its `Some` value without \
                 evaluating `fallback`, or evaluates `fallback` once on `None`.",
            ),
            K::GtGt => (
                "f >> g",
                "Composes functions: applies `f`, then `g`. `f` and `g` are \
                 evaluated once, where the composition is written.",
            ),
            _ => (
                "g << f",
                "Composes functions: applies `f`, then `g`, as `f >> g` does. \
                 `g` and `f` are evaluated once, left to right, where the \
                 composition is written.",
            ),
        };
        let shown = m.typing.shown(at, ty);
        let mut markdown = format!("```flick\n{code} : {shown}\n```\n\n{note}");
        if let Some(grouped) = parsed_as(&path, self.text(file)) {
            markdown.push_str("\n\nParsed as `");
            markdown.push_str(&grouped);
            markdown.push_str("`.");
        }
        Some((markdown, at))
    }
}

fn grouping_op(node: &Node) -> Option<K> {
    if node.kind != K::BinExpr {
        return None;
    }
    node.child_tokens()
        .map(|t| t.kind)
        .find(|k| matches!(k, K::OrElse | K::PipeGt | K::LtPipe))
}

fn contains_orelse(node: &Node) -> bool {
    match grouping_op(node) {
        Some(K::OrElse) => true,
        Some(_) => node.nodes().any(contains_orelse),
        None => false,
    }
}

/// Explicit parenthesized grouping when `orelse` is chained right-associatively
/// or mixed with `|>` / `<|`.
fn parsed_as(path: &[&Node], text: &str) -> Option<String> {
    let mut top = path.len().checked_sub(1)?;
    grouping_op(path[top])?;
    while top > 0 && grouping_op(path[top - 1]).is_some() {
        top -= 1;
    }
    let root = path[top];
    if !contains_orelse(root) || !root.nodes().any(|c| grouping_op(c).is_some()) {
        return None;
    }
    let rendered = render_grouping(root, text, true)?;
    (rendered.len() <= 120).then_some(rendered)
}

fn render_grouping(node: &Node, text: &str, top_level: bool) -> Option<String> {
    if let Some(op) = grouping_op(node) {
        let mut children = node.nodes();
        let lhs = children.next()?;
        let rhs = children.next()?;
        if lhs.kind == K::Error || rhs.kind == K::Error {
            return None;
        }
        let left = render_grouping(lhs, text, false)?;
        let right = render_grouping(rhs, text, false)?;
        let sym = match op {
            K::OrElse => "orelse",
            K::PipeGt => "|>",
            K::LtPipe => "<|",
            _ => return None,
        };
        let inner = format!("{left} {sym} {right}");
        return Some(if top_level {
            inner
        } else {
            format!("({inner})")
        });
    }
    let raw = node.text(text);
    let compact: Vec<&str> = raw.split_whitespace().collect();
    (!compact.is_empty()).then(|| compact.join(" "))
}
