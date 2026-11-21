//! Widening closed (or rigid-tailed) service and error rows in covariant
//! positions when a value fixes an open type variable under subsumption.

use super::solve;
use crate::types::infer::eff;
use crate::types::lang::Lang;
use crate::types::ty::{Effects, Row, Type, Written};
use crate::types::unify::Unified;
use crate::types::variance::Variance;
use crate::types::{Checker, Kind};

impl Checker<'_> {
    /// Binds an open type variable `meta` under subsumption `actual ≤ meta` to
    /// `actual` with closed service/error rows in covariant positions widened by
    /// fresh open row tails, so a later branch or argument needing a service or
    /// error fits regardless of order. Falls back to `None` when nothing widens
    /// or `meta` cannot take the widened type (e.g. numeric meta, occurs check,
    /// inner `forall` escape).
    pub(in crate::types) fn bind_subsumed_meta(
        &mut self,
        actual: &Type,
        meta: &Type,
        bound_live: bool,
    ) -> Option<Unified> {
        if matches!(self.store.resolve(actual), Type::Meta(_)) {
            return None;
        }
        self.inference_trial(|this| {
            let widened = this.widen_for_meta(actual, bound_live).ok_or(())?;
            this.unify(&widened, meta).map_err(|_| ())
        })
        .ok()
        .map(|()| Ok(()))
    }

    /// `ty` with closed (or rigid-tailed) service and error rows in covariant
    /// positions widened by fresh open row tails; `None` when nothing changes.
    /// Contravariant parameters, invariant data (`Ref`, GADTs), opaque aliases,
    /// pure arrows, `@sync` service/error rows, and rows already carrying an
    /// inference tail (including `-S | ?r`) stay untouched.
    pub(in crate::types) fn widen_for_meta(&mut self, ty: &Type, bound_live: bool) -> Option<Type> {
        let mut added = Vec::new();
        let widened = self.widen_at(ty, bound_live, &mut added)?;
        self.cx.widened_rows.extend(added);
        Some(widened)
    }

    fn widen_at(&mut self, ty: &Type, bound_live: bool, added: &mut Vec<Type>) -> Option<Type> {
        if matches!(
            ty,
            Type::Var(_)
                | Type::Skolem(_)
                | Type::Error
                | Type::Forall { .. }
                | Type::Row(_)
                | Type::Union(_, _)
                | Type::Without { .. }
        ) {
            return None;
        }
        // Opaque aliases stay nominal when fixing a type variable.
        if self.projected_alias_type(ty).is_some() {
            return None;
        }
        let head = self.witness_head(ty);
        match head {
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Some(Type::Captured {
                ty: Box::new(self.widen_at(&ty, bound_live, added)?),
                captures,
                value,
                elided,
            }),
            Type::Bounded { ty, captures } => Some(Type::Bounded {
                ty: Box::new(self.widen_at(&ty, bound_live, added)?),
                captures,
            }),
            Type::Fun {
                param,
                effects,
                result,
            } => self.widen_fun(param, effects, result, bound_live, added),
            Type::Tuple(items) => self.widen_tuple(items, added),
            Type::Record(row) if row.tail.is_none() => self.widen_record(row, added),
            other @ Type::App { .. } => self.widen_app(other, bound_live, added),
            _ => None,
        }
    }

    fn widen_fun(
        &mut self,
        param: Box<Type>,
        effects: Option<Box<Effects>>,
        result: Box<Type>,
        bound_live: bool,
        added: &mut Vec<Type>,
    ) -> Option<Type> {
        let next_result = self.widen_at(&result, false, added);
        let next_effects = match effects.as_deref() {
            None => None,
            Some(e) if e.sync => bound_live.then(|| {
                Box::new(Effects {
                    services: self.with_live_row(&clear_written(&e.services)),
                    errors: clear_written(&e.errors),
                    sync: true,
                    direct: e.direct,
                    written: Written::default(),
                })
            }),
            Some(e) => {
                let services = self.widen_row(&e.services, &Kind::ServiceRow, added);
                let errors = self.widen_row(&e.errors, &Kind::ErrorRow, added);
                let live_services = (services.is_none() && bound_live)
                    .then(|| self.with_live_row(&clear_written(&e.services)));
                if services.is_some() || live_services.is_some() || errors.is_some() {
                    Some(Box::new(Effects {
                        services: services
                            .or(live_services)
                            .unwrap_or_else(|| clear_written(&e.services)),
                        errors: errors.unwrap_or_else(|| clear_written(&e.errors)),
                        sync: false,
                        direct: false,
                        written: Written::default(),
                    }))
                } else {
                    None
                }
            }
        };
        if next_effects.is_none() && next_result.is_none() {
            return None;
        }
        let effects = next_effects.or_else(|| {
            effects.map(|mut e| {
                e.services = clear_written(&e.services);
                e.errors = clear_written(&e.errors);
                e.written = Written::default();
                e
            })
        });
        Some(Type::Fun {
            param,
            effects,
            result: Box::new(next_result.unwrap_or(*result)),
        })
    }

    fn widen_tuple(&mut self, items: Vec<Type>, added: &mut Vec<Type>) -> Option<Type> {
        let mut changed = false;
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            match self.widen_at(&item, false, added) {
                Some(widened) => {
                    changed = true;
                    out.push(widened);
                }
                None => out.push(item),
            }
        }
        changed.then_some(Type::Tuple(out))
    }

    fn widen_record(&mut self, row: Row, added: &mut Vec<Type>) -> Option<Type> {
        let mut changed = false;
        let mut entries = Vec::with_capacity(row.entries.len());
        for (label, field) in row.entries {
            match self.widen_at(&field, false, added) {
                Some(widened) => {
                    changed = true;
                    entries.push((label, widened));
                }
                None => entries.push((label, field)),
            }
        }
        changed.then(|| {
            Type::Record(Row {
                entries,
                tail: None,
                written: Written::default(),
            })
        })
    }

    fn widen_app(&mut self, other: Type, bound_live: bool, added: &mut Vec<Type>) -> Option<Type> {
        if let Some((s, e, r)) = self.eff_parts(&other) {
            let s2 = self.widen_row(&s, &Kind::ServiceRow, added);
            let e2 = self.widen_row(&e, &Kind::ErrorRow, added);
            let r2 = self.widen_at(&r, false, added);
            let live_s =
                (s2.is_none() && bound_live).then(|| self.with_live_row(&clear_written(&s)));
            if s2.is_some() || live_s.is_some() || e2.is_some() || r2.is_some() {
                return Some(eff(
                    s2.or(live_s).unwrap_or_else(|| clear_written(&s)),
                    e2.unwrap_or_else(|| clear_written(&e)),
                    r2.unwrap_or(r),
                ));
            }
            return None;
        }
        if let Some((inputs, errors, outputs)) = self.layer_parts(&other) {
            let i2 = self.widen_row(&inputs, &Kind::ServiceRow, added);
            let e2 = self.widen_row(&errors, &Kind::ErrorRow, added);
            if i2.is_some() || e2.is_some() {
                return Some(Type::apps(
                    Type::lang(Lang::Layer),
                    [
                        i2.unwrap_or_else(|| clear_written(&inputs)),
                        e2.unwrap_or_else(|| clear_written(&errors)),
                        outputs,
                    ],
                ));
            }
            return None;
        }
        // Peel single-argument covariant chains (`List (List ...)` etc.)
        // iteratively so deeply nested types never grow the call stack.
        let mut chain: Vec<Type> = Vec::new();
        let mut cur = other;
        let leaf = loop {
            let (head, args) = cur.spine();
            let Type::Con { module, name } = head else {
                return None;
            };
            let signs = name
                .strip_suffix("$Dict")
                .and_then(|class| self.class_signs(module, class))
                .or_else(|| self.signs(module, name))?;
            if signs.len() != args.len() {
                return None;
            }
            if args.len() == 1 && signs[0] == Variance::Covariant {
                let head = head.clone();
                let arg = args[0].clone();
                chain.push(head);
                if self.projected_alias_type(&arg).is_some() {
                    return None;
                }
                let next = self.witness_head(&arg);
                if matches!(next, Type::App { .. })
                    && self.eff_parts(&next).is_none()
                    && self.layer_parts(&next).is_none()
                {
                    cur = next;
                    continue;
                }
                break self.widen_at(&next, false, added)?;
            }
            let head = head.clone();
            let args: Vec<Type> = args.into_iter().cloned().collect();
            let mut changed = false;
            let mut out = Vec::with_capacity(args.len());
            for (arg, sign) in args.into_iter().zip(signs.iter()) {
                match sign {
                    Variance::Covariant => match self.widen_at(&arg, false, added) {
                        Some(widened) => {
                            changed = true;
                            out.push(widened);
                        }
                        None => out.push(arg),
                    },
                    Variance::Contravariant | Variance::Invariant | Variance::Bivariant => {
                        out.push(arg);
                    }
                }
            }
            if !changed {
                return None;
            }
            break Type::apps(head, out);
        };
        Some(
            chain
                .into_iter()
                .rev()
                .fold(leaf, |inner, head| Type::apps(head, [inner])),
        )
    }

    /// Adds a fresh open tail to a service or error row that has no inference
    /// tail yet; `None` when the row already has an open tail (such as `?r` or
    /// `-Log | ?r`) or an error marker.
    fn widen_row(&mut self, row: &Type, kind: &Kind, added: &mut Vec<Type>) -> Option<Type> {
        let mut norm = self.store.normalize(row);
        if norm
            .tails
            .iter()
            .any(|t| matches!(t, Type::Error) || solve::unknown_tail(t).is_some())
        {
            return None;
        }
        let tail = self.store.fresh_widened_row(kind);
        added.push(tail.clone());
        norm.tails.push(tail);
        Some(norm.into_type())
    }
}

/// `t` without `Written` source-edit provenance on its rows and arrows.
pub(in crate::types) fn clear_written(t: &Type) -> Type {
    match t {
        Type::Fun {
            param,
            effects,
            result,
        } => Type::Fun {
            param: Box::new(clear_written(param)),
            effects: effects.as_ref().map(|e| {
                Box::new(Effects {
                    services: clear_written(&e.services),
                    errors: clear_written(&e.errors),
                    sync: e.sync,
                    direct: e.direct,
                    written: Written::default(),
                })
            }),
            result: Box::new(clear_written(result)),
        },
        Type::Row(row) => Type::Row(Row {
            entries: row
                .entries
                .iter()
                .map(|(l, t)| (l.clone(), clear_written(t)))
                .collect(),
            tail: row.tail.as_ref().map(|t| Box::new(clear_written(t))),
            written: Written::default(),
        }),
        _ => t.map_children(&mut clear_written),
    }
}
