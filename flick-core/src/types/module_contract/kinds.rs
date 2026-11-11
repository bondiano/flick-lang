fn kind_witnesses(
    requirements: &[ModuleRequirement],
    actual: &Interface,
    errors: &mut Vec<Mismatch>,
) -> Vec<(super::Kind, super::Kind)> {
    let mut witnesses = Vec::new();
    for requirement in requirements {
        let ModuleRequirement::Kind {
            name,
            identity,
            definition,
        } = requirement
        else {
            continue;
        };
        let Some(export) = actual.find(Namespace::Type, name) else {
            errors.push(Mismatch::Missing(name.clone()));
            continue;
        };
        let Some(found) = &export.definition else {
            errors.push(Mismatch::Kind(name.clone()));
            continue;
        };
        if definition.form != found.form {
            errors.push(Mismatch::Kind(name.clone()));
        }
        if let Some(identity) = identity {
            if identity != &export.entity {
                errors.push(Mismatch::NominalIdentity(name.clone()));
            }
        } else if let Some((_, previous)) =
            witnesses.iter().find(|(old, _)| old == &definition.kind)
        {
            if previous != &found.kind {
                errors.push(Mismatch::Kind(name.clone()));
            }
        } else {
            witnesses.push((definition.kind.clone(), found.kind.clone()));
        }
    }
    witnesses
}

fn substitute_kind(kind: &super::Kind, witnesses: &[(super::Kind, super::Kind)]) -> super::Kind {
    if let Some((_, actual)) = witnesses.iter().find(|(expected, _)| expected == kind) {
        return actual.clone();
    }
    match kind {
        super::Kind::Arrow(a, b) => {
            super::Kind::arrow(substitute_kind(a, witnesses), substitute_kind(b, witnesses))
        }
        super::Kind::Row(inner) => super::Kind::Row(Box::new(substitute_kind(inner, witnesses))),
        _ => kind.clone(),
    }
}

fn substitute_type_kinds(ty: &Type, witnesses: &[(super::Kind, super::Kind)]) -> Type {
    if let Type::Con { module, name } = ty {
        for (expected, actual) in witnesses {
            if let (
                super::Kind::Named {
                    module: old_module,
                    name: old_name,
                },
                super::Kind::Named {
                    module: new_module,
                    name: new_name,
                },
            ) = (expected, actual)
                && module == old_module
                && name == old_name
            {
                return Type::con(new_module, new_name);
            }
        }
    }
    let mut mapped = ty.map_children(&mut |child| substitute_type_kinds(child, witnesses));
    if let Type::Forall { vars, .. } = &mut mapped {
        for var in vars {
            var.kind = substitute_kind(&var.kind, witnesses);
        }
    }
    mapped
}

fn substitute_child_signature(
    signature: &super::ModuleSignature,
    subst: &[(String, Type)],
    kinds: &[(super::Kind, super::Kind)],
) -> super::ModuleSignature {
    let mut signature = signature.clone();
    signature.substitute_outer_types(subst);
    signature.map_types(&mut |ty| substitute_type_kinds(ty, kinds));
    signature
}
