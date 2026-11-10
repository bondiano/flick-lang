//! The live discipline of regions, `[build] regions = "live"`, beside the
//! discipline of captures the rest of this module checks: a closure or a
//! task gets no qualifier for the locals it holds. What it uses of a region
//! shows in its rows as `Live C`, an element of a service row no program
//! can write, so a `scope` keeps out every value that would use its region
//! after it ends, and lets out one that only holds it. The reference model
//! checks the same discipline (`flick-model/src/check/regions/live.rs`).
//!
//! - A runtime arrow of a signature needs `Live` of what its parameters, and
//!   the parameters of the stages before it, reach (`elide`).
//! - Calling a function or awaiting a task qualified `^C` needs `Live C`
//!   (`live_call`, `live_await`); one checked against `^C` may need it.
//! - `scope s` discharges `Live` of `s` (`discharge_live`); a row whose
//!   bound lacks a region it needs live is `FLICK-G002` (`report_live`).
//! - Work an owner keeps needs every region of its `Live` to outlive the
//!   owner (`keep_live`, `FLICK-G003`).

use super::Outlives;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::effects::{Site, Stage};
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::{Effects, Norm, Row, Type};

/// How the checker decides what a value may take out of a `scope`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionMode {
    /// A closure or a task is qualified by what it holds (`docs/formal/regions.md`).
    #[default]
    Captures,
    /// A region is live where a row needs it: experimental.
    Live,
}

/// The label of `Live C` in a service row; no written row has a space in a label.
pub const LIVE: &str = "Fiber.Live ";

/// `t` without `Live`: as messages, interfaces and the emitted code show it,
/// since every use of a signature elides it again.
pub fn strip_live(t: &Type) -> Type {
    match t {
        Type::Row(row) if row.entries.iter().any(|(l, _)| l == LIVE) => Type::Row(Row {
            entries: (row.entries.iter())
                .filter(|(l, _)| l != LIVE)
                .map(|(l, t)| (l.clone(), strip_live(t)))
                .collect(),
            tail: row.tail.as_ref().map(|t| Box::new(strip_live(t))),
            written: row.written,
        }),
        _ => t.map_children(&mut |c| strip_live(c)),
    }
}

/// Whether `t` has a `Live`.
pub fn has_live(t: &Type) -> bool {
    t.contains(&mut |t| matches!(t, Type::Row(r) if r.entries.iter().any(|(l, _)| l == LIVE)))
}

/// `services` that also need `atoms` live.
fn add_live(services: &Type, atoms: Norm) -> Type {
    if atoms.entries.is_empty() && atoms.tails.is_empty() {
        return services.clone();
    }
    let live = (LIVE.to_owned(), atoms.into_type());
    match services {
        Type::Row(row) => {
            let mut row = row.clone();
            let at = row.entries.partition_point(|(l, _)| l.as_str() < LIVE);
            row.entries.insert(at, live);
            Type::Row(row)
        }
        other => Type::Row(Row {
            entries: vec![live],
            tail: Some(Box::new(other.clone())),
            ..Row::default()
        }),
    }
}

/// Whether `captures` is what `stages` gives a later stage of a signature
/// checked under captures, which starts from the empty row.
fn implied(captures: &Type) -> bool {
    match captures {
        Type::Union(left, _) => {
            matches!(&**left, Type::Row(r) if r.entries.is_empty() && r.tail.is_none())
                || implied(left)
        }
        _ => false,
    }
}

fn merge(into: &mut Norm, atoms: Norm) {
    into.entries.extend(atoms.entries);
    into.entries.sort_by(|a, b| a.0.cmp(&b.0));
    into.entries.dedup_by(|a, b| a.0 == b.0);
    for t in atoms.tails {
        if !into.tails.contains(&t) {
            into.tails.push(t);
        }
    }
}

