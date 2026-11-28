//! Subtyping of stages, functions, `forall`s, `Eff`, `Lazy`, `Layer`, and data.

use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::{Binder, Type};
use crate::types::unify::{Mismatch, Unified};

impl Checker<'_> {
    /// Like unification, but a value may need fewer services and errors than
    /// its position allows: runtime arrows and `Eff` are covariant in their
    /// rows and results, and a pure function passes for a runtime one.
    pub fn subsume(&mut self, actual: &Type, expected: &Type, span: Span) -> Unified {
        let outer = self.cx.regions.deferring.replace(span);
        let fits = self.subsume_at(actual, expected, span);
        self.cx.regions.deferring = outer;
        fits
    }

    fn subsume_at(&mut self, actual: &Type, expected: &Type, span: Span) -> Unified {
        self.compare_unfolding(actual, expected, |c| {
            c.subsume_unfolded(actual, expected, span)
        })
    }

    fn subsume_unfolded(&mut self, actual: &Type, expected: &Type, span: Span) -> Unified {
        if let Some(result) = self.unify_opaque_meta(actual, expected) {
            return result;
        }
        if let Some(result) = self.subsume_bound(actual, expected, span) {
            return result;
        }
        let (a, e) = (self.whnf_captured(actual), self.whnf_captured(expected));
        if a.1.is_some() || e.1.is_some() {
            return self.subsume_captured((actual, a), (expected, e), span);
        }
        let (mut a, e) = (a.0, e.0);
        if matches!(a, Type::Fun { .. }) {
            self.attach_discharged_provides(&mut a);
        }
        if matches!(e, Type::Meta(_))
            && let Some(result) = self.bind_subsumed_meta(&a, &e, false)
        {
            return result;
        }
        match (&a, &e) {
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
                match (e1, e2) {
                    (Some(_), None) => {
                        return Err(Mismatch::Types(a.clone(), e.clone()));
                    }
                    (Some(x), Some(y)) => {
                        self.tighten_closed_subsumed(&x.services, &y.services);
                        let site = self.expected_site(&e, self.row_place_at(y, span));
                        self.sub_row(&x.services, &y.services, site, span, false);
                        self.sub_row(&x.errors, &y.errors, site, span, true);
                    }
                    (None, _) => {}
                }
                self.subsume(p2, p1, span)?;
                let result = self.value_bound((**r1).clone());
                self.subsume(&result, r2, span)
            }
            (
                Type::Forall {
                    vars: v1,
                    constraints: c1,
                    body: b1,
                },
                Type::Forall {
                    vars: v2,
                    constraints: c2,
                    body: b2,
                },
            ) => {
                if v1.len() != v2.len()
                    || !v1.iter().zip(v2).all(|(x, y)| x.kind == y.kind)
                    || c1.len() != c2.len()
                {
                    return Err(Mismatch::Types(a.clone(), e.clone()));
                }
                self.subsume_forall((v1, c1, b1), (v2, c2, b2), span)
            }
            _ => match (self.eff_parts(&a), self.eff_parts(&e)) {
                (Some(left), Some(right)) => self.subsume_suspended(left, right, &e, span),
                _ => match (self.lazy_parts(&a), self.lazy_parts(&e)) {
                    (Some(left), Some(right)) => self.subsume_suspended(left, right, &e, span),
                    // a layer that needs less fits where more is allowed; it makes the same services
                    _ => match (self.layer_parts(&a), self.layer_parts(&e)) {
                        (Some((n1, e1, o1)), Some((n2, e2, o2))) => {
                            let written = self.local_place_at(&n2, span);
                            let site = self.expected_site(&e, written);
                            self.sub_row(&n1, &n2, site, span, false);
                            self.sub_row(&e1, &e2, site, span, true);
                            self.unify(&o1, &o2)
                        }
                        _ => self.subsume_data(&a, &e, span),
                    },
                },
            },
        }
    }

    fn subsume_suspended(
        &mut self,
        (s1, e1, r1): (Type, Type, Type),
        (s2, e2, r2): (Type, Type, Type),
        expected: &Type,
        span: Span,
    ) -> Unified {
        self.tighten_closed_subsumed(&s1, &s2);
        let written = self.local_place_at(&s2, span);
        let site = self.expected_site(expected, written);
        self.sub_row(&s1, &s2, site, span, false);
        self.sub_row(&e1, &e2, site, span, true);
        self.subsume(&r1, &r2, span)
    }

    fn subsume_forall(
        &mut self,
        (v1, c1, b1): (&[Binder], &[Type], &Type),
        (v2, c2, b2): (&[Binder], &[Type], &Type),
        span: Span,
    ) -> Unified {
        self.store.level += 1;
        let skolems: Vec<Type> = v1
            .iter()
            .map(|v| {
                let skolem = self.store.skolem(&v.name);
                if let Type::Skolem(s) = &skolem {
                    self.cx.regions.rigid_kinds.push((*s, v.kind.clone()));
                }
                skolem
            })
            .collect();
        let s1: Vec<_> = v1
            .iter()
            .map(|v| v.name.clone())
            .zip(skolems.clone())
            .collect();
        let s2: Vec<_> = v2.iter().map(|v| v.name.clone()).zip(skolems).collect();
        let result = c1
            .iter()
            .zip(c2)
            .try_for_each(|(x, y)| self.unify(&x.substitute(&s1), &y.substitute(&s2)))
            .and_then(|()| self.subsume(&b1.substitute(&s1), &b2.substitute(&s2), span));
        self.store.level -= 1;
        result
    }
}
