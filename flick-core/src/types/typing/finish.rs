//! Settling deferred typing facts at the end of type checking.

use std::mem;

use super::Typing;
use crate::types::{Checker, Type};
#[cfg(feature = "ide")]
use crate::types::{Elem, regions, ty::Norm};

impl Checker<'_> {
    #[cfg(feature = "ide")]
    /// `t` with each rigid variable written as the variable it stands for.
    fn named_skolems(&self, t: &Type) -> Type {
        match t {
            Type::Skolem(s) => Type::Var(self.store.skolem_name(*s).to_owned()),
            other => other.map_children(&mut |c| self.named_skolems(c)),
        }
    }

    /// Settles what waited for the whole module to be solved.
    pub fn finish_typing(&mut self) -> Typing {
        self.finish_foreigns();
        let mut typing = mem::take(&mut self.typing);
        let pending = mem::take(&mut typing.pending_values);
        for (at, ty) in pending.iter().chain(&typing.pending_types) {
            let observed = self.store.head(ty);
            if let t @ Type::Con { .. } = observed.as_ref() {
                typing.values.entry(*at).or_insert_with(|| t.clone());
            }
        }
        #[cfg(feature = "ide")]
        {
            typing.aliases = self
                .env
                .local_aliases()
                .map(|(name, params, body)| (name.to_owned(), (params.to_vec(), body.clone())))
                .collect();
            for (at, ty) in mem::take(&mut typing.pending_types) {
                let ty = self.named_skolems(&self.zonk(&ty));
                typing.types.insert(at, ty);
            }
            for (at, ty) in mem::take(&mut typing.pending_fields) {
                let ty = self.named_skolems(&self.zonk(&ty));
                typing.fields.insert(at, ty);
            }
            for (at, ty) in mem::take(&mut typing.pending_refined) {
                let ty = self.named_skolems(&self.zonk(&ty));
                typing.refined.insert(at, ty);
            }
            for (at, parts) in mem::take(&mut typing.pending_result_services) {
                let mut combined = Norm::default();
                for (actual, excluded, provides) in parts {
                    let norm = self.store.normalize(&actual);
                    for (label, ty) in norm.entries {
                        if matches!(label, Elem::Live) || excluded.contains(&label) {
                            continue;
                        }
                        if let Some(region) = &provides
                            && label == *regions::scope()
                            && ty.spine().1.first().is_some_and(|owner| {
                                self.store.resolve(owner) == self.store.resolve(region)
                            })
                        {
                            continue;
                        }
                        if combined.get(&label).is_none() {
                            combined
                                .entries
                                .push((label, self.named_skolems(&self.zonk(&ty))));
                        }
                    }
                    for tail in norm.tails {
                        let tail = self.named_skolems(&self.zonk(&tail));
                        if !matches!(tail, Type::Meta(_) | Type::Error)
                            && !combined.tails.contains(&tail)
                        {
                            combined.tails.push(tail);
                        }
                    }
                }
                if !combined.entries.is_empty() || !combined.tails.is_empty() {
                    typing.result_services.insert(at, combined.into_type());
                }
            }
        }
        #[cfg(not(feature = "ide"))]
        typing.pending_types.clear();
        for (at, ty) in mem::take(&mut typing.pending_labels) {
            let ty = self.zonk(&ty);
            if let Type::App { arg, .. } = ty
                && let Type::Symbol(label) = *arg
            {
                typing.labels.insert(at, label);
            }
        }
        self.finish_owner_typing(&mut typing);
        let mut dicts = mem::take(&mut typing.pending_dicts);
        dicts.sort_by_key(|(at, order, _)| (*at, *order));
        for (at, _, dict) in dicts {
            typing.dicts.entry(at).or_default().push(dict);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.finish_local_schemes(&mut typing);
        typing
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_local_schemes(&mut self, typing: &mut Typing) {
        for (at, ty) in mem::take(&mut typing.pending_local_schemes) {
            let exported = self.exported(&ty);
            if !exported.contains(&mut |t| {
                matches!(
                    t,
                    Type::Meta(_) | Type::Error | Type::Exists { .. } | Type::Symbol(_)
                )
            }) {
                exported.visit(&mut |t| {
                    if let Type::Skolem(id) = t {
                        typing
                            .skolem_names
                            .entry(*id)
                            .or_insert_with(|| self.store.skolem_name(*id).to_owned());
                    }
                });
                typing.local_schemes.insert(at, exported);
            }
        }
    }
}
