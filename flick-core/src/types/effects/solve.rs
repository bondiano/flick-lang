//! Solving `actual ⊆ declared` between rows. A declared row that is still
//! unknown grows by the labels and row variables flowing into it; what stays
//! unknown at the end is the empty row, the least solution. Then every
//! constraint either holds or names what its bound lacks.

use crate::types::Elem;
use std::mem;

use super::Site;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::fmt::foralls;
use crate::source::Span;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Norm, Row, Type, Written};
use crate::types::{Checker, regions};

/// The row `actual` of a call must be part of `declared`.
#[derive(Debug, Clone)]
pub struct SubRow {
    pub actual: Type,
    pub declared: Type,
    pub site: usize,
    pub span: Span,
    /// An error row; a service row otherwise.
    pub errors: bool,
    /// For the body of a `scope`: its region, whose `Scope` the scope
    /// provides. `Scope` of another region passes on to `declared`, the
    /// services around the scope; one of a region not known yet is the
    /// scope's own.
    pub provides: Option<Type>,
    /// The equalities of the GADT branches the call is in.
    pub givens: Vec<Type>,
}

/// What one bound lacks.
struct Excess {
    site: usize,
    span: Span,
    services: Vec<String>,
    errors: Vec<String>,
    /// The row labels of what is lacking, for the fixes that name a service.
    labels: Vec<Elem>,
    /// Where the bound's row is written, once rows are solved: a variable
    /// may stand for a row written elsewhere.
    bound: Written,
    /// A name the bound takes out of its row variable is lacking: writing
    /// it into the row is no fix.
    taken: bool,
}

