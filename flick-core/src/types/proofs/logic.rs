//! What facts establish of a formula. A formula is evaluated left to right
//! and stops early as `&&` and `||` do, and an atom may be a call that never
//! returns. A fact is a formula that ran to its result: it tells what each
//! part that ran gave, and nothing of a part that may not have run.
//! [`Knowledge::holds`] proves a result only from parts it shows ran, so
//! `f x || true` is never proved for an `f x` nothing observed, while
//! `true || f x` is. `docs/formal/refinements.md` states the rules.

use std::rc::Rc;

use super::interval::{Bound, Interval, Op};

/// A Boolean expression in the order the program evaluates it.
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum Formula<A> {
    Const(bool),
    Atom(A),
    Not(Box<Formula<A>>),
    And(Box<Formula<A>>, Box<Formula<A>>),
    Or(Box<Formula<A>>, Box<Formula<A>>),
    /// What the logic does not read: it may give either result, or none.
    Opaque,
}

impl<A> Formula<A> {
    /// Each atom, in the order the formula runs them.
    pub fn each_atom(&self, f: &mut dyn FnMut(&A)) {
        match self {
            Formula::Atom(a) => f(a),
            Formula::Not(x) => x.each_atom(f),
            Formula::And(x, y) | Formula::Or(x, y) => {
                x.each_atom(f);
                y.each_atom(f);
            }
            Formula::Const(_) | Formula::Opaque => {}
        }
    }

    /// The formula with each atom replaced by what `f` makes of it.
    pub fn map<B>(&self, f: &mut dyn FnMut(&A) -> Formula<B>) -> Formula<B> {
        match self {
            Formula::Const(b) => Formula::Const(*b),
            Formula::Atom(a) => f(a),
            Formula::Not(x) => Formula::Not(Box::new(x.map(f))),
            Formula::And(x, y) => Formula::And(Box::new(x.map(f)), Box::new(y.map(f))),
            Formula::Or(x, y) => Formula::Or(Box::new(x.map(f)), Box::new(y.map(f))),
            Formula::Opaque => Formula::Opaque,
        }
    }
}

/// The steps learning from facts may take. Past them nothing more is
/// learned: what is known stays justified, and a proof that needed more is
/// unknown, never accepted for lack of time.
const BUDGET: u32 = 10_000;

/// What the logic reads of an atom beyond its identity.
pub trait Atomic: Clone + PartialEq {
    /// What an atom compares with a constant.
    type Subject: Clone + PartialEq;
    /// `subject op bound`, when the atom is such a comparison.
    fn comparison(&self) -> Option<(&Self::Subject, Op, Bound)>;
    /// The result the atom gives by itself: one of constants alone.
    fn constant(&self) -> Option<bool>;
}

/// What facts establish: the result each atom gave when it ran, and the
/// values each compared subject may still have. A handful of each: lists
/// cost less code than maps.
pub struct Knowledge<A: Atomic> {
    atoms: Vec<(A, bool)>,
    intervals: Vec<(A::Subject, Interval)>,
    steps: u32,
}

impl<A: Atomic> Knowledge<A> {
    /// What `facts`, formulas each with the result it ran to, establish.
    /// `unfold` gives the definition of an atom that is a call of a check
    /// the logic reads: it ran, with the result of the call, where the call
    /// ran.
    pub fn of(facts: &[(Rc<Formula<A>>, bool)], unfold: &dyn Fn(&A) -> Option<Formula<A>>) -> Self {
        let mut knowledge = Knowledge {
            atoms: Vec::new(),
            intervals: Vec::new(),
            steps: 0,
        };
        // one fact can make another tell more: `p || q` once `p` is false
        loop {
            let known = knowledge.atoms.len();
            for (fact, result) in facts {
                knowledge.assume(fact, *result, unfold);
            }
            if knowledge.atoms.len() == known || knowledge.steps >= BUDGET {
                return knowledge;
            }
        }
    }

