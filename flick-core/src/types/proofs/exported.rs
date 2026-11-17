//! Invariants across modules (`docs/formal/refinements.md` §6). A module
//! publishes the invariant of each type it exports with one, and passes on
//! those it learned, as instances travel; an importer validates each against
//! the exports it sees before `T.prove` reads it. One that does not match,
//! or comes from an interface that lost declarations, gives nothing.

use super::formula::{Atom, Literal, Value};
use super::interval::{Bound, Op};
use super::logic::Formula;
use super::{Checker, Contract};
use crate::diagnostics::Severity;
use crate::iface::{CheckOf, Entity, Interface, Invariant, Namespace};
use crate::source::Span;
use crate::types::env::Form;
use crate::types::lang::Lang;
use crate::types::ty::Type;

/// The bases the logic reads, by the name an interface gives them.
const BASES: [(&str, Lang); 4] = [
    ("Int", Lang::Int),
    ("Float", Lang::Float),
    ("String", Lang::String),
    ("Bool", Lang::Bool),
];

impl Checker<'_> {
    /// The invariants an importer of the module learns: of each exported
    /// type of the module whose contract holds, and those it learned. A
    /// module with errors publishes none of its own.
    pub(in crate::types) fn export_invariants(&self, interface: &mut Interface) {
        let mut out = self.env.invariants.clone();
        let failed = self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
        for (i, contract) in self.proofs.contracts.iter().enumerate() {
            let own = contract.module == self.env.module && contract.body.is_some();
            let exported = (interface.exports.iter())
                .any(|e| e.entity.namespace == Namespace::Type && e.entity.name == contract.ty);
            let base = self.contract_base(contract);
            if !own || !contract.active || failed || !exported {
                continue;
            }
            let (Some(base), Some(summary)) = (base, self.check_summary(i)) else {
                continue;
            };
            out.push(Invariant {
                module: contract.module.clone(),
                ty: contract.ty.clone(),
                constructor: contract.constructor.clone(),
                check: contract.check.clone(),
                value: contract.value.clone(),
                prove: contract.prove.clone(),
                base: base.to_owned(),
                summary: check_of(&summary),
            });
        }
        interface.invariants = out;
    }

    /// The name of the base of a contract of the module.
    fn contract_base(&self, contract: &Contract) -> Option<&'static str> {
        let entity = Entity {
            module: contract.module.clone(),
            namespace: Namespace::Constructor,
            name: contract.constructor.clone(),
        };
        let Some(Type::Fun { param, .. }) = self.env.value(&entity).map(|s| self.whnf(s)) else {
            return None;
        };
        let base = self.whnf(&param);
        BASES
            .iter()
            .find(|(_, l)| base.is_lang(*l))
            .map(|(n, _)| *n)
    }

    /// The invariants of the modules the module imports, each where the
    /// exports agree with it: an opaque type whose constructor is hidden,
    /// `check : Base -> Bool` and `value : T -> Base`.
    pub(super) fn imported_invariants(&mut self) {
        for invariant in self.env.invariants.clone() {
            if invariant.module == self.env.module {
                continue;
            }
            let Some((_, base)) = BASES.iter().find(|(n, _)| *n == invariant.base) else {
                continue;
            };
            let base = Type::lang(*base);
            let ty = Type::Con {
                module: invariant.module.clone(),
                name: invariant.ty.clone(),
            };
            let value_of = |name: &str| {
                let entity = Entity {
                    module: invariant.module.clone(),
                    namespace: Namespace::Value,
                    name: name.to_owned(),
                };
                self.env.value(&entity).map(|s| self.whnf(s))
            };
            let is_fun = |t: Option<Type>, from: &Type, to: &Type| {
                matches!(t, Some(Type::Fun { param, effects: None, result })
                    if self.whnf(&param) == *from && self.whnf(&result) == *to)
            };
            let opaque = matches!(
                self.env.type_def(&invariant.module, &invariant.ty).map(|d| &d.form),
                Some(Form::Nominal { literals, .. }) if literals.is_empty()
            );
            let holds = opaque
                && invariant.constructor.contains('$')
                && is_fun(value_of(&invariant.check), &base, &Type::lang(Lang::Bool))
                && is_fun(value_of(&invariant.value), &ty, &base);
            if !holds {
                continue;
            }
            self.proofs.contracts.push(Contract {
                module: invariant.module.clone(),
                ty: invariant.ty.clone(),
                constructor: invariant.constructor.clone(),
                check: invariant.check.clone(),
                value: invariant.value.clone(),
                prove: invariant.prove.clone(),
                active: true,
                at: Span::new(self.file, 0, 0),
                body: None,
                summary: Some(formula_of(&invariant.summary)),
                base: Some(base),
            });
        }
    }
}

