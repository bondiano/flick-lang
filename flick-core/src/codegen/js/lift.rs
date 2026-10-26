//! Lifts: a function adapted where one of another stage is expected, and
//! an `Eff` whose result is.

use super::Gen;
use crate::ir::{Expr, Stage};
use crate::types::Lift;

impl Gen<'_> {
    /// `inner`, a function adapted where one of another stage is expected.
    pub(super) fn lifted(&mut self, inner: &Expr, lift: &Lift) -> String {
        let v = self.expr(inner);
        let f = self.lift(lift);
        format!("({f})({v})")
    }

    /// A JS function that turns a function into the one of the stage `lift` expects.
    fn lift(&mut self, lift: &Lift) -> String {
        match lift {
            Lift::Stage { from, to, result } => {
                let inner = result.as_deref().map(|r| self.lift(r));
                let call = "$f($x)";
                let lifted = |value: String| match &inner {
                    Some(r) => format!("({r})({value})"),
                    None => value,
                };
                let body = match (from, to) {
                    (Stage::Runtime, Stage::Runtime) => match &inner {
                        Some(r) => format!("{}({call}, {r})", self.effect("map")),
                        None => call.to_owned(),
                    },
                    (Stage::Pure, Stage::Runtime) => {
                        format!("{}(() => {})", self.effect("sync"), lifted(call.to_owned()))
                    }
                    (Stage::Sync, Stage::Runtime) => {
                        format!("{}(() => {})", self.rt("sync"), lifted(call.to_owned()))
                    }
                    (Stage::Runtime, _) => lifted(format!("{}({call})", self.rt("runIn"))),
                    // a pure or Sync function is called alike
                    _ => lifted(call.to_owned()),
                };
                format!("{} {} {body}", self.arrow("$f"), self.arrow("$x"))
            }
            Lift::Eff(inner) => {
                let inner = self.lift(inner);
                let map = self.effect("map");
                format!("{} {map}($e, {inner})", self.arrow("$e"))
            }
        }
    }
}
