//! Capture reachability and stages of checked types.

use super::{AliasKey, HOLD_NOTHING, Unfold, leave_alias, push_atom, qualify, without_rows};
use crate::types::lang::Lang;
use crate::types::{
    Checker, Elem, Elided,
    kinds::Kind,
    ty::{Norm, Type},
};

impl Checker<'_> {
    /// What a value of `t` may hold, as a capture row: regions and type
    /// variables as entries, capture rows not inferred yet as tails. A
    /// qualifier bounds abstract reachable captures; concrete region
    /// dependencies remain visible even beneath a bound.
    pub fn captures_of(&self, t: &Type) -> Norm {
        let mut out = Norm::default();
        self.collect_captures(t, &mut out);
        out.sort();
        out.entries.dedup_by(|a, b| a.0 == b.0);
        out
    }

    pub(super) fn collect_captures(&self, t: &Type, out: &mut Norm) {
        self.collect_bounded_captures(t, out, false, &mut Vec::new());
    }

    /// `path` holds the aliases the walk is unfolding: one met again inside
    /// its own unfolding holds what its arguments hold.
    pub(super) fn collect_bounded_captures(
        &self,
        t: &Type,
        out: &mut Norm,
        bounded: bool,
        path: &mut Vec<AliasKey>,
    ) {
        match self.enter_alias(t, path) {
            Unfold::Again => self.applied_captures(t, out, bounded, path, false),
            unfold => {
                self.collect_unfolded(t, out, bounded, path);
                leave_alias(path, unfold);
            }
        }
    }

    pub(super) fn collect_unfolded(
        &self,
        t: &Type,
        out: &mut Norm,
        bounded: bool,
        path: &mut Vec<AliasKey>,
    ) {
        let t = self.borrowed_head(t);
        match &*t {
            Type::Bounded { ty, captures } => {
                if self.holds_nothing(ty) {
                    return;
                }
                self.capture_row_filtered(captures, out, false, path);
                self.collect_bounded_captures(ty, out, true, path);
            }
            Type::Captured { ty, captures, .. } => {
                if self.holds_nothing(ty) {
                    return;
                }
                self.capture_row_filtered(captures, out, bounded, path);
                self.collect_bounded_captures(ty, out, true, path);
            }
            // a capture row not inferred yet is a tail: it may still be empty
            #[allow(
                clippy::collapsible_match,
                reason = "a known tail must not fall to an atom"
            )]
            Type::Meta(m) if self.store.is_capture_row(*m) => {
                if !out.tails.contains(&*t) {
                    out.tails.push(t.into_owned());
                }
            }
            // an integer literal's type is a number, which captures nothing
            Type::Meta(m) if self.store.is_numeric(*m) => {}
            Type::Var(_) | Type::Skolem(_) | Type::Meta(_)
                if !bounded || matches!(self.atom_kind(&t, &[]), Some(Kind::Region)) =>
            {
                push_atom(out, &t);
            }
            Type::App { .. } => self.applied_captures(&t, out, bounded, path, true),
            Type::Tuple(items) => items
                .iter()
                .for_each(|i| self.collect_bounded_captures(i, out, bounded, path)),
            Type::Record(row) => row
                .entries
                .iter()
                .for_each(|(_, t)| self.collect_bounded_captures(t, out, bounded, path)),
            Type::Fun { param, result, .. } => {
                if bounded {
                    without_rows(&self.zonk(&t)).visit(&mut |t| {
                        if matches!(self.atom_kind(t, &[]), Some(Kind::Region)) {
                            push_atom(out, t);
                        }
                    });
                } else {
                    self.collect_bounded_captures(param, out, false, path);
                    self.collect_bounded_captures(result, out, false, path);
                }
            }
            // what the `forall` binds is no capture of the value: each use
            // instantiates it (a recursive local sees its own scheme)
            Type::Forall { vars, body, .. } => {
                let mut inner = Norm::default();
                self.collect_bounded_captures(body, &mut inner, bounded, path);
                let bound =
                    |t: &Type| matches!(t, Type::Var(v) if vars.iter().any(|b| b.name == *v));
                out.entries
                    .extend(inner.entries.into_iter().filter(|(_, t)| !bound(t)));
                for tail in inner.tails {
                    if !out.tails.contains(&tail) {
                        out.tails.push(tail);
                    }
                }
            }
            _ => {}
        }
    }

    /// What the application `t` holds, of a head and the arguments of a kind
    /// that holds.
    pub(super) fn applied_captures(
        &self,
        t: &Type,
        out: &mut Norm,
        bounded: bool,
        path: &mut Vec<AliasKey>,
        with_head: bool,
    ) {
        let (head, args) = t.spine();
        // an `Eff` holds what its result holds, as a function does: a
        // capture shown only in the result is lost when the result is dropped
        if HOLD_NOTHING.into_iter().any(|n| head.is_lang(n)) {
            return;
        }
        // `f a` holds what `f` stands for, as `Holder h a` holds `h`
        if with_head && !bounded {
            self.collect_bounded_captures(head, out, false, path);
        }
        let kinds = match head {
            Type::Con { module, name } => self
                .env
                .type_def(module, name)
                .map(|d| self.kinds.resolve(&d.kind)),
            _ => None,
        };
        let mut kind = kinds.as_ref();
        for arg in args {
            let param = match kind {
                Some(Kind::Arrow(param, rest)) => {
                    kind = Some(rest);
                    Some(&**param)
                }
                _ => None,
            };
            // a service row is needed again wherever the value runs; a
            // symbol is text; the error row of an `Eff` is that of its
            // stage, as a function's is
            match param {
                Some(Kind::ServiceRow | Kind::Symbol) => {}
                Some(Kind::ErrorRow) if head.is_lang(Lang::Eff) => {}
                Some(Kind::ErrorRow) => self.error_row_captures(arg, out, bounded, path),
                _ => match self.store.resolve(arg) {
                    row @ (Type::Row(_) | Type::Union(..)) => {
                        self.capture_row_filtered(&row, out, bounded, path);
                    }
                    _ => self.collect_bounded_captures(arg, out, bounded, path),
                },
            }
        }
    }

    /// Adds the regions of a capture row; its rigid variables count as
    /// entries, so `{s | c}` and `{s, c}` are the same set.
    pub fn capture_row(&self, row: &Type, out: &mut Norm) {
        self.capture_row_filtered(row, out, false, &mut Vec::new());
        out.sort();
        out.entries.dedup_by(|a, b| a.0 == b.0);
    }

    pub(super) fn capture_row_filtered(
        &self,
        row: &Type,
        out: &mut Norm,
        bounded: bool,
        path: &mut Vec<AliasKey>,
    ) {
        let norm = self.store.normalize(row);
        for (label, t) in &norm.entries {
            if matches!(label, Elem::Capture(_)) {
                match self.store.resolve(t) {
                    // an error-row variable or unknown, held as an atom, may
                    // have become a row of errors
                    row @ (Type::Row(_) | Type::Union(..)) => {
                        if self.of_errors(&self.store.normalize(&row)) {
                            self.error_row_captures(&row, out, bounded, path);
                        } else {
                            self.capture_row_filtered(&row, out, bounded, path);
                        }
                    }
                    other => self.collect_bounded_captures(&other, out, bounded, path),
                }
            }
        }
        for tail in norm.tails {
            match tail {
                Type::Var(_) | Type::Skolem(_)
                    if !bounded || matches!(self.atom_kind(&tail, &[]), Some(Kind::Region)) =>
                {
                    push_atom(out, &tail);
                }
                Type::Meta(_) if !bounded && !out.tails.contains(&tail) => out.tails.push(tail),
                Type::Error if !out.tails.contains(&tail) => out.tails.push(tail),
                _ => {}
            }
        }
    }

    /// What an error row reaches: what its errors reach and its row
    /// variables, which stand for errors that may reach anything. A row not
    /// inferred yet is an atom, as a type not inferred yet is, so a qualifier
    /// closed before it is known still holds what it becomes, a row or the
    /// variable it is generalized into; it is no tail, which a capture row
    /// would grow.
    pub(super) fn error_row_captures(
        &self,
        row: &Type,
        out: &mut Norm,
        bounded: bool,
        path: &mut Vec<AliasKey>,
    ) {
        let norm = self.store.normalize(row);
        for (_, error) in &norm.entries {
            self.collect_bounded_captures(error, out, bounded, path);
        }
        for tail in norm.tails {
            match tail {
                Type::Var(_) | Type::Skolem(_) | Type::Meta(_) if !bounded => push_atom(out, &tail),
                Type::Error if !out.tails.contains(&tail) => out.tails.push(tail),
                _ => {}
            }
        }
    }

    /// `t` with what each stage holds of the parameters before it: what the
    /// retention of a foreign function checks (`ffi/retention.rs`).
    pub(in crate::types) fn held_stages(&self, t: &Type) -> Type {
        self.stages(t, &mut Vec::new(), &Type::empty_row(), &mut Vec::new())
    }

    /// `path` holds the aliases it is unfolding: one met again inside its
    /// own unfolding stays as written.
    pub(super) fn stages(
        &self,
        t: &Type,
        kinds: &mut Vec<(String, Kind)>,
        held: &Type,
        path: &mut Vec<AliasKey>,
    ) -> Type {
        match t {
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let mark = kinds.len();
                kinds.extend(vars.iter().map(|v| (v.name.clone(), v.kind.clone())));
                let body = self.stages(body, kinds, held, path);
                kinds.truncate(mark);
                Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints.clone(),
                    body: Box::new(body),
                }
            }
            Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
                let ty = Box::new(self.stages(ty, kinds, captures, path));
                if matches!(t, Type::Bounded { .. }) {
                    Type::Bounded {
                        ty,
                        captures: captures.clone(),
                    }
                } else {
                    Type::Captured {
                        ty,
                        captures: captures.clone(),
                        value: matches!(t, Type::Captured { value: true, .. }),
                        elided: Elided::default(),
                    }
                }
            }
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = self.stages(param, kinds, &Type::empty_row(), path);
                let held = self.stage_holds(&param, kinds, held);
                let explicit = self.whnf_captured(result).1.is_some();
                let mut result = self.stages(result, kinds, &held, path);
                let retained = self.store.normalize(&held);
                if !explicit
                    && matches!(self.whnf(&result), Type::Fun { .. })
                    && (!retained.entries.is_empty() || !retained.tails.is_empty())
                {
                    result = qualify(result, held);
                }
                Type::Fun {
                    param: Box::new(param),
                    effects: effects.clone(),
                    result: Box::new(result),
                }
            }
            other => {
                let unfold = self.enter_alias(other, path);
                if matches!(unfold, Unfold::Again) {
                    return other.clone();
                }
                let expanded = self.qualified_head(other);
                let staged = if matches!(expanded, Type::Fun { .. }) {
                    self.stages(&expanded, kinds, held, path)
                } else {
                    other.map_children(&mut |c| self.stages(c, kinds, &Type::empty_row(), path))
                };
                leave_alias(path, unfold);
                staged
            }
        }
    }

    /// What a stage after the parameter `param` holds, with `held` what the
    /// parameters before it hold.
    pub(in crate::types) fn stage_holds(
        &self,
        param: &Type,
        kinds: &[(String, Kind)],
        held: &Type,
    ) -> Type {
        let mut captures = self.captures_of(param);
        captures.entries.retain(|(_, atom)| {
            matches!(
                self.atom_kind(atom, kinds),
                None | Some(
                    Kind::Type | Kind::Arrow(..) | Kind::Region | Kind::CaptureRow | Kind::ErrorRow
                )
            )
        });
        Type::Union(Box::new(held.clone()), Box::new(captures.into_type()))
    }

    /// Whether `atom` is a region or a capture row, not a type variable.
    pub(super) fn is_region(&self, atom: &Type, kinds: &[(String, Kind)]) -> bool {
        matches!(
            self.atom_kind(atom, kinds),
            Some(Kind::Region | Kind::CaptureRow)
        )
    }

    /// Whether a row is one of errors, not of captures: an entry of it is an
    /// error, as a capture row labels each of its entries `^…`, or a row
    /// variable of it stands for errors, as in a row with no error yet.
    pub(super) fn of_errors(&self, row: &Norm) -> bool {
        row.entries
            .iter()
            .any(|(label, _)| !matches!(label, Elem::Capture(_)))
            || row
                .tails
                .iter()
                .any(|t| self.atom_kind(t, &[]) == Some(Kind::ErrorRow))
    }

    pub(in crate::types) fn atom_kind(
        &self,
        atom: &Type,
        kinds: &[(String, Kind)],
    ) -> Option<Kind> {
        let kind = match atom {
            Type::Var(name) => kinds
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, k)| k.clone())
                .or_else(|| {
                    let v = self.cx.tyvars.iter().rev().find(|v| v.name == *name)?;
                    Some(v.kind.clone())
                }),
            Type::Skolem(s) => self
                .cx
                .regions
                .rigid_kinds
                .iter()
                .rev()
                .find(|(id, _)| id == s)
                .map(|(_, kind)| kind.clone())
                .or_else(|| self.scope_of(*s).map(|_| Kind::Region)),
            Type::Meta(m) if self.store.is_error_row(*m) => Some(Kind::ErrorRow),
            _ => None,
        };
        kind.map(|k| self.kinds.resolve(&k))
    }

    /// What a type shows its values may hold: exactly what they may reach, so
    /// a region a type mentions where no value keeps it hides no capture.
    pub fn shown_by(&self, t: &Type) -> Vec<Elem> {
        self.captures_of(t)
            .entries
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    }
}
