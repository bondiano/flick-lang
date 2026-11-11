/// Module substitution changes service/error identities and their derived
/// set keys together. Record field labels are source names and stay fixed.
fn map_module_children(ty: &Type, f: &mut dyn FnMut(&Type) -> Type) -> Type {
    let mut mapped = ty.map_children(f);
    if let (Type::Row(before), Type::Row(after)) = (ty, &mut mapped) {
        for ((old_key, old), (key, new)) in before.entries.iter().zip(&mut after.entries) {
            if let (
                Type::Con { module, name },
                Type::Con {
                    module: actual,
                    name: member,
                },
            ) = (old.spine().0, new.spine().0)
                && *old_key == format!("{module}.{name}")
            {
                *key = format!("{actual}.{member}");
            }
        }
        after.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
    mapped
}

fn map_definition_types(definition: &mut TypeDef, f: &mut dyn FnMut(&Type) -> Type) {
    match &mut definition.form {
        Form::Alias { body, .. } => *body = f(body),
        Form::Class {
            supers,
            default_sigs,
            ..
        } => {
            for ty in supers
                .iter_mut()
                .chain(default_sigs.iter_mut().map(|(_, ty)| ty))
            {
                *ty = f(ty);
            }
        }
        Form::Service { operations, .. } => {
            for operation in operations {
                operation.ty = f(&operation.ty);
            }
        }
        Form::ModuleContract { requirements } => {
            let mut signature = ModuleSignature::Structure {
                requirements: mem::take(requirements),
            };
            signature.map_types(f);
            let ModuleSignature::Structure {
                requirements: mapped,
            } = signature
            else {
                unreachable!()
            };
            *requirements = mapped;
        }
        Form::ModuleSignature { signature } => signature.map_types(f),
        _ => {}
    }
}

fn map_interface_types(interface: &mut Interface, f: &mut dyn FnMut(&Type) -> Type) {
    for export in &mut interface.exports {
        if let Some(ty) = &mut export.scheme {
            *ty = f(ty);
        }
        if let Some(definition) = &mut export.definition {
            map_definition_types(definition, f);
        }
    }
    if let Some(signature) = &mut interface.functor {
        signature.map_types(f);
    }
    for child in interface.modules.values_mut() {
        map_interface_types(child, f);
    }
}

fn map_requirement_types(requirement: &mut R, f: &mut dyn FnMut(&Type) -> Type) {
    match requirement {
        R::Type {
            manifest: Some(ty), ..
        }
        | R::Value { ty, .. } => *ty = f(ty),
        R::Dictionary { head, .. } => *head = f(head),
        R::Module { signature, .. } => signature.map_types(f),
        R::Nominal {
            identity,
            definition,
            methods,
            ..
        } => {
            if let Some(entity) = identity
                && let Type::Con { module, name } = f(&Type::con(&entity.module, &entity.name))
            {
                entity.module = module;
                entity.name = name;
            }
            for (_, ty) in methods {
                *ty = f(ty);
            }
            if let Some(definition) = definition {
                map_definition_types(definition, f);
            }
        }
        R::Type { manifest: None, .. } => {}
    }
}

/// Equality checking may use transparent definitions in a known argument;
/// its abstraction-preserving parameter projection stays unchanged.
pub(crate) fn normalize_module_interface(interface: &mut Interface, witness: &Interface) {
    map_interface_types(interface, &mut |ty| {
        super::module_contract::expand_actual(ty, witness, 32)
    });
}

impl ModuleSignature {
    /// Substitute an argument's exported identities, rather than inventing
    /// identities below the name of a module alias.
    pub(crate) fn instantiate_parameter(&mut self, binder: &str, actual: &Interface) {
        fn member(ty: &Type, marker: &str, actual: &Interface) -> Type {
            if let Type::Con { module, name } = ty {
                let suffix = if module == marker {
                    Some("")
                } else {
                    module
                        .strip_prefix(marker)
                        .and_then(|s| s.strip_prefix('.'))
                };
                if let Some(suffix) = suffix {
                    let child = suffix
                        .split('.')
                        .filter(|s| !s.is_empty())
                        .try_fold(actual, |child, name| child.modules.get(name));
                    if let Some(child) = child {
                        if let Some(export) = child.find(Namespace::Type, name) {
                            return Type::con(&export.entity.module, &export.entity.name);
                        }
                        if let Some(class) = name.strip_suffix("$Dict")
                            && let Some(export) = child.find(Namespace::Type, class)
                        {
                            return Type::con(
                                &export.entity.module,
                                &format!("{}$Dict", export.entity.name),
                            );
                        }
                        return Type::con(&child.module, name);
                    }
                }
            }
            map_module_children(ty, &mut |ty| member(ty, marker, actual))
        }
        let marker = format!("$argument({binder})({})", actual.module);
        self.rename_parameter(binder, &marker);
        self.map_types(&mut |ty| member(ty, &marker, actual));
    }

    /// Resolve structure-bound paths through the supplied child identities.
    pub(crate) fn instantiate_members(requirements: &[R], actual: &Interface) -> Vec<R> {
        let mut signature = Self::Structure {
            requirements: requirements.to_vec(),
        };
        for requirement in requirements {
            if let R::Module { name, binder, .. } = requirement
                && !binder.is_empty()
                && let Some(child) = actual.modules.get(name)
            {
                signature.instantiate_parameter(binder, child);
            }
        }
        let Self::Structure { requirements } = signature else {
            unreachable!()
        };
        requirements
    }

    pub fn rename_parameter(&mut self, binder: &str, path: &str) {
        fn replace(ty: &Type, from: &str, path: &str) -> Type {
            if let Type::Con { module, name } = ty {
                if module == from {
                    return Type::con(path, name);
                }
                if let Some(suffix) = module.strip_prefix(&format!("{from}.")) {
                    return Type::con(&format!("{path}.{suffix}"), name);
                }
            }
            map_module_children(ty, &mut |ty| replace(ty, from, path))
        }

        let from = format!("$module:{binder}");

        match self {
            Self::Functor {
                binder: inner,
                parameter,
                result,
                ..
            } => {
                parameter.rename_parameter(binder, path);
                if inner != binder {
                    result.rename_parameter(binder, path);
                }
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.rename_parameter(binder, path);
                    } else {
                        map_requirement_types(requirement, &mut |ty| replace(ty, &from, path));
                    }
                }
            }
        }
    }

    /// A nested signature can mention types bound by its containing
    /// structure. Its own structure members shadow those outer type names;
    /// the input of an arrow does not bind unqualified names in its result.
    pub(crate) fn substitute_outer_types(&mut self, subst: &[(String, Type)]) {
        if subst.is_empty() {
            return;
        }
        match self {
            Self::Functor {
                parameter, result, ..
            } => {
                parameter.substitute_outer_types(subst);
                result.substitute_outer_types(subst);
            }
            Self::Structure { requirements } => {
                let subst: Vec<_> = subst
                    .iter()
                    .filter(|(name, _)| {
                        let base = name.strip_suffix("$Dict").unwrap_or(name);
                        !requirements.iter().any(|r| {
                            matches!(r,
                        R::Type { name, .. } | R::Nominal { name, .. } if name == base)
                        })
                    })
                    .cloned()
                    .collect();
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.substitute_outer_types(&subst);
                    } else {
                        map_requirement_types(requirement, &mut |ty| ty.substitute(&subst));
                    }
                }
            }
        }
    }

    pub(crate) fn canonicalize(&mut self, next: &mut usize) {
        if let Self::Structure { requirements } = self {
            let binders: Vec<_> = requirements
                .iter()
                .filter_map(|r| match r {
                    R::Module { binder, .. } if !binder.is_empty() => Some(binder.clone()),
                    _ => None,
                })
                .collect();
            for binder in binders {
                let fresh = format!("'{}", *next);
                *next += 1;
                self.rename_parameter(&binder, &format!("$module:{fresh}"));
                if let Self::Structure { requirements } = self {
                    for requirement in requirements {
                        if let R::Module { binder: old, .. } = requirement
                            && old == &binder
                        {
                            old.clone_from(&fresh);
                        }
                    }
                }
            }
        }
        match self {
            Self::Functor {
                binder,
                parameter,
                result,
                ..
            } => {
                parameter.canonicalize(next);
                let fresh = format!("'{}", *next);
                *next += 1;
                result.rename_parameter(binder, &format!("$module:{fresh}"));
                *binder = fresh;
                result.canonicalize(next);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.canonicalize(next);
                    }
                }
            }
        }
        self.map_types(&mut canonical_type);
    }

    pub fn refine(&mut self, path: &[String], ty: Type) -> Result<(), String> {
        let Self::Structure { requirements } = self else {
            return Err("`with type` requires a structure signature".into());
        };
        let Some((name, rest)) = path.split_first() else {
            return Err("empty type refinement".into());
        };
        for requirement in requirements {
            match requirement {
                R::Type {
                    name: member,
                    params,
                    manifest,
                } if member == name && rest.is_empty() => {
                    if !params.is_empty() {
                        return Err(format!("type `{name}` requires its declared parameters"));
                    }
                    if let Some(previous) = manifest
                        && canonical_type(previous) != canonical_type(&ty)
                    {
                        return Err(format!(
                            "type refinement contradicts the declared equality for `{name}`"
                        ));
                    }
                    *manifest = Some(ty);
                    return Ok(());
                }
                R::Module {
                    name: member,
                    signature,
                    ..
                } if member == name && !rest.is_empty() => return signature.refine(rest, ty),
                _ => {}
            }
        }
        Err(format!("signature has no type `{}`", path.join(".")))
    }

    /// Visits all types, including types inside nested nominal requirements.
    pub fn map_types(&mut self, f: &mut dyn FnMut(&Type) -> Type) {
        match self {
            Self::Functor {
                parameter, result, ..
            } => {
                parameter.map_types(f);
                result.map_types(f);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    map_requirement_types(requirement, f);
                }
            }
        }
    }

    pub fn has_functor(&self) -> bool {
        match self {
            Self::Functor { .. } => true,
            Self::Structure { requirements } => requirements
                .iter()
                .any(|r| matches!(r, R::Module { signature, .. } if signature.has_functor())),
        }
    }
}
