//! The Omega test (Pugh, 1991), recording a certificate as it refutes.
//!
//! An equality is eliminated through a variable with a unit coefficient,
//! or, where it has none, through a fresh variable that shrinks its
//! coefficients. A variable bounded on one side only is dropped with its
//! bounds. Otherwise its lower and upper bounds are combined: exactly when
//! one side's coefficients are all one; else the real shadow may refute,
//! and when it does not, the integer line is split near each lower bound
//! into equalities, and away from all of them into combinations that imply
//! the dark shadow, so that a point of what remains has an integer `x`.

#![allow(
    clippy::many_single_char_names,
    reason = "the names of the Omega test's account: a, b, k, x"
)]

use super::{Linear, Proof, Step};

/// What the facts are: refuted, with a point that satisfies them all, or
/// not decided within the budget.
#[derive(Debug)]
pub enum Outcome {
    Refuted(Proof),
    Model(Vec<i128>),
    Unknown,
}

/// The Omega test of `hyps` over `vars` variables, spending from `budget`
/// derivations and leaving the remaining allowance in `*budget`.
pub fn decide_budgeted(hyps: &[Linear], vars: usize, budget: &mut u32) -> Outcome {
    let mut search = Search {
        facts: hyps.to_vec(),
        vars,
        budget: *budget,
    };
    let active = (0..hyps.len()).collect();
    let outcome = match search.solve(active) {
        Ok(Res::Refuted(p)) => Outcome::Refuted(p),
        Ok(Res::Model(mut m)) => {
            m.truncate(vars);
            let model: Vec<i128> = m.into_iter().map(|v| v.unwrap_or(0)).collect();
            // the model is checked, not trusted
            if hyps.iter().all(|h| h.eval(&model).is_some_and(|v| v >= 0)) {
                Outcome::Model(model)
            } else {
                Outcome::Unknown
            }
        }
        Err(Stop) => Outcome::Unknown,
    };
    *budget = search.budget;
    outcome
}

/// The Omega test of `hyps` over `vars` variables, in at most `budget`
/// derivations.
pub fn decide(hyps: &[Linear], vars: usize, mut budget: u32) -> Outcome {
    decide_budgeted(hyps, vars, &mut budget)
}

struct Search {
    facts: Vec<Linear>,
    vars: usize,
    budget: u32,
}

/// The budget ran out, or a number overflowed.
struct Stop;

enum Res {
    Refuted(Proof),
    /// A value for each variable the facts name; `None` is free.
    Model(Vec<Option<i128>>),
}

impl Search {
    fn spend(&mut self) -> Result<(), Stop> {
        self.budget = self.budget.checked_sub(1).ok_or(Stop)?;
        Ok(())
    }

    /// Adds the fact of `step`, recording it.
    fn derive(&mut self, steps: &mut Vec<Step>, step: Step) -> Result<usize, Stop> {
        self.spend()?;
        let fact = match &step {
            Step::Comb(parts) => {
                Linear::combine(parts.iter().map(|(i, c)| (&self.facts[*i], *c))).ok_or(Stop)?
            }
            Step::Tighten(i, d) => self.facts[*i].tighten(*d).ok_or(Stop)?,
            Step::Fresh { .. } => unreachable!("fresh variables are added by `fresh`"),
        };
        steps.push(step);
        self.facts.push(fact);
        Ok(self.facts.len() - 1)
    }

    fn solve(&mut self, active: Vec<usize>) -> Result<Res, Stop> {
        let mark = self.facts.len();
        let vars = self.vars;
        let mut steps = Vec::new();
        let out = self.solve_with(active, &mut steps);
        self.facts.truncate(mark);
        self.vars = vars;
        Ok(match out? {
            Res::Refuted(p) if steps.is_empty() => Res::Refuted(p),
            Res::Refuted(p) => Res::Refuted(Proof::Derive(steps, Box::new(p))),
            model @ Res::Model(_) => model,
        })
    }

