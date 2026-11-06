//! Diagnostics for unresolved class constraints and resolution budgets.

use super::solve::Outcome;
use super::{Wanted, args_of, split};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::ty::Type;

impl Checker<'_> {
    pub(super) fn report_classes(&mut self, left: Vec<(Wanted, Outcome)>) {
        let mut budget = false;
        for (w, outcome) in left {
            if self.explained(w.span) {
                continue;
            }
            let constraint = self.canon(&w.constraint);
            match outcome {
                Outcome::Missing(part, reason) => {
                    self.missing(&constraint, &part, reason.as_deref(), w.span);
                }
                // a route that is no path has its error where it is written;
                // services still unknown at the end are none, so no `control` one
                Outcome::Stuck
                    if args_of(&constraint, Lang::PathParams).is_some()
                        || args_of(&constraint, Lang::Detached).is_some() => {}
                Outcome::Stuck => {
                    let shown = self.show(&constraint);
                    let candidates = self.candidates(&constraint);
                    self.diagnostics.push(
                        Diagnostic::error(
                            codes::C002,
                            format!("cannot choose an instance for `{shown}`: a type in it is not known here; {candidates}"),
                            w.span,
                        ).with_fix(|| ("annotate the type of the value", Vec::new())),
                    );
                }
                Outcome::Budget(chain) if !budget => {
                    budget = true;
                    self.over_budget(&constraint, &chain, w.span);
                }
                _ => {}
            }
        }
    }

    /// An error inside or around `span` already explains a failure there.
    fn explained(&self, span: Span) -> bool {
        self.diagnostics.iter().any(|d| {
            d.severity == Severity::Error
                && d.span.is_some_and(|s| {
                    s.file == span.file && s.start <= span.end && span.start <= s.end
                })
        })
    }

    fn missing(&mut self, constraint: &Type, part: &Type, reason: Option<&str>, span: Span) {
        let (whole, shown) = (self.show(constraint), self.show(part));
        let rigid = part.contains(&mut |t| matches!(t, Type::Skolem(_) | Type::Var(_)));
        let mut message = match (&reason, rigid) {
            (Some(reason), _) => format!("`{shown}` does not hold: {reason}"),
            (None, true) => format!("cannot deduce `{shown}` from the constraints in scope"),
            (None, false) => format!("no instance for `{shown}`"),
        };
        if whole != shown {
            message = format!("{message}, needed for `{whole}`");
        }
        let mut diagnostic = if reason.is_none() && !rigid {
            let candidates = self.candidates(part);
            Diagnostic::error(codes::C001, format!("{message}; {candidates}"), span)
        } else if args_of(constraint, Lang::Detached).is_some() {
            Diagnostic::error(codes::E017, message, span)
        } else {
            Diagnostic::error(codes::C001, message, span)
        };
        if rigid && reason.is_none() {
            diagnostic =
                diagnostic.with_fix(|| (format!("add `{shown} =>` to the signature"), Vec::new()));
        }
        self.diagnostics.push(diagnostic);
    }

    /// The instances of the class of `c`, for a diagnostic.
    fn candidates(&self, c: &Type) -> String {
        let Some((class, _)) = split(c) else {
            return String::new();
        };
        let names: Vec<String> = self
            .env
            .instances()
            .filter(|i| split(&i.head).is_some_and(|(k, _)| k == class))
            .map(|i| format!("`{}`", i.name))
            .collect();
        match names.len() {
            0 => format!("`{}` has no instances", class.1),
            n if n <= 6 => format!("instances of `{}`: {}", class.1, names.join(", ")),
            n => format!(
                "instances of `{}`: {}, and {} more",
                class.1,
                names[..5].join(", "),
                n - 5
            ),
        }
    }

    fn over_budget(&mut self, constraint: &Type, chain: &[Type], span: Span) {
        let limits = self.options.limits;
        let what = if chain.len() > limits.depth as usize {
            format!("{} nested constraints", limits.depth)
        } else {
            format!("{} steps", limits.steps)
        };
        let short = |t: &Type| {
            let s = self.show(t);
            match s.char_indices().nth(60) {
                Some((i, _)) => format!("`{}…`", &s[..i]),
                None => format!("`{s}`"),
            }
        };
        let mut links: Vec<String> = chain.iter().take(3).map(short).collect();
        if chain.len() > 5 {
            links.push("…".to_owned());
        }
        links.extend(
            chain
                .iter()
                .skip(3.max(chain.len().saturating_sub(2)))
                .map(short),
        );
        let shown = short(constraint);
        self.diagnostics.push(
            Diagnostic::error(
                codes::C009,
                format!(
                    "resolving {shown} exceeds the budget of {what}: {}",
                    links.join(" -> ")
                ),
                span,
            ).with_fix(|| ("simplify the types involved, or raise `solver-steps` or `solver-depth` in `[build]` of `flick.toml`", Vec::new())),
        );
    }
}