impl Checker<'_> {
    pub(in crate::types) fn live(&self) -> bool {
        self.options.regions == RegionMode::Live
    }

    /// What of the capture row `captures` a stage needs alive: its regions
    /// and capture rows, and what is not known yet. A type variable is no
    /// use of what it stands for: generic code cannot take it apart.
    fn live_atoms(&self, captures: &Type, kinds: &[(String, Kind)]) -> Norm {
        let mut all = Norm::default();
        self.capture_row(captures, &mut all);
        let region = |t: &Type| {
            matches!(self.store.resolve(t), Type::Meta(_))
                || matches!(
                    self.atom_kind(t, kinds),
                    Some(Kind::Region | Kind::CaptureRow)
                )
        };
        all.entries.retain(|(_, t)| region(t));
        all.tails.retain(|t| matches!(t, Type::Meta(_)));
        all
    }

    /// The regions the service row `services` needs live.
    fn live_in(&self, services: &Norm) -> Norm {
        let mut out = Norm::default();
        for (label, ty) in &services.entries {
            if label == LIVE {
                merge(&mut out, self.live_atoms(ty, &[]));
            }
        }
        out
    }

    /// What `function` or a task needs live to be used: what its qualifier
    /// holds, but the region of `resume` in arms that run at this stage.
    fn qualifier_live(&self, t: &Type) -> Option<Norm> {
        if !self.live() {
            return None;
        }
        self.atoms_live(&self.whnf_captured(t).1?)
    }

    fn atoms_live(&self, captures: &Type) -> Option<Norm> {
        let mut atoms = self.live_atoms(captures, &[]);
        atoms.entries.retain(|(_, a)| !self.arm_region(a));
        (!atoms.entries.is_empty() || !atoms.tails.is_empty()).then_some(atoms)
    }

    /// What a value checked against `expected` may use: of value bounds
    /// `(τ^C₁)^C₂` the innermost, the capacity its storage keeps, so the
    /// cell `Ref.set` takes gets no region its own capacity lacks.
    fn expected_live(&self, expected: &Type) -> Option<Norm> {
        if !self.live() {
            return None;
        }
        let mut t = self.qualified_head(expected);
        let mut innermost = None;
        for _ in 0..64 {
            match t {
                Type::Bounded { ty, captures } => {
                    innermost = Some(*captures);
                    t = self.qualified_head(&ty);
                }
                Type::Captured { ty, .. } => t = self.qualified_head(&ty),
                _ => break,
            }
        }
        match innermost {
            Some(captures) => self.atoms_live(&captures),
            None => self.qualifier_live(expected),
        }
    }

    /// The region of `resume` of a handler whose arms run at this stage.
    fn arm_region(&self, atom: &Type) -> bool {
        matches!(self.store.resolve(atom), Type::Skolem(s) if self.scope_of(s)
            .is_some_and(|scope| scope.handler && scope.outer == self.stage.services))
    }

    /// The rows a call of `function` needs: those of its arrow and, under
    /// this discipline, `Live` of its qualifier.
    pub(in crate::types) fn live_call(&self, function: &Type, effects: &Effects) -> Effects {
        let mut out = effects.clone();
        if let Some(atoms) = self.qualifier_live(function) {
            out.services = add_live(&out.services, atoms);
        }
        out
    }

    /// The services awaiting `task` needs: those of its `Eff`, and `Live`
    /// of its qualifier.
    pub(in crate::types) fn live_await(&self, task: &Type, services: Type) -> Type {
        match self.qualifier_live(task) {
            Some(atoms) => add_live(&services, atoms),
            None => services,
        }
    }

    /// The stage of a function or a `task` checked against `expected`: what
    /// its qualifier holds may be live in it.
    pub(in crate::types) fn live_bound(&self, stage: &mut Stage, expected: Option<&Type>) {
        if stage.pure.is_some() {
            return;
        }
        if let Some(atoms) = expected.and_then(|e| self.expected_live(e)) {
            stage.services = add_live(&stage.services, atoms);
        }
    }

    /// A signature as its body and its uses see it: under captures a later
    /// stage holds what the parameters before it hold (`implicit_captures`);
    /// under this discipline its arrow needs them live instead.
    pub(in crate::types) fn signature_stages(&self, t: &Type) -> Type {
        self.live_scheme(&self.implicit_captures(t))
    }

    /// The type of a value of another module at a use: its interface leaves
    /// out `Live`, which elision gives again.
    pub(in crate::types) fn live_scheme(&self, t: &Type) -> Type {
        if self.live() {
            self.elide(t, &Norm::default(), &mut Vec::new())
        } else {
            t.clone()
        }
    }

    /// `t` with `Live` of `held` and of what the parameters of each stage
    /// reach in the row of every runtime arrow and `Eff` after them.
    fn elide(&self, t: &Type, held: &Norm, kinds: &mut Vec<(String, Kind)>) -> Type {
        let none = Norm::default();
        match t {
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let mark = kinds.len();
                kinds.extend(vars.iter().map(|v| (v.name.clone(), v.kind.clone())));
                let body = self.elide(body, held, kinds);
                kinds.truncate(mark);
                Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints.clone(),
                    body: Box::new(body),
                }
            }
            // what a stage holds of the parameters before it is `Live` here
            Type::Captured { ty, captures, .. } if implied(captures) => self.elide(ty, held, kinds),
            Type::Captured {
                ty,
                captures,
                value,
            } => Type::Captured {
                ty: Box::new(self.elide(ty, held, kinds)),
                captures: captures.clone(),
                value: *value,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.elide(ty, held, kinds)),
                captures: captures.clone(),
            },
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = self.elide(param, &none, kinds);
                let mut held = held.clone();
                let mut reached = self.captures_of(&param);
                reached.entries.retain(|(_, atom)| {
                    matches!(
                        self.atom_kind(atom, kinds),
                        Some(Kind::Region | Kind::CaptureRow)
                    )
                });
                merge(&mut held, reached);
                let effects = effects.as_ref().map(|e| {
                    Box::new(Effects {
                        services: add_live(&e.services, held.clone()),
                        ..(**e).clone()
                    })
                });
                Type::Fun {
                    param: Box::new(param),
                    effects,
                    result: Box::new(self.elide(result, &held, kinds)),
                }
            }
            other => {
                let (eff, args) = other.spine();
                if let [services, errors, result] = args[..]
                    && eff.is_lang(Lang::Eff)
                {
                    return Type::apps(
                        eff.clone(),
                        [
                            add_live(services, held.clone()),
                            errors.clone(),
                            self.elide(result, &none, kinds),
                        ],
                    );
                }
                let expanded = self.qualified_head(other);
                if matches!(expanded, Type::Fun { .. }) {
                    self.elide(&expanded, held, kinds)
                } else {
                    other.map_children(&mut |c| self.elide(c, &none, kinds))
                }
            }
        }
    }

    /// The types of a subsumption with a qualifier: using either side needs
    /// its qualifier live, so a qualifier does not hide `Live`.
    pub(in crate::types) fn live_sides(
        &self,
        (a, ca): (Type, Option<&Type>),
        (e, ce): (Type, Option<&Type>),
    ) -> (Type, Type) {
        if !self.live() {
            return (a, e);
        }
        let side = |t: Type, c: Option<&Type>| {
            let Some(atoms) = c.map(|c| self.live_atoms(c, &[])) else {
                return t;
            };
            match t {
                Type::Fun {
                    param,
                    effects: Some(effects),
                    result,
                } => Type::Fun {
                    param,
                    effects: Some(Box::new(Effects {
                        services: add_live(&effects.services, atoms),
                        ..*effects
                    })),
                    result,
                },
                other => match self.eff_parts(&other) {
                    Some((s, e, r)) => super::super::infer::eff(add_live(&s, atoms), e, r),
                    None => other,
                },
            }
        };
        (side(a, ca), side(e, ce))
    }

    /// Whether a closure that holds `held` holds the reader of a lazy `for`.
    pub(in crate::types) fn holds_reader(&self, held: &[(Option<String>, Type)]) -> bool {
        held.iter().any(|(_, t)| {
            (self.captures_of(t).entries.iter())
                .any(|(_, a)| matches!(a, Type::Skolem(s) if self.regions.lent.contains(s)))
        })
    }

    /// Unification of service rows: `Live` that one side has and the other,
    /// a closed row, lacks, is left out while it needs no region known yet,
    /// as of a cell's capacity a generic signature names.
    pub(in crate::types) fn unify_live(&self, left: &mut Norm, right: &mut Norm) {
        let lacks =
            |n: &Norm| n.get(LIVE).is_none() && !n.tails.iter().any(|t| matches!(t, Type::Meta(_)));
        let unknown = |this: &Self, t: &Type| {
            (this.live_atoms(t, &[]).entries.iter())
                .all(|(_, a)| matches!(this.store.resolve(a), Type::Meta(_)))
        };
        if lacks(right) {
            left.entries.retain(|(l, t)| l != LIVE || !unknown(self, t));
        }
        if lacks(left) {
            right
                .entries
                .retain(|(l, t)| l != LIVE || !unknown(self, t));
        }
    }

    /// The end of `scope` of `region`: its region is live no further.
    pub(in crate::types) fn discharge_live(&self, actual: &mut Norm, region: &Type) {
        if !actual.entries.iter().any(|(l, _)| l == LIVE) {
            return;
        }
        let region = self.store.resolve(region);
        let mut live = self.live_in(actual);
        actual.entries.retain(|(l, _)| l != LIVE);
        live.entries
            .retain(|(_, a)| self.store.resolve(a) != region);
        if !live.entries.is_empty() || !live.tails.is_empty() {
            let at = actual.entries.partition_point(|(l, _)| l.as_str() < LIVE);
            actual
                .entries
                .insert(at, (LIVE.to_owned(), live.into_type()));
        }
    }

    /// The regions `actual` needs live that `declared` does not.
    fn live_missing(&self, actual: &Norm, declared: &Norm) -> Vec<(String, Type)> {
        let (mine, theirs) = (self.live_in(actual), self.live_in(declared));
        mine.entries
            .into_iter()
            .filter(|(label, _)| theirs.get(label).is_none())
            .collect()
    }

    /// Grows what `declared` needs live by what `actual` needs and it lacks:
    /// a qualifier not inferred yet in its `Live`, else its open row `open`.
    pub(in crate::types) fn grow_live(
        &mut self,
        actual: &Norm,
        declared: &Norm,
        open: Option<u32>,
    ) -> bool {
        let missing = self.live_missing(actual, declared);
        if missing.is_empty() {
            return false;
        }
        let inner = (declared.entries.iter())
            .filter(|(l, _)| l == LIVE)
            .find_map(|(_, t)| {
                self.store
                    .normalize(t)
                    .tails
                    .into_iter()
                    .find_map(|t| match t {
                        Type::Meta(m) => Some(m),
                        _ => None,
                    })
            });
        let (m, grown) = match (inner, open) {
            (Some(m), _) => {
                let rest = self.store.fresh_beside(m);
                let row = Row {
                    entries: missing,
                    tail: Some(Box::new(rest)),
                    ..Row::default()
                };
                (m, Type::Row(row))
            }
            (None, Some(m)) => {
                let rest = self.store.fresh_beside(m);
                let live = Type::Row(Row::closed(missing));
                let row = Row {
                    entries: vec![(LIVE.to_owned(), live)],
                    tail: Some(Box::new(rest)),
                    ..Row::default()
                };
                (m, Type::Row(row))
            }
            (None, None) => return false,
        };
        self.bind(m, &grown).is_ok()
    }

    /// Once rows are solved: a region `actual` needs live and `declared`, the
    /// bound of the stage at `site`, does not keep live (`FLICK-G002`).
    pub(in crate::types) fn report_live(
        &mut self,
        actual: &Norm,
        declared: &Norm,
        site: usize,
        span: Span,
    ) {
        if declared.tails.contains(&Type::Error) {
            return;
        }
        let missing = self.live_missing(actual, declared);
        let Some(atom) = (missing.iter())
            .map(|(_, a)| self.store.resolve(a))
            .find(|a| !matches!(a, Type::Meta(_)))
        else {
            return;
        };
        let reported = |d: &Diagnostic| d.code == codes::G002 && d.span == Some(span);
        if self.diagnostics.iter().any(reported) {
            return;
        }
        let (region, scope) = self.describe(&atom);
        let bound = match &self.sites[site] {
            Site::Signature(name, _) => format!("the signature of `{name}`"),
            Site::Expected(ty, _) => format!("the expected type `{}`", self.show(ty)),
            Site::Inferred => "the type of this function".to_owned(),
        };
        let mut diagnostic = Diagnostic::error(
            codes::G002,
            format!(
                "this uses {region}, which {bound} does not keep live: a function or a task uses a region only while the region is live"
            ),
            span,
        );
        if let Some(scope) = scope {
            diagnostic = diagnostic.with_related(scope, "the region's scope ends here");
        }
        self.diagnostics.push(diagnostic.with_fix(|| {
            (
                "use the value inside the region's `scope`, or name the region in a parameter of the signature",
                Vec::new(),
            )
        }));
    }

    /// `Live` of the row of work an owner keeps (`Fiber.Provided`): each
    /// region it needs outlives the owner (`Outlives`, `FLICK-G003`).
    pub(in crate::types) fn keep_live(
        &mut self,
        live: &Type,
        owner: &Type,
        givens: &[Type],
        span: Span,
        last: bool,
    ) {
        let o = Outlives {
            captures: live.clone(),
            owner: owner.clone(),
            givens: givens.to_vec(),
            span,
        };
        if !last {
            self.regions.outlives.push(o);
        } else if !(self.diagnostics.iter()).any(|d| d.code == codes::G003 && d.span == Some(span))
        {
            self.check_outlives(&o);
        }
    }
}
