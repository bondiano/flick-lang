//! Recursion without an explicit `forall`: the recursive uses of the
//! functions of a group, or of a local `let rec`, must be monomorphic.

use std::mem;

use super::ty::Type;
use super::{Checker, Rec};
use crate::diagnostics::{Diagnostic, codes};

impl Checker<'_> {
    pub fn new_rec(&mut self, name: &str) -> usize {
        self.recs.push(Rec {
            name: name.to_owned(),
            ..Rec::default()
        });
        self.recs.len() - 1
    }

    /// Recursive uses of the functions of one group without an explicit
    /// `forall` are monomorphic: each puts every variable of the used
    /// signature on a variable of the group, and no two variables of one
    /// signature become the same one. Each body is checked against its own
    /// skolems, so `a` of one member is the same variable as `a` of another
    /// only through a use.
    pub fn finish_recs(&mut self, recs: &[usize]) {
        let recs: Vec<Rec> = recs.iter().map(|&r| mem::take(&mut self.recs[r])).collect();
        let owners: Vec<(u32, usize)> = recs
            .iter()
            .enumerate()
            .flat_map(|(m, rec)| {
                rec.skolems.iter().filter_map(move |(_, s)| match s {
                    Type::Skolem(s) => Some((*s, m)),
                    _ => None,
                })
            })
            .collect();
        let mut classes: Vec<Vec<u32>> = Vec::new();
        for rec in &recs {
            let (Some(scheme), Some(target)) = (&rec.scheme, &rec.target) else {
                continue;
            };
            for (ty, span) in &rec.uses {
                if self.monomorphic_use(rec, scheme, ty, &owners, &mut classes) {
                    continue;
                }
                let (name, span) = (&rec.name, *span);
                let (used, own) = (self.show(ty), self.show(target));
                self.diagnostics.push(
                    Diagnostic::error(
                        codes::T015,
                        format!(
                            "`{name}` is used at type `{used}` inside its own definition, which has type `{own}`: that is polymorphic recursion"
                        ),
                        span,
                    ).with_fix(|| (format!("quantify every variable of the signature explicitly: `{name} : forall … .`"), Vec::new())),
                );
            }
        }
    }

    /// Whether the use of `rec` at `ty` is monomorphic, recording in
    /// `classes` the skolems it makes one variable; `owners` tells the
    /// member each skolem of the group is a variable of.
    fn monomorphic_use(
        &mut self,
        rec: &Rec,
        scheme: &Type,
        ty: &Type,
        owners: &[(u32, usize)],
        classes: &mut Vec<Vec<u32>>,
    ) -> bool {
        // at the level the body was checked at, where its skolems live
        self.store.level += 1;
        let (opened, vars) = self.open(scheme);
        let unified = self.unify(ty, &opened).is_ok();
        self.store.level -= 1;
        if !unified {
            return false;
        }
        for ((name, var), (own_name, own)) in vars.iter().zip(&rec.skolems) {
            let Type::Skolem(own) = own else { continue };
            debug_assert_eq!(name, own_name, "open and skolemize agree");
            match self.store.resolve(var) {
                Type::Skolem(s) => {
                    if !identify(classes, owners, *own, s) {
                        return false;
                    }
                }
                // nothing fixed it: it is the member's own variable
                m @ Type::Meta(_) => {
                    let _ = self.unify(&m, &Type::Skolem(*own));
                }
                _ => return false,
            }
        }
        true
    }

    /// `t` with fresh variables for its quantified ones, as
    /// [`Self::instantiate`] makes it without wanting its constraints, and
    /// the variables by name.
    fn open(&mut self, t: &Type) -> (Type, Vec<(String, Type)>) {
        match self.store.resolve(t) {
            Type::Captured {
                ty,
                captures,
                value,
            } => {
                let (ty, vars) = self.open(&ty);
                let ty = Box::new(ty);
                (
                    Type::Captured {
                        ty,
                        captures,
                        value,
                    },
                    vars,
                )
            }
            Type::Bounded { ty, captures } => {
                let (ty, vars) = self.open(&ty);
                (
                    Type::Bounded {
                        ty: Box::new(ty),
                        captures,
                    },
                    vars,
                )
            }
            Type::Forall { vars, body, .. } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| (v.name.clone(), self.store.fresh_of(&v.kind)))
                    .collect();
                let (body, mut more) = self.open(&body.substitute(&subst));
                more.splice(0..0, subst);
                (body, more)
            }
            other => (other, Vec::new()),
        }
    }
}

/// Makes `a` and `b` one variable in `classes`; false when that makes two
/// variables of one member's signature the same.
fn identify(classes: &mut Vec<Vec<u32>>, owners: &[(u32, usize)], a: u32, b: u32) -> bool {
    let class_of = |s: u32, classes: &mut Vec<Vec<u32>>| {
        classes
            .iter()
            .position(|c| c.contains(&s))
            .unwrap_or_else(|| {
                classes.push(vec![s]);
                classes.len() - 1
            })
    };
    let (ca, cb) = (class_of(a, classes), class_of(b, classes));
    if ca == cb {
        return true;
    }
    let merged = classes.swap_remove(ca.max(cb));
    let class = &mut classes[ca.min(cb)];
    class.extend(merged);
    let member = |s: &u32| owners.iter().find(|(o, _)| o == s).map(|(_, m)| *m);
    let owned: Vec<usize> = class.iter().filter_map(member).collect();
    owned
        .iter()
        .enumerate()
        .all(|(i, m)| !owned[i + 1..].contains(m))
}
