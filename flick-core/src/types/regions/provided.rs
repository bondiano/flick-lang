//! `Fiber.Provided r s` (`regions.md` §2.7, Р37): a child or finalizer of
//! the owner of region `s` runs with the services of the place that
//! registered it, so each service of `r` comes from a provider that
//! outlives `s`. A service a `provide` builds ends with that `provide`.
//!
//! The constraint splits into one obligation per service, settled where the
//! call is, as `Scope s` is: by the `provide` that builds the service, by a
//! `scope` of the stage, or by a given `Provided`; otherwise it stays in the
//! stage's row as `Provided {S} s` and goes to whatever runs the stage.

use std::mem;

use super::{SCOPE, Scope};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::classes::split;
use crate::types::ty::{Norm, Row, Type};

/// Labels of the obligations in a services row: the prefix, then the
/// service's label.
pub const PROVIDED: &str = "Fiber.Provided ";

/// A `provide` around the code being checked.
#[derive(Debug, Clone)]
pub struct Provide {
    /// The labels of the services it builds.
    pub labels: Vec<String>,
    /// Whether it also builds a row variable's services, unknown here.
    pub open: bool,
    /// Its entry in `State::scopes`.
    pub scope: usize,
}

/// What settles an obligation at the place it was wanted.
#[derive(Debug, Clone)]
pub struct Lexical {
    /// The `provide`s of the stage around the place, innermost last.
    provides: Vec<Provide>,
    /// The `scope`s of the stage around the place.
    scopes: Vec<usize>,
    givens: Vec<Type>,
    services: Type,
    site: usize,
    pure: bool,
}

/// A `Provided row owner` constraint, with the services settled so far.
#[derive(Debug, Clone)]
pub struct Want {
    row: Type,
    owner: Type,
    span: Span,
    at: Lexical,
    settled: Vec<String>,
}

