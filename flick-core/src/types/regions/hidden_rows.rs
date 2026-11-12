//! The hidden row of data with hidden storage written bare in a signature
//! (`docs/formal/regions.md` §1.4): in a parameter it is a fresh capture-row
//! variable the scheme quantifies, in the result what the parameters before
//! it reach. A written qualifier, a constant and data inside a type argument
//! keep the row empty.

use super::live::merge;
use crate::iface::{Entity, Namespace};
use crate::types::Checker;
use crate::types::env::Form;
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::{Binder, Elided, Norm, Type};

impl Checker<'_> {
    /// Whether `t` is data with hidden storage as Lean and the model read
    /// it (#217): an ADT with a field that is a function or a computation,
    /// through the fields of the data it holds, or an opaque type exported
    /// with one. A foreign nominal type hides what G4 bounds, not a row.
    pub fn stores_hidden(&self, t: &Type) -> bool {
        self.data_hides(t, &mut Vec::new())
    }

    /// Whether `t`, data, has hidden storage.
    fn data_hides(&self, t: &Type, seen: &mut Vec<(String, String)>) -> bool {
        let t = self.whnf(t);
        let (head, args) = t.spine();
        let Type::Con { module, name } = head else {
            return false;
        };
        if seen.iter().any(|(m, n)| m == module && n == name) {
            return false;
        }
        match self.env.type_def(module, name).map(|d| &d.form) {
            Some(Form::Nominal { hidden, .. }) => *hidden,
            Some(Form::Adt { constructors, .. }) => {
                seen.push((module.clone(), name.clone()));
                let fields = self.constructor_fields_at(module, constructors, &args);
                fields.iter().any(|f| self.field_hides(f, seen))
            }
            _ => false,
        }
    }

    /// Whether a field of type `t` is a function or a computation, or holds
    /// one in the data it is.
    fn field_hides(&self, t: &Type, seen: &mut Vec<(String, String)>) -> bool {
        let t = self.whnf(t);
        match &t {
            Type::Fun { .. } => true,
            _ if self.eff_parts(&t).is_some() => true,
            Type::Captured { ty, .. } | Type::Bounded { ty, .. } => self.field_hides(ty, seen),
            Type::Tuple(items) => items.iter().any(|t| self.field_hides(t, seen)),
            Type::Record(row) => row.entries.iter().any(|(_, t)| self.field_hides(t, seen)),
            Type::Forall { body, .. } => self.field_hides(body, seen),
            Type::Con { .. } | Type::App { .. } if t.spine().0.is_lang(Lang::List) => {
                t.spine().1.iter().any(|a| self.field_hides(a, seen))
            }
            Type::Con { .. } | Type::App { .. } => self.data_hides(&t, seen),
            _ => false,
        }
    }

    /// The fields of the constructors of a type at its arguments `args`.
    fn constructor_fields_at(
        &self,
        module: &str,
        constructors: &[String],
        args: &[&Type],
    ) -> Vec<Type> {
        let mut out = Vec::new();
        for name in constructors {
            let entity = Entity {
                module: module.to_owned(),
                namespace: Namespace::Constructor,
                name: name.clone(),
            };
            let Some(scheme) = self.env.value(&entity) else {
                continue;
            };
            let mut current = match scheme {
                Type::Forall { vars, body, .. } => {
                    let subst: Vec<_> = (vars.iter())
                        .zip(args)
                        .map(|(var, arg)| (var.name.clone(), (*arg).clone()))
                        .collect();
                    body.substitute(&subst)
                }
                other => other.clone(),
            };
            while let Type::Fun { param, result, .. } = current {
                out.push(*param);
                current = *result;
            }
        }
        out
    }

    /// `scheme` with the hidden rows of the data it writes bare at the outer
    /// level of a parameter or of its result.
    pub fn hidden_rows(&self, scheme: &Type) -> Type {
        let (vars, constraints, body) = match scheme {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (vars.clone(), constraints.clone(), (**body).clone()),
            other => (Vec::new(), Vec::new(), other.clone()),
        };
        let mut kinds: Vec<(String, Kind)> = vars
            .iter()
            .map(|v| (v.name.clone(), v.kind.clone()))
            .collect();
        let mut fresh = Vec::new();
        let body = self.rows_of_stages(&body, &Norm::default(), &mut kinds, &mut fresh, 0);
        if fresh.is_empty() && vars.is_empty() && constraints.is_empty() {
            return body;
        }
        let mut vars = vars;
        vars.extend(fresh);
        Type::Forall {
            vars,
            constraints,
            body: Box::new(body),
        }
    }

    fn rows_of_stages(
        &self,
        t: &Type,
        held: &Norm,
        kinds: &mut Vec<(String, Kind)>,
        fresh: &mut Vec<Binder>,
        depth: usize,
    ) -> Type {
        match t {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = if self.bare_hidden(param) {
                    let name = format!("{FRESH}{}", fresh.len());
                    kinds.push((name.clone(), Kind::CaptureRow));
                    fresh.push(Binder {
                        name: name.clone(),
                        kind: Kind::CaptureRow,
                    });
                    elided((**param).clone(), Type::Var(name))
                } else {
                    (**param).clone()
                };
                let mut held = held.clone();
                let mut reached = self.captures_of(&param);
                reached.entries.retain(|(_, atom)| {
                    matches!(
                        self.atom_kind(atom, kinds),
                        Some(Kind::Region | Kind::CaptureRow)
                    )
                });
                merge(&mut held, reached);
                Type::Fun {
                    param: Box::new(param),
                    effects: effects.clone(),
                    result: Box::new(self.rows_of_stages(result, &held, kinds, fresh, depth + 1)),
                }
            }
            result if depth > 0 && self.bare_hidden(result) => {
                elided(result.clone(), held.clone().into_type())
            }
            other => other.clone(),
        }
    }

    /// Data with hidden storage written without a qualifier.
    fn bare_hidden(&self, t: &Type) -> bool {
        !matches!(t, Type::Captured { .. } | Type::Bounded { .. }) && self.stores_hidden(t)
    }
}

/// `t` as its signature writes it: without the hidden rows the checker read
/// and the variables it quantified for them, as messages show a signature.
pub fn strip_elided(t: &Type) -> Type {
    match t {
        Type::Captured {
            ty,
            elided: Elided(true),
            ..
        } => strip_elided(ty),
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let vars: Vec<Binder> = (vars.iter())
                .filter(|v| !is_elided_row(v))
                .cloned()
                .collect();
            let body = strip_elided(body);
            if vars.is_empty() && constraints.is_empty() {
                return body;
            }
            Type::Forall {
                vars,
                constraints: constraints.clone(),
                body: Box::new(body),
            }
        }
        _ => t.map_children(&mut |c| strip_elided(c)),
    }
}

/// The prefix of a variable quantified for a hidden row: no source can
/// write it, and as `captures` it is a hidden capture (`is_hidden_captures`).
pub(super) const FRESH: &str = "captures$";

/// Whether `v` is a variable quantified for a hidden row the checker read.
pub fn is_elided_row(v: &Binder) -> bool {
    v.name.starts_with(FRESH)
}

fn elided(ty: Type, captures: Type) -> Type {
    Type::Captured {
        ty: Box::new(ty),
        captures: Box::new(captures),
        value: false,
        elided: Elided(true),
    }
}
