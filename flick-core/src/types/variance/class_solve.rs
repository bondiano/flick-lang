//! Order-independent two-phase least-fixed-point inference for class parameter
//! variance (`docs/formal/regions.md` §2.2.1, Lean `inferClassVar` /
//! `Sig.dictVar_ok` / `Ty.PolD`, model `flick-model/src/variance.rs`).

use std::collections::HashMap;

use super::class_walk::occurs_free;
use super::{Key, Variance, param_kinds, valid_exported_variance};
use crate::iface::{Entity, Namespace};
use crate::types::Checker;
use crate::types::classes::split;
use crate::types::env::Form;
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::Type;

/// Joined variance uses for one class parameter.
pub(super) struct Uses(pub(super) Variance);

impl Uses {
    pub(super) fn at(&mut self, positive: bool, flipped: bool) {
        self.join(match (positive, flipped) {
            (true, false) => Variance::Covariant,
            (true, true) => Variance::Invariant,
            (false, _) => Variance::Contravariant,
        });
    }

    pub(super) fn join(&mut self, sign: Variance) {
        self.0 = self.0.join(sign);
    }

    pub(super) fn fixed(&mut self, stands: bool) {
        if stands {
            self.join(Variance::Invariant);
        }
    }
}

/// A declared class in `env`: its parameter kinds, parameter names, superclasses,
/// method names, and validated serialized variance (when imported).
struct ClassData<'t> {
    kinds: Vec<Kind>,
    params: &'t [String],
    supers: &'t [Type],
    methods: &'t [String],
    exported: Option<&'t [Variance]>,
}