impl Checker<'_> {
    /// The body of a `provide` at `span` building the services `labels`:
    /// it nests like a `scope`, so owners compare with it (`encloses`).
    pub fn enter_provide(&mut self, node: &Node, span: Span, labels: Vec<String>, open: bool) {
        let Type::Skolem(region) = self.store.skolem("provide") else {
            unreachable!("a skolem")
        };
        self.regions.scopes.push(Scope {
            region,
            span,
            node: node.range(),
            outer: self.stage.services.clone(),
            parent: self.regions.nest.last().copied(),
        });
        let scope = self.regions.scopes.len() - 1;
        self.regions.nest.push(scope);
        self.regions.provides.push(Provide {
            labels,
            open,
            scope,
        });
    }

    pub fn leave_provide(&mut self) {
        self.regions.provides.pop();
        self.regions.nest.pop();
    }

    fn lexical(&self) -> Lexical {
        Lexical {
            provides: self.regions.provides[self.stage.provides..].to_vec(),
            scopes: self.regions.open[self.stage.scopes..].to_vec(),
            givens: self.classes.givens.clone(),
            services: self.stage.services.clone(),
            site: self.stage.site,
            pure: self.stage.pure.is_some(),
        }
    }

    /// `Provided row owner` wanted at `span`: settled at the end of the
    /// call, once its arguments say what the task needs.
    pub fn want_provided(&mut self, row: Type, owner: Type, span: Span) {
        let at = self.lexical();
        self.regions.provided.push(Want {
            row,
            owner,
            span,
            at,
            settled: Vec::new(),
        });
    }

    /// Settles the `Provided` constraints wanted since `mark`. A row not
    /// inferred yet, as of a function applied to fewer arguments, waits for
    /// the end of the declaration.
    pub fn settle_provided(&mut self, mark: usize) {
        let wanted: Vec<Want> = self.regions.provided.drain(mark..).collect();
        for mut want in wanted {
            if self.settle_want(&mut want, false) {
                self.regions.provided_late.push(want);
            }
        }
    }

    /// At the end of the declaration, when rows no longer grow: what is
    /// left must already be in the row of its stage.
    pub fn settle_late_provided(&mut self) {
        let mut wanted = mem::take(&mut self.regions.provided);
        wanted.append(&mut self.regions.provided_late);
        for mut want in wanted {
            self.settle_want(&mut want, true);
        }
        for (provide, owner, service, span) in mem::take(&mut self.regions.late_owners) {
            let owner = self.store.resolve(&owner);
            match self.scope_index(&owner) {
                Some(scope) if self.encloses(provide, scope) => {}
                Some(_) => self.outlived_provide(&service, provide, &owner, span),
                // never solved: an error was reported already
                None => {}
            }
        }
    }

    /// Settles the services of `want` not settled yet; `true` when its row
    /// may still grow.
    fn settle_want(&mut self, want: &mut Want, last: bool) -> bool {
        // an owner a row names is known once rows are solved
        if !last && matches!(self.store.resolve(&want.owner), Type::Meta(_)) {
            return true;
        }
        let norm = self.store.normalize(&want.row);
        let mut keep = Vec::new();
        for (label, ty) in norm.entries {
            if label == SCOPE || label.starts_with(PROVIDED) || want.settled.contains(&label) {
                continue;
            }
            want.settled.push(label.clone());
            if !self.discharge(&label, &want.owner, &want.at, want.span) {
                keep.push(obligation(&label, ty, &want.owner));
            }
        }
        let mut open = false;
        for tail in &norm.tails {
            match tail {
                Type::Meta(_) => open = true,
                Type::Var(_) | Type::Skolem(_)
                    if !self.discharge_tail(tail, &want.owner, &want.at) =>
                {
                    let (service, region) = (self.show(tail), self.show(&want.owner));
                    self.unprovided(&service, &region, want.span);
                }
                _ => {}
            }
        }
        if last {
            self.check_passed(keep, &want.at, want.span);
        } else {
            self.pass_on(keep, &want.at, want.span);
        }
        open && !last
    }

    /// The obligations of a row performed in the current stage that it
    /// settles are left out of the row.
    pub fn settle_performed(&mut self, services: &Type, span: Span) -> Type {
        let mut norm = self.store.normalize(services);
        if !norm.entries.iter().any(|(l, _)| l.starts_with(PROVIDED)) {
            return services.clone();
        }
        let at = self.lexical();
        let entries = mem::take(&mut norm.entries);
        for (label, ty) in entries {
            let owner = match ty.spine().1[..] {
                [_, owner] => Some(owner.clone()),
                _ => None,
            };
            let settled = match (label.strip_prefix(PROVIDED), owner) {
                (Some(service), Some(owner)) => self.discharge(service, &owner, &at, span),
                _ => false,
            };
            if !settled {
                norm.entries.push((label, ty));
            }
        }
        norm.into_type()
    }

    /// Whether the place `at` settles that `service` outlives `owner`.
    fn discharge(&mut self, service: &str, owner: &Type, at: &Lexical, span: Span) -> bool {
        let provider = at
            .provides
            .iter()
            .rev()
            .find(|p| p.open || p.labels.iter().any(|l| l == service))
            .map(|p| p.scope);
        let owner = self.store.resolve(owner);
        if let Some(provide) = provider {
            match (self.scope_index(&owner), &owner) {
                (Some(scope), _) if self.encloses(provide, scope) => {}
                (_, Type::Meta(_)) => {
                    let late = (provide, owner.clone(), service.to_owned(), span);
                    self.regions.late_owners.push(late);
                }
                _ => self.outlived_provide(service, provide, &owner, span),
            }
            return true;
        }
        self.scope_index(&owner)
            .is_some_and(|i| at.scopes.contains(&i))
            || self.given(at, &owner, |row| row.get(service).is_some())
    }

    /// A row variable's services come from what runs the stage: settled
    /// when the owner's scope is the stage's, or by a given.
    fn discharge_tail(&self, tail: &Type, owner: &Type, at: &Lexical) -> bool {
        let owner = self.store.resolve(owner);
        self.scope_index(&owner)
            .is_some_and(|i| at.scopes.contains(&i))
            || self.given(at, &owner, |row| row.tails.contains(tail))
    }

    /// Whether a given `Provided R owner` has a row `R` that `names`.
    fn given(&self, at: &Lexical, owner: &Type, names: impl Fn(&Norm) -> bool) -> bool {
        at.givens.iter().any(|g| match split(g) {
            Some((("Fiber", "Provided"), args)) => match args[..] {
                [row, region] => {
                    self.store.resolve(region) == *owner && names(&self.store.normalize(row))
                }
                _ => false,
            },
            _ => false,
        })
    }

    /// Obligations the stage cannot settle go to its row.
    fn pass_on(&mut self, entries: Vec<(String, Type)>, at: &Lexical, span: Span) {
        if entries.is_empty() || at.pure {
            return;
        }
        let row = Type::Row(Row {
            entries,
            tail: None,
        });
        self.sub_row(&row, &at.services, at.site, span, false);
    }

    /// Once rows are solved, an obligation not settled must be in its row.
    fn check_passed(&mut self, entries: Vec<(String, Type)>, at: &Lexical, span: Span) {
        if at.pure {
            return;
        }
        let row = self.store.normalize(&at.services);
        for (label, ty) in entries {
            if row.get(&label).is_none() {
                let (service, region) = match ty.spine().1[..] {
                    [service, owner] => (self.show(service), self.show(owner)),
                    _ => continue,
                };
                self.unprovided(service.trim_matches(['{', '}', ' ']), &region, span);
            }
        }
    }

    fn outlived_provide(&mut self, service: &str, provide: usize, owner: &Type, span: Span) {
        let Scope { span: at, .. } = self.regions.scopes[provide];
        let (region, _) = self.describe(owner);
        let name = service.rsplit('.').next().unwrap_or(service);
        self.diagnostics.push(
            Diagnostic::error(
                codes::G003,
                format!(
                    "this keeps work that uses `{name}` until {region} closes, but the `provide` that builds `{name}` releases it earlier"
                ),
                span,
            )
            .with_related(at, "the service is released when this `provide` ends")
            .with_fix(|| {
                (
                    "give the work an owner inside the `provide`, or provide the service outside the owner's `scope`",
                    Vec::new(),
                )
            }),
        );
    }

    /// An obligation that reached a signature, or a row variable none
    /// settles.
    pub fn unprovided(&mut self, service: &str, region: &str, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                codes::G003,
                format!(
                    "this keeps work that uses `{service}` until `{region}` closes, but the service comes from the caller, which may release it earlier"
                ),
                span,
            )
            .with_fix(|| {
                (
                    format!("add `Fiber.Provided {{{service}}} {region}` to the signature"),
                    Vec::new(),
                )
            }),
        );
    }
}

/// The row element `Provided {S} owner`.
fn obligation(label: &str, service: Type, owner: &Type) -> (String, Type) {
    let row = Type::Row(Row {
        entries: vec![(label.to_owned(), service)],
        tail: None,
    });
    let ty = Type::apps(Type::con("Fiber", "Provided"), [row, owner.clone()]);
    (format!("{PROVIDED}{label}"), ty)
}
