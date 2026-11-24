//! Source-visible proof states. Rendering folds definitions and uses its own
//! budget, leaving the verifier's normalization budget untouched.

use std::rc::Rc;

use super::defs::Admitted;
use super::eval::{Env, Machine, Stop, Value};
use super::show::{show, source_name};
use super::{Equation, Goal, Ty};
use crate::source::Span;

/// The context passed to a step by the certificate checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStep {
    pub span: Span,
    pub goal: String,
    pub binders: Vec<(String, String)>,
    pub premises: Vec<(String, String)>,
    pub substitutions: Vec<(String, String)>,
    /// Normalization work for this step and its children in a real check.
    pub normalization_steps: u64,
}

/// Printing work allowed across all steps in a declaration. It bounds
/// reduction of source terms such as deeply nested beta-redexes.
const RENDER_STEPS: u64 = 20_000;

pub fn show_equation(eq: &Equation) -> String {
    Terms::new(&[], RENDER_STEPS).equation(eq)
}

pub fn show_goal(goal: &Goal) -> String {
    match goal {
        Goal::Equal(eq) => show_equation(eq),
        Goal::SameType(a, b) => same_type(a, b),
    }
}

pub(super) fn same_type(a: &Ty, b: &Ty) -> String {
    format!("same_type ({a}) ({b})")
}

pub(super) struct Trace<'a> {
    pub steps: &'a mut Vec<ProofStep>,
    remaining: u64,
}

impl<'a> Trace<'a> {
    pub fn new(steps: &'a mut Vec<ProofStep>) -> Self {
        Self {
            steps,
            remaining: RENDER_STEPS,
        }
    }

    pub fn record(
        &mut self,
        span: Span,
        goal: Result<&Equation, (&Ty, &Ty)>,
        vars: &[(Rc<str>, Ty)],
        facts: &[(Rc<str>, Equation)],
        refined: &[(Rc<str>, Value)],
    ) -> usize {
        let mut terms = Terms::new(refined, self.remaining);
        let goal = match goal {
            Ok(eq) => terms.equation(eq),
            Err((a, b)) => same_type(a, b),
        };
        let binders = vars
            .iter()
            .filter(|(name, _)| !name.starts_with('$'))
            .map(|(name, ty)| (source_name(name).to_owned(), ty.to_string()))
            .collect();
        let premises = facts
            .iter()
            .filter(|(name, _)| !name.starts_with('$'))
            .map(|(name, eq)| (source_name(name).to_owned(), terms.equation(eq)))
            .collect();
        let substitutions = refined
            .iter()
            .map(|(name, value)| (source_name(name).to_owned(), terms.value(value.clone())))
            .collect();
        self.remaining = terms.remaining;
        let i = self.steps.len();
        self.steps.push(ProofStep {
            span,
            goal,
            binders,
            premises,
            substitutions,
            normalization_steps: 0,
        });
        i
    }
}

struct Terms<'a> {
    refined: &'a [(Rc<str>, Value)],
    remaining: u64,
}

impl<'a> Terms<'a> {
    fn new(refined: &'a [(Rc<str>, Value)], remaining: u64) -> Self {
        Self { refined, remaining }
    }

    fn equation(&mut self, eq: &Equation) -> String {
        let left = self.read(|machine| machine.eval(&eq.left, &Env::default()));
        let right = self.read(|machine| machine.eval(&eq.right, &Env::default()));
        format!("{left} = {right}")
    }

    fn value(&mut self, value: Value) -> String {
        self.read(|_| Ok(value))
    }

    fn read(&mut self, eval: impl FnOnce(&mut Machine<'_>) -> Result<Value, Stop>) -> String {
        let defs = Admitted::default();
        let mut machine = Machine::new(self.refined, &defs, self.remaining);
        machine.unfold = false;
        let found = eval(&mut machine).and_then(|value| machine.read(value, 0));
        self.remaining = machine.steps;
        found.map_or_else(|_| "…".into(), |nf| show(&nf))
    }
}