impl Checker<'_> {
    fn class_data(&self, (module, name): &Key) -> Option<ClassData<'_>> {
        let def = self.env.type_def(module, name)?;
        let Form::Class {
            params,
            supers,
            methods,
            ..
        } = &def.form
        else {
            return None;
        };
        let kinds: Vec<Kind> = param_kinds(&def.kind).into_iter().cloned().collect();
        let imported = module != &self.env.module;
        let exported = imported
            .then(|| valid_exported_variance(&kinds, &def.variance))
            .flatten();
        Some(ClassData {
            kinds,
            params,
            supers,
            methods,
            exported,
        })
    }

    /// Computes variances for the connected component of local classes
    /// containing `key` via a two-phase least fixed point from `Bivariant`:
    ///
    /// - Phase 1 (`bi_is_fixed = false`): propagates `+`, `-`, `inv`
    ///   monotonically across superclasses and method constraints without
    ///   treating unvisited `bi` class parameters as `inv` (order-independent).
    /// - Phase 2 (`bi_is_fixed = true`): closes upward treating any remaining
    ///   genuinely unused (`bi`) class parameter as exact (`inv`) when
    ///   referenced by another class (Lean `Ty.PolD.dict`, `inferCnVar`).
    pub(super) fn solve_class_group(&self, key: &Key) -> HashMap<Key, Vec<Variance>> {
        let Some(root) = self.class_data(key) else {
            return HashMap::new();
        };
        if key.0 != self.env.module {
            let signs = root.exported.map_or_else(
                || vec![Variance::Invariant; root.kinds.len()],
                <[Variance]>::to_vec,
            );
            return HashMap::from([(key.clone(), signs)]);
        }
        if root.kinds.is_empty() {
            return HashMap::from([(key.clone(), Vec::new())]);
        }

        let mut group: Vec<Key> = Vec::new();
        let mut todo = vec![key.clone()];
        while let Some(k) = todo.pop() {
            if group.contains(&k) || self.class_signs.contains_key(&k) || k.0 != self.env.module {
                continue;
            }
            let Some(data) = self.class_data(&k) else {
                continue;
            };
            let mut seen_aliases = Vec::new();
            for sup in data.supers {
                self.enqueue_local_classes(sup, &mut seen_aliases, &mut todo, 0);
            }
            for method in data.methods {
                if let Some(scheme) = self.env.value(&Entity {
                    module: k.0.clone(),
                    namespace: Namespace::Value,
                    name: method.clone(),
                }) {
                    self.enqueue_local_classes(scheme, &mut seen_aliases, &mut todo, 0);
                }
            }
            group.push(k);
        }
        group.sort();

        let mut current: HashMap<Key, Vec<Variance>> = group
            .iter()
            .filter_map(|k| {
                let data = self.class_data(k)?;
                let init = data
                    .kinds
                    .iter()
                    .map(|kind| {
                        if *kind == Kind::Type {
                            Variance::Bivariant
                        } else {
                            Variance::Invariant
                        }
                    })
                    .collect();
                Some((k.clone(), init))
            })
            .collect();

        for bi_is_fixed in [false, true] {
            loop {
                let mut changed = false;
                for k in &group {
                    let next = self.class_sign_of(k, &current, bi_is_fixed);
                    let old = &current[k];
                    let joined: Vec<Variance> =
                        old.iter().zip(&next).map(|(a, b)| a.join(*b)).collect();
                    if joined != *old {
                        current.insert(k.clone(), joined);
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        current
    }

    fn class_sign_of(
        &self,
        key: &Key,
        current: &HashMap<Key, Vec<Variance>>,
        bi_is_fixed: bool,
    ) -> Vec<Variance> {
        let Some(data) = self.class_data(key) else {
            return Vec::new();
        };
        let mut uses: Vec<Uses> = data
            .params
            .iter()
            .map(|_| Uses(Variance::Bivariant))
            .collect();
        for sup in data.supers {
            for (param, u) in data.params.iter().zip(&mut uses) {
                self.walk_constraint(current, bi_is_fixed, param, sup, true, false, u);
            }
        }
        for method in data.methods {
            let Some(scheme) = self.env.value(&Entity {
                module: key.0.clone(),
                namespace: Namespace::Value,
                name: method.clone(),
            }) else {
                return vec![Variance::Invariant; data.params.len()];
            };
            let (constraints, body) = match scheme {
                Type::Forall {
                    constraints, body, ..
                } => (constraints.as_slice(), &**body),
                body => (&[][..], body),
            };
            // Match parameter names from the leading class constraint when present.
            let method_params: Vec<&str> = constraints
                .first()
                .and_then(|c| split(c))
                .filter(|((m, n), args)| {
                    *m == key.0 && *n == key.1 && args.len() == data.params.len()
                })
                .and_then(|(_, args)| {
                    args.iter()
                        .map(|arg| match arg {
                            Type::Var(v) => Some(v.as_str()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_else(|| data.params.iter().map(String::as_str).collect());

            let extra = constraints.get(1..).unwrap_or_default();
            for (param, u) in method_params.iter().zip(&mut uses) {
                for c in extra {
                    if let Some(((cm, cn), _)) = split(c)
                        && !Type::con(cm, cn).is_lang(Lang::Outlives)
                        && !Type::con(cm, cn).is_lang(Lang::Provided)
                        && self.class_info(cm, cn).is_some()
                    {
                        self.walk_constraint(current, bi_is_fixed, param, c, false, true, u);
                    } else {
                        u.fixed(occurs_free(param, c));
                    }
                }
                self.walk_class_ty(current, bi_is_fixed, param, body, true, false, u, 0);
            }
        }
        data.kinds
            .iter()
            .zip(uses)
            .map(|(kind, u)| {
                if *kind == Kind::Type {
                    u.0
                } else {
                    Variance::Invariant
                }
            })
            .collect()
    }

    fn enqueue_local_classes(
        &self,
        ty: &Type,
        seen_aliases: &mut Vec<Key>,
        todo: &mut Vec<Key>,
        depth: usize,
    ) {
        if depth > 64 {
            return;
        }
        let expanded = self.witness_head(ty);
        if let Type::Con { module, name } = &expanded {
            let base = name.strip_suffix("$Dict").unwrap_or(name);
            if module == &self.env.module {
                todo.push((module.clone(), base.to_owned()));
            }
            // Also inspect an unsaturated transparent alias body so local class
            // dependencies hidden behind aliases are always discovered in one
            // connected component regardless of resolution order.
            if let Some(def) = self.env.type_def(module, name)
                && let Form::Alias {
                    opaque: false,
                    body,
                    ..
                } = &def.form
            {
                let alias_key = (module.clone(), name.clone());
                if !seen_aliases.contains(&alias_key) {
                    seen_aliases.push(alias_key);
                    self.enqueue_local_classes(body, seen_aliases, todo, depth + 1);
                    seen_aliases.pop();
                }
            }
            return;
        }
        let head_alias = match ty.spine().0 {
            Type::Con { module, name }
                if matches!(
                    self.env.type_def(module, name).map(|d| &d.form),
                    Some(Form::Alias { opaque: false, .. })
                ) =>
            {
                Some((module.clone(), name.clone()))
            }
            _ => None,
        };
        if let Some(alias_key) = head_alias {
            if seen_aliases.contains(&alias_key) {
                return;
            }
            seen_aliases.push(alias_key);
            expanded.for_each_child(&mut |child| {
                self.enqueue_local_classes(child, seen_aliases, todo, depth + 1);
            });
            seen_aliases.pop();
        } else {
            expanded.for_each_child(&mut |child| {
                self.enqueue_local_classes(child, seen_aliases, todo, depth + 1);
            });
        }
    }

    pub(super) fn lookup_class_variance(
        &self,
        current: &HashMap<Key, Vec<Variance>>,
        module: &str,
        name: &str,
    ) -> Option<Vec<Variance>> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(vs) = current.get(&key) {
            return Some(vs.clone());
        }
        if let Some(found) = self.class_signs.get(&key) {
            return found.as_ref().map(|vs| vs.to_vec());
        }
        let data = self.class_data(&key)?;
        if module != self.env.module {
            return Some(data.exported.map_or_else(
                || vec![Variance::Invariant; data.kinds.len()],
                <[Variance]>::to_vec,
            ));
        }
        None
    }
}