    /// Learns that `formula` ran to `result`.
    pub fn assume(
        &mut self,
        formula: &Formula<A>,
        result: bool,
        unfold: &dyn Fn(&A) -> Option<Formula<A>>,
    ) {
        if self.steps >= BUDGET {
            return;
        }
        self.steps += 1;
        match formula {
            Formula::Const(_) | Formula::Opaque => {}
            Formula::Atom(atom) => {
                // a second result for an atom is a path no run takes
                if self.atoms.iter().any(|(a, _)| a == atom) {
                    return;
                }
                self.atoms.push((atom.clone(), result));
                if let Some((subject, op, bound)) = atom.comparison() {
                    if let Some((_, interval)) =
                        self.intervals.iter_mut().find(|(s, _)| s == subject)
                    {
                        interval.learn(op, bound, result);
                    } else {
                        let mut interval = Interval::of(bound);
                        interval.learn(op, bound, result);
                        self.intervals.push((subject.clone(), interval));
                    }
                }
                if let Some(definition) = unfold(atom) {
                    self.assume(&definition, result, unfold);
                }
            }
            Formula::Not(x) => self.assume(x, !result, unfold),
            // both ran, to the result of the whole
            Formula::And(x, y) if result => {
                self.assume(x, true, unfold);
                self.assume(y, true, unfold);
            }
            Formula::Or(x, y) if !result => {
                self.assume(x, false, unfold);
                self.assume(y, false, unfold);
            }
            // `x && y` is false: `y` ran and was false if `x` was true, and
            // `x` was false if `y` gives true wherever it runs, as a pure
            // call does; `x || y` is true the other way round
            Formula::And(x, y) | Formula::Or(x, y) => {
                let and = matches!(formula, Formula::And(..));
                if self.holds(x) == Some(and) {
                    self.assume(y, result, unfold);
                } else if self.holds(y) == Some(and) {
                    self.assume(x, result, unfold);
                }
            }
        }
    }

    /// The result `formula` gives, when what is known proves it gives one.
    pub fn holds(&self, formula: &Formula<A>) -> Option<bool> {
        match formula {
            Formula::Const(b) => Some(*b),
            Formula::Atom(atom) => self.atom(atom),
            Formula::Not(x) => self.holds(x).map(|b| !b),
            // the second runs only after the first gave its result; a first
            // that gives `false` to `&&`, `true` to `||`, decides alone
            Formula::And(x, y) | Formula::Or(x, y) => {
                let decides = matches!(formula, Formula::Or(..));
                if self.holds(x)? == decides {
                    Some(decides)
                } else {
                    self.holds(y)
                }
            }
            Formula::Opaque => None,
        }
    }

    /// The result `atom` gives: as it gave it, by itself, or as every value
    /// its subject may have compares.
    fn atom(&self, atom: &A) -> Option<bool> {
        let known = self.atoms.iter().find(|(a, _)| a == atom).map(|(_, b)| *b);
        if let Some(known) = known.or_else(|| atom.constant()) {
            return Some(known);
        }
        let (subject, op, bound) = atom.comparison()?;
        let interval = (self.intervals.iter())
            .find(|(s, _)| s == subject)
            .map(|(_, i)| *i);
        interval
            .unwrap_or_else(|| Interval::of(bound))
            .decide(op, bound)
    }