    fn solve_with(&mut self, active: Vec<usize>, steps: &mut Vec<Step>) -> Result<Res, Stop> {
        self.spend()?;
        // normal facts: no constant, and coefficients without a common divisor
        let mut facts = Vec::new();
        for i in active {
            let f = &self.facts[i];
            if f.coeffs.is_empty() {
                if f.constant < 0 {
                    return Ok(Res::Refuted(Proof::Absurd(i)));
                }
                continue;
            }
            let g = f.gcd();
            facts.push(if g > 1 {
                self.derive(steps, Step::Tighten(i, g))?
            } else {
                i
            });
        }
        // of facts with the same coefficients, the one with the least
        // constant implies the others
        let mut strongest: Vec<usize> = Vec::with_capacity(facts.len());
        for i in facts {
            match strongest
                .iter()
                .position(|&j| self.facts[j].coeffs == self.facts[i].coeffs)
            {
                Some(n) if self.facts[i].constant < self.facts[strongest[n]].constant => {
                    strongest[n] = i;
                }
                Some(_) => {}
                None => strongest.push(i),
            }
        }
        let facts = strongest;
        // a tightened pair may now contradict
        for (n, &i) in facts.iter().enumerate() {
            for &j in &facts[n + 1..] {
                let sum =
                    Linear::combine([(&self.facts[i], 1), (&self.facts[j], 1)]).ok_or(Stop)?;
                if sum.coeffs.is_empty() && sum.constant < 0 {
                    let k = self.derive(steps, Step::Comb(vec![(i, 1), (j, 1)]))?;
                    return Ok(Res::Refuted(Proof::Absurd(k)));
                }
            }
        }
        if let Some((e, n)) = self.equality(&facts) {
            return self.eliminate_equality(facts, e, n, steps);
        }
        let Some(x) = self.choose(&facts) else {
            return Ok(Res::Model(vec![None; self.vars]));
        };
        self.eliminate(&facts, x, steps)
    }

    /// Two facts that are each other's negation: an equality.
    fn equality(&self, facts: &[usize]) -> Option<(usize, usize)> {
        for (n, &i) in facts.iter().enumerate() {
            let neg = self.facts[i].neg()?;
            if let Some(&j) = facts[n + 1..].iter().find(|&&j| self.facts[j] == neg) {
                return Some((i, j));
            }
        }
        None
    }

    /// The variable whose elimination combines the fewest pairs, exact
    /// ones first.
    fn choose(&self, facts: &[usize]) -> Option<usize> {
        let mut best: Option<(bool, usize, usize)> = None;
        let mut xs: Vec<usize> = facts
            .iter()
            .flat_map(|&i| self.facts[i].coeffs.iter().map(|(x, _)| *x))
            .collect();
        xs.sort_unstable();
        xs.dedup();
        for x in xs {
            let (lower, upper) = self.bounds(facts, x);
            let exact = lower.iter().all(|&i| self.facts[i].coeff(x) == 1)
                || upper.iter().all(|&i| self.facts[i].coeff(x) == -1);
            let cost = lower.len() * upper.len();
            if best.is_none_or(|(e, c, _)| (!exact, cost) < (!e, c)) {
                best = Some((exact, cost, x));
            }
        }
        best.map(|(_, _, x)| x)
    }

    fn bounds(&self, facts: &[usize], x: usize) -> (Vec<usize>, Vec<usize>) {
        let lower = facts
            .iter()
            .copied()
            .filter(|&i| self.facts[i].coeff(x) > 0)
            .collect();
        let upper = facts
            .iter()
            .copied()
            .filter(|&i| self.facts[i].coeff(x) < 0)
            .collect();
        (lower, upper)
    }

    /// `e == 0`, with `n` its negation: through a unit coefficient, or a
    /// fresh variable that makes one.
    fn eliminate_equality(
        &mut self,
        facts: Vec<usize>,
        e: usize,
        n: usize,
        steps: &mut Vec<Step>,
    ) -> Result<Res, Stop> {
        let eq = self.facts[e].clone();
        if let Some(&(x, a)) = eq.coeffs.iter().find(|(_, a)| a.abs() == 1) {
            let mut rest = Vec::new();
            for &h in facts.iter().filter(|&&h| h != e && h != n) {
                let b = self.facts[h].coeff(x);
                rest.push(if b == 0 {
                    h
                } else {
                    // `b * x` cancelled by `|b|` times the side of the equality with `-sign(b) * x`
                    let side = if b.signum() == a.signum() { n } else { e };
                    self.derive(steps, Step::Comb(vec![(h, 1), (side, b.abs())]))?
                });
            }
            return Ok(match self.solve(rest)? {
                Res::Model(mut m) => {
                    // `a * x + r == 0`, so `x == -a * r`
                    let r = eval_without(&eq, x, &m).ok_or(Stop)?;
                    m[x] = Some(a.checked_neg().and_then(|a| a.checked_mul(r)).ok_or(Stop)?);
                    Res::Model(m)
                }
                refuted @ Res::Refuted(_) => refuted,
            });
        }
        // no unit coefficient: `m * s == form`, where `form` has one
        let (k, ak) = *eq.coeffs.iter().min_by_key(|(_, a)| a.abs()).ok_or(Stop)?;
        let m = ak.abs() + 1;
        let form = Linear::new(
            eq.coeffs.iter().map(|(x, a)| (*x, mod_hat(*a, m))),
            mod_hat(eq.constant, m),
        )
        .ok_or(Stop)?;
        debug_assert_eq!(form.coeff(k), -ak.signum());
        let s = self.vars;
        self.vars += 1;
        self.spend()?;
        steps.push(Step::Fresh {
            eq: e,
            neg: n,
            m,
            form: form.clone(),
        });
        let up = Linear::new(
            form.neg().ok_or(Stop)?.coeffs.into_iter().chain([(s, m)]),
            form.constant.checked_neg().ok_or(Stop)?,
        )
        .ok_or(Stop)?;
        let down =
            Linear::new(form.coeffs.iter().copied().chain([(s, -m)]), form.constant).ok_or(Stop)?;
        self.facts.push(up);
        self.facts.push(down);
        let (up, down) = (self.facts.len() - 2, self.facts.len() - 1);
        let mut all = facts;
        all.extend([up, down]);
        // `down` is `form - m * s == 0`, with a unit coefficient on `x_k`
        let out = self.eliminate_equality(all, down, up, steps)?;
        Ok(match out {
            Res::Model(mut m) => {
                m.truncate(s + 1);
                Res::Model(m)
            }
            refuted @ Res::Refuted(_) => refuted,
        })
    }

