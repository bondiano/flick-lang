//! FLICK-T075: `from_result (Err e)` builds a `Result` only to fail with
//! its error; `Fail.fail e` says that directly.

use super::super::Checker;
use super::super::typing::Ref;
use super::unparen;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// T075 on `node`, an application, when it is `from_result (Err e)` of
    /// the Prelude and `Fail` here is the core module.
    pub(super) fn raise_lint(&mut self, node: &Node) {
        let Some(error) = self.raised(node) else {
            return;
        };
        let fail = self.env.qualified_module("Fail").map(|(module, _)| module);
        if self.builtin || fail != Some("Fail") {
            return;
        }
        let error = self.source_of(error).trim().to_owned();
        let (start, end) = node.range();
        let warning = Diagnostic::new(
            codes::T075,
            Severity::Warning,
            "`from_result (Err …)` builds a `Result` only to fail with its error: `Fail.fail` fails with it directly",
        )
        .with_span(self.span(node));
        self.diagnostics.push(warning.with_safe_fix(|| {
            (
                "write `Fail.fail`",
                vec![self.edit(start, end, &format!("Fail.fail {error}"))],
            )
        }));
    }

    /// The argument of `Err` in `node`, `from_result (Err e)` of the Prelude,
    /// as written: an atom, which `Fail.fail` takes as it is.
    pub(super) fn raised<'n>(&self, node: &'n Node) -> Option<&'n Node> {
        let parts: Vec<&Node> = node.nodes().collect();
        let (K::AppExpr, [head, arg]) = (node.kind, &parts[..]) else {
            return None;
        };
        let from_result = matches!(self.typing.names.get(&head.range()),
            Some(Ref::Value { module, name }) if lang::is(Lang::FromResult, module, name));
        let built = unparen(arg);
        let inner: Vec<&Node> = built.nodes().collect();
        let (K::AppExpr, [con, error]) = (built.kind, &inner[..]) else {
            return None;
        };
        let err = matches!(self.typing.names.get(&con.range()),
            Some(Ref::Constructor(c)) if lang::is(Lang::Err, &c.module, &c.name));
        (from_result && err).then_some(*error)
    }
}

#[cfg(test)]
#[path = "raise_tests.rs"]
mod tests;
