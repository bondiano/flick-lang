// Production lexical inference is separate from the contextual solver prototype.
use super::module_contract::{self, lexical};
use super::ty::{LexicalRoot, LexicalScope};
use std::mem;

impl Store {
    pub(in crate::types) fn lexical_context(&self) -> Vec<LexicalScope> {
        self.active_lexical.clone()
    }

    pub(in crate::types) fn enter_lexical(&mut self, scopes: Vec<LexicalScope>) {
        self.active_lexical = scopes;
    }

    pub(in crate::types) fn new_lexical_scope(&mut self, namespace: &str) -> LexicalScope {
        let scope = LexicalScope {
            id: super::ty::Scope::default().identity(),
            namespace: namespace.into(),
        };
        self.lexical_scopes.push(scope.clone());
        scope
    }

    fn lexical_mappings(
        &self,
        meta: &MetaOccurrence,
    ) -> Result<Vec<(LexicalScope, LexicalScope)>, ()> {
        let context = self.meta_contexts.get(meta.root() as usize).ok_or(())?;
        if (meta.legacy_root().is_some() && context.canonical.len() != 0)
            || !self.scopes.valid_context(&context.canonical)
        {
            return Err(());
        }
        context.lexical.mappings(meta, self.scopes.owner(), false)
    }

    pub(in crate::types) fn fresh_of_in(&mut self, kind: &Kind, scopes: &[LexicalScope]) -> Type {
        let saved = mem::replace(&mut self.active_lexical, scopes.to_vec());
        let fresh = self.fresh_of(kind);
        self.active_lexical = saved;
        fresh
    }
}

impl Checker<'_> {
    pub(in crate::types) fn with_lexical_witness<T>(
        &mut self,
        witness: &str,
        check: impl FnOnce(&mut Self, &LexicalScope) -> T,
    ) -> T {
        let scope = self.store.new_lexical_scope(witness);
        self.kinds.register_lexical(scope.clone());
        let old = self.store.lexical_context();
        let mut inside = old.clone();
        inside.push(scope.clone());
        self.store.enter_lexical(inside.clone());
        let old_kinds = self.kinds.lexical_context();
        self.kinds.enter_lexical(inside);
        let result = check(self, &scope);
        self.store.enter_lexical(old);
        self.kinds.enter_lexical(old_kinds);
        result
    }
}

