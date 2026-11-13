//! Membership in a row, at one label, is a disjunction of row-variable
//! memberships and a known-label bit. Row equalities are therefore dual-Horn
//! clauses. Their greatest model decides inclusion without choosing a least
//! residual for an idempotent equation such as `{Db | r} = {Db}`.

use std::collections::HashSet;

/// Whether a label belongs to a union: a known element or any variable.
#[derive(Clone, Debug, Default)]
pub(super) struct Union {
    pub known: bool,
    pub variables: Vec<usize>,
}

/// If every variable on the right is false, the left must be false too.
/// A missing left denotes the constant true, so this premise is inconsistent.
struct Rule {
    right: Vec<usize>,
    left: Option<usize>,
}

#[derive(Default)]
pub(super) struct Theory {
    rules: Vec<Rule>,
}

impl Theory {
    pub fn equal(&mut self, a: &Union, b: &Union) {
        self.include(a, b);
        self.include(b, a);
    }

    fn include(&mut self, a: &Union, b: &Union) {
        if b.known {
            return;
        }
        for &variable in &a.variables {
            self.rules.push(Rule {
                right: b.variables.clone(),
                left: Some(variable),
            });
        }
        if a.known {
            self.rules.push(Rule {
                right: b.variables.clone(),
                left: None,
            });
        }
    }

    /// All variables not forced false are true in the greatest model.
    fn greatest(&self, false_variables: &[usize]) -> Option<HashSet<usize>> {
        let mut absent: HashSet<_> = false_variables.iter().copied().collect();
        loop {
            let mut changed = false;
            for rule in &self.rules {
                if rule.right.iter().all(|v| absent.contains(v)) {
                    match rule.left {
                        Some(v) => changed |= absent.insert(v),
                        None => return None,
                    }
                }
            }
            if !changed {
                return Some(absent);
            }
        }
    }

    pub fn entails(&self, a: &Union, b: &Union) -> bool {
        if b.known {
            return true;
        }
        // A countermodel has b false and a true. The greatest model with
        // b false satisfies a iff any model with b false can satisfy a.
        match self.greatest(&b.variables) {
            None => true,
            Some(absent) => !a.known && a.variables.iter().all(|v| absent.contains(v)),
        }
    }

    pub fn consistent(&self) -> bool {
        self.greatest(&[]).is_some()
    }
}

#[test]
fn inclusion_agrees_with_every_boolean_assignment() {
    let expression = |bits: usize| Union {
        known: bits & 8 != 0,
        variables: (0..3).filter(|v| bits & (1 << v) != 0).collect(),
    };
    let value = |row: &Union, assignment: usize| {
        row.known || row.variables.iter().any(|v| assignment & (1 << v) != 0)
    };
    for left in 0..16 {
        for right in 0..16 {
            let (left, right) = (expression(left), expression(right));
            let mut theory = Theory::default();
            theory.equal(&left, &right);
            for a in 0..16 {
                for b in 0..16 {
                    let (a, b) = (expression(a), expression(b));
                    let expected = (0..8).all(|assignment| {
                        value(&left, assignment) != value(&right, assignment)
                            || !value(&a, assignment)
                            || value(&b, assignment)
                    });
                    assert_eq!(
                        theory.entails(&a, &b),
                        expected,
                        "{left:?} = {right:?}; {a:?} <= {b:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn equalities_compose_and_do_not_choose_empty_residuals() {
    let r = Union {
        known: false,
        variables: vec![0],
    };
    let s = Union {
        known: false,
        variables: vec![1],
    };
    let present = Union {
        known: true,
        variables: vec![],
    };
    let absent = Union::default();
    let mut theory = Theory::default();
    theory.equal(&r, &s);
    theory.equal(&s, &present);
    assert!(theory.entails(&present, &r));
    assert!(!theory.entails(&r, &absent));

    // At Db, `{Db | r} = {Db}` leaves r unconstrained. At another label,
    // the same equation forces r absent. No single empty-row witness is chosen.
    let mut residual = Theory::default();
    residual.equal(
        &Union {
            known: true,
            variables: vec![0],
        },
        &present,
    );
    assert!(!residual.entails(&r, &absent));
    let mut elsewhere = Theory::default();
    elsewhere.equal(&r, &absent);
    assert!(elsewhere.entails(&r, &absent));
}
