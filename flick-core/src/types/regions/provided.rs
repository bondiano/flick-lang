//! `Fiber.Provided r s` (`regions.md` §2.7, `docs/formal/runtime.md#a-provide-service-outlives-the-release-of-its-layer`): a child or finalizer of
//! the owner of region `s` runs with the services of the place that
//! registered it, so each service of `r` comes from a provider that
//! outlives `s`. A service a `provide` builds ends with that `provide`.
//!
//! The constraint splits into one obligation per service, settled where the
//! call is, as `Scope s` is: by the `provide` that builds the service, by a
//! `scope` of the stage, or by a given `Provided`; otherwise it stays in the
//! stage's row as `Provided {S} s` and goes to whatever runs the stage.

use crate::types::Elem;
use crate::types::elem::gone_excludes;
use std::mem;

use super::{Scope, scope};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::classes::args_of;
use crate::types::gadt::branch::type_eqs;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Row, Type};

/// A `provide` around the code being checked.
#[derive(Debug, Clone)]
pub struct Provide {
    /// The labels of the services it builds.
    pub labels: Vec<Elem>,
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
    owner_related: Option<Span>,
}

impl Want {
    fn claim(&mut self, key: String) -> bool {
        if self.settled.contains(&key) {
            false
        } else {
            self.settled.push(key);
            true
        }
    }
}

