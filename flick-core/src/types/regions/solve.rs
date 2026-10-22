//! Solving capture constraints: a qualifier not inferred yet grows by what
//! the values it bounds hold, the least solution, as effect rows do. A
//! qualifier that cannot take a region, because it is written out or because
//! the region's scope ends before the qualified value does, reports what it
//! lacks.

use std::mem;

use super::{Hold, Visible};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::{Norm, Row, Type};

/// `Outlives c s`: the captures `c` live at least as long as the region `s`.
#[derive(Debug, Clone)]
pub struct Outlives {
    pub captures: Type,
    pub owner: Type,
    pub givens: Vec<Type>,
    pub span: Span,
}

impl Checker<'_> {
    /// What `hold` holds beyond what its type shows.
    fn hidden(&self, hold: &Hold) -> Norm {
        let mut out = Norm::default();
        for (_, ty) in &hold.held {
            let captures = self.captures_of(ty);
            out.entries.extend(captures.entries);
            out.tails.extend(captures.tails);
        }
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
        let late;
        let visible = match &hold.visible {
            Visible::Late(ty) => {
                late = self.visible_in(ty);
                &late
            }
            Visible::Now(labels) => labels,
        };
        out.entries.retain(|(label, _)| !visible.contains(label));
        out
    }

    fn bound_of(&self, hold: &Hold) -> Norm {
        let mut out = Norm::default();
        self.capture_row(&hold.bound, &mut out);
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
        out
    }

    /// Adds to qualifiers not inferred yet what their values hold; `true`
    /// when one grew.
    pub fn grow_holds(&mut self) -> bool {
        let mut changed = false;
        for i in 0..self.regions.holds.len() {
            let hold = &self.regions.holds[i];
            let (actual, bound) = (self.hidden(hold), self.bound_of(hold));
            let Some(m) = bound.tails.iter().find_map(|t| match t {
                Type::Meta(m) => Some(*m),
                _ => None,
            }) else {
                continue;
            };
            for (label, atom) in actual.entries {
                if bound.get(&label).is_some() {
                    continue;
                }
                let rest = self.store.fresh_beside(m);
                let row = Type::Row(Row {
                    entries: vec![(label, atom)],
                    tail: Some(Box::new(rest)),
                });
                // a region whose scope ends before the qualifier does stays out
                if self.bind(m, &row).is_ok() {
                    changed = true;
                    break;
                }
            }
        }
        changed
    }

    /// Makes the qualifiers left unknown hold nothing; with `level`, only
    /// those deeper than it.
    pub fn close_holds(&mut self, level: Option<u32>) {
        let mut open = Vec::new();
        for hold in &self.regions.holds {
            for t in self.store.normalize(&hold.bound).tails {
                if let Type::Meta(m) = t {
                    open.push(m);
                }
            }
        }
        for m in open {
            if self
                .store
                .meta_level(m)
                .is_some_and(|l| level.is_none_or(|level| l > level))
            {
                self.store.set(m, Type::empty_row());
            }
        }
    }

    /// Reports every value that holds more than its qualifier allows, every
    /// child that may outlive what it holds, and every error that would
    /// carry a region.
    pub fn report_regions(&mut self) {
        for hold in mem::take(&mut self.regions.holds) {
            let (actual, bound) = (self.hidden(&hold), self.bound_of(&hold));
            if bound.tails.contains(&Type::Error) || actual.tails.contains(&Type::Error) {
                continue;
            }
            let lacking: Vec<Type> = actual
                .entries
                .into_iter()
                .filter(|(label, _)| bound.get(label).is_none())
                .map(|(_, atom)| atom)
                .collect();
            if let Some(atom) = lacking.first() {
                self.report_hold(&hold, atom, &bound);
            }
        }
        for o in mem::take(&mut self.regions.outlives) {
            self.check_outlives(&o);
        }
        for (errors, span) in mem::take(&mut self.regions.raised) {
            self.check_raised(&errors, span);
        }
    }

    /// The region `atom` as a diagnostic names it, and the scope it ends with.
    fn describe(&self, atom: &Type) -> (String, Option<Span>) {
        match atom {
            Type::Skolem(s) => {
                let name = self.store.skolem_name(*s);
                match self.scope_of(*s) {
                    Some(scope) => (format!("region `{name}`"), Some(scope.span)),
                    None => (format!("`{name}`"), None),
                }
            }
            other => (format!("`{}`", self.show(other)), None),
        }
    }

    /// An inferred qualifier lacks only what its value takes out of a scope;
    /// a written one lacks what it does not allow.
    fn report_hold(&mut self, hold: &Hold, atom: &Type, bound: &Norm) {
        let label = crate::types::ty::capture_label(atom).unwrap_or_default();
        let holder = hold
            .held
            .iter()
            .find(|(_, ty)| self.captures_of(ty).get(&label).is_some())
            .and_then(|(name, _)| name.clone());
        let (region, scope) = self.describe(atom);
        let subject = match &holder {
            Some(name) => format!("this {} uses `{name}`, which holds {region}", hold.what),
            None => format!("this {} holds {region}", hold.what),
        };
        if self.diagnostics.iter().any(|d| {
            d.span
                .is_some_and(|s| s.start <= hold.span.start && hold.span.end <= s.end)
                && [codes::G001, codes::G002].contains(&d.code)
        }) {
            return;
        }
        let diagnostic = match hold.expected.as_ref().filter(|_| hold.written) {
            None => Diagnostic::error(
                codes::G002,
                format!("{subject}, so it cannot leave the `scope` that owns the region"),
                hold.span,
            ),
            Some(expected) => {
                let allowed: Vec<String> = bound
                    .entries
                    .iter()
                    .map(|(_, t)| self.describe(t).0)
                    .collect();
                let allowed = if allowed.is_empty() {
                    "holds no captures".to_owned()
                } else {
                    format!("allows only {}", allowed.join(", "))
                };
                let what = format!("the expected type `{}`", self.show(expected));
                Diagnostic::error(
                    codes::G001,
                    format!("{subject}, but {what} {allowed}"),
                    hold.span,
                )
                .with_fix(
                    format!(
                        "add the region to the qualifier: `T^{{{}}}`",
                        self.show(atom)
                    ),
                    Vec::new(),
                )
            }
        };
        let diagnostic = match scope {
            Some(span) => diagnostic.with_related(span, "the region's scope ends here"),
            None => diagnostic,
        };
        self.diagnostics.push(diagnostic);
    }

    /// Whether the scope `outer` is `inner` or around it.
    fn encloses(&self, outer: usize, inner: usize) -> bool {
        let mut at = Some(inner);
        while let Some(i) = at {
            if i == outer {
                return true;
            }
            at = self.regions.scopes[i].parent;
        }
        false
    }

    fn scope_index(&self, t: &Type) -> Option<usize> {
        match t {
            Type::Skolem(s) => self.regions.scopes.iter().position(|x| x.region == *s),
            _ => None,
        }
    }

    /// A child registered with `owner` runs until the owner's scope closes,
    /// so it may hold only regions that end no earlier.
    fn check_outlives(&mut self, o: &Outlives) {
        let owner = self.store.resolve(&o.owner);
        let owner_scope = self.scope_index(&owner);
        let mut held = Norm::default();
        self.capture_row(&o.captures, &mut held);
        for (label, atom) in held.entries {
            if atom == owner {
                continue;
            }
            let fits = match self.scope_index(&atom) {
                Some(scope) => owner_scope.is_some_and(|owner| self.encloses(scope, owner)),
                // what the signature names outlives every scope inside the body
                None => {
                    owner_scope.is_some()
                        || o.givens.iter().any(|g| match g.spine().1[..] {
                            [c, s] => {
                                self.store.resolve(s) == owner
                                    && self
                                        .captures_of(&Type::Captured {
                                            ty: Box::new(Type::prim("Unit")),
                                            captures: Box::new(c.clone()),
                                        })
                                        .get(&label)
                                        .is_some()
                            }
                            _ => false,
                        })
                }
            };
            if !fits {
                let (region, scope) = self.describe(&atom);
                let (owner_name, _) = self.describe(&owner);
                let mut diagnostic = Diagnostic::error(
                    codes::G003,
                    format!(
                        "the child holds {region}, which may end before its owner {owner_name}: a child runs until its owner's scope closes"
                    ),
                    o.span,
                );
                if let Some(span) = scope {
                    diagnostic = diagnostic.with_related(span, "the region's scope ends here");
                }
                self.diagnostics.push(diagnostic);
                return;
            }
        }
    }

    /// An error carries data out of every scope, even when a handler inside
    /// the scope catches it: its payload holds no region.
    fn check_raised(&mut self, errors: &Type, span: Span) {
        for (_, error) in self.store.normalize(errors).entries {
            let regions = self.captures_of(&error);
            if let Some((_, atom)) = regions
                .entries
                .iter()
                .find(|(_, t)| self.scope_index(t).is_some())
            {
                let (region, _) = self.describe(atom);
                let shown = self.show(&error);
                self.error(
                    codes::E005,
                    span,
                    format!(
                        "the error `{shown}` would carry {region}; an error carries data such as a path, an id or a copy"
                    ),
                );
                return;
            }
        }
    }
}