impl Checker<'_> {
    /// Grows unknown declared rows until nothing changes.
    pub fn grow(&mut self) {
        loop {
            // `|`, not `||`: every row grows in each round
            let rows =
                (0..self.cx.subrows.len()).fold(false, |changed, i| changed | self.grow_one(i));
            if !(self.grow_holds() || rows) {
                break;
            }
        }
    }

    /// The actual row of constraint `i` without what its scope provides.
    fn actual(&mut self, i: usize) -> Norm {
        let mut actual = self.store.normalize(&self.cx.subrows[i].actual);
        let Some(region) = self.cx.subrows[i].provides.clone() else {
            return actual;
        };
        self.discharge_live(&mut actual, &region);
        let Some(scope) = actual.get(regions::scope()).cloned() else {
            return actual;
        };
        if let [owner] = scope.spine().1[..] {
            let owner = owner.clone();
            if let Type::Meta(_) = self.store.resolve(&owner) {
                let _ = self.unify(&owner, &region);
            }
            if self.store.resolve(&owner) == self.store.resolve(&region) {
                actual
                    .entries
                    .retain(|(label, _)| label != regions::scope());
            }
        }
        actual
    }

    /// Adds to the declared row of constraint `i` one label or row variable
    /// it lacks, if that row is open; `true` when it did.
    fn grow_one(&mut self, i: usize) -> bool {
        let givens = mem::take(&mut self.cx.subrows[i].givens);
        let grown = self.under(&givens, |this| this.grow_row(i));
        self.cx.subrows[i].givens = givens;
        grown
    }

    fn grow_row(&mut self, i: usize) -> bool {
        let actual = self.actual(i);
        let declared = self.store.normalize(&self.cx.subrows[i].declared);
        // a row still unknown, maybe less some names: it grows by what the
        // actual row holds, but never by a name it is taken without
        let unknown_tail = declared.tails.iter().find(|t| unknown(t).is_some());
        let open = unknown_tail.and_then(unknown);
        let never: Vec<Elem> = unknown_tail.map_or_else(Vec::new, |t| less(t).1.to_vec());
        if self.grow_live(&actual, &declared, open) {
            return true;
        }
        for (label, ty) in actual
            .entries
            .iter()
            .filter(|(l, _)| !matches!(l, Elem::Live))
        {
            match declared.get(label) {
                Some(bound) => {
                    let bound = bound.clone();
                    let _ = self.unify(ty, &bound);
                }
                None => {
                    if let Some(m) = open.filter(|_| !never.contains(label)) {
                        let rest = self.store.fresh_beside(m);
                        let row = Type::Row(Row {
                            entries: vec![(label.clone(), ty.clone())],
                            tail: Some(Box::new(rest)),
                            ..Row::default()
                        });
                        return self.bind(m, &row).is_ok();
                    }
                }
            }
        }
        for tail in &actual.tails {
            if matches!(tail, Type::Error) || unknown(tail).is_some() || allows(&declared, tail) {
                continue;
            }
            if let Some(m) = open {
                let rest = self.store.fresh_beside(m);
                let union = Type::Union(Box::new(tail.clone()), Box::new(rest));
                return self.bind(m, &union).is_ok();
            }
        }
        false
    }

    /// Before a binding is generalized: solves the constraints so far and
    /// closes the unknown rows of its calls, so they become no row
    /// variables of its type.
    pub fn settle_rows(&mut self) {
        self.grow();
        let level = self.store.level;
        let mut close = Vec::new();
        for c in &self.cx.subrows {
            for t in self.store.normalize(&c.actual).tails {
                if let Some(m) = unknown(&t)
                    && self.store.meta_level(m).is_some_and(|l| l > level)
                {
                    close.push(m);
                }
            }
        }
        for m in close {
            if self.store.meta_level(m).is_some() {
                self.store.set(m, Type::empty_row());
            }
        }
        self.close_holds(Some(level));
    }

    /// At the end of a declaration: solves its constraints, makes what is
    /// still unknown empty, and reports every bound that lacks something.
    pub fn solve_rows(&mut self) {
        self.close_rows();
        self.report_rows();
        self.report_regions();
    }

    /// Solves the constraints so far and makes every row they leave unknown
    /// empty.
    pub fn close_rows(&mut self) {
        self.close_rows_above(None);
    }

    /// `close_rows` for the rows of variables deeper than `level` only: a
    /// binding's own, while an enclosing declaration still gathers its calls.
    pub fn close_rows_above(&mut self, level: Option<u32>) {
        self.grow();
        let mut open = Vec::new();
        for c in &self.cx.subrows {
            for side in [&c.actual, &c.declared] {
                for t in self.store.normalize(side).tails {
                    open.extend(unknown(&t));
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
        self.close_holds(level);
    }

    fn report_rows(&mut self) {
        let mut excess: Vec<Excess> = Vec::new();
        for i in 0..self.cx.subrows.len() {
            let actual = self.actual(i);
            let c = self.cx.subrows[i].clone();
            let declared = self.store.normalize(&c.declared);
            // a declared element that is no error is reported as such, once
            if c.errors && declared.entries.iter().any(|(_, t)| !self.may_be_error(t)) {
                continue;
            }
            let mut lacking = Vec::new();
            let mut labels = Vec::new();
            self.report_live(&actual, &declared, c.site, c.span);
            for (label, ty) in actual
                .entries
                .iter()
                .filter(|(l, _)| !matches!(l, Elem::Live))
            {
                match declared.get(label) {
                    Some(bound) => {
                        if let Err(m) = self.unify(ty, bound) {
                            self.mismatch(&m, ty, bound, c.span);
                        }
                    }
                    None if label == regions::scope() => self.no_owner(ty, &c),
                    None if matches!(label, Elem::Provided(_)) => {
                        if let [service, owner] = ty.spine().1[..] {
                            let service = self.show(service);
                            let owner = self.show(owner);
                            self.unprovided(service.trim_matches(['{', '}', ' ']), &owner, c.span);
                        }
                    }
                    None => {
                        lacking.push(self.show(ty));
                        labels.push(label.clone());
                    }
                }
            }
            let mut taken = false;
            for tail in (actual.tails.iter()).filter(|t| !matches!(t, Type::Error)) {
                match owed(&declared, tail) {
                    // the variable is allowed, less names nothing gives back
                    Some(names) => {
                        taken |= !names.is_empty();
                        lacking.extend(names.iter().map(Elem::short));
                        labels.extend(names);
                    }
                    None => lacking.push(self.show(tail)),
                }
            }
            if declared.tails.contains(&Type::Error) || lacking.is_empty() {
                continue;
            }
            if !excess.iter().any(|e| e.site == c.site) {
                excess.push(Excess {
                    site: c.site,
                    span: c.span,
                    services: Vec::new(),
                    errors: Vec::new(),
                    labels: Vec::new(),
                    bound: Written::default(),
                    taken: false,
                });
            }
            let i = excess
                .iter()
                .position(|e| e.site == c.site)
                .expect("just added");
            excess[i].taken |= taken;
            let list = if c.errors {
                &mut excess[i].errors
            } else {
                &mut excess[i].services
            };
            for name in lacking {
                if !list.contains(&name) {
                    list.push(name);
                }
            }
            for label in labels {
                if !excess[i].labels.contains(&label) {
                    excess[i].labels.push(label);
                }
            }
            if !c.errors && excess[i].bound.0.is_none() {
                excess[i].bound = self.local_place(&c.declared);
            }
        }
        self.cx.subrows.clear();
        for e in excess {
            self.report_excess(&e);
        }
    }

    /// A resource acquired where no `scope` owns it. The fix is not safe: the
    /// row says the caller provides the owner, which fixes a public boundary.
    fn no_owner(&mut self, scope: &Type, c: &SubRow) {
        let region = match scope.spine().1[..] {
            [region] if !matches!(self.store.resolve(region), Type::Meta(_)) => self.show(region),
            // a region not known yet gets a name the signature does not use
            _ => {
                let taken = self
                    .signature_around(c.span)
                    .map(|sig| foralls::type_names(sig, self.text))
                    .unwrap_or_default();
                let fresh = (0..).map(|i| {
                    if i == 0 {
                        "s".to_owned()
                    } else {
                        format!("s{i}")
                    }
                });
                fresh
                    .into_iter()
                    .find(|name| !taken.contains(name))
                    .expect("names never run out")
            }
        };
        let added = format!("Scope {region}");
        let bound = self.local_place(&c.declared);
        let written = match (bound.0, &self.sites[c.site]) {
            (Some(_), _) => Some(bound),
            (None, Site::Signature(_, written) | Site::Expected(_, written)) => Some(*written),
            (None, Site::Inferred) => None,
        };
        let edits: Vec<Edit> = written
            .and_then(|w| w.adding(&added))
            .map(|(span, replacement)| Edit { span, replacement })
            .into_iter()
            .collect();
        let advice = if edits.is_empty() {
            format!(
                "call it inside `scope {{ … }}`, or add `{added}` to the services of the signature"
            )
        } else {
            format!("add `{added}` to the services of the signature")
        };
        self.diagnostics.push(
            Diagnostic::error(
                codes::G004,
                "this acquires a resource, which needs `Scope`: only `scope { … }` provides an owner",
                c.span,
            )
            .with_fix(|| (advice, edits)),
        );
    }

    fn report_excess(&mut self, e: &Excess) {
        let named = |what: &str, names: &[String]| {
            let quoted: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
            match quoted.len() {
                0 => None,
                1 => Some(format!("{what} {}", quoted[0])),
                _ => Some(format!("{what}s {}", quoted.join(", "))),
            }
        };
        let list = [named("service", &e.services), named("error", &e.errors)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" and ");
        if let Site::Expected(ty, _) = &self.sites[e.site]
            && self.reads_only(&ty.clone())
        {
            return self.render_excess(e, &list);
        }
        if let Site::Expected(ty, _) = &self.sites[e.site]
            && self.setup_only(&ty.clone())
        {
            let diagnostic = Diagnostic::error(
                codes::E002,
                format!("a component's setup runs once per instance, so {list} is not allowed in it"),
                e.span,
            ).with_fix(|| ("read atoms in the component's last expression, its render, as `ui { let x = … }`, or make an `Atom.derived` here", Vec::new()));
            self.diagnostics.push(diagnostic);
            return;
        }
        let (bound, needer, written) = match &self.sites[e.site] {
            Site::Signature(name, written) => (
                format!("the signature of `{name}`"),
                "its body",
                Some(*written),
            ),
            Site::Expected(ty, written) => (
                format!("the expected type `{}`", self.show(ty)),
                "this",
                written.0.is_some().then_some(*written),
            ),
            Site::Inferred => ("the type of this function".to_owned(), "its body", None),
        };
        let mut diagnostic = Diagnostic::error(
            codes::E002,
            format!("{bound} does not allow {list}, which {needer} needs"),
            e.span,
        );
        let written = if e.bound.0.is_some() {
            Some(e.bound)
        } else {
            written
        };
        if let Some(written) = written.filter(|_| !e.taken) {
            // the services go after the `{`; the errors, after the `;`, are left to the user
            let edits: Vec<Edit> = (e.errors.is_empty())
                .then(|| written.adding(&e.services.join(", ")))
                .flatten()
                .map(|(span, replacement)| Edit { span, replacement })
                .into_iter()
                .collect();
            // a function that suspends says so: adding `Async` alone changes nothing else
            let only_async = e.errors.is_empty()
                && !edits.is_empty()
                // a row variable it lacks has no label
                && e.labels.len() == e.services.len()
                && (e.labels.iter()).all(|l| lang::label(Lang::Async) == l.name());
            let fix = || (format!("add {list} to {bound}"), edits);
            diagnostic = if only_async {
                diagnostic.with_safe_fix(fix)
            } else {
                diagnostic.with_fix(fix)
            };
        }
        self.diagnostics.push(diagnostic);
    }

    /// A render that does more than read atoms.
    fn render_excess(&mut self, e: &Excess, list: &str) {
        self.diagnostics.push(
            Diagnostic::error(
                codes::U004,
                format!(
                    "a view's render and a derived atom only read atoms: {list} is not allowed here"
                ),
                e.span,
            ).with_fix(|| {
        let is = |l: Lang| e.labels.iter().any(|s| lang::label(l) == s.name());
        let writes = is(Lang::AtomWrite) || is(Lang::AtomRegistry);
        let (setup, document) = (is(Lang::Setup), is(Lang::UiDocument));
        let fixes = [
            setup.then_some("call it before a component's last expression, in its setup, which runs once per instance"),
            document.then_some("reach the DOM in a task: of an `Action`, `Ui.on_mount` or `Ui.watch`"),
            writes.then_some("write atoms in an `Action`, which runs on an event"),
            (!writes && !setup && !document && !e.services.is_empty())
                .then_some("run the task as `Atom.task` and read its result"),
            (!e.errors.is_empty()).then_some(
                "handle the failure here, or render with `Ui.suspended`, which gives it a view",
            ),
        ];
        let fix = fixes.into_iter().flatten().collect::<Vec<_>>().join("; ");
                (fix, Vec::new())
            }),
        );
    }
}

/// A row variable and the names it is taken without, if any.
fn less(tail: &Type) -> (&Type, &[Elem]) {
    match tail {
        Type::Without { row, gone } => (row, gone),
        other => (other, &[]),
    }
}

/// The variable of a tail still to be inferred, maybe less some names.
fn unknown(tail: &Type) -> Option<u32> {
    match less(tail).0 {
        Type::Meta(m) => Some(*m),
        _ => None,
    }
}

/// The names `declared` takes out of the variable of `tail` that neither
/// the tail is less too nor `declared` names itself, or `None` when it does
/// not hold the variable: `s` is within `{L | s - L}`, whatever `s` is.
fn owed(declared: &Norm, tail: &Type) -> Option<Vec<Elem>> {
    let (variable, gone) = less(tail);
    (declared.tails.iter().map(less))
        .filter(|(held, _)| *held == variable)
        .map(|(_, taken)| {
            (taken.iter())
                .filter(|l| !gone.contains(l) && declared.get(*l).is_none())
                .cloned()
                .collect::<Vec<_>>()
        })
        .min_by_key(Vec::len)
}

/// Whether `declared` allows all a `tail` stands for.
fn allows(declared: &Norm, tail: &Type) -> bool {
    owed(declared, tail).is_some_and(|names| names.is_empty())
}
