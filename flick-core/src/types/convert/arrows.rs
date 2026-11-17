//! An arrow written `-> {r; e} B`: with the space, `{r; e}` is a row applied
//! to `B`, not the effect annotation `->{r; e}` it is usually meant as.

use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;

impl Checker<'_> {
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
