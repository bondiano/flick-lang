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
            ty.map_children(&mut |ty| replace(ty, from, path))
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
                        let mut single = Self::Structure {
                            requirements: vec![requirement.clone()],
                        };
                        single.map_types(&mut |ty| replace(ty, &from, path));
                        let Self::Structure { mut requirements } = single else {
                            unreachable!()
                        };
                        *requirement = requirements.remove(0);
                    }
                }
            }
        }
    }

    pub(crate) fn canonicalize(&mut self, next: &mut usize) {
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
                } if member == name && !rest.is_empty() => return signature.refine(rest, ty),
                _ => {}
            }
        }
        Err(format!("signature has no type `{}`", path.join(".")))
    }

    /// Visits all types, including types inside nested nominal requirements.
    pub fn map_types(&mut self, f: &mut impl FnMut(&Type) -> Type) {
        match self {
            Self::Functor {
                parameter, result, ..
            } => {
                parameter.map_types(f);
                result.map_types(f);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
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
                                && let Type::Con { module, name } =
                                    f(&Type::con(&entity.module, &entity.name))
                            {
                                entity.module = module;
                                entity.name = name;
                            }
                            for (_, ty) in methods {
                                *ty = f(ty);
                            }
                            if let Some(definition) = definition {
                                match &mut definition.form {
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
                                    _ => {}
                                }
                            }
                        }
                        R::Type { manifest: None, .. } => {}
                    }
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
