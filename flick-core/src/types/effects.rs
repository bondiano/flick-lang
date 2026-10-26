//! Effects: every expression is checked at a *stage*, the part of
//! the program that runs when one runtime arrow (or one `Eff`) is applied.
//! A runtime call adds its service and error rows to the stage it runs at;
//! a pure stage takes none, not even `->{}` (an empty row is not purity).
//! The rows a stage may need are an upper bound: a constraint `actual ⊆
//! declared` per call, solved per declaration (see [`solve`]).
//!
//! A pure function is accepted where a runtime one is expected: the IR lifts
//! it. The opposite never holds.

mod solve;

use super::Checker;
use super::ty::{Effects, Type};
use crate::diagnostics::codes;
use crate::source::Span;

pub use solve::SubRow;

/// Why nothing runs at runtime under `effect = "none"`.
pub const NO_EFFECTS: &str = "the project builds with `effect = \"none\"`";

/// The stage being checked.
#[derive(Debug, Clone)]
pub struct Stage {
    /// Why runtime calls are not allowed here; `None` where they are.
    pub pure: Option<String>,
    /// Upper bounds of the rows its runtime calls need.
    pub services: Type,
    pub errors: Type,
    /// Index into [`Checker::sites`]: what the bounds come from.
    pub site: usize,
    /// A runtime call ran at this stage.
    pub runtime: bool,
    /// The stage is Sync: its calls run at once (see [`super::ty::Stage`]).
    pub sync: bool,
    /// Runtime calls so far, to tell whether an argument ran one.
    pub calls: usize,
    /// How many `scope`s were open when the stage began: those opened
    /// after it provide `Scope` to its calls.
    pub scopes: usize,
}

/// Where the upper bound of a stage comes from, for diagnostics.
#[derive(Debug, Clone)]
pub enum Site {
    /// The signature of the named function.
    Signature(String),
    /// The type a value or a `task` is checked against.
    Expected(Type),
    /// The rows inferred for a function without an expected type.
    Inferred,
}

impl Stage {
    pub fn pure(reason: impl Into<String>) -> Stage {
        Stage {
            pure: Some(reason.into()),
            services: Type::empty_row(),
            errors: Type::empty_row(),
            site: 0,
            runtime: false,
            sync: false,
            calls: 0,
            scopes: 0,
        }
    }
}

impl Default for Stage {
    fn default() -> Stage {
        Stage::pure("module initialization is pure: runtime calls start at the entry point")
    }
}

