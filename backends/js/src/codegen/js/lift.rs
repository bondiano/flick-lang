//! Lifts: a function adapted where one of another stage is expected, and
//! an `Eff` whose result is.

use flick_pretty::docvec;

use super::Gen;
use super::doc::{After, Before, Form, Js, call};
use crate::ir::{Expr, Stage};
use flick_ir::Lift;

impl<'d> Gen<'_, 'd> {
    /// `inner`, a function adapted where one of another stage is expected.
    pub(super) fn lifted(&mut self, inner: &Expr, lift: &Lift) -> Js<'d> {
        let v = self.expr(inner);
        let f = self.lift(lift);
        call(self.a, self.parenthesized(f), &[v])
    }

    /// `value` passed on to the lift `inner` of the result, if any.
    fn then(&self, inner: Option<Js<'d>>, value: Js<'d>) -> Js<'d> {
        match inner {
            Some(r) => call(self.a, self.parenthesized(r), &[value]),
            None => value,
        }
    }

    /// `(js)`, to be called.
    fn parenthesized(&self, js: Js<'d>) -> Js<'d> {
        let a = self.a;
        let doc = docvec![a, "(", js.at(a, Before::Delimiter, After::Delimiter), ")"];
        Js {
            form: Form::Member,
            ..Js::code(doc, js.after_paren(), &[js])
        }
    }

    /// A JS function that turns a function into the one of the stage `lift` expects.
    fn lift(&mut self, lift: &Lift) -> Js<'d> {
        if self.kernel {
            return self.kernel_lift(lift);
        }
        let a = self.a;
        match lift {
            Lift::Stage { from, to, result } => {
                let inner = result.as_deref().map(|r| self.lift(r));
                let applied = call(a, self.leaf("$f"), &[self.leaf("$x")]);
                let body = match (from, to) {
                    (Stage::Runtime, Stage::Runtime) => match inner {
                        Some(r) => {
                            let map = self.effect("map");
                            call(a, map, &[applied, r])
                        }
                        None => applied,
                    },
                    (Stage::Pure, Stage::Runtime) => {
                        let sync = self.effect("sync");
                        call(a, sync, &[self.thunk(self.then(inner, applied))])
                    }
                    (Stage::Sync, Stage::Runtime) => {
                        let sync = self.rt("sync");
                        call(a, sync, &[self.thunk(self.then(inner, applied))])
                    }
                    (Stage::Runtime, _) => {
                        let run_in = self.rt("runIn");
                        self.then(inner, call(a, run_in, &[applied]))
                    }
                    // a pure or Sync function is called alike
                    _ => self.then(inner, applied),
                };
                let curried = self.lambda_of("$x", body);
                self.lambda_of("$f", curried)
            }
            Lift::Eff(inner) => {
                let inner = self.lift(inner);
                let map = self.effect("map");
                let mapped = call(a, map, &[self.leaf("$e"), inner]);
                self.lambda_of("$e", mapped)
            }
        }
    }
}
