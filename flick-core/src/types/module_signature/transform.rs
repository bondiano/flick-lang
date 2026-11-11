impl ModuleSignature {
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
            Self::Parameterized {
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
            Self::Parameterized {
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
            Self::Parameterized {
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
                    match requirement {
                        R::Module { signature, .. } => signature.canonicalize(next),
                        R::Type {
                            params, manifest, ..
                        } => {
                            let subst: Vec<_> = params
                                .iter_mut()
                                .map(|parameter| {
                                    let old = parameter.name.clone();
                                    parameter.name = format!("'{}", *next);
                                    *next += 1;
                                    (old, Type::Var(parameter.name.clone()))
                                })
                                .collect();
                            if let Some(ty) = manifest {
                                *ty = ty.substitute(&subst);
                            }
                        }
                        R::Nominal {
                            definition: Some(definition),
                            methods,
                            ..
                        } => {
                            let params = match &mut definition.form {
                                Form::Class { params, .. } | Form::Service { params, .. } => {
                                    Some(params)
                                }
                                _ => None,
                            };
                            if let Some(params) = params {
                                let subst: Vec<_> = params
                                    .iter_mut()
                                    .map(|parameter| {
                                        let old = parameter.clone();
                                        *parameter = format!("'{}", *next);
                                        *next += 1;
                                        (old, Type::Var(parameter.clone()))
                                    })
                                    .collect();
                                map_definition_types(definition, &mut |ty| ty.substitute(&subst));
                                for (_, ty) in methods {
                                    *ty = ty.substitute(&subst);
                                }
                            }
                        }
                        _ => {}
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
            Self::Parameterized {
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
}
