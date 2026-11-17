//! Each obligation decided once the module is solved: the facts of its path
//! settled into atoms, the invariants of the projections they name added,
//! and the check of the type proved, refuted, or neither.

use std::mem;
use std::rc::Rc;

use super::formula::{Atom, Raw, Value, word};
use super::interval::Bound;
use super::logic::{Formula, Knowledge};
use super::{Checker, Pending};
use crate::diagnostics::codes;
use crate::numeric::Num;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::patterns::is_pattern;
use crate::types::sugar_lints::unparen;
use crate::types::typing::{Range, Ref};

impl Checker<'_> {
    /// Decides each obligation once the module is solved: its operators and
    /// its dictionaries are known.
    pub(in crate::types) fn discharge_proofs(&mut self) {
        if self.proofs.contracts.is_empty() {
            return;
        }
        self.validate_contracts();
        for (at, span, index) in mem::take(&mut self.proofs.uses) {
            // a contract that does not hold is reported, and gives nothing
            if self.proofs.applied.contains(&at) || !self.proofs.contracts[index].active {
                continue;
            }
            let shown = self.operation_name(index, at);
            let message = format!(
                "`{shown}` makes a value only applied to one: it is no function to name alone, pass or apply in part"
            );
            self.error(codes::T078, span, message);
        }
        let summaries: Vec<Option<Formula<Atom>>> = (0..self.proofs.contracts.len())
            .map(|i| self.check_summary(i))
            .collect();
        for pending in mem::take(&mut self.proofs.pending) {
            if self.proofs.contracts[pending.contract].active {
                self.decide_proof(&pending, &summaries);
            }
        }
    }

    fn operation_name(&self, index: usize, at: Range) -> String {
        let contract = &self.proofs.contracts[index];
        if self.proofs.spelled.contains(&at) {
            format!("{}.{}", contract.ty, contract.prove)
        } else {
            contract.constructor.clone()
        }
    }

    fn decide_proof(&mut self, pending: &Pending, summaries: &[Option<Formula<Atom>>]) {
        let contract = &self.proofs.contracts[pending.contract];
        let check = Formula::Atom(Atom::Call {
            module: contract.module.clone(),
            name: contract.check.clone(),
            argument: pending.subject.clone(),
        });
        let mut facts: Vec<(Rc<Formula<Atom>>, bool)> = (pending.facts.iter())
            .map(|(f, b)| (Rc::new(self.settle_fact(f)), *b))
            .collect();
        // each `T.value v` the facts name holds the invariant of `T`
        let mut projections = Vec::new();
        projections_in(&pending.subject, &mut projections);
        for (fact, _) in &facts {
            fact.each_atom(&mut |a| match a {
                Atom::Call { argument: v, .. }
                | Atom::Truth(v)
                | Atom::Compare { value: v, .. }
                | Atom::Equals { value: v, .. } => projections_in(v, &mut projections),
            });
        }
        for (index, of) in projections {
            let held = &self.proofs.contracts[index];
            let atom = Atom::Call {
                module: held.module.clone(),
                name: held.check.clone(),
                argument: Value::Projection(index, of),
            };
            facts.push((Rc::new(Formula::Atom(atom)), true));
        }
        let unfold = |atom: &Atom| {
            let Atom::Call {
                module,
                name,
                argument,
            } = atom
            else {
                return None;
            };
            let index = self.proofs.checked_by(module, name)?;
            let summary = summaries[index].as_ref()?;
            Some(summary.map(&mut |a| Formula::Atom(a.given(argument))))
        };
        let knowledge = Knowledge::of(&facts, &unfold);
        let summary = (summaries[pending.contract].as_ref())
            .map(|s| s.map(&mut |a: &Atom| Formula::Atom(a.given(&pending.subject))));
        let results = [Some(&check), summary.as_ref()].map(|f| f.and_then(|f| knowledge.holds(f)));
        if results.contains(&Some(true)) {
            return;
        }
        let ty = contract.ty.clone();
        let argument = &self.text[pending.argument.start as usize..pending.argument.end as usize];
        let runtime = format!("`{ty}.parse {argument}` checks it when the program runs");
        let wanted = summary.as_ref().unwrap_or(&check);
        if results.contains(&Some(false)) {
            let part = (knowledge.missing(wanted, true)).map_or_else(String::new, |f| {
                format!(": `{}` is false here", self.proof_shown(f))
            });
            let message = format!("the check of `{ty}` fails for this value{part}; {runtime}");
            self.error(codes::T077, pending.span, message);
            return;
        }
        let part = match knowledge.missing(wanted, true) {
            Some(f) => format!("nothing here establishes `{}`", self.proof_shown(f)),
            None => "its check goes beyond what the compiler reads".to_owned(),
        };
        let message =
            format!("cannot prove the check of `{ty}` holds for this value: {part}; {runtime}");
        self.error(codes::T076, pending.span, message);
    }

    /// The atoms of `fact` that stay atoms once the module is solved: a call
    /// that takes a dictionary depends on more than its argument.
    fn settle_fact(&self, fact: &Formula<Raw>) -> Formula<Atom> {
        fact.map(&mut |raw: &Raw| match &raw.atom {
            Atom::Call { .. }
                if self
                    .typing
                    .dicts
                    .get(&raw.at)
                    .is_some_and(|d| !d.is_empty()) =>
            {
                Formula::Opaque
            }
            // a comparison an `Ord` decides, or of a constant the type does not
            // hold, is none the logic reads
            Atom::Compare { value, op, bound } => {
                let c = bound.value();
                let bound = match self.proofs.compared.get(&raw.at) {
                    Some(Num::Int) if c.fract() == 0.0 => {
                        let within = (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&c);
                        #[allow(clippy::cast_possible_truncation, reason = "an integer within i32")]
                        within.then_some(Bound::Int(c as i32))
                    }
                    Some(Num::Float) if c.is_finite() => Some(*bound),
                    _ => None,
                };
                bound.map_or(Formula::Opaque, |bound| {
                    Formula::Atom(Atom::Compare {
                        value: value.clone(),
                        op: *op,
                        bound,
                    })
                })
            }
            atom => Formula::Atom(atom.clone()),
        })
    }

    /// What the check of the invariant at `index` is, of its parameter: the
    /// body of a `fun`, or a call of the function it names.
    pub(super) fn check_summary(&self, index: usize) -> Option<Formula<Atom>> {
        let contract = &self.proofs.contracts[index];
        if let Some(summary) = &contract.summary {
            return Some(summary.clone());
        }
        let body = contract.body?;
        let mut statements = unparen(body).nodes();
        let node = match (statements.next(), statements.next()) {
            (Some(only), None) => unparen(only),
            _ => return None,
        };
        let mut size = 64;
        let raw = match node.kind {
            K::FunExpr => {
                let mut params = node.nodes().filter(|n| is_pattern(n.kind));
                let (Some(param), None) = (params.next(), params.next()) else {
                    return None;
                };
                let name = word(param, self.text)?;
                let lambda = node.nodes().find(|n| n.kind == K::Block)?;
                let parameter = |n: &Node| {
                    let n = unparen(n);
                    let local = self.typing.names.get(&n.range()) == Some(&Ref::Local);
                    (local && word(n, self.text) == Some(name)).then_some(Value::Param)
                };
                if param.kind != K::VarPat {
                    return None;
                }
                self.proof_formula(lambda, &mut size, &parameter)
            }
            K::NameExpr => match self.typing.names.get(&node.range()) {
                Some(Ref::Value { module, name }) => Formula::Atom(Raw {
                    atom: Atom::Call {
                        module: module.clone(),
                        name: name.clone(),
                        argument: Value::Param,
                    },
                    at: node.range(),
                }),
                _ => return None,
            },
            _ => return None,
        };
        Some(self.settle_fact(&raw))
    }
}

/// The projections `value` is or holds, innermost first.
fn projections_in(value: &Value, out: &mut Vec<(usize, Rc<Value>)>) {
    if let Value::Projection(index, of) = value {
        projections_in(of, out);
        if !out.iter().any(|(i, v)| i == index && v == of) {
            out.push((*index, Rc::clone(of)));
        }
    }
}
