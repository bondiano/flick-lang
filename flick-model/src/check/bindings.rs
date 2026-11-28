//! The rules that bind (`docs/formal/core.md` §2.3, §2.5): (Let-Gen),
//! (Let-Mono), (Let-Sig), (Let-Rec) and the (Fix) a step gives, a local
//! signature of (Let-Sig) and (Fix) with its outer qualifier inferred
//! (`regions.md` §2.6).

use std::mem;
use std::rc::Rc;

use super::expr::mono;
use super::regions::place_qualifiers;
use super::{Check, Checker, error};
use crate::subst::free_vars;
use crate::syntax::{Name, Term};
use crate::types::{Caps, Scheme, Stage, Type};

#[derive(Clone, Copy)]
enum OperandTarget<'a> {
    App(&'a Term, &'a Term),
    Await(&'a Term),
}

fn is_operand_let(name: &Name) -> bool {
    name.starts_with("$operand")
}

fn mentions_any(term: &Term, bindings: &[(&Name, &Term)]) -> bool {
    let free = free_vars(term);
    bindings.iter().any(|(name, _)| free.contains(*name))
}

/// Lowering (`owners.rs` `evidence_call`) spills non-local/non-literal operands
/// of a runtime call or `await` into unannotated `$operand` lets immediately
/// before `WithOwnerEvidence` so they evaluate under the previous owner
/// evidence. Recover the original call/await operands for bidirectional
/// checking without changing evaluation order.
fn operand_call<'a>(
    x: &'a Name,
    e1: &'a Term,
    e2: &'a Term,
) -> Option<(&'a Term, OperandTarget<'a>)> {
    if !is_operand_let(x) {
        return None;
    }
    let mut bindings = vec![(x, e1)];
    let mut cur = e2;
    while let Term::Let(next_x, None, next_e1, next_e2) = cur {
        if !is_operand_let(next_x)
            || bindings.iter().any(|(b, _)| *b == next_x)
            || mentions_any(next_e1, &bindings)
        {
            break;
        }
        bindings.push((next_x, next_e1));
        cur = next_e2;
    }
    let Term::WithOwnerEvidence(owner, inner) = cur else {
        return None;
    };
    if mentions_any(owner, &bindings) {
        return None;
    }
    let mut used = vec![false; bindings.len()];
    let mut resolve = |term: &'a Term| -> Option<&'a Term> {
        if let Term::Var(v) = term
            && let Some(i) = bindings.iter().position(|(name, _)| *name == v)
        {
            if used[i] {
                return None;
            }
            used[i] = true;
            return Some(bindings[i].1);
        }
        if mentions_any(term, &bindings) {
            return None;
        }
        Some(term)
    };
    let target = match &**inner {
        Term::App(f, a) => {
            let f = resolve(f)?;
            let a = resolve(a)?;
            OperandTarget::App(f, a)
        }
        Term::Let(tmp, None, rhs, body)
            if tmp.starts_with("$arg")
                && matches!(&**rhs, Term::Var(_))
                && matches!(&**body, Term::App(f, a) if matches!(&**a, Term::Var(v) if v == tmp) && !free_vars(f).contains(tmp)) =>
        {
            let Term::App(f, _) = &**body else {
                unreachable!()
            };
            let f = resolve(f)?;
            let a = resolve(rhs)?;
            OperandTarget::App(f, a)
        }
        Term::Await(v) => OperandTarget::Await(resolve(v)?),
        _ => return None,
    };
    used.iter().all(|u| *u).then_some((owner, target))
}

