//! Solving `actual ⊆ declared` between rows. A declared row that is still
//! unknown grows by the labels and row variables flowing into it; what stays
//! unknown at the end is the empty row, the least solution. Then every
//! constraint either holds or names what its bound lacks.

use super::Site;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Norm, Row, Type};
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
}

/// What one bound lacks.
struct Excess {
    site: usize,
    span: Span,
    services: Vec<String>,
    errors: Vec<String>,
}

impl Checker<'_> {
    /// Grows unknown declared rows until nothing changes.
    pub fn grow(&mut self) {
        loop {
            // `|`, not `||`: every row grows in each round
            let rows = (0..self.subrows.len()).fold(false, |changed, i| changed | self.grow_one(i));
            if !(self.grow_holds() || rows) {
                break;
            }
        }
    }

    /// The actual row of constraint `i` without what its scope provides.
    fn actual(&mut self, i: usize) -> Norm {
        let mut actual = self.store.normalize(&self.subrows[i].actual);
        let Some(region) = self.subrows[i].provides.clone() else {
            return actual;
        };
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
        let actual = self.actual(i);
        let declared = self.store.normalize(&self.subrows[i].declared);
        let open = declared.tails.iter().find_map(|t| match t {
            Type::Meta(m) => Some(*m),
            _ => None,
        });
        for (label, ty) in &actual.entries {
            match declared.get(label) {
                Some(bound) => {
                    let bound = bound.clone();
                    let _ = self.unify(ty, &bound);
                }
                None => {
                    if let Some(m) = open {
                        let rest = self.store.fresh_beside(m);
                        let row = Type::Row(Row {
                            entries: vec![(label.clone(), ty.clone())],
                            tail: Some(Box::new(rest)),
                        });
                        return self.bind(m, &row).is_ok();
                    }
                }
            }
        }
        for tail in &actual.tails {
            if matches!(tail, Type::Meta(_) | Type::Error) || declared.tails.contains(tail) {
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
        for c in &self.subrows {
            for t in self.store.normalize(&c.actual).tails {
                if let Type::Meta(m) = t
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
        for c in &self.subrows {
            for side in [&c.actual, &c.declared] {
                for t in self.store.normalize(side).tails {
                    if let Type::Meta(m) = t {
                        open.push(m);
                    }
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
        for i in 0..self.subrows.len() {
            let actual = self.actual(i);
            let c = self.subrows[i].clone();
            let declared = self.store.normalize(&c.declared);
            // a declared element that is no error is reported as such, once
            if c.errors && declared.entries.iter().any(|(_, t)| !self.may_be_error(t)) {
                continue;
            }
            let mut lacking = Vec::new();
            for (label, ty) in &actual.entries {
                match declared.get(label) {
                    Some(bound) => {
                        if let Err(m) = self.unify(ty, bound) {
                            self.mismatch(&m, ty, bound, c.span);
                        }
                    }
                    None if label == regions::scope() => self.no_owner(c.span),
                    None if label.starts_with(regions::PROVIDED) => {
                        if let [service, owner] = ty.spine().1[..] {
                            let service = self.show(service);
                            let owner = self.show(owner);
                            self.unprovided(service.trim_matches(['{', '}', ' ']), &owner, c.span);
                        }
                    }
                    None => lacking.push(self.show(ty)),
                }
            }
            for tail in &actual.tails {
                if !matches!(tail, Type::Error) && !declared.tails.contains(tail) {
                    lacking.push(self.show(tail));
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
                });
            }
            let i = excess
                .iter()
                .position(|e| e.site == c.site)
                .expect("just added");
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
        }
        self.subrows.clear();
        for e in excess {
            self.report_excess(&e);
        }
    }

    /// A resource acquired where no `scope` owns it.
    fn no_owner(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                codes::G004,
                "this acquires a resource, which needs `Scope`: only `scope { … }` provides an owner",
                span,
            ).with_fix(|| ("call it inside `scope { … }`, or add `Scope s` to the services of the signature", Vec::new())),
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
        if let Site::Expected(ty) = &self.sites[e.site]
            && self.reads_only(&ty.clone())
        {
            return self.render_excess(e, &list);
        }
        if let Site::Expected(ty) = &self.sites[e.site]
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
        let (bound, needer) = match &self.sites[e.site] {
            Site::Signature(name) => (format!("the signature of `{name}`"), "its body"),
            Site::Expected(ty) => (format!("the expected type `{}`", self.show(ty)), "this"),
            Site::Inferred => ("the type of this function".to_owned(), "its body"),
        };
        let mut diagnostic = Diagnostic::error(
            codes::E002,
            format!("{bound} does not allow {list}, which {needer} needs"),
            e.span,
        );
        if matches!(self.sites[e.site], Site::Signature(_)) {
            diagnostic = diagnostic.with_fix(|| (format!("add {list} to {bound}"), Vec::new()));
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
        let writes = e
            .services
            .iter()
            .any(|s| [Lang::AtomWrite, Lang::AtomRegistry].into_iter().any(|l| lang::label(l) == Some(s.as_str())));
        let setup = e.services.iter().any(|s| s.contains("Setup"));
        let document = e.services.iter().any(|s| s.contains("Document"));
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
