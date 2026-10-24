//! Checks of the UI library the types alone cannot state.

use std::collections::HashMap;

use super::typing::Ref;
use super::{Checker, Range};
use crate::consteval::{self, Const};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// `Ui.Event.queue c` and `Ui.Event.concurrent c` of a constant `c`:
    /// the capacity or limit must be a positive integer.
    pub fn ui_capacity(&mut self, head: &Node, arg: &Node) {
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()) else {
            return;
        };
        if module != "Ui.Event" || !matches!(name.as_str(), "queue" | "concurrent") {
            return;
        }
        let what = if name == "queue" { "capacity" } else { "limit" };
        if let Some(Const::Number(n)) = consteval::eval(arg, self.text)
            && !(n >= 1.0 && n.fract() == 0.0 && n.is_finite())
        {
            self.error(
                codes::U001,
                self.span(arg),
                format!("the {what} of `Ui.Event.{name}` must be a positive integer"),
            );
        }
    }

    /// `Ui.view render`: a handler made from a `fun` inside the render is
    /// a new closure on every render, so it is worth a warning; an action
    /// declared at the top level, or `Action.with` data, is made once.
    pub fn ui_render_handlers(&mut self, head: &Node, render: &Node) {
        let is_view = matches!(
            self.typing.names.get(&head.range()),
            Some(Ref::Value { module, name }) if module == "Ui" && name == "view"
        );
        if !is_view {
            return;
        }
        let mut found = Vec::new();
        handlers_of_lambdas(render, &self.typing.names, &mut found);
        for (span, what) in found {
            let span = Span::new(self.file, span.0, span.1);
            self.warning(
                codes::U002,
                span,
                format!("`{what}` of a `fun` makes a handler on every render: declare an `Action` at the top level, or pass data with `Action.with`"),
            );
        }
    }
}

/// The applications of an event policy or action constructor to a `fun`
/// under `node`: their spans and the constructor's name.
fn handlers_of_lambdas(node: &Node, names: &HashMap<Range, Ref>, out: &mut Vec<(Range, String)>) {
    if node.kind == K::AppExpr {
        let parts: Vec<&Node> = node.nodes().collect();
        let constructor = parts
            .first()
            .and_then(|head| match names.get(&head.range()) {
                Some(Ref::Value { module, name })
                    if matches!(module.as_str(), "Ui.Event" | "Action")
                        && matches!(
                            name.as_str(),
                            "exhaust" | "latest" | "queue" | "concurrent"
                        ) =>
                {
                    Some(format!("{module}.{name}"))
                }
                _ => None,
            });
        let unwrap = |mut n: &Node| {
            while n.kind == K::ParenExpr {
                match n.nodes().next() {
                    Some(inner) => n = inner,
                    None => break,
                }
            }
            n.kind
        };
        if let (Some(what), Some(last)) = (constructor, parts.last())
            && parts.len() > 1
            && unwrap(last) == K::FunExpr
        {
            out.push((node.range(), what));
        }
    }
    for child in node.nodes() {
        handlers_of_lambdas(child, names, out);
    }
}
