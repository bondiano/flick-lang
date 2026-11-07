//! Solving capture constraints: a qualifier not inferred yet grows by what
//! the values it bounds hold, the least solution, as effect rows do. A
//! qualifier that cannot take a region, because it is written out or because
//! the region's scope ends before the qualified value does, reports what it
//! lacks.

use std::{collections::HashSet, mem};

use super::{Hold, Visible};
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::types::env::Form;
use crate::types::kinds::Kind;
use crate::types::ty::{Norm, Row, Type};
use crate::types::{Checker, ty};

/// Whether `captures` is what `stages` gives a later stage, which starts from
/// the empty row: a written `^(c + d)` does not.
fn implied_stage(captures: &Type) -> bool {
    match captures {
        Type::Union(left, _) => {
            matches!(&**left, Type::Row(r) if r.entries.is_empty() && r.tail.is_none())
                || implied_stage(left)
        }
        _ => false,
    }
}

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
            Visible::Late(ty) | Visible::Refined(ty) => {
                late = self.shown_by(ty);
                &late
            }
            Visible::Now(labels) => labels,
        };
        out.entries.retain(|(label, _)| !visible.contains(label));
        if matches!(hold.visible, Visible::Refined(_)) {
            out.entries
                .retain(|(_, atom)| self.atom_kind(atom, &[]) == Some(Kind::Region));
            out.tails.clear();
        }
        out
    }

    fn bound_of(&self, hold: &Hold) -> Norm {
        let mut out = Norm::default();
        self.capture_row(&hold.bound, &mut out);
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
        out
    }

    /// Each hold as solving reads it, until a qualifier is bound.
    fn summaries(&self) -> Vec<Summary> {
        self.regions
            .holds
            .iter()
            .map(|hold| self.summary(hold))
            .collect()
    }

    fn summary(&self, hold: &Hold) -> Summary {
        Summary {
            visible: match &hold.visible {
                Visible::Now(labels) => labels.clone(),
                Visible::Late(ty) | Visible::Refined(ty) => self.shown_by(ty),
            },
            bound: self.bound_of(hold),
            hidden: self.hidden(hold),
        }
    }

    /// The variables open in what the summary of `hold` reads: binding
    /// another one leaves it as it is.
    fn open_in(&self, hold: &Hold) -> HashSet<u32> {
        let mut open = HashSet::new();
        let mut read = |t: &Type| {
            self.store.zonk(t).visit(&mut |x| {
                if let Type::Meta(m) = x {
                    open.insert(*m);
                }
            });
        };
        hold.held.iter().for_each(|(_, ty)| read(ty));
        read(&hold.bound);
        if let Visible::Late(ty) | Visible::Refined(ty) = &hold.visible {
            read(ty);
        }
        open
    }

    /// Adds to qualifiers not inferred yet what their values hold; `true`
    /// when one grew.
    pub fn grow_holds(&mut self) -> bool {
        let open_bound = |hold: &Hold| {
            let bound = self.bound_of(hold);
            bound.tails.iter().any(|t| matches!(t, Type::Meta(_)))
        };
        if !self.regions.holds.iter().any(open_bound) {
            return false;
        }
        let mut changed = false;
        let mut summaries = self.summaries();
        let mut open: Vec<_> = self.regions.holds.iter().map(|h| self.open_in(h)).collect();
        for i in 0..summaries.len() {
            let Summary { bound, hidden, .. } = &summaries[i];
            if !bound.tails.iter().any(|t| matches!(t, Type::Meta(_))) {
                continue;
            }
            let (bound, actual) = (bound.clone(), hidden.entries.clone());
            for (label, atom) in actual {
                if bound.get(&label).is_some() {
                    continue;
                }
                let blocked = blocked_capture_rows(&summaries, &label);
                let metas = bound.tails.iter().filter_map(|t| match t {
                    Type::Meta(m) => Some(*m),
                    _ => None,
                });
                // blocking only chooses among the rows: with every one
                // blocked, the first still grows, and its bound reports
                let (unblocked, shut): (Vec<u32>, Vec<u32>) =
                    metas.partition(|m| !blocked.contains(m));
                let mut grew = None;
                for m in unblocked.into_iter().chain(shut) {
                    let rest = self.store.fresh_beside(m);
                    let row = Type::Row(Row {
                        entries: vec![(label.clone(), atom.clone())],
                        tail: Some(Box::new(rest)),
                        ..Row::default()
                    });
                    if self.bind(m, &row).is_ok() {
                        changed = true;
                        grew = Some(m);
                        break;
                    }
                }
                if let Some(m) = grew {
                    // only the holds that read it change: all of them at each
                    // growth would take time quadratic in the holds of a module
                    for (j, hold) in self.regions.holds.iter().enumerate() {
                        if open[j].contains(&m) {
                            summaries[j] = self.summary(hold);
                            open[j] = self.open_in(hold);
                        }
                    }
                    debug_assert_eq!(summaries, self.summaries());
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
        let mut holds = mem::take(&mut self.regions.holds);
        // a written bound names the failure exactly; reporting it first lets
        // it cover an inferred one at the same span
        holds.sort_by_key(|hold| !hold.written);
        for hold in holds {
            let (actual, bound) = (self.hidden(&hold), self.bound_of(&hold));
            if bound.tails.contains(&Type::Error) || actual.tails.contains(&Type::Error) {
                continue;
            }
            let lacking: Vec<Type> = actual
                .entries
                .into_iter()
                .filter(|(label, _)| bound.get(label).is_none())
                .map(|(_, atom)| atom)
                // a region never solved comes from an error reported already
                .filter(|atom| !matches!(self.store.resolve(atom), Type::Meta(_)))
                .collect();
            if let Some(atom) = lacking.first() {
                self.report_hold(&hold, atom, &bound);
            }
        }
        // after the holds: what a qualifier holds too much is theirs to name
        self.compare_deferred();
        for o in mem::take(&mut self.regions.outlives) {
            self.under(&o.givens, |this| this.check_outlives(&o));
        }
        self.settle_late_provided();
        for (errors, span) in mem::take(&mut self.regions.raised) {
            self.check_raised(&errors, span);
        }
        for (needs, allowed, span) in mem::take(&mut self.regions.tail_calls) {
            self.check_tail_call(&needs, &allowed, span);
        }
        for (element, span) in mem::take(&mut self.regions.error_elements) {
            let element = self.zonk(&element);
            if !self.may_be_error(&element) {
                let shown = self.show_exact(&element);
                self.error(
                    codes::T002,
                    span,
                    format!("`{shown}` is not an error: an error row holds types declared with `type error`"),
                );
            }
        }
    }

    /// Whether `t` is, or may still become, an error type: a variable
    /// passes, as its callers are checked where they name it.
    pub fn may_be_error(&self, t: &Type) -> bool {
        match t.spine().0 {
            Type::Con { module, name } => matches!(
                self.env.type_def(module, name).map(|d| &d.form),
                Some(Form::Error)
            ),
            _ => true,
        }
    }

    /// The region `atom` as a diagnostic names it, and the scope it ends with.
    pub(super) fn describe(&self, atom: &Type) -> (String, Option<Span>) {
        match atom {
            Type::Skolem(s) if self.regions.lent.contains(s) => (
                "the reader of a lazy `for` that reads atoms".to_owned(),
                None,
            ),
            Type::Skolem(s) => {
                let name = self.store.skolem_name(*s);
                match self.scope_of(*s) {
                    Some(scope) => (format!("region `{name}`"), Some(scope.span)),
                    None => (format!("`{name}`"), None),
                }
            }
            other => (format!("`{}`", self.show_exact(other)), None),
        }
    }

    /// An inferred qualifier lacks only what its value takes out of a scope;
    /// a written one lacks what it does not allow.
    fn report_hold(&mut self, hold: &Hold, atom: &Type, bound: &Norm) {
        let label = ty::capture_label(atom).unwrap_or_default();
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
        let reader = matches!(atom, Type::Skolem(s) if self.regions.lent.contains(s));
        let diagnostic = match hold.expected.as_ref().filter(|_| hold.written) {
            None if reader => Diagnostic::error(
                codes::G002,
                format!("{subject}, so it cannot leave the function whose atoms it reads"),
                hold.span,
            )
            .with_fix(|| {
                (
                    "build a list with `[for …]`, or iterate the sequence in that function",
                    Vec::new(),
                )
            }),
            None if self.handler_region(atom) => Diagnostic::error(
                codes::G002,
                format!("{subject}: `resume` stays in the arms of its handler"),
                hold.span,
            ),
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
                let what = format!("the expected type `{}`", self.show_exact(expected));
                // a stage's qualifier may be one no one wrote, so say where it is from
                let origin = if implied_stage(&hold.bound) {
                    "; the signature does not write this qualifier: a later stage of a function holds what its earlier parameters hold"
                } else {
                    ""
                };
                let mut edits = Vec::new();
                let advice = if reader {
                    "build a list with `[for …]`, or iterate the sequence in the function that builds it".to_owned()
                } else if !self.is_region(atom, &[]) {
                    let what = match self.atom_kind(atom, &[]) {
                        Some(Kind::ErrorRow) => "the error row",
                        _ => "the type variable",
                    };
                    format!(
                        "a qualifier names regions, not {what} `{}`: build the value without what holds it, such as with `match` instead of a callback that uses it",
                        self.show_exact(atom)
                    )
                } else {
                    let region = self.show_exact(atom);
                    // shortcut: only a qualifier written in braces gets the edit, not `T` or `T^c`; upgrade when the type of a value leads to where it is written.
                    edits.extend(
                        self.local_place(&hold.bound)
                            .adding(&region)
                            .map(|(span, replacement)| Edit { span, replacement }),
                    );
                    format!("add the region to the qualifier: `T^{{{region}}}`")
                };
                Diagnostic::error(
                    codes::G001,
                    format!("{subject}, but {what} {allowed}{origin}"),
                    hold.span,
                )
                .with_fix(|| (advice, edits))
            }
        };
        let ends = if self.handler_region(atom) {
            "the handler ends here"
        } else {
            "the region's scope ends here"
        };
        let diagnostic = match scope {
            Some(span) => diagnostic.with_related(span, ends),
            None => diagnostic,
        };
        self.diagnostics.push(diagnostic);
    }

    /// Whether the scope `outer` is `inner` or around it.
    pub(super) fn encloses(&self, outer: usize, inner: usize) -> bool {
        let mut at = Some(inner);
        while let Some(i) = at {
            if i == outer {
                return true;
            }
            at = self.regions.scopes[i].parent;
        }
        false
    }

    pub(super) fn scope_index(&self, t: &Type) -> Option<usize> {
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
                            [c, s] if self.store.resolve(s) == owner => {
                                let mut allowed = Norm::default();
                                self.capture_row(c, &mut allowed);
                                allowed.get(&label).is_some()
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
                let shown = self.show_exact(&error);
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

/// What a hold shows, may hold and holds beyond what it shows.
#[derive(Debug, PartialEq)]
struct Summary {
    visible: Vec<String>,
    bound: Norm,
    hidden: Norm,
}

/// Upper bounds decide which side of an inferred union may retain an atom.
fn blocked_capture_rows(summaries: &[Summary], label: &str) -> HashSet<u32> {
    let mut blocked = HashSet::new();
    loop {
        let mut changed = false;
        for summary in summaries {
            if summary.visible.iter().any(|l| l == label)
                || summary.bound.get(label).is_some()
                || summary.bound.tails.iter().any(|t| {
                    matches!(t, Type::Meta(m) if !blocked.contains(m)) || matches!(t, Type::Error)
                })
            {
                continue;
            }
            for tail in &summary.hidden.tails {
                if let Type::Meta(m) = tail {
                    changed |= blocked.insert(*m);
                }
            }
        }
        if !changed {
            return blocked;
        }
    }
}
