//! Where a type that lacks only `Async` is written: the row a fix adds it
//! to, as `Layer none none {Database}` lacks it once building the layer
//! may suspend.

use crate::types::Checker;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Type, Written};

impl Checker<'_> {
    /// The written row of `expected` where `actual` has `Async` and it does
    /// not, looking through applications, functions and records alike.
    pub(super) fn async_place(&self, actual: &Type, expected: &Type) -> Option<Written> {
        let label = lang::label(Lang::Async)?;
        let has = |t: &Type| (self.store.normalize(t).entries.iter()).any(|(l, _)| l == label);
        let (a, e) = (self.store.resolve(actual), self.store.resolve(expected));
        match (&a, &e) {
            (Type::Captured { ty, .. } | Type::Bounded { ty, .. }, _) => self.async_place(ty, &e),
            (_, Type::Captured { ty, .. } | Type::Bounded { ty, .. }) => self.async_place(&a, ty),
            (Type::Row(_) | Type::Union(..), Type::Row(_)) => {
                let place = self.local_place(&e);
                (has(&a) && !has(&e) && place.0.is_some()).then_some(place)
            }
            (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => {
                (self.async_place(h1, h2)).or_else(|| self.async_place(a1, a2))
            }
            (
                Type::Fun {
                    param: p1,
                    effects: e1,
                    result: r1,
                },
                Type::Fun {
                    param: p2,
                    effects: e2,
                    result: r2,
                },
            ) => {
                let row = match (e1, e2) {
                    (Some(x), Some(y)) if has(&x.services) && !has(&y.services) => {
                        let own = self.local_place(&y.services);
                        Some(if own.0.is_some() {
                            own
                        } else {
                            self.local(y.written)
                        })
                    }
                    _ => None,
                };
                (row.filter(|w| w.0.is_some()))
                    .or_else(|| self.async_place(p1, p2))
                    .or_else(|| self.async_place(r1, r2))
            }
            (Type::Record(r1), Type::Record(r2)) => (r1.entries.iter()).find_map(|(l, t)| {
                let (_, other) = r2.entries.iter().find(|(m, _)| m == l)?;
                self.async_place(t, other)
            }),
            _ => None,
        }
    }
}