    /// `x`, bounded on both sides, eliminated from `facts`.
    fn eliminate(&mut self, facts: &[usize], x: usize, steps: &mut Vec<Step>) -> Result<Res, Stop> {
        let (lower, upper) = self.bounds(facts, x);
        let others: Vec<usize> = facts
            .iter()
            .copied()
            .filter(|&i| self.facts[i].coeff(x) == 0)
            .collect();
        if lower.is_empty() || upper.is_empty() {
            let out = self.solve(others)?;
            return self.with_x(out, x, &lower, &upper);
        }
        let exact = lower.iter().all(|&i| self.facts[i].coeff(x) == 1)
            || upper.iter().all(|&i| self.facts[i].coeff(x) == -1);
        let shadow =
            |s: &mut Self, steps: &mut Vec<Step>, lower: &[usize]| -> Result<Vec<usize>, Stop> {
                let mut out = others.clone();
                for &l in lower {
                    for &u in &upper {
                        let (a, b) = (s.facts[l].coeff(x), -s.facts[u].coeff(x));
                        out.push(s.derive(steps, Step::Comb(vec![(l, b), (u, a)]))?);
                    }
                }
                Ok(out)
            };
        if exact {
            let combined = shadow(self, steps, &lower)?;
            let out = self.solve(combined)?;
            return self.with_x(out, x, &lower, &upper);
        }
        // the real shadow refutes the facts when it is refuted
        let mut real_steps = Vec::new();
        let mark = self.facts.len();
        let real = shadow(self, &mut real_steps, &lower)?;
        let real = self.solve(real)?;
        self.facts.truncate(mark);
        if let Res::Refuted(p) = real {
            steps.extend(real_steps);
            return Ok(Res::Refuted(p));
        }
        // the dark shadow: no derivation of the facts, but each of its points
        // has an integer `x`; a refutation of it proves nothing
        let mark = self.facts.len();
        let mut dark = others.clone();
        for &l in &lower {
            for &u in &upper {
                let (a, b) = (self.facts[l].coeff(x), -self.facts[u].coeff(x));
                let both =
                    Linear::combine([(&self.facts[l], b), (&self.facts[u], a)]).ok_or(Stop)?;
                let slack = (a - 1).checked_mul(b - 1).ok_or(Stop)?;
                let shadow =
                    Linear::new(both.coeffs, both.constant.checked_sub(slack).ok_or(Stop)?)
                        .ok_or(Stop)?;
                self.facts.push(shadow);
                dark.push(self.facts.len() - 1);
            }
        }
        let dark = self.solve(dark);
        self.facts.truncate(mark);
        if let Res::Model(m) = dark? {
            return self.with_x(Res::Model(m), x, &lower, &upper);
        }
        self.near(
            &lower,
            0,
            Vec::new(),
            x,
            &upper,
            &others,
            steps,
            lower[0],
            0,
        )
    }

