//! Regions are checked by use: a closure or a task gets no qualifier for
//! the locals it holds. What it uses of a region
//! shows in its rows as `Live C`, an element of a service row no program
//! can write, so a `scope` keeps out every value that would use its region
//! after it ends, and lets out one that only holds it. The reference model
//! checks the same discipline (`flick-model/src/check/regions/live.rs`).
//!
//! - A runtime arrow of a signature needs `Live` of what its parameters, and
//!   the parameters of the stages before it, reach (`elide`).
//! - Calling a function or awaiting a task qualified `^C` needs `Live C`
//!   (`live_call`, `live_await`); one checked against `^C` may need it.
//! - `scope s` discharges `Live` of `s`; a row whose bound lacks a region
//!   it needs live, neither in its `Live` nor its `Scope`, is `FLICK-G002`;
//!   work an owner keeps needs every region of its `Live` to outlive the
//!   owner, `FLICK-G003` (`live_rows.rs`).

use super::{AliasKey, Unfold, leave_alias};
use crate::types::Checker;
use crate::types::Elem;
use crate::types::effects::Stage;
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::{Effects, Norm, Row, Type};

/// `t` without `Live`: as messages, interfaces and the emitted code show it,
/// since every use of a signature elides it again.
pub fn strip_live(t: &Type) -> Type {
    match t {
        // a row of `Live` alone and a tail is the tail, not `{_}`
        Type::Row(row)
            if !row.entries.is_empty()
                && row.entries.iter().all(|(l, _)| matches!(l, Elem::Live))
                && row.tail.is_some() =>
        {
            strip_live(row.tail.as_deref().expect("a tail"))
        }
        Type::Row(row) if row.entries.iter().any(|(l, _)| matches!(l, Elem::Live)) => {
            Type::Row(Row {
                entries: (row.entries.iter())
                    .filter(|(l, _)| !matches!(l, Elem::Live))
                    .map(|(l, t)| (l.clone(), strip_live(t)))
                    .collect(),
                tail: row.tail.as_ref().map(|t| Box::new(strip_live(t))),
                written: row.written,
            })
        }
        _ => t.map_children(&mut |c| strip_live(c)),
    }
}

/// Whether `t` has a `Live`.
pub fn has_live(t: &Type) -> bool {
    t.contains(&mut |t| matches!(t, Type::Row(r) if r.entries.iter().any(|(l, _)| matches!(l, Elem::Live))))
}

/// Whether the row `r` needs the skolem `s` live.
fn row_needs(r: &Type, s: u32) -> bool {
    matches!(r, Type::Row(r) if (r.entries.iter())
        .any(|(l, c)| matches!(l, Elem::Live) && c.contains(&mut |a| *a == Type::Skolem(s))))
}

/// `services` that also need `atoms` live.
pub(super) fn add_live(services: &Type, atoms: Norm) -> Type {
    if atoms.entries.is_empty() && atoms.tails.is_empty() {
        return services.clone();
    }
    let live = (Elem::Live, atoms.into_type());
    match services {
        Type::Row(row) => {
            let mut row = row.clone();
            let at = row.entries.partition_point(|(l, _)| *l < Elem::Live);
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

pub(super) fn merge(into: &mut Norm, atoms: Norm) {
    into.entries.extend(atoms.entries);
    Row::sort_entries(&mut into.entries);
    into.entries.dedup_by(|a, b| a.0 == b.0);
    for t in atoms.tails {
        if !into.tails.contains(&t) {
            into.tails.push(t);
        }
    }
}

impl Checker<'_> {
    /// Whether a value of `t` needs the region `skolem` live.
    pub(in crate::types) fn needs_live(&self, t: &Type, skolem: u32) -> bool {
        self.zonk(t).contains(&mut |r| row_needs(r, skolem))
    }

    /// What of the capture row `captures` a stage needs alive: its regions
    /// and capture rows, and what is not known yet. A type variable is no
    /// use of what it stands for: generic code cannot take it apart.
    pub(super) fn live_atoms(&self, captures: &Type, kinds: &[(String, Kind)]) -> Norm {
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
    pub(super) fn live_in(&self, services: &Norm) -> Norm {
        let mut out = Norm::default();
        for (label, ty) in &services.entries {
            if matches!(label, Elem::Live) {
                merge(&mut out, self.live_atoms(ty, &[]));
            }
        }
        out
    }

    /// What `function` or a task needs live to be used: what its qualifier
    /// holds, but the region of `resume` in arms that run at this stage.
    fn qualifier_live(&self, t: &Type) -> Option<Norm> {
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
            .is_some_and(|scope| scope.handler && scope.outer == self.cx.stage.services))
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

    /// A signature as its body and its uses see it: each runtime arrow needs
    /// live what the parameters before it reach. An interface leaves `Live`
    /// out, which elision gives again; a type elided already is elided anew,
    /// not twice.
    pub(in crate::types) fn live_scheme(&self, t: &Type) -> Type {
        self.elide(
            &strip_live(t),
            &Norm::default(),
            &mut Vec::new(),
            &mut Vec::new(),
        )
    }

    /// `t` with `Live` of `held` and of what the parameters of each stage
    /// reach in the row of every runtime arrow and `Eff` after them.
    /// `path` holds the aliases it is unfolding: one met again inside its
    /// own unfolding stays as written.
    fn elide(
        &self,
        t: &Type,
        held: &Norm,
        kinds: &mut Vec<(String, Kind)>,
        path: &mut Vec<AliasKey>,
    ) -> Type {
        let none = Norm::default();
        match t {
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let mark = kinds.len();
                kinds.extend(vars.iter().map(|v| (v.name.clone(), v.kind.clone())));
                let body = self.elide(body, held, kinds, path);
                kinds.truncate(mark);
                Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints.clone(),
                    body: Box::new(body),
                }
            }
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Type::Captured {
                ty: Box::new(self.elide(ty, held, kinds, path)),
                captures: captures.clone(),
                value: *value,
                elided: *elided,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.elide(ty, held, kinds, path)),
                captures: captures.clone(),
            },
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = self.elide(param, &none, kinds, path);
                let mut held = held.clone();
                let mut reached = self.parameter_captures(&param);
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
                    result: Box::new(self.elide(result, &held, kinds, path)),
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
                            self.elide(result, &none, kinds, path),
                        ],
                    );
                }
                let unfold = self.enter_alias(other, path);
                if matches!(unfold, Unfold::Again) {
                    return other.clone();
                }
                let expanded = self.qualified_head(other);
                let elided = if matches!(expanded, Type::Fun { .. }) {
                    self.elide(&expanded, held, kinds, path)
                } else {
                    other.map_children(&mut |c| self.elide(c, &none, kinds, path))
                };
                leave_alias(path, unfold);
                elided
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
                .any(|(_, a)| matches!(a, Type::Skolem(s) if self.cx.regions.lent.contains(s)))
        })
    }
}