    /// The first part of `formula`, in the order it runs, that stops a proof
    /// that it gives `wanted`: an atom whose result is unknown, or one that
    /// gives the other result; none when only an opaque part stops it.
    pub fn missing<'f>(&self, formula: &'f Formula<A>, wanted: bool) -> Option<&'f Formula<A>> {
        match formula {
            Formula::Const(b) => (*b != wanted).then_some(formula),
            Formula::Atom(_) => (self.holds(formula) != Some(wanted)).then_some(formula),
            Formula::Not(x) => self.missing(x, !wanted),
            Formula::And(x, y) | Formula::Or(x, y) => {
                // past a first part that does not decide alone, the second
                let decides = matches!(formula, Formula::Or(..));
                if self.holds(x) == Some(!decides) {
                    self.missing(y, wanted)
                } else {
                    self.missing(x, wanted)
                }
            }
            Formula::Opaque => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{Atomic, Bound, Formula, Knowledge, Op};

    impl Atomic for &'static str {
        type Subject = ();
        fn comparison(&self) -> Option<(&(), Op, Bound)> {
            None
        }
        fn constant(&self) -> Option<bool> {
            None
        }
    }

    type F = Formula<&'static str>;

    fn atom(a: &'static str) -> F {
        Formula::Atom(a)
    }

    fn and(x: F, y: F) -> F {
        Formula::And(Box::new(x), Box::new(y))
    }

    fn or(x: F, y: F) -> F {
        Formula::Or(Box::new(x), Box::new(y))
    }

    fn not(x: F) -> F {
        Formula::Not(Box::new(x))
    }

    fn knowing(facts: Vec<(F, bool)>) -> Knowledge<&'static str> {
        let facts: Vec<_> = facts.into_iter().map(|(f, b)| (Rc::new(f), b)).collect();
        Knowledge::of(&facts, &|_| None)
    }

    #[test]
    fn a_conjunction_that_ran_to_true_establishes_both_parts() {
        let k = knowing(vec![(and(atom("p"), atom("q")), true)]);
        assert_eq!(k.holds(&atom("q")), Some(true));
        assert_eq!(k.holds(&and(atom("q"), atom("p"))), Some(true));
    }

    #[test]
    fn a_disjunction_that_ran_to_true_establishes_neither_part() {
        let k = knowing(vec![(or(atom("p"), atom("q")), true)]);
        assert_eq!(k.holds(&atom("p")), None);
        assert_eq!(k.holds(&atom("q")), None);
        // the same disjunction is proved, the other order is not: `q` may
        // never return where `p` held
        assert_eq!(k.holds(&or(atom("q"), atom("p"))), None);
    }

    #[test]
    fn a_disjunction_learns_its_second_part_once_the_first_is_false() {
        let k = knowing(vec![(or(atom("p"), atom("q")), true), (atom("p"), false)]);
        assert_eq!(k.holds(&atom("q")), Some(true));
    }

    #[test]
    fn an_unobserved_call_proves_nothing_even_where_classical_logic_would() {
        let k = knowing(vec![]);
        let tautology = or(atom("f"), Formula::Const(true));
        assert_eq!(k.holds(&tautology), None);
        assert_eq!(k.holds(&or(atom("f"), not(atom("f")))), None);
        // the constant runs first and decides alone
        assert_eq!(k.holds(&or(Formula::Const(true), atom("f"))), Some(true));
    }

    #[test]
    fn negation_and_false_branches_flip_results() {
        let k = knowing(vec![
            (not(atom("p")), false),
            (and(atom("q"), atom("r")), false),
            (atom("q"), true),
        ]);
        assert_eq!(k.holds(&atom("p")), Some(true));
        assert_eq!(k.holds(&atom("r")), Some(false));
    }

    #[test]
    fn the_missing_part_is_the_first_unproved_in_running_order() {
        let k = knowing(vec![(atom("p"), true)]);
        let wanted = and(atom("p"), and(atom("q"), atom("r")));
        assert_eq!(k.missing(&wanted, true), Some(&atom("q")));
        assert_eq!(k.missing(&or(atom("s"), atom("t")), true), Some(&atom("s")));
        assert_eq!(k.missing(&and(atom("p"), Formula::Opaque), true), None);
    }

    #[test]
    fn learning_stays_within_its_budget() {
        // a chain the loop learns one link a round, its facts last to first
        let chain = |n: usize| {
            let names: Vec<&'static str> = (0..n)
                .map(|i| &*Box::leak(format!("a{i}").into_boxed_str()))
                .collect();
            let mut facts = vec![(atom(names[0]), true)];
            for pair in names.windows(2).rev() {
                facts.push((or(not(atom(pair[0])), atom(pair[1])), true));
            }
            (knowing(facts), names[n - 1])
        };
        let (short, last) = chain(10);
        assert_eq!(short.holds(&atom(last)), Some(true));
        // a long one stops learning: what it learned holds, the rest is unknown
        let (long, last) = chain(2000);
        assert_eq!(long.holds(&atom(last)), None);
    }
}
