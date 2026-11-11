impl ModuleSignature {
    pub fn project(&self, actual: &Interface) -> Interface {
        match self {
            Self::Structure { .. } => super::module_contract::project(&self.definition(), actual),
            Self::Parameterized { .. } => Interface {
                module: actual.module.clone(),
                parameterized: Some(self.clone()),
                signature_explicit: true,
                ..Interface::default()
            },
        }
    }

    /// A result annotation is an abstraction boundary. Parameter projection
    /// uses `project` instead: it hides representations while keeping the
    /// argument's existing identities.
    pub fn seal(&self, actual: &Interface, path: &str) -> Interface {
        let Self::Structure { requirements } = self else {
            return self.project(actual);
        };
        let mut witness = actual.clone();
        for requirement in requirements {
            if let R::Module {
                name, signature, ..
            } = requirement
                && let Some(child) = actual.modules.get(name)
            {
                witness.modules.insert(
                    name.clone(),
                    signature.seal(child, &format!("{path}.{name}")),
                );
            }
        }
        let mut projected = self.project(&witness);
        let replacements: Vec<_> = requirements
            .iter()
            .filter_map(|r| {
                if let R::Type {
                    name,
                    manifest: None,
                    ..
                } = r
                {
                    let export = projected.find(Namespace::Type, name)?;
                    Some((
                        Type::con(&export.entity.module, &export.entity.name),
                        Type::con(path, name),
                    ))
                } else {
                    None
                }
            })
            .collect();

        for export in &mut projected.exports {
            let old = Type::con(&export.entity.module, &export.entity.name);
            if replacements.iter().any(|(ty, _)| ty == &old) {
                path.clone_into(&mut export.entity.module);
            }
        }
        map_interface_types(&mut projected, &mut |ty| replace_types(ty, &replacements));
        path.clone_into(&mut projected.module);
        projected
    }

    pub fn accepts(&self, actual: &Interface) -> bool {
        match self {
            Self::Structure { .. } => {
                actual.parameterized.is_none()
                    && super::module_contract::check(&self.definition(), actual).is_empty()
            }
            Self::Parameterized { .. } => actual
                .parameterized
                .as_ref()
                .is_some_and(|provided| provided.is_subtype_of(self)),
        }
    }

    pub fn is_subtype_of(&self, expected: &Self) -> bool {
        match (self, expected) {
            (
                Self::Parameterized {
                    group_size: ag,
                    binder: a,
                    parameter: ap,
                    result: ar,
                },
                Self::Parameterized {
                    group_size: bg,
                    binder: b,
                    parameter: bp,
                    result: br,
                },
            ) => {
                if ag != bg || !bp.is_subtype_of(ap) {
                    return false;
                }
                let mut ar = (**ar).clone();
                let mut br = (**br).clone();
                let matched = format!("$matched({a})({b})");
                ar.rename_parameter(a, &matched);
                br.rename_parameter(b, &matched);
                let witness = bp.interface(&matched);
                ar.normalize_in(&witness);
                br.normalize_in(&witness);
                ar.is_subtype_of(&br)
            }
            (Self::Structure { .. }, Self::Structure { .. }) => {
                expected.accepts(&self.interface("$signature"))
            }
            _ => false,
        }
    }
}
