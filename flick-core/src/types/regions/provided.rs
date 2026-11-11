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
use std::mem;

use super::{Scope, scope};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::classes::args_of;
use crate::types::convert::is_type;
use crate::types::gadt::branch::type_eqs;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Norm, Row, Type};

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
        let at = self.lexical();
        self.cx.regions.provided.push(Want {
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
        let wanted: Vec<Want> = self.cx.regions.provided.drain(mark..).collect();
        for mut want in wanted {
            let equalities = type_eqs(&want.at.givens);
            if self.under(&equalities, |this| this.settle_want(&mut want, false)) {
                self.cx.regions.provided_late.push(want);
            }
        }
    }

    /// At the end of the declaration, when rows no longer grow: what is
    /// left must already be in the row of its stage.
    pub fn settle_late_provided(&mut self) {
        let mut wanted = mem::take(&mut self.cx.regions.provided);
        wanted.append(&mut self.cx.regions.provided_late);
        for mut want in wanted {
            let equalities = type_eqs(&want.at.givens);
            self.under(&equalities, |this| this.settle_want(&mut want, true));
        }
        for (provide, owner, service, span) in mem::take(&mut self.cx.regions.late_owners) {
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
            if matches!(label, Elem::Live) {
                let key = format!("{label}{ty:?}");
                if !want.settled.contains(&key) {
                    want.settled.push(key);
                    let span = want.span;
                    self.keep_live(&ty, &want.owner, &want.at.givens, span, last);
                }
                continue;
            }
            // `Async` is the root's: no owner provides it
            let root = lang::label(Lang::Async) == label.name();
            let key = label.to_string();
            if root
                || label.name() == Some(scope())
                || matches!(label, Elem::Provided(_))
                || want.settled.contains(&key)
            {
                continue;
            }
            want.settled.push(key);
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
                    let (service, region) = (self.show_exact(tail), self.show_exact(&want.owner));
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
                    self.discharge(service, &owner, &at, span)
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
    fn discharge(&mut self, service: &Elem, owner: &Type, at: &Lexical, span: Span) -> bool {
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
                    self.cx.regions.late_owners.push(late);
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
        at.givens.iter().any(|g| match args_of(g, Lang::Provided) {
            Some(args) => match args[..] {
                [row, region] => {
                    self.store.resolve(region) == *owner && names(&self.store.normalize(row))
                }
                _ => false,
            },
            _ => false,
        })
    }

    /// Obligations the stage cannot settle go to its row.
    fn pass_on(&mut self, entries: Vec<(Elem, Type)>, at: &Lexical, span: Span) {
        if entries.is_empty() || at.pure {
            return;
        }
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
                let (service, region) = match ty.spine().1[..] {
                    [service, owner] => (self.show_exact(service), self.show_exact(owner)),
                    _ => continue,
                };
                self.unprovided(service.trim_matches(['{', '}', ' ']), &region, span);
            }
        }
    }

    fn outlived_provide(&mut self, service: &Elem, provide: usize, owner: &Type, span: Span) {
        let Scope { span: at, .. } = self.cx.regions.scopes[provide];
        let (region, _) = self.describe(owner);
        let service = service.to_string();
        let name = service.rsplit('.').next().unwrap_or(&service);
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
        // a row variable is the row itself; a service is an element of one
        let row = if service.starts_with(char::is_lowercase) {
            service.to_owned()
        } else {
            format!("{{{service}}}")
        };
        let constraint = format!("Fiber.Provided {row} {region}");
        let edits = self.constraint_edits(&constraint, span);
        self.diagnostics.push(
            Diagnostic::error(
                codes::G003,
                format!(
                    "this keeps work that uses `{service}` until `{region}` closes, but the service comes from the caller, which may release it earlier"
                ),
                span,
            )
            .with_fix(|| (format!("add `{constraint}` to the signature"), edits)),
        );
    }

    /// The edits that add `constraint` to the signature of the top-level
    /// function around `span`; none when it has no signature to edit.
    fn constraint_edits(&self, constraint: &str, span: Span) -> Vec<Edit> {
        let Some(mut ty) = self
            .signature_around(span)
            .and_then(|sig| sig.nodes().find(|n| is_type(n.kind)))
        else {
            return Vec::new();
        };
        if ty.kind == K::ForallType {
            let Some(body) = ty.nodes().next() else {
                return Vec::new();
            };
            ty = body;
        }
        let insert = |at: u32, replacement: String| Edit {
            span: Span::new(self.file, at, at),
            replacement,
        };
        match (ty.kind == K::ConstrainedType)
            .then(|| ty.nodes().next())
            .flatten()
        {
            Some(context) => {
                let (start, end) = context.range();
                if context.kind == K::TupleType {
                    vec![insert(end - 1, format!(", {constraint}"))]
                } else {
                    vec![
                        insert(start, "(".to_owned()),
                        insert(end, format!(", {constraint})")),
                    ]
                }
            }
            None => vec![insert(ty.range().0, format!("{constraint} => "))],
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
