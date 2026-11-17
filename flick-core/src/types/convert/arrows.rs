//! The parts of an arrow that need a word of their own: `-> {r; e} B`, whose
//! space makes `{r; e}` a row applied to `B` rather than the effect
//! annotation `->{r; e}` it is usually meant as, and a suspended parameter
//! `~(Eff r e a)`, which is its `Eff` to the type (`docs/formal/lazy.md` §7).

use super::Free;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::kinds::Kind;
use crate::types::report::uncaptured;
use crate::types::ty::Type;

impl Checker<'_> {
    /// The parameter `node` of an arrow. `~T` is `T`, which must be an `Eff`
    /// (`FLICK-T076` otherwise).
    pub(super) fn parameter(&mut self, node: &Node, free: Free) -> Type {
        let inner = (node.kind == K::SuspendedType)
            .then(|| node.nodes().next())
            .flatten();
        let Some(inner) = inner else {
            return self.ty(node, &Kind::Type, free);
        };
        let ty = self.ty(inner, &Kind::Type, free);
        if ty != Type::Error && self.eff_parts(&uncaptured(&ty)).is_none() {
            let shown = self.text_of(inner);
            let message = format!(
                "`~` marks a parameter that takes a computation, `~(Eff r e a)`, but `{shown}` is no `Eff`"
            );
            self.error(codes::T076, self.span(node), message);
        }
        ty
    }

    /// `~T` other than as the parameter of an arrow: `FLICK-T076`, and `T`.
    pub(super) fn misplaced_suspension(
        &mut self,
        node: &Node,
        expected: &Kind,
        free: Free,
    ) -> (Type, Kind) {
        let message = "`~` marks the parameter of a function, left of its `->`";
        self.error(codes::T076, self.span(node), message);
        match node.nodes().next() {
            Some(inner) => (self.ty(inner, expected, free), expected.clone()),
            None => (Type::Error, Kind::Type),
        }
    }

    /// Whether `result`, the result of the arrow `node`, is a row in braces
    /// applied to a type; reported with a fix that joins the brace to its
    /// arrow when only spaces part them.
    pub(super) fn spaced_effect_arrow(&mut self, node: &Node, result: &Node) -> bool {
        let row = (result.kind == K::AppType)
            .then(|| result.nodes().next())
            .flatten()
            .filter(|head| head.kind == K::RowType);
        let arrow = node.child_tokens().find(|t| t.kind == K::Arrow);
        let (Some(row), Some(arrow)) = (row, arrow) else {
            return false;
        };
        let span = self.span(row);
        let text = self.text_of(row);
        let message = format!(
            "`-> {text}` with a space applies the row `{text}` to a type; an effect annotation touches its arrow: `->{text}`"
        );
        let mut diagnostic = Diagnostic::error(codes::T002, message, span);
        let gap = &self.text[arrow.end as usize..span.start as usize];
        if gap.chars().all(|c| c == ' ') {
            let join = Edit {
                span: Span::new(span.file, arrow.end, span.start),
                replacement: String::new(),
            };
            diagnostic = diagnostic.with_safe_fix(|| (format!("write `->{text}`"), vec![join]));
        }
        self.diagnostics.push(diagnostic);
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::types::lint_tests::assert_fix;

    /// The fix joins the brace to its arrow, and the signature then means
    /// the effect annotation it was written as.
    #[test]
    fn a_spaced_effect_arrow_is_fixed_to_its_annotation() {
        let head = "service Log =\n  info : String ->{} Unit\n\n";
        assert_fix(
            &format!("{head}f : Int -> {{Log}} Int\nlet f n = n\n"),
            "FLICK-T002",
            &format!("{head}f : Int ->{{Log}} Int\nlet f n = n\n"),
        );
    }

    /// A record touching its arrow gets the space that makes it a record.
    #[test]
    fn a_record_touching_its_arrow_is_fixed_to_a_record() {
        assert_fix(
            "f : Int ->{ x : Int }\nlet f n = { x = n }\n",
            "FLICK-S007",
            "f : Int -> { x : Int }\nlet f n = { x = n }\n",
        );
    }
}
