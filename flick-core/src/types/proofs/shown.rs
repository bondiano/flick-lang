//! Formulas as the program would write them, for diagnostics.

use super::Checker;
use super::formula::{Atom, Literal, Value};
use super::logic::Formula;

impl Checker<'_> {
    pub(super) fn proof_shown(&self, formula: &Formula<Atom>) -> String {
        match formula {
            Formula::Const(b) => b.to_string(),
            Formula::Atom(Atom::Truth(v)) => self.proof_value_shown(v),
            Formula::Atom(Atom::Call {
                module,
                name,
                argument,
            }) => {
                let name = name.replace('$', ".");
                let callee = match self.env.qualifier_of(module) {
                    Some(q) if module != &self.env.module => format!("{q}.{name}"),
                    _ => name,
                };
                format!("{callee} {}", self.proof_argument_shown(argument))
            }
            Formula::Atom(Atom::Compare { value, op, bound }) => {
                let bound = Literal::Number(bound.value().to_bits());
                let shown = |v| self.proof_value_shown(v);
                format!(
                    "{} {} {}",
                    shown(value),
                    op.text(),
                    shown(&Value::Lit(bound))
                )
            }
            Formula::Atom(Atom::Equals { value, literal }) => {
                let literal = self.proof_value_shown(&Value::Lit(literal.clone()));
                format!("{} == {literal}", self.proof_value_shown(value))
            }
            Formula::Not(x) => format!("not ({})", self.proof_shown(x)),
            Formula::And(x, y) => format!("{} && {}", self.proof_shown(x), self.proof_shown(y)),
            Formula::Or(x, y) => format!("{} || {}", self.proof_shown(x), self.proof_shown(y)),
            Formula::Opaque => "…".to_owned(),
        }
    }

    fn proof_argument_shown(&self, value: &Value) -> String {
        match value {
            Value::Projection(..) => format!("({})", self.proof_value_shown(value)),
            _ => self.proof_value_shown(value),
        }
    }

    fn proof_value_shown(&self, value: &Value) -> String {
        match value {
            Value::Local(at) => {
                let rest = &self.text[*at as usize..];
                let end = rest
                    .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\''))
                    .unwrap_or(rest.len());
                rest[..end].to_owned()
            }
            Value::Projection(index, of) => {
                let ty = &self.proofs.contracts[*index].ty;
                format!("{ty}.value {}", self.proof_argument_shown(of))
            }
            Value::Once((start, end)) => self.text[*start as usize..*end as usize].to_owned(),
            Value::Param => "_".to_owned(),
            Value::Lit(Literal::Number(bits)) => f64::from_bits(*bits).to_string(),
            Value::Lit(Literal::Text(s)) => format!("{s:?}"),
            Value::Lit(Literal::Bool(b)) => b.to_string(),
        }
    }
}
