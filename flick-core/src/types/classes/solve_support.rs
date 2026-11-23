//! Functional-dependency improvement and structural record/tuple instances
//! supporting the main instance solver (`solve.rs`).

use super::matching::{Match, Subst, match_type};
use super::solve::{Givens, Outcome};
use super::{Dict, split, vars};
use crate::types::Checker;
use crate::types::env::Instance;
use crate::types::ty::Type;

impl Checker<'_> {
    /// Reconstructs a class constraint when its first argument determines
    /// the other arguments. Substitution preserves higher-rank outputs.
    /// The caller still has to solve the reconstructed constraint.
    pub(in crate::types) fn project_constraint(
        &self,
        class: &Type,
        input: &Type,
        givens: &Givens,
    ) -> Option<Type> {
        let (class_name, _) = split(class)?;
        let info = self.class_info(class_name.0, class_name.1)?;
        let produces_all = info
            .fundeps
            .iter()
            .any(|fd| fd.from == [0] && (1..info.params.len()).all(|i| fd.to.contains(&i)));
        if !produces_all {
            return None;
        }
        for (given, _) in givens {
            if let Some((name, args)) = split(given)
                && name == class_name
                && args.first().copied() == Some(input)
            {
                return Some(given.clone());
            }
        }
        let mut projections = self.env.instances().filter_map(|instance| {
            let (name, args) = split(&instance.head)?;
            let pattern = args.first()?;
            let mut subst = Subst::new();
            (name == class_name && match_type(pattern, input, &mut subst) == Match::Yes)
                .then(|| instance.head.substitute(&subst))
        });
        let projected = projections.next()?;
        projections
            .all(|other| other == projected)
            .then_some(projected)
    }

    /// An assumption of the same class that agrees on the determining
    /// arguments of a functional dependency fixes the determined ones.
    pub(super) fn improve_by_givens(
        &mut self,
        module: &str,
        name: &str,
        args: &[Type],
        givens: &Givens,
    ) -> bool {
        let Some(info) = self.class_info(module, name) else {
            return false;
        };
        for (g, _) in givens {
            let Some((class, gargs)) = split(g) else {
                continue;
            };
            if class != (module, name) {
                continue;
            }
            for fd in &info.fundeps {
                let agree = fd
                    .from
                    .iter()
                    .all(|&i| gargs.get(i).copied() == args.get(i));
                let differ = fd.to.iter().any(|&j| gargs.get(j).copied() != args.get(j));
                if agree
                    && differ
                    && self
                        .inference_trial(|this| {
                            for &j in &fd.to {
                                let (Some(a), Some(b)) = (args.get(j), gargs.get(j)) else {
                                    return Err(());
                                };
                                this.unify(a, b).map_err(|_| ())?;
                            }
                            Ok(())
                        })
                        .is_ok()
                {
                    return true;
                }
            }
        }
        false
    }

    /// An instance whose head matches the determining arguments of a
    /// functional dependency fixes the determined ones.
    pub(super) fn improve_by_instances(
        &mut self,
        instances: &[Instance],
        class: (&str, &str),
        args: &[Type],
    ) -> bool {
        let Some(info) = self.class_info(class.0, class.1) else {
            return false;
        };
        for instance in instances {
            let Some((_, pattern)) = split(&instance.head) else {
                continue;
            };
            for fd in &info.fundeps {
                let mut subst = Subst::new();
                let determined = fd
                    .from
                    .iter()
                    .all(|&i| match (pattern.get(i), args.get(i)) {
                        (Some(p), Some(t)) => match_type(p, t, &mut subst) == Match::Yes,
                        _ => false,
                    });
                if !determined {
                    continue;
                }
                if fd.to.iter().any(|&j| match (pattern.get(j), args.get(j)) {
                    (Some(p), Some(t)) => match_type(p, t, &mut subst) == Match::No,
                    _ => true,
                }) {
                    continue;
                }
                let attempt = self.inference_trial(|this| {
                    let mut candidate = subst.clone();
                    let mut changed = false;
                    for &j in &fd.to {
                        let (Some(p), Some(t)) = (pattern.get(j), args.get(j)) else {
                            return Err(());
                        };
                        for v in vars(p) {
                            if !candidate.iter().any(|(n, _)| *n == v) {
                                candidate.push((v, this.fresh()));
                            }
                        }
                        let target = p.substitute(&candidate);
                        this.unify(t, &target).map_err(|_| ())?;
                        changed |= this.canon(t) != *t;
                    }
                    Ok(changed)
                });
                if attempt == Ok(true) {
                    return true;
                }
            }
        }
        false
    }

    /// `C { a : A; b : B }` from `C A` and `C B`, `C (A, B)` likewise.
    pub(super) fn structural_instance(
        &mut self,
        c: &Type,
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
        let (head, args) = c.spine();
        let class = head.clone();
        let items: Vec<Type> = match args.first() {
            Some(Type::Tuple(items)) => items.clone(),
            Some(Type::Record(row)) => match row.tail.as_deref() {
                None => row.entries.iter().map(|(_, t)| t.clone()).collect(),
                Some(Type::Meta(_)) => return Outcome::Stuck,
                Some(_) => {
                    return Outcome::Missing(
                        c.clone(),
                        Some("the record is open, so its other fields are not known; require the constraint for the whole record".to_owned()),
                    );
                }
            },
            _ => return Outcome::Missing(c.clone(), None),
        };
        let mut fields = Vec::new();
        let mut stuck = false;
        for item in items {
            let c = self.settled(Type::app(class.clone(), item));
            match self.entail(&c, givens, chain) {
                Outcome::Solved(dict) => fields.push(dict),
                Outcome::Stuck => stuck = true,
                other => return other,
            }
        }
        if stuck {
            return Outcome::Stuck;
        }
        let class = split(c).map(|((_, n), _)| n.to_owned()).unwrap_or_default();
        Outcome::Solved(Dict::Structural { class, fields })
    }
}