    /// The case split near lower bound `lower[i]`, at `k` above it: `ge` is
    /// the fact `P_i - k >= 0`; `far` the facts already past their bounds.
    #[allow(
        clippy::too_many_arguments,
        reason = "the state of one walk along the lower bounds"
    )]
    fn near(
        &mut self,
        lower: &[usize],
        i: usize,
        mut far: Vec<usize>,
        x: usize,
        upper: &[usize],
        others: &[usize],
        steps: &mut Vec<Step>,
        ge: usize,
        k: i128,
    ) -> Result<Res, Stop> {
        self.spend()?;
        let Some(&l) = lower.get(i) else {
            return self.far(&far, lower, x, upper, others, steps);
        };
        let a = self.facts[l].coeff(x);
        // past `K`, a point is far enough from this bound for the dark shadow
        let reach = upper
            .iter()
            .map(|&u| {
                let b = -self.facts[u].coeff(x);
                ((a - 1) * (b - 1) + b - 1) / b - 1
            })
            .max()
            .ok_or(Stop)?;
        if k > reach {
            far.push(ge);
            let next = lower.get(i + 1).copied().unwrap_or(ge);
            return self.near(lower, i + 1, far, x, upper, others, steps, next, 0);
        }
        // `P_i >= k + 1`, or `P_i <= k`: with `P_i >= k`, an equality
        let p = &self.facts[l];
        let above = Linear::new(p.coeffs.clone(), p.constant.checked_sub(k + 1).ok_or(Stop)?)
            .ok_or(Stop)?;
        let below = Linear::new(
            above.neg().ok_or(Stop)?.coeffs,
            k.checked_sub(p.constant).ok_or(Stop)?,
        )
        .ok_or(Stop)?;
        let mark = self.facts.len();
        self.facts.push(above.clone());
        let mut left_steps = Vec::new();
        let left = self.near(
            lower,
            i,
            far.clone(),
            x,
            upper,
            others,
            &mut left_steps,
            mark,
            k + 1,
        );
        self.facts.truncate(mark);
        let left = wrap(left?, left_steps);
        self.facts.push(below);
        let down = self.facts.len() - 1;
        let mut active: Vec<usize> = upper.iter().chain(others).chain(lower).copied().collect();
        active.extend(far.iter().copied());
        active.extend([ge, down]);
        let right = self.solve(active);
        self.facts.pop();
        match (left, right?) {
            (Res::Refuted(l), Res::Refuted(r)) => {
                Ok(Res::Refuted(Proof::Split(above, Box::new(l), Box::new(r))))
            }
            (Res::Model(m), _) | (_, Res::Model(m)) => Ok(Res::Model(m)),
        }
    }

    /// Every point far from all lower bounds: each far bound combined with
    /// each upper one, which imply the dark shadow of the bounds `lower` and
    /// `upper`, so that a point of the rest has an integer `x` between them.
    fn far(
        &mut self,
        far: &[usize],
        lower: &[usize],
        x: usize,
        upper: &[usize],
        others: &[usize],
        steps: &mut Vec<Step>,
    ) -> Result<Res, Stop> {
        let mut active = others.to_vec();
        for &l in far {
            for &u in upper {
                let (a, b) = (self.facts[l].coeff(x), -self.facts[u].coeff(x));
                active.push(self.derive(steps, Step::Comb(vec![(l, b), (u, a)]))?);
            }
        }
        let out = self.solve(active)?;
        self.with_x(out, x, lower, upper)
    }

    /// `x` given the others: between its bounds.
    fn with_x(&self, out: Res, x: usize, lower: &[usize], upper: &[usize]) -> Result<Res, Stop> {
        let Res::Model(mut m) = out else {
            return Ok(out);
        };
        let mut low: Option<i128> = None;
        for &l in lower {
            let f = &self.facts[l];
            let a = f.coeff(x);
            let r = eval_without(f, x, &m).ok_or(Stop)?;
            // `a * x + r >= 0`: `x >= ceil(-r / a)`
            let v = (-r).div_euclid(a) + i128::from((-r).rem_euclid(a) != 0);
            low = Some(low.map_or(v, |w| w.max(v)));
        }
        let mut high: Option<i128> = None;
        for &u in upper {
            let f = &self.facts[u];
            let b = -f.coeff(x);
            let r = eval_without(f, x, &m).ok_or(Stop)?;
            // `-b * x + r >= 0`: `x <= floor(r / b)`
            let v = r.div_euclid(b);
            high = Some(high.map_or(v, |w| w.min(v)));
        }
        let v = match (low, high) {
            (Some(l), Some(h)) if l > h => return Err(Stop),
            (Some(l), _) => l,
            (None, Some(h)) => h,
            (None, None) => 0,
        };
        if m.len() <= x {
            m.resize(x + 1, None);
        }
        m[x] = Some(v);
        Ok(Res::Model(m))
    }
}

fn wrap(res: Res, steps: Vec<Step>) -> Res {
    match res {
        Res::Refuted(p) if !steps.is_empty() => Res::Refuted(Proof::Derive(steps, Box::new(p))),
        other => other,
    }
}

/// `f` at the values `m`, without its `x` term; a free variable is zero.
fn eval_without(f: &Linear, x: usize, m: &[Option<i128>]) -> Option<i128> {
    let mut sum = f.constant;
    for (y, a) in &f.coeffs {
        if *y != x {
            let v = m.get(*y).copied().flatten().unwrap_or(0);
            sum = sum.checked_add(a.checked_mul(v)?)?;
        }
    }
    Some(sum)
}

/// `a` modulo `m`, between `-m/2` and `m/2`.
fn mod_hat(a: i128, m: i128) -> i128 {
    a - m * (2 * a + m).div_euclid(2 * m)
}
