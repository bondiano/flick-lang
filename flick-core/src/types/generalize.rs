//! Generalization (§5 "Generalization при наличии Ref"): a binding built
//! purely becomes polymorphic over the variables that are not free in its
//! environment; the result of a runtime call stays monomorphic, even of `->{}`.

use super::Checker;
use super::kinds::Kind;
use super::ty::{Binder, Type};

impl Checker<'_> {
    /// Quantifies the variables of `ty` that are deeper than the current level.
    pub fn generalize(&mut self, ty: &Type) -> Type {
        self.settle_rows();
        let ty = self.zonk(ty);
        let metas = self.store.deep_metas(&ty);
        if metas.is_empty() {
            return ty;
        }
        let mut names = self.free_names(&ty);
        let mut vars = Vec::new();
        for m in metas {
            let name = names.next().expect("names are endless");
            vars.push(Binder {
                name: name.clone(),
                kind: kind_of(&ty, m),
            });
            self.store.set(m, Type::Var(name));
        }
        Type::Forall {
            vars,
            constraints: Vec::new(),
            body: Box::new(self.zonk(&ty)),
        }
    }

    /// `ty` with every unknown named, for showing a signature to write.
    pub fn generalize_copy(&self, ty: &Type) -> Type {
        let ty = self.zonk(ty);
        let mut metas = Vec::new();
        ty.visit(&mut |t| {
            if let Type::Meta(m) = t
                && !metas.contains(m)
            {
                metas.push(*m);
            }
        });
        let mut names = self.free_names(&ty);
        let subst: Vec<(u32, Type)> = metas
            .into_iter()
            .map(|m| (m, Type::Var(names.next().expect("names are endless"))))
            .collect();
        replace_metas(&ty, &subst)
    }

    /// Variable names not used in `ty` or by a variable in scope.
    fn free_names(&self, ty: &Type) -> impl Iterator<Item = String> + use<> {
        let mut taken: Vec<String> = self.tyvars.iter().map(|v| v.name.clone()).collect();
        ty.visit(&mut |t| match t {
            Type::Var(name) => taken.push(name.clone()),
            Type::Skolem(s) => taken.push(self.store.skolem_name(*s).to_owned()),
            _ => {}
        });
        (0..).filter_map(move |i: u32| {
            let letter = char::from(b'a' + u8::try_from(i % 26).expect("< 26"));
            let name = if i < 26 {
                letter.to_string()
            } else {
                format!("{letter}{}", i / 26)
            };
            (!taken.contains(&name)).then_some(name)
        })
    }
}

fn replace_metas(t: &Type, subst: &[(u32, Type)]) -> Type {
    match t {
        Type::Meta(m) => subst
            .iter()
            .find(|(x, _)| x == m)
            .map_or_else(|| t.clone(), |(_, v)| v.clone()),
        _ => t.map_children(&mut |c| replace_metas(c, subst)),
    }
}

/// The kind of a variable by where it occurs: a record tail is a field row,
/// the rows of a runtime arrow are service and error rows.
fn kind_of(ty: &Type, m: u32) -> Kind {
    fn is(t: &Type, m: u32) -> bool {
        match t {
            Type::Meta(x) => *x == m,
            Type::Row(row) => row.tail.as_deref().is_some_and(|t| is(t, m)),
            Type::Union(a, b) => is(a, m) || is(b, m),
            _ => false,
        }
    }
    let is = |t: &Type| is(t, m);
    let mut kind = Kind::Type;
    ty.visit(&mut |t| match t {
        Type::Record(row) if row.tail.as_deref().is_some_and(is) => {
            kind = Kind::Row(Box::new(Kind::Type));
        }
        Type::Fun {
            effects: Some(e), ..
        } => {
            if is(&e.services) {
                kind = Kind::ServiceRow;
            } else if is(&e.errors) {
                kind = Kind::ErrorRow;
            }
        }
        Type::App { .. } => {
            let (head, args) = t.spine();
            let rows: &[Kind] = if head.is_prim("Eff") {
                &[Kind::ServiceRow, Kind::ErrorRow]
            } else if head.is_prim("Layer") {
                &[Kind::ServiceRow, Kind::ErrorRow, Kind::ServiceRow]
            } else {
                &[]
            };
            for (arg, row) in args.iter().zip(rows) {
                if is(arg) {
                    kind = row.clone();
                }
            }
        }
        _ => {}
    });
    kind
}
