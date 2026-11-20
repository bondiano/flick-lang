//! Linear arithmetic over the integers: the Omega test decides a
//! conjunction of linear inequalities, and every refutation it finds is a
//! certificate `check` validates again by integer arithmetic alone.
//!
//! A fact is a linear form read as `form >= 0`; an equality is a fact and
//! its negation. A certificate derives facts by:
//! - a combination with positive coefficients of facts it has;
//! - tightening: a fact whose coefficients a `d` divides, divided by `d`
//!   with its constant rounded down, which holds of every integer point;
//! - a split: `form >= 0`, then `-form - 1 >= 0`, each refuted in turn;
//! - a fresh variable `s` with `m * s == form` (as two facts), when an
//!   equality `e == 0` it has makes `form - e` a multiple of `m`, so some
//!   integer `s` is `form / m` at every point of the facts.
//!
//! It refutes them with a fact that is a negative constant.

mod search;

pub use search::{Outcome, decide};

/// The derivations a decision may take: enough for every system of a few
/// variables with small coefficients, and bounded for every system.
pub const BUDGET: u32 = 5_000_000;

/// `sum of coeffs[i] * x_i + constant`, read as `>= 0`, with the variables
/// in increasing order and no zero coefficient.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Linear {
    pub coeffs: Vec<(usize, i128)>,
    pub constant: i128,
}

impl Linear {
    pub fn constant(constant: i128) -> Self {
        Linear {
            coeffs: Vec::new(),
            constant,
        }
    }

    /// `sum of terms + constant`, terms in any order and repeated.
    pub fn new(terms: impl IntoIterator<Item = (usize, i128)>, constant: i128) -> Option<Self> {
        let mut coeffs: Vec<(usize, i128)> = Vec::new();
        for (x, a) in terms {
            match coeffs.binary_search_by_key(&x, |(v, _)| *v) {
                Ok(i) => coeffs[i].1 = coeffs[i].1.checked_add(a)?,
                Err(i) => coeffs.insert(i, (x, a)),
            }
        }
        coeffs.retain(|(_, a)| *a != 0);
        Some(Linear { coeffs, constant })
    }

    pub fn coeff(&self, x: usize) -> i128 {
        (self.coeffs.binary_search_by_key(&x, |(v, _)| *v)).map_or(0, |i| self.coeffs[i].1)
    }

    /// `-self`.
    pub fn neg(&self) -> Option<Self> {
        let coeffs = (self.coeffs.iter())
            .map(|(x, a)| Some((*x, a.checked_neg()?)))
            .collect::<Option<_>>()?;
        Some(Linear {
            coeffs,
            constant: self.constant.checked_neg()?,
        })
    }

    /// `sum of c * fact` over `parts`, each `c` positive.
    pub fn combine<'a>(parts: impl IntoIterator<Item = (&'a Linear, i128)>) -> Option<Self> {
        let mut terms = Vec::new();
        let mut constant: i128 = 0;
        for (fact, c) in parts {
            if c <= 0 {
                return None;
            }
            for (x, a) in &fact.coeffs {
                terms.push((*x, a.checked_mul(c)?));
            }
            constant = constant.checked_add(fact.constant.checked_mul(c)?)?;
        }
        Linear::new(terms, constant)
    }

    /// `self / d`, the constant rounded down: `None` unless `d > 0` divides
    /// every coefficient.
    pub fn tighten(&self, d: i128) -> Option<Self> {
        if d <= 0 || self.coeffs.iter().any(|(_, a)| a % d != 0) {
            return None;
        }
        Some(Linear {
            coeffs: self.coeffs.iter().map(|(x, a)| (*x, a / d)).collect(),
            constant: self.constant.div_euclid(d),
        })
    }

    /// The greatest common divisor of the coefficients, `0` for a constant.
    pub fn gcd(&self) -> i128 {
        self.coeffs.iter().fold(0, |g, (_, a)| gcd(g, a.abs()))
    }

    /// The form at `values`, `None` where it overflows.
    pub fn eval(&self, values: &[i128]) -> Option<i128> {
        let mut sum = self.constant;
        for (x, a) in &self.coeffs {
            sum = sum.checked_add(a.checked_mul(*values.get(*x)?)?)?;
        }
        Some(sum)
    }
}