/// The check as an interface writes it: of the parameter alone.
fn check_of(formula: &Formula<Atom>) -> CheckOf {
    let of = |x: &Formula<Atom>| Box::new(check_of(x));
    match formula {
        Formula::Const(b) => CheckOf::Const(*b),
        Formula::Not(x) => CheckOf::Not(of(x)),
        Formula::And(x, y) => CheckOf::And(of(x), of(y)),
        Formula::Or(x, y) => CheckOf::Or(of(x), of(y)),
        Formula::Atom(Atom::Call {
            module,
            name,
            argument: Value::Param,
        }) => CheckOf::Call {
            module: module.clone(),
            name: name.clone(),
        },
        Formula::Atom(Atom::Truth(Value::Param)) => CheckOf::Truth,
        Formula::Atom(Atom::Compare {
            value: Value::Param,
            op,
            bound,
        }) => {
            let (int, float) = match bound {
                Bound::Int(i) => (Some(*i), None),
                Bound::Float(bits) => (None, Some(*bits)),
            };
            CheckOf::Compare {
                op: op.text().to_owned(),
                int,
                float,
            }
        }
        Formula::Atom(Atom::Equals {
            value: Value::Param,
            literal,
        }) => match literal {
            Literal::Number(n) => CheckOf::Equals {
                number: Some(*n),
                text: None,
                boolean: None,
            },
            Literal::Text(s) => CheckOf::Equals {
                number: None,
                text: Some(s.clone()),
                boolean: None,
            },
            Literal::Bool(b) => CheckOf::Equals {
                number: None,
                text: None,
                boolean: Some(*b),
            },
        },
        Formula::Atom(_) | Formula::Opaque => CheckOf::Opaque,
    }
}

/// The check an interface writes, as a formula of the parameter; a part
/// that does not read as one is opaque.
fn formula_of(check: &CheckOf) -> Formula<Atom> {
    let of = |x: &CheckOf| Box::new(formula_of(x));
    let atom = |atom| Formula::Atom(atom);
    match check {
        CheckOf::Const(b) => Formula::Const(*b),
        CheckOf::Not(x) => Formula::Not(of(x)),
        CheckOf::And(x, y) => Formula::And(of(x), of(y)),
        CheckOf::Or(x, y) => Formula::Or(of(x), of(y)),
        CheckOf::Call { module, name } => atom(Atom::Call {
            module: module.clone(),
            name: name.clone(),
            argument: Value::Param,
        }),
        CheckOf::Truth => atom(Atom::Truth(Value::Param)),
        CheckOf::Compare { op, int, float } => {
            let op = [Op::Lt, Op::Le, Op::Gt, Op::Ge]
                .into_iter()
                .find(|o| o.text() == op);
            let bound = match (int, float) {
                (Some(i), None) => Some(Bound::Int(*i)),
                (None, Some(bits)) if f64::from_bits(*bits).is_finite() => {
                    Some(Bound::Float(*bits))
                }
                _ => None,
            };
            match (op, bound) {
                (Some(op), Some(bound)) => atom(Atom::Compare {
                    value: Value::Param,
                    op,
                    bound,
                }),
                _ => Formula::Opaque,
            }
        }
        CheckOf::Equals {
            number,
            text,
            boolean,
        } => {
            let literal = match (number, text, boolean) {
                (Some(n), None, None) => Some(Literal::Number(*n)),
                (None, Some(s), None) => Some(Literal::Text(s.clone())),
                (None, None, Some(b)) => Some(Literal::Bool(*b)),
                _ => None,
            };
            literal.map_or(Formula::Opaque, |literal| {
                atom(Atom::Equals {
                    value: Value::Param,
                    literal,
                })
            })
        }
        CheckOf::Opaque => Formula::Opaque,
    }
}

#[cfg(test)]
mod tests {
    use super::{check_of, formula_of};
    use crate::iface::CheckOf;

    #[test]
    fn a_check_reads_back_as_it_was_written() {
        let port = CheckOf::And(
            Box::new(CheckOf::Compare {
                op: ">".into(),
                int: None,
                float: Some(0f64.to_bits()),
            }),
            Box::new(CheckOf::Or(
                Box::new(CheckOf::Call {
                    module: "Checks".into(),
                    name: "email".into(),
                }),
                Box::new(CheckOf::Not(Box::new(CheckOf::Truth))),
            )),
        );
        assert_eq!(check_of(&formula_of(&port)), port);
        // what does not read as a check is opaque, and so proves nothing
        let broken = CheckOf::Compare {
            op: "=>".into(),
            int: Some(1),
            float: None,
        };
        assert_eq!(check_of(&formula_of(&broken)), CheckOf::Opaque);
        let infinite = CheckOf::Compare {
            op: "<".into(),
            int: None,
            float: Some(f64::INFINITY.to_bits()),
        };
        assert_eq!(check_of(&formula_of(&infinite)), CheckOf::Opaque);
    }
}
