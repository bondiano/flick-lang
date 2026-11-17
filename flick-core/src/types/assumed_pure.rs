//! A local applied before its type is known is taken for a pure function,
//! `(fun h -> h ()) f`. When a runtime function then meets that arrow, the
//! mismatch points at the local and offers the annotation it needs.

use super::Checker;
use super::ty::Type;
use crate::diagnostics::Edit;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Notes that `head`, applied with no type known, was taken for `arrow`.
    pub(super) fn assume_pure(&mut self, head: &Node, arrow: &Type) {
        if head.kind != K::NameExpr {
            return;
        }
        let (start, end) = head.range();
        let name = &self.text[start as usize..end as usize];
        let found = self.cx.locals.iter().rposition(|l| l.name == name);
        if let Some(at) = found.and_then(|i| self.cx.locals[i].at) {
            self.cx
                .assumed_pure
                .push((arrow.clone(), name.to_owned(), at));
        }
    }

    /// When `expected`, a pure arrow taken for a local, met the runtime
    /// function `actual`: the last diagnostic says so at the local's binder
    /// and offers to annotate it.
    pub(super) fn explain_assumed_pure(&mut self, actual: &Type, expected: &Type) {
        let runtime = matches!(
            actual,
            Type::Fun {
                effects: Some(_),
                ..
            }
        );
        if !runtime || !matches!(expected, Type::Fun { effects: None, .. }) {
            return;
        }
        let found = (self.cx.assumed_pure.iter())
            .find(|(arrow, _, _)| super::report::uncaptured(&self.zonk(arrow)) == *expected)
            .map(|(_, name, at)| (name.clone(), *at));
        let Some((name, at)) = found else {
            return;
        };
        let Some(last) = self.diagnostics.pop() else {
            return;
        };
        let end = at + u32::try_from(name.len()).expect("source fits in u32");
        let binder = Span::new(self.file, at, end);
        let shown = self.show(actual);
        let annotate = Edit {
            span: binder,
            replacement: format!("({name} : {shown})"),
        };
        let note =
            format!("`{name}` has no type written, so applying it took it for a pure function");
        let last = last.with_related(binder, note).with_fix(|| {
            (
                format!("annotate `{name}` with the runtime function it is given"),
                vec![annotate],
            )
        });
        self.diagnostics.push(last);
    }
}

#[cfg(test)]
mod tests {
    use crate::types::lint_tests::assert_fix;

    /// Annotated with the runtime arrow it is given, the callback fits it.
    #[test]
    fn a_callback_taken_for_pure_is_annotated() {
        let head = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\napply : Unit ->{Log} Unit\n";
        assert_fix(
            &format!("{head}let apply () = (fun h -> h ()) say\n"),
            "FLICK-T003",
            &format!("{head}let apply () = (fun (h : Unit ->{{Log}} Unit) -> h ()) say\n"),
        );
    }
}
