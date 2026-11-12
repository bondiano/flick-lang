//! The pipes and the compositions: the type they give and what they do.

use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;

impl Ide<'_> {
    /// `|>`, `<|`, `>>` or `<<` at `offset`: the type the checker gave there,
    /// the value after a pipe's step or the function a composition makes,
    /// and what the operator does.
    pub(super) fn operator_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let parent = *tree::path_to(&m.tree, offset).last()?;
        let op = parent.child_tokens().find(|t| {
            t.start <= offset
                && offset <= t.end
                && matches!(t.kind, K::PipeGt | K::LtPipe | K::GtGt | K::LtLt)
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
        Some((format!("```flick\n{code} : {shown}\n```\n\n{note}"), at))
    }
}
