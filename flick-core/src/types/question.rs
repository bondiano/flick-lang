//! Postfix `?`. On a `Result (Errors e) a` in a runtime stage it is
//! `from_result`: the value, or a failure with `e`, which joins the stage's
//! errors. On an `Option a` in a function returning an `Option`, it is the
//! value of `Some`, and `None` returns `None` from the function at once.

use super::Checker;
use super::ty::Type;
use super::typing::range;
use crate::diagnostics::{Diagnostic, codes};
use crate::syntax::cst::Node;

/// What a `?` does, by the operand it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    /// `from_result`: a runtime call.
    Result,
    /// The value of `Some`, or a return of `None`.
    Option,
}

impl Checker<'_> {
    pub fn question(&mut self, node: &Node, operand: &Node) -> Type {
        let ty = self.infer(operand);
        let span = self.span(node);
        let resolved = self.whnf(&ty);
        let (head, args) = resolved.spine();
        let head = self.whnf(head);
        match args[..] {
            [item] if head.is_prim("Option") => {
                let item = item.clone();
                let Some(returns) = self.stage.returns.clone() else {
                    self.error(
                        codes::T003,
                        span,
                        "`?` on an `Option` returns `None` from the function around it, but this code runs on its own (in a `task`, `scope`, `provide`, `try` or a `for`): match the `Option` here",
                    );
                    return item;
                };
                let some = Type::app(Type::prim("Option"), self.fresh());
                if self.unify(&returns, &some).is_err() {
                    let shown = self.show(&returns);
                    let message = format!(
                        "`?` on an `Option` returns `None` from the function around it, but that function returns `{shown}`"
                    );
                    self.diagnostics
                        .push(Diagnostic::error(codes::T003, message, span).with_fix(
                        "in a runtime function, fail instead: `(Option.to_result error option)?`",
                        Vec::new(),
                    ));
                    return item;
                }
                self.typing.questions.insert(range(span), Question::Option);
                item
            }
            [_, value] if head.is_prim("Result") => {
                let value = value.clone();
                if self.stage.pure.is_some() || self.stage.sync {
                    self.error(
                        codes::E001,
                        span,
                        "`?` on a `Result` fails with its error, which needs a runtime stage with errors (`->{r; e}`): match the `Result` here",
                    );
                    return value;
                }
                let errors = self.fresh();
                let result = Type::apps(
                    Type::prim("Result"),
                    [
                        Type::app(Type::prim("Errors"), errors.clone()),
                        value.clone(),
                    ],
                );
                if self.unify(&ty, &result).is_err() {
                    let shown = self.show(&ty);
                    self.error(
                        codes::T003,
                        span,
                        format!("`?` on a `Result` fails with its error, so the error must be one declared with `type error`, but this is `{shown}`"),
                    );
                    return value;
                }
                self.perform(&Type::empty_row(), &errors, span);
                self.typing.runtime.insert(range(span));
                self.typing.questions.insert(range(span), Question::Result);
                value
            }
            _ if resolved == Type::Error => Type::Error,
            _ => {
                let shown = self.show(&ty);
                self.error(
                    codes::T003,
                    span,
                    format!("`?` takes an `Option` or a `Result`, but this has type `{shown}`"),
                );
                Type::Error
            }
        }
    }
}