impl Checker<'_> {
    /// A stage that may need at most `services` and `errors`; a pure one
    /// when the project builds without effects.
    pub fn bounded_stage(&mut self, services: Type, errors: Type, site: Site) -> Stage {
        if !self.options.effects {
            return Stage::pure(NO_EFFECTS);
        }
        self.sites.push(site);
        Stage {
            pure: None,
            services,
            errors,
            site: self.sites.len() - 1,
            runtime: false,
            sync: false,
            calls: 0,
            scopes: self.regions.open.len(),
        }
    }

    /// A stage whose rows are inferred: they grow with its calls.
    pub fn open_stage(&mut self) -> Stage {
        let (services, errors) = (self.fresh(), self.fresh());
        self.bounded_stage(services, errors, Site::Inferred)
    }

    /// The rows an inferred stage ended up with, if it ran a runtime call.
    pub fn stage_effects(stage: &Stage) -> Option<Box<Effects>> {
        stage.runtime.then(|| {
            Box::new(Effects {
                services: stage.services.clone(),
                errors: stage.errors.clone(),
                sync: false,
            })
        })
    }

    /// Records a runtime call needing `effects` at the current stage.
    pub fn perform(&mut self, services: &Type, errors: &Type, span: Span) {
        let services = &self.lexical_owner(services, span);
        self.regions.raised.push((errors.clone(), span));
        self.runtime = true;
        self.stage.runtime = true;
        self.stage.calls += 1;
        if let Some(reason) = &self.stage.pure {
            let message = format!("this is a runtime call, but {reason}");
            if !self.diagnostics.iter().any(|d| d.span == Some(span)) {
                self.error(codes::E001, span, message);
            }
            return;
        }
        let (bound_services, bound_errors) =
            (self.stage.services.clone(), self.stage.errors.clone());
        let site = self.stage.site;
        self.sub_row(services, &bound_services, site, span, false);
        self.sub_row(errors, &bound_errors, site, span, true);
    }

    /// Records the application or pipe at `range` as a call of `effects`.
    pub fn record_call(&mut self, effects: &Effects, range: super::typing::Range) {
        if effects.sync {
            self.typing.sync.insert(range);
        } else {
            self.typing.runtime.insert(range);
        }
    }

    /// Requires the row `actual` to be part of `declared`.
    pub fn sub_row(
        &mut self,
        actual: &Type,
        declared: &Type,
        site: usize,
        span: Span,
        errors: bool,
    ) {
        let norm = self.store.normalize(actual);
        if norm.entries.is_empty() && norm.tails.is_empty() {
            return;
        }
        self.subrows.push(SubRow {
            actual: actual.clone(),
            declared: declared.clone(),
            site,
            span,
            errors,
            provides: None,
        });
    }

    /// The services `needed` by the body of a `scope` of `region` must be
    /// `outer`, the services around it, besides `Scope region`.
    pub fn scope_row(&mut self, needed: &Type, outer: &Type, region: &Type, span: Span) {
        let site = self.stage.site;
        self.subrows.push(SubRow {
            actual: needed.clone(),
            declared: outer.clone(),
            site,
            span,
            errors: false,
            provides: Some(region.clone()),
        });
    }

    /// Like unification, but a value may need fewer services and errors than
    /// its position allows: runtime arrows and `Eff` are covariant in their
    /// rows and results, and a pure function passes for a runtime one.
    pub fn subsume(&mut self, actual: &Type, expected: &Type, span: Span) -> super::unify::Unified {
        let (a, e) = (self.whnf_captured(actual), self.whnf_captured(expected));
        if a.1.is_some() || e.1.is_some() {
            return self.subsume_captured((actual, a), (expected, e), span);
        }
        let (a, e) = (a.0, e.0);
        match (&a, &e) {
            (
                Type::Fun {
                    param: p1,
                    effects: e1,
                    result: r1,
                },
                Type::Fun {
                    param: p2,
                    effects: e2,
                    result: r2,
                },
            ) => {
                match (e1, e2) {
                    (Some(_), None) => {
                        return Err(super::unify::Mismatch::Types(a.clone(), e.clone()));
                    }
                    (Some(x), Some(y)) => {
                        let site = self.expected_site(&e);
                        self.sub_row(&x.services, &y.services, site, span, false);
                        self.sub_row(&x.errors, &y.errors, site, span, true);
                    }
                    (None, _) => {}
                }
                self.unify(p1, p2)?;
                self.subsume(r1, r2, span)
            }
            _ => match (self.eff_parts(&a), self.eff_parts(&e)) {
                (Some((s1, e1, r1)), Some((s2, e2, r2))) => {
                    let site = self.expected_site(&e);
                    self.sub_row(&s1, &s2, site, span, false);
                    self.sub_row(&e1, &e2, site, span, true);
                    self.subsume(&r1, &r2, span)
                }
                _ => self.unify(&a, &e),
            },
        }
    }

    fn expected_site(&mut self, expected: &Type) -> usize {
        self.sites.push(Site::Expected(expected.clone()));
        self.sites.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use crate::source::SourceMap;

    /// The type a function without a signature is suggested to have.
    fn suggestion(definition: &str) -> String {
        let text = format!(
            "module Probe exposing (..)\n\
             type error NotFound = NotFound Number\n\
             service Users =\n  get : Number ->{{; NotFound}} String\n\
             {definition}\n"
        );
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text);
        let diagnostics = crate::check(&sources, file);
        let fixes: Vec<&str> = diagnostics
            .iter()
            .flat_map(|d| &d.fixes)
            .map(|f| f.message.as_str())
            .collect();
        match fixes[..] {
            [fix] => fix.to_owned(),
            _ => panic!("expected one suggestion, got {diagnostics:?}"),
        }
    }

    #[test]
    fn lambdas_infer_the_arrow_of_each_stage() {
        assert_eq!(
            suggestion("let f = fun x -> String.trim x"),
            "add `f : String -> String`"
        );
        assert_eq!(
            suggestion("let f = fun x -> task { Users.get x }"),
            "add `f : Number -> Eff {Users} {NotFound} String`"
        );
        assert_eq!(
            suggestion("let f = fun x -> Users.get x"),
            "add `f : Number ->{Users; NotFound} String`"
        );
        assert_eq!(
            suggestion("let f = fun x -> fun y -> Users.get (x + y)"),
            "add `f : Number -> Number ->{Users; NotFound} String`"
        );
    }
}
