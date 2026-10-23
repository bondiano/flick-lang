//! Checks of the UI library the types alone cannot state.

use super::Checker;
use super::typing::Ref;
use crate::consteval::{self, Const};
use crate::diagnostics::codes;
use crate::syntax::cst::Node;

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
}