impl Checker<'_> {
    fn lexical_label(
        &mut self,
        label: &Elem,
        allowed: &[LexicalScope],
        bound: &[LexicalScope],
    ) -> Result<Elem, ()> {
        use super::elem::Atom;
        Ok(match label {
            Elem::Named(name) => {
                if !super::ty::lexical_reference_allowed(
                    name,
                    allowed,
                    bound,
                    &self.store.lexical_scopes,
                ) {
                    return Err(());
                }
                label.clone()
            }
            Elem::Provided(inner) => {
                Elem::Provided(Box::new(self.lexical_label(inner, allowed, bound)?))
            }
            Elem::Capture(Atom::Meta(meta))
            | Elem::Error(Atom::Meta(meta))
            | Elem::Service(Atom::Meta(meta)) => {
                let ty = self.lexical_solution(
                    &Type::Meta(meta.clone()),
                    allowed,
                    bound,
                    false,
                    &mut Vec::new(),
                )?;
                relabel(label, &ty)
            }
            _ => label.clone(),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn lexical_solution(
        &mut self,
        ty: &Type,
        allowed: &[LexicalScope],
        bound: &[LexicalScope],
        field_row: bool,
        aliases: &mut Vec<(String, String)>,
    ) -> Result<Type, ()> {
        if self.store.lexical_scopes.is_empty() {
            let mut view = false;
            lexical::visit_type_metas(ty, &mut |meta, _| view |= meta.legacy_root().is_none());
            if !view && !ty.contains(&mut |ty| matches!(ty,Type::Con { module,.. } if module.starts_with("$exists:"))) {
                return Ok(ty.clone());
            }
        }
        let mut ty = self.store.resolve(ty);
        // Expand transparent aliases only when lexical dependencies need to be
        // discharged. Ordinary inference retains its existing alias spelling.
        let forbidden = self.store.lexical_scopes.iter().any(|scope| {
            !allowed
                .iter()
                .chain(bound)
                .any(|allowed| allowed.id == scope.id)
                && module_contract::witness_scope_occurs(&ty, &scope.namespace)
        });
        let mut alias = None;
        if forbidden {
            let (head, _) = ty.spine();
            if let Type::Con { module, name } = head {
                let key = (module.clone(), name.clone());
                if matches!(
                    self.env.type_def(module, name).map(|d| &d.form),
                    Some(Form::Alias { opaque: false, .. })
                ) {
                    if aliases.contains(&key) {
                        return Err(());
                    }
                    let reduced = self.witness_head(&ty);
                    if reduced != ty {
                        aliases.push(key.clone());
                        alias = Some(key);
                        ty = reduced;
                    }
                }
            }
        }
        let result = (|| {
            Ok(match &ty {
                Type::Meta(meta) => {
                    // Existing contextual carriers may be retained opaquely;
                    // this does not enable binding or pruning their roots.
                    if meta.contextual().is_some() {
                        return self
                            .store
                            .valid_occurrence(&ty)
                            .then(|| ty.clone())
                            .ok_or(());
                    }
                    let mappings = self.store.lexical_mappings(meta)?;
                    let root = meta.inference_type_root().ok_or(())?;
                    let permitted = mappings
                        .iter()
                        .filter(|(_, target)| {
                            allowed
                                .iter()
                                .chain(bound)
                                .any(|scope| scope.id == target.id)
                        })
                        .map(|(source, _)| source.id)
                        .collect::<Vec<_>>();
                    let mut context = self.store.meta_contexts[root as usize].clone();
                    if let Some(kind) = &context.kind {
                        let projected = lexical::transport_kind(kind, &mappings);
                        self.kinds.restrict_lexical(&projected, allowed, bound)?;
                    }
                    if context.lexical.permitted != permitted {
                        context.lexical.permitted = permitted;
                        self.store.write_context(root, context);
                    }
                    Type::Meta(meta.map_lexical(|coordinate| {
                        allowed
                            .iter()
                            .chain(bound)
                            .any(|scope| scope.id == coordinate.target.id)
                            .then(|| coordinate.clone())
                    }))
                }
                Type::Con { module, .. } => {
                    if !super::ty::lexical_reference_allowed(
                        module,
                        allowed,
                        bound,
                        &self.store.lexical_scopes,
                    ) {
                        return Err(());
                    }
                    ty.clone()
                }
                Type::Exists {
                    scope,
                    binder,
                    contract,
                    body,
                } => {
                    let contract =
                        self.lexical_solution(contract, allowed, bound, false, aliases)?;
                    let mut inside = bound.to_vec();
                    inside.push(LexicalScope {
                        id: scope.identity(),
                        namespace: format!("$exists:{binder}"),
                    });
                    Type::Exists {
                        scope: *scope,
                        binder: binder.clone(),
                        contract: Box::new(contract),
                        body: Box::new(
                            self.lexical_solution(body, allowed, &inside, false, aliases)?,
                        ),
                    }
                }
                Type::Forall {
                    vars,
                    constraints,
                    body,
                } => Type::Forall {
                    vars: vars
                        .iter()
                        .map(|binder| {
                            Ok(super::ty::Binder {
                                name: binder.name.clone(),
                                kind: self.kinds.restrict_lexical(&binder.kind, allowed, bound)?,
                            })
                        })
                        .collect::<Result<_, ()>>()?,
                    constraints: constraints
                        .iter()
                        .map(|ty| self.lexical_solution(ty, allowed, bound, false, aliases))
                        .collect::<Result<_, _>>()?,
                    body: Box::new(self.lexical_solution(body, allowed, bound, false, aliases)?),
                },
                Type::Row(row) | Type::Record(row) => {
                    let fields = field_row || matches!(&ty, Type::Record(_));
                    let mut row = Row {
                        entries: row
                            .entries
                            .iter()
                            .map(|(label, value)| {
                                Ok((
                                    if fields {
                                        label.clone()
                                    } else {
                                        self.lexical_label(label, allowed, bound)?
                                    },
                                    self.lexical_solution(value, allowed, bound, false, aliases)?,
                                ))
                            })
                            .collect::<Result<_, ()>>()?,
                        tail: row
                            .tail
                            .as_ref()
                            .map(|tail| {
                                self.lexical_solution(tail, allowed, bound, fields, aliases)
                                    .map(Box::new)
                            })
                            .transpose()?,
                        written: row.written,
                    };
                    row.sort();
                    if matches!(&ty, Type::Record(_)) {
                        Type::Record(row)
                    } else {
                        Type::Row(row)
                    }
                }
                Type::Union(a, b) => Type::Union(
                    Box::new(self.lexical_solution(a, allowed, bound, field_row, aliases)?),
                    Box::new(self.lexical_solution(b, allowed, bound, field_row, aliases)?),
                ),
                Type::Without { row, gone } => Type::Without {
                    row: Box::new(self.lexical_solution(row, allowed, bound, field_row, aliases)?),
                    gone: gone
                        .iter()
                        .map(|label| {
                            if field_row {
                                Ok(label.clone())
                            } else {
                                self.lexical_label(label, allowed, bound)
                            }
                        })
                        .collect::<Result<_, _>>()?,
                },
                _ => {
                    let mut failure = false;
                    let mapped = ty.map_children(&mut |child| {
                        if let Ok(ty) = self.lexical_solution(child, allowed, bound, false, aliases)
                        {
                            ty
                        } else {
                            failure = true;
                            child.clone()
                        }
                    });
                    if failure {
                        return Err(());
                    }
                    mapped
                }
            })
        })();
        if alias.is_some() {
            aliases.pop();
        }
        result
    }

    pub(in crate::types) fn restrict_lexical_result(
        &mut self,
        ty: &Type,
        allowed: &[LexicalScope],
    ) -> Result<Type, ()> {
        self.lexical_solution(ty, allowed, &[], false, &mut Vec::new())
    }
}

impl Store {
    pub(in crate::types) fn fresh_beside_occurrence(&mut self, variable: &Type) -> Option<Type> {
        let Type::Meta(meta) = variable else {
            return None;
        };
        let root = meta.inference_type_root()?;
        let mappings = self.lexical_mappings(meta).ok()?;
        let sibling = self.fresh_beside(root);
        Some(lexical::transport_type(&sibling, &mappings))
    }
}

impl Checker<'_> {
    pub(in crate::types) fn bind_inference(&mut self, variable: &Type, solution: &Type) -> Unified {
        let Type::Meta(meta) = variable else {
            return Err(Mismatch::Types(variable.clone(), solution.clone()));
        };
        self.bind_occurrence(meta, solution)
    }
}

#[cfg(test)]
#[path = "lexical/tests.rs"]
mod lexical_tests;