impl Checker<'_> {
    /// The body of a `provide` at `span` building the services `labels`:
    /// it nests like a `scope`, so owners compare with it (`encloses`).
    pub fn enter_provide(&mut self, node: &Node, span: Span, labels: Vec<Elem>, open: bool) {
        let Type::Skolem(region) = self.store.skolem("provide") else {
            unreachable!("a skolem")
        };
        self.cx.regions.scopes.push(Scope {
            region,
            span,
            node: node.range(),
            #[cfg(feature = "ide")]
            name: None,
            outer: self.cx.stage.services.clone(),
            parent: self.cx.regions.nest.last().copied(),
            handler: false,
        });
        let scope = self.cx.regions.scopes.len() - 1;
        self.cx.regions.nest.push(scope);
        self.cx.regions.provides.push(Provide {
            labels,
            open,
            scope,
        });
    }

    pub fn leave_provide(&mut self) {
        self.cx.regions.provides.pop();
        self.cx.regions.nest.pop();
    }

    fn lexical(&self) -> Lexical {
        Lexical {
            provides: self.cx.regions.provides[self.cx.stage.provides..].to_vec(),
            scopes: self.cx.regions.open[self.cx.stage.scopes..].to_vec(),
            givens: self.classes.givens.clone(),
            services: self.cx.stage.services.clone(),
            site: self.cx.stage.site,
            pure: self.cx.stage.pure.is_some(),
        }
    }

    /// `Provided row owner` wanted at `span`: settled at the end of the
    /// call, once its arguments say what the task needs.
    pub fn want_provided(&mut self, row: Type, owner: Type, span: Span) {
        self.want_provided_owned(row, owner, span, None);
    }

    pub fn want_provided_owned(
        &mut self,
        row: Type,
        owner: Type,
        span: Span,
        owner_related: Option<Span>,
    ) {
        let at = self.lexical();
        self.cx.regions.provided.push(Want {
            row,
            owner,
            span,
            at,
            settled: Vec::new(),
            owner_related,
        });
    }

    /// Settles the `Provided` constraints wanted since `mark`. A row not
    /// inferred yet, as of a function applied to fewer arguments, waits for
    /// the end of the declaration.
    pub fn settle_provided(&mut self, mark: usize) {
        let late = self.cx.regions.provided_late.len();
        self.settle_provided_since(mark, late);
    }

    /// Settles `Provided` constraints wanted since `mark`, and those deferred
    /// during a sub-call since `late_mark` whose owner or row an outer step
    /// (such as `|>`) just supplied.
    pub fn settle_provided_since(&mut self, mark: usize, late_mark: usize) {
        if self.cx.regions.provided.len() == mark
            && self.cx.regions.provided_late.len() == late_mark
        {
            return;
        }
        self.grow();
        let mut wanted: Vec<Want> = self.cx.regions.provided.drain(mark..).collect();
        wanted.extend(self.cx.regions.provided_late.drain(late_mark..));
        self.settle_wants(wanted, false);
    }

    /// At the end of the declaration, when rows no longer grow: what is
    /// left must already be in the row of its stage.
    pub fn settle_late_provided(&mut self) {
        let mut wanted = mem::take(&mut self.cx.regions.provided);
        wanted.append(&mut self.cx.regions.provided_late);
        self.settle_wants(wanted, true);
        for (provide, owner, service, span, owner_related) in
            mem::take(&mut self.cx.regions.late_owners)
        {
            let owner = self.store.resolve(&owner);
            match self.scope_index(&owner) {
                Some(scope) if self.encloses(provide, scope) => {}
                Some(_) => self.outlived_provide(&service, provide, &owner, span, owner_related),
                // never solved: an error was reported already
                None => {}
            }
        }
    }

    fn settle_wants(&mut self, wanted: Vec<Want>, last: bool) {
        for mut want in wanted {
            let equalities = type_eqs(&want.at.givens);
            if self.under(&equalities, |this| this.settle_want(&mut want, last)) {
                self.cx.regions.provided_late.push(want);
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
            if matches!(label, Elem::Live) {
                if want.claim(format!("live{ty:?}")) {
                    let span = want.span;
                    self.keep_live(
                        &ty,
                        &want.owner,
                        &want.at.givens,
                        span,
                        want.owner_related,
                        last,
                    );
                }
                continue;
            }
            // `Async` is the root's: no owner provides it
            let root = lang::label(Lang::Async) == label.name();
            if root
                || label.name() == Some(scope())
                || matches!(label, Elem::Provided(_))
                || !want.claim(label.to_string())
            {
                continue;
            }
            if !self.discharge(&label, &want.owner, &want.at, want.span, want.owner_related) {
                keep.push(obligation(&label, ty, &want.owner));
            }
        }
        let mut open = false;
        for tail in &norm.tails {
            match tail {
                Type::Meta(_) => open = true,
                Type::Var(_) | Type::Skolem(_) => {
                    let service = self.show_exact(tail);
                    let owner = self.store.resolve(&want.owner);
                    if want.claim(service.clone())
                        && !self.scope_or_given(&want.at, &owner, None, Some(tail))
                    {
                        let region = self.show_exact(&want.owner);
                        let named = self.signed(tail) && self.signed(&want.owner);
                        self.unprovided(&service, &region, named, want.span);
                    }
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
        if self.cx.regions.passed_provided {
            self.grow();
        }
        let mut norm = self.store.normalize(services);
        if !norm
            .entries
            .iter()
            .any(|(l, _)| matches!(l, Elem::Provided(_)))
        {
            return services.clone();
        }
        let at = self.lexical();
        let entries = mem::take(&mut norm.entries);
        for (label, ty) in entries {
            let owner = match ty.spine().1[..] {
                [_, owner] => Some(owner.clone()),
                _ => None,
            };
            let settled = match (&label, owner) {
                (Elem::Provided(service), Some(owner)) => {
                    self.discharge(service, &owner, &at, span, None)
                }
                _ => false,
            };
            if !settled {
                norm.entries.push((label, ty));
            }
        }
        norm.into_type()
    }

    /// Whether the place `at` settles that `service` outlives `owner`.
    fn discharge(
        &mut self,
        service: &Elem,
        owner: &Type,
        at: &Lexical,
        span: Span,
        owner_related: Option<Span>,
    ) -> bool {
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
                    let late = (
                        provide,
                        owner.clone(),
                        service.to_owned(),
                        span,
                        owner_related,
                    );
                    self.cx.regions.late_owners.push(late);
                }
                _ => self.outlived_provide(service, provide, &owner, span, owner_related),
            }
            return true;
        }
        if self
            .scope_index(&owner)
            .is_some_and(|i| at.scopes.contains(&i))
        {
            return true;
        }
        // A callback stage whose row explicitly excludes `Provided(service)`
        // (from a `Layer.run` wrapper) shadows any outer `Fiber.Provided` given
        // for that service.
        if self.stage_excludes_provided(&at.services, service) {
            return false;
        }
        self.given_provided(at, &owner, Some(service), None)
    }

    /// Whether `services` carries a `Type::Without` tail excluding `Provided(service)`.
    fn stage_excludes_provided(&self, services: &Type, service: &Elem) -> bool {
        let target = Elem::Provided(Box::new(service.clone()));
        self.store.normalize(services).tails.iter().any(|tail| {
            let Type::Without { gone, .. } = tail else {
                return false;
            };
            gone_excludes(gone, &target)
        })
    }

    /// Whether `owner` is a `scope` of `at`, or a given `Provided R owner`
    /// names `service` or `tail`.
    fn scope_or_given(
        &self,
        at: &Lexical,
        owner: &Type,
        service: Option<&Elem>,
        tail: Option<&Type>,
    ) -> bool {
        self.scope_index(owner)
            .is_some_and(|i| at.scopes.contains(&i))
            || (!self.stage_excludes_any_provided(&at.services)
                && self.given_provided(at, owner, service, tail))
    }

    fn stage_excludes_any_provided(&self, services: &Type) -> bool {
        self.store.normalize(services).tails.iter().any(|tail| {
            let Type::Without { gone, .. } = tail else {
                return false;
            };
            gone.contains(&Elem::ProvidedAny)
        })
    }

    /// Whether a given `Provided R owner` in `at` names `service` or `tail`.
    fn given_provided(
        &self,
        at: &Lexical,
        owner: &Type,
        service: Option<&Elem>,
        tail: Option<&Type>,
    ) -> bool {
        at.givens.iter().any(|g| {
            let Some(args) = args_of(g, Lang::Provided) else {
                return false;
            };
            let [row, region] = &args[..] else {
                return false;
            };
            if self.store.resolve(region) != *owner {
                return false;
            }
            let norm = self.store.normalize(row);
            service.is_some_and(|s| norm.get(s).is_some())
                || tail.is_some_and(|t| norm.tails.contains(t))
        })
    }

    /// Obligations the stage cannot settle go to its row.
    fn pass_on(&mut self, entries: Vec<(Elem, Type)>, at: &Lexical, span: Span) {
        if entries.is_empty() || at.pure {
            return;
        }
        self.cx.regions.passed_provided = true;
        let row = Type::Row(Row {
            entries,
            tail: None,
            ..Row::default()
        });
        self.sub_row(&row, &at.services, at.site, span, false);
    }

    /// Once rows are solved, an obligation not settled must be in its row.
    fn check_passed(&mut self, entries: Vec<(Elem, Type)>, at: &Lexical, span: Span) {
        if at.pure {
            return;
        }
        let row = self.store.normalize(&at.services);
        for (label, ty) in entries {
            if row.get(&label).is_none() {
                let (service, region, named) = match ty.spine().1[..] {
                    [service, owner] => (
                        self.show_exact(service),
                        self.show_exact(owner),
                        self.signed(owner),
                    ),
                    _ => continue,
                };
                self.unprovided(service.trim_matches(['{', '}', ' ']), &region, named, span);
            }
        }
    }
}

/// The row element `Provided {S} owner`.
fn obligation(label: &Elem, service: Type, owner: &Type) -> (Elem, Type) {
    let row = Type::Row(Row {
        entries: vec![(label.clone(), service)],
        tail: None,
        ..Row::default()
    });
    let ty = Type::apps(Type::lang(Lang::Provided), [row, owner.clone()]);
    (Elem::Provided(Box::new(label.clone())), ty)
}
