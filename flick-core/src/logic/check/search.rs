//! Capture the innermost obligation visited by the real proof traversal.

use std::rc::Rc;

use super::{Checker, Equation, Goal, Site, Span, Ty, Value};
use crate::logic::ProofSearchRequest;

pub(super) struct Capture<'a> {
    request: &'a ProofSearchRequest,
    pub selected: Option<Selection>,
}

impl<'a> Capture<'a> {
    pub fn new(request: &'a ProofSearchRequest) -> Self {
        Self {
            request,
            selected: None,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.request.is_cancelled()
    }

    pub fn record(
        &mut self,
        checker: &Checker<'_>,
        span: Span,
        goal: Result<&Equation, (&Ty, &Ty)>,
        site: Site,
    ) {
        let width = span.end - span.start;
        if matches!(site, Site::Scope)
            || self.is_cancelled()
            || self.request.offset < span.start
            || span.end < self.request.offset
            || self
                .selected
                .as_ref()
                .is_some_and(|s| s.span.end - s.span.start < width)
        {
            return;
        }
        let goal = match goal {
            Ok(eq) => Goal::Equal(eq.clone()),
            Err((a, b)) => Goal::SameType(a.clone(), b.clone()),
        };
        self.selected = Some(Selection {
            span,
            goal,
            synthesizing: matches!(site, Site::Synthesized),
            vars: checker.vars.clone(),
            facts: checker.facts.clone(),
            refined: checker.refined.clone(),
        });
    }
}

pub(super) struct Selection {
    pub span: Span,
    pub goal: Goal,
    pub synthesizing: bool,
    pub vars: Vec<(Rc<str>, Ty)>,
    pub facts: Vec<(Rc<str>, Equation)>,
    pub refined: Vec<(Rc<str>, Value)>,
}
