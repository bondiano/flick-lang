// Read only declared member kinds; never instantiate bodies or lexical stamps.

impl ModuleSignature {
    pub(in crate::types) fn header_kind(
        &self,
        path: &str,
        module: &str,
        name: &str,
    ) -> Option<Kind> {
        header_kind(self, path, module, name, &[], &[])
    }
}

fn header_kind(
    signature: &ModuleSignature,
    path: &str,
    module: &str,
    name: &str,
    outer_kinds: &[(Kind, Kind)],
    outer_modules: &[(String, String)],
) -> Option<Kind> {
    let ModuleSignature::Structure { requirements } = signature else {
        return None;
    };
    let mut kinds = outer_kinds.to_vec();
    let mut modules = outer_modules.to_vec();
    for requirement in requirements {
        match requirement {
            R::Kind {
                name,
                identity: None,
                definition,
            } => {
                if let Kind::Named { module, .. } = &definition.kind {
                    kinds.push((
                        Kind::Named {
                            module: module.clone(),
                            name: name.clone(),
                        },
                        Kind::Named {
                            module: path.into(),
                            name: name.clone(),
                        },
                    ));
                }
            }
            R::Module { name, binder, .. } if !binder.is_empty() => {
                modules.push((format!("$module:{binder}"), format!("{path}.{name}")));
            }
            _ => {}
        }
    }
    let kind = if module == path {
        unique_header(
            requirements
                .iter()
                .filter_map(|requirement| local_header_kind(requirement, name)),
        )?
    } else {
        let suffix = module.strip_prefix(path)?.strip_prefix('.')?;
        let (child, _) = suffix.split_once('.').unwrap_or((suffix, ""));
        if let Some(signature) = requirements
            .iter()
            .find_map(|requirement| match requirement {
                R::Module {
                    name, signature, ..
                } if name == child => Some(signature.as_ref()),
                _ => None,
            })
        {
            return header_kind(
                signature,
                &format!("{path}.{child}"),
                module,
                name,
                &kinds,
                &modules,
            );
        }
        return None;
    };
    Some(rebase_header_kind(&kind, &kinds, &modules))
}

fn unique_header(mut kinds: impl Iterator<Item = Kind>) -> Option<Kind> {
    let first = kinds.next()?;
    kinds.all(|kind| kind == first).then_some(first)
}

fn local_header_kind(requirement: &R, requested: &str) -> Option<Kind> {
    match requirement {
        R::Type { name, params, .. } if name == requested => Some(Kind::arrows(
            params.iter().map(|parameter| parameter.kind.clone()),
            Kind::Type,
        )),
        R::Kind {
            name, definition, ..
        }
        | R::Nominal {
            name,
            definition: Some(definition),
            ..
        } if name == requested => Some(definition.kind.clone()),
        R::Nominal {
            name,
            category: ModuleNominalKind::Class,
            definition: Some(definition),
            ..
        } if requested.strip_suffix("$Dict") == Some(name.as_str())
            && matches!(definition.form, Form::Class { .. }) =>
        {
            Some(super::convert::as_value_kind(definition.kind.clone()))
        }
        R::Nominal {
            name,
            category: ModuleNominalKind::Service,
            definition: Some(definition),
            ..
        } if requested.strip_suffix(".Impl") == Some(name.as_str())
            && matches!(definition.form, Form::Service { .. }) =>
        {
            super::services::impl_def(definition).map(|implementation| implementation.kind)
        }
        _ => None,
    }
}

fn rebase_header_kind(kind: &Kind, kinds: &[(Kind, Kind)], modules: &[(String, String)]) -> Kind {
    if let Some((_, mapped)) = kinds.iter().rev().find(|(old, _)| old == kind) {
        return mapped.clone();
    }
    match kind {
        Kind::Named { module, name } => {
            let module = modules
                .iter()
                .rev()
                .find_map(|(from, to)| rename_module_reference(module, from, to))
                .unwrap_or_else(|| module.clone());
            Kind::Named {
                module,
                name: name.clone(),
            }
        }
        Kind::Arrow(a, b) => Kind::arrow(
            rebase_header_kind(a, kinds, modules),
            rebase_header_kind(b, kinds, modules),
        ),
        Kind::Row(inner) => Kind::Row(Box::new(rebase_header_kind(inner, kinds, modules))),
        _ => kind.clone(),
    }
}