impl Checker<'_> {
    #[inline(never)]
    fn operand_let(
        &mut self,
        whole: &Term,
        owner: &Term,
        target: OperandTarget<'_>,
        expected: Option<&Type>,
    ) -> Check<Type> {
        self.owner_context(owner, false)?;
        match target {
            OperandTarget::App(f, a) => {
                if let (Some(exp), true, Term::Lam(p, body)) = (expected, self.config, f) {
                    return self.redex(p, body, a, Some(exp));
                }
                let t = self.app(f, a)?;
                match expected {
                    Some(exp) => self.fit(whole, &t, exp).map(|()| exp.clone()),
                    None => Ok(t),
                }
            }
            OperandTarget::Await(v) => {
                if let Some(exp) = expected.filter(|_| self.config) {
                    let (r, x_rows) = (self.fresh_row(), self.fresh_row());
                    let task = Type::Eff(r.clone(), x_rows.clone(), Rc::new(exp.clone()));
                    let held = self.fresh_caps();
                    self.check(v, &Type::Captured(Rc::new(task), held))?;
                    self.perform(&Stage::Runtime(r, x_rows))?;
                    return Ok(exp.clone());
                }
                let t = self.await_(v)?;
                match expected {
                    Some(exp) => self.fit(whole, &t, exp).map(|()| exp.clone()),
                    None => Ok(t),
                }
            }
        }
    }

    /// Type-check or infer the right-hand side of a `let` binding.
    fn let_rhs(&mut self, sig: Option<&Scheme>, e1: &Term) -> Check<Scheme> {
        if let Some(s) = sig {
            self.signed(e1, s, None)
        } else {
            self.level += 1;
            let outer = mem::replace(&mut self.stage, Stage::Pure);
            let t1 = self.infer(e1);
            let inner = mem::replace(&mut self.stage, outer);
            self.level -= 1;
            let t1 = t1?;
            if inner == Stage::Pure {
                Ok(self.generalize(&t1))
            } else {
                // the result of a runtime step is not generalized, by a
                // later `let` either
                self.perform(&inner)?;
                self.lower(&t1, self.level);
                Ok(mono(t1))
            }
        }
    }

    /// (Let-Gen), (Let-Mono) and (Let-Sig).
    ///
    /// Sequential `let` chains in a block iterate rather than recursing through
    /// `check`/`infer` -> `let_` -> `body`, keeping long blocks within a 2 MiB
    /// test stack.
    pub(super) fn let_(
        &mut self,
        whole: &Term,
        x: &Name,
        sig: Option<&Scheme>,
        e1: &Term,
        e2: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let saved_locals = self.locals.len();
        let mut traced_infer: Vec<&Term> = Vec::new();
        let mut cur_whole = whole;
        let mut cur_x = x;
        let mut cur_sig = sig;
        let mut cur_e1 = e1;
        let mut cur_e2 = e2;

        let result = loop {
            if cur_sig.is_none()
                && let Some((owner, target)) = operand_call(cur_x, cur_e1, cur_e2)
            {
                break self.operand_let(cur_whole, owner, target, expected);
            }
            let s = match self.let_rhs(cur_sig, cur_e1) {
                Ok(s) => s,
                Err(err) => break Err(err),
            };
            let bound = s.clone();
            self.note(cur_e1, |n| n.scheme = Some(bound));
            let (s, opened) = match self.open_let_binding(cur_x, s, cur_e2) {
                Ok(v) => v,
                Err(err) => break Err(err),
            };
            if let Some((w, sig)) = opened {
                self.level += 1;
                self.witness_levels.insert(w, self.level);
                let res = self.body(cur_x, s, cur_e2, expected);
                self.level -= 1;
                let t = match res {
                    Ok(t) => self.zonk(&t),
                    Err(err) => break Err(err),
                };
                let t = if expected.is_none() {
                    Self::close_witness(&t, w)
                } else {
                    t
                };
                if self.mentions_witness(&t, w) {
                    break error(
                        "Module",
                        format!("abstract type of `{}` leaves its `unpack` scope", sig.name),
                    );
                }
                self.lower(&t, self.level);
                break Ok(t);
            }
            self.locals.push((cur_x.clone(), s));
            if let Term::Let(next_x, next_sig, next_e1, next_e2) = cur_e2 {
                let can_continue =
                    expected.is_none_or(|exp| !matches!(self.resolve(exp), Type::Forall(_)));
                if can_continue {
                    if self.trace.is_some() {
                        if let Some(exp) = expected {
                            let exp = exp.clone();
                            self.note(cur_e2, |n| n.expected = Some(exp));
                        } else {
                            traced_infer.push(cur_e2);
                        }
                    }
                    cur_whole = cur_e2;
                    cur_x = next_x;
                    cur_sig = next_sig.as_deref();
                    cur_e1 = next_e1;
                    cur_e2 = next_e2;
                    continue;
                }
            }
            break match expected {
                Some(t) => self.check(cur_e2, t).map(|()| t.clone()),
                None => self.infer(cur_e2),
            };
        };

        self.locals.truncate(saved_locals);
        if let Ok(ref t) = result
            && self.trace.is_some()
        {
            for term in traced_infer.into_iter().rev() {
                let inferred = t.clone();
                self.note(term, |n| {
                    n.inferred = Some(inferred);
                });
            }
        }
        result
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
    /// inferred, as a `let`, `let rec` and the `fix` its step gives read it;
    /// `rec` is bound at it in `e`.
    fn signed(&mut self, e: &Term, s: &Scheme, rec: Option<&Name>) -> Check<Scheme> {
        let s = self.local_signature(s);
        let (bare, qualifier) = match &s.body {
            Type::Captured(t, c) => ((**t).clone(), c.clone()),
            t => (t.clone(), Caps::default()),
        };
        let bare = Scheme {
            body: bare,
            ..s.clone()
        };
        let checked = self.check_qualified_forall(e, &bare, qualifier, rec.map(|f| (f, &s)), true);
        checked.map(|()| s)
    }

    /// A local signature of a function: its own qualifier is inferred, a
    /// fresh capture row, unless written (`regions.md` §2.6, `docs/formal/regions.md#local-signatures-infer-only-the-outer-qualifier`).
    fn local_signature(&mut self, s: &Scheme) -> Scheme {
        let s = self.in_enclosing(s);
        let s = self.live_scheme(&s);
        match &s.body {
            Type::Fun(..) => {
                let q = self.fresh_caps();
                let qualified = place_qualifiers(&Type::Captured(Rc::new(s.body.clone()), q));
                Scheme {
                    body: qualified,
                    ..s
                }
            }
            _ => s,
        }
    }

    /// (Let-Rec): without a signature the recursive calls are monomorphic;
    /// with one they take its regions, as its body is checked at them.
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

    /// (Fix): `fix f. λx. e` is `let rec f : σ = λx. e in f`, with `σ` the
    /// local signature of the `let` whose step made it: its own qualifier
    /// is inferred as (Let-Sig) inferred it, so a state types the λ as the
    /// source did (`docs/formal/core.md#stepping-a-signed-let-loses-its-signature`).
    pub(super) fn fix(
        &mut self,
        f: &Name,
        sig: Option<&Scheme>,
        lam: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let at = Term::Var(f.clone());
        let Some(sig) = sig else {
            return self.let_rec(f, None, lam, &at, expected);
        };
        let s = self.signed(lam, sig, Some(f))?;
        self.body(f, s, &at, expected)
    }
}