pub fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.abs()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A positive combination of facts, by index.
    Comb(Vec<(usize, i128)>),
    /// A fact divided by `d`, its constant rounded down.
    Tighten(usize, i128),
    /// A fresh variable `s`, the next index, with `m * s - form >= 0` and
    /// `form - m * s >= 0`: the facts `eq` and `neg` are `e` and `-e`, and
    /// `m` divides every coefficient and the constant of `form - e`.
    Fresh {
        eq: usize,
        neg: usize,
        m: i128,
        form: Linear,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proof {
    /// Fact `i` is a constant below zero.
    Absurd(usize),
    /// The facts of the steps, in order, then the rest of the proof.
    Derive(Vec<Step>, Box<Proof>),
    /// `form >= 0` refuted by the first, `-form - 1 >= 0` by the second.
    Split(Linear, Box<Proof>, Box<Proof>),
}

/// Whether `proof` refutes `hyps` over `vars` integer variables.
pub fn check(hyps: &[Linear], vars: usize, proof: &Proof) -> bool {
    let mut facts = hyps.to_vec();
    if facts
        .iter()
        .any(|f| f.coeffs.iter().any(|(x, _)| *x >= vars))
    {
        return false;
    }
    refutes(&mut facts, vars, proof)
}

fn refutes(facts: &mut Vec<Linear>, vars: usize, proof: &Proof) -> bool {
    match proof {
        Proof::Absurd(i) => facts
            .get(*i)
            .is_some_and(|f| f.coeffs.is_empty() && f.constant < 0),
        Proof::Derive(steps, rest) => {
            let mark = facts.len();
            let mut vars = vars;
            for step in steps {
                let Some(new) = derive(facts, &mut vars, step) else {
                    facts.truncate(mark);
                    return false;
                };
                facts.extend(new);
            }
            let ok = refutes(facts, vars, rest);
            facts.truncate(mark);
            ok
        }
        Proof::Split(form, left, right) => {
            if form.coeffs.iter().any(|(x, _)| *x >= vars) {
                return false;
            }
            let Some(other) = form
                .neg()
                .and_then(|n| Linear::new(n.coeffs, n.constant.checked_sub(1)?))
            else {
                return false;
            };
            facts.push(form.clone());
            let ok = refutes(facts, vars, left);
            facts.pop();
            if !ok {
                return false;
            }
            facts.push(other);
            let ok = refutes(facts, vars, right);
            facts.pop();
            ok
        }
    }
}

/// The facts `step` derives from `facts`, with `vars` grown by a fresh one.
fn derive(facts: &[Linear], vars: &mut usize, step: &Step) -> Option<Vec<Linear>> {
    match step {
        Step::Comb(parts) => {
            let parts = (parts.iter())
                .map(|(i, c)| Some((facts.get(*i)?, *c)))
                .collect::<Option<Vec<_>>>()?;
            Some(vec![Linear::combine(parts)?])
        }
        Step::Tighten(i, d) => Some(vec![facts.get(*i)?.tighten(*d)?]),
        Step::Fresh { eq, neg, m, form } => {
            let (e, n) = (facts.get(*eq)?, facts.get(*neg)?);
            if *m <= 0 || e.neg()? != *n || form.coeffs.iter().any(|(x, _)| *x >= *vars) {
                return None;
            }
            let diff = Linear::combine([(form, 1)]).and_then(|f| {
                let back = e.neg()?;
                Linear::new(
                    f.coeffs.into_iter().chain(back.coeffs),
                    f.constant.checked_add(back.constant)?,
                )
            })?;
            if diff.constant % m != 0 || diff.coeffs.iter().any(|(_, a)| a % m != 0) {
                return None;
            }
            let s = *vars;
            *vars += 1;
            let up = Linear::new(
                form.neg()?.coeffs.into_iter().chain([(s, *m)]),
                form.constant.checked_neg()?,
            )?;
            let down = Linear::new(
                form.coeffs.iter().copied().chain([(s, m.checked_neg()?)]),
                form.constant,
            )?;
            Some(vec![up, down])
        }
    }
}

#[cfg(test)]
mod tests;
