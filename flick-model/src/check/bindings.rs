//! The rules that bind (`docs/formal/core.md` §2.3, §2.5): (Let-Gen),
//! (Let-Mono), (Let-Sig) and (Let-Rec), a local signature with its outer
//! qualifier inferred (`regions.md` §2.6).

use std::mem;
use std::rc::Rc;

use super::expr::mono;
use super::{Check, Checker};
use crate::syntax::{Name, Term};
use crate::types::{Caps, Scheme, Stage, Type};

impl Checker<'_> {
    /// (Let-Gen), (Let-Mono) and (Let-Sig).
    pub(super) fn let_(
        &mut self,
        x: &Name,
        sig: Option<&Scheme>,
        e1: &Term,
        e2: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let s = if let Some(s) = sig {
            self.signed(e1, s, None)?
        } else {
            self.level += 1;
            let outer = mem::replace(&mut self.stage, Stage::Pure);
            let t1 = self.infer(e1);
            let inner = mem::replace(&mut self.stage, outer);
            self.level -= 1;
            let t1 = t1?;
            if inner == Stage::Pure {
                self.generalize(&t1)
            } else {
                // the result of a runtime step is not generalized, by a
                // later `let` either
                self.perform(&inner)?;
                self.lower(&t1, self.level);
                mono(t1)
            }
        };
        let bound = s.clone();
        self.note(e1, |n| n.scheme = Some(bound));
        self.body(x, s, e2, expected)
    }

    fn body(&mut self, x: &Name, s: Scheme, e2: &Term, expected: Option<&Type>) -> Check<Type> {
        self.locals.push((x.clone(), s));
        let t = match expected {
            Some(t) => self.check(e2, t).map(|()| t.clone()),
            None => self.infer(e2),
        };
        self.locals.pop();
        t
    }

    /// `e` against the local signature `s`, with its outer qualifier
    /// inferred, as a `let` and the `fix` its step gives read it; `rec` is
    /// bound at it in `e`.
    fn signed(&mut self, e: &Term, s: &Scheme, rec: Option<&Name>) -> Check<Scheme> {
        let s = self.local_signature(&self.live_scheme(s));
        let (bare, qualifier) = match &s.body {
            Type::Captured(t, c) => ((**t).clone(), c.clone()),
            t => (t.clone(), Caps::default()),
        };
        let bare = Scheme {
            body: bare,
            ..s.clone()
        };
        self.locals.extend(rec.map(|f| (f.clone(), s.clone())));
        let checked = self.check_qualified_forall(e, &bare, qualifier);
        if rec.is_some() {
            self.locals.pop();
        }
        checked.map(|()| s)
    }

    /// A local signature of a function: its own qualifier is inferred, a
    /// fresh capture row, unless written (`regions.md` §2.6, `docs/formal/regions.md#local-signatures-infer-only-the-outer-qualifier`).
    fn local_signature(&mut self, s: &Scheme) -> Scheme {
        let s = &self.in_scopes(s);
        match &s.body {
            Type::Fun(..) => {
                let q = self.fresh_caps();
                Scheme {
                    body: Type::Captured(Rc::new(s.body.clone()), q),
                    ..s.clone()
                }
            }
            _ => s.clone(),
        }
    }

    /// (Let-Rec): without a signature the recursive calls are monomorphic.
    pub(super) fn let_rec(
        &mut self,
        f: &Name,
        sig: Option<&Scheme>,
        lam: &Term,
        e2: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let s = if let Some(s) = sig {
            self.signed(lam, s, Some(f))?
        } else {
            self.level += 1;
            let t = self.fresh();
            self.locals.push((f.clone(), mono(t.clone())));
            let inferred = self.infer(lam);
            self.locals.pop();
            self.level -= 1;
            self.unify(&t, &inferred?)?;
            self.generalize(&t)
        };
        let bound = s.clone();
        self.note(lam, |n| n.scheme = Some(bound));
        self.body(f, s, e2, expected)
    }
}
