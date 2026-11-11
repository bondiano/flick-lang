impl ModuleSignature {
    /// An abstract interface usable when checking a body independently of its
    /// implementations. Abstract members are stable names in this interface.
    #[allow(clippy::too_many_lines)]
    pub fn interface(&self, path: &str) -> Interface {
        let mut interface = Interface {
            module: path.into(),
            ..Interface::default()
        };
        let Self::Structure { requirements } = self else {
            interface.parameterized = Some(self.clone());
            interface.signature_explicit = true;
            return interface;
        };
        for requirement in requirements {
            if let R::Module {
                name, signature, ..
            } = requirement
            {
                interface
                    .modules
                    .insert(name.clone(), signature.interface(&format!("{path}.{name}")));
            }
        }
        let requirements = Self::instantiate_members(requirements, &interface);
        let mut subst: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                R::Type {
                    name,
                    params,
                    manifest,
                } => Some((
                    name.clone(),
                    if params.is_empty() {
                        manifest.clone().unwrap_or_else(|| Type::con(path, name))
                    } else {
                        Type::con(path, name)
                    },
                )),
                R::Nominal { name, identity, .. } => Some((
                    name.clone(),
                    identity
                        .as_ref()
                        .map_or_else(|| Type::con(path, name), |e| Type::con(&e.module, &e.name)),
                )),
                _ => None,
            })
            .collect();
        for requirement in &requirements {
            if let R::Nominal {
                name,
                category: ModuleNominalKind::Class,
                identity,
                ..
            } = requirement
            {
                let (module, actual_name) = identity.as_ref().map_or((path, name.as_str()), |e| {
                    (e.module.as_str(), e.name.as_str())
                });
                subst.push((
                    format!("{name}$Dict"),
                    Type::con(module, &format!("{actual_name}$Dict")),
                ));
            }
        }
        for _ in 0..subst.len() {
            let next: Vec<_> = subst
                .iter()
                .map(|(name, ty)| (name.clone(), ty.substitute(&subst)))
                .collect();
            if next == subst {
                break;
            }
            subst = next;
        }
        for requirement in &requirements {
            let (name, namespace, decl, definition, scheme) = match requirement {
                R::Type {
                    name,
                    params,
                    manifest,
                } => (
                    name,
                    Namespace::Type,
                    Decl::Abstract,
                    Some(TypeDef {
                        kind: Kind::arrows(params.iter().map(|p| p.kind.clone()), Kind::Type),
                        form: manifest.as_ref().map_or(Form::NOMINAL, |ty| Form::Alias {
                            params: params.iter().map(|p| p.name.clone()).collect(),
                            body: ty.substitute(&subst),
                            opaque: false,
                        }),
                    }),
                    None,
                ),
                R::Value { name, ty } => (
                    name,
                    Namespace::Value,
                    Decl::Let,
                    None,
                    Some(ty.substitute(&subst)),
                ),
                R::Nominal {
                    name,
                    category,
                    identity,
                    definition,
                    methods,
                } => {
                    let entity = identity.clone().unwrap_or(Entity {
                        module: path.into(),
                        namespace: Namespace::Type,
                        name: name.clone(),
                    });
                    for (name, ty) in methods {
                        interface.exports.push(Export {
                            entity: Entity {
                                module: entity.module.clone(),
                                namespace: Namespace::Value,
                                name: name.clone(),
                            },
                            decl: Decl::Method,
                            scheme: Some(ty.substitute(&subst)),
                            definition: None,
                            parent: Some(entity.name.clone()),
                            blocked: None,
                            at: None,
                        });
                    }
                    interface.exports.push(Export {
                        entity,
                        decl: if matches!(category, ModuleNominalKind::Class) {
                            Decl::Class
                        } else {
                            Decl::Service
                        },
                        definition: definition.as_deref().cloned().map(|mut def| {
                            map_definition_types(&mut def, &mut |ty| ty.substitute(&subst));
                            def
                        }),
                        scheme: None,
                        parent: None,
                        blocked: None,
                        at: None,
                    });
                    continue;
                }
                R::Module {
                    name, signature, ..
                } => {
                    let mut signature = (**signature).clone();
                    signature.substitute_outer_types(&subst);
                    interface
                        .modules
                        .insert(name.clone(), signature.interface(&format!("{path}.{name}")));
                    continue;
                }
                R::Dictionary { .. } => continue,
            };
            interface.exports.push(Export {
                entity: Entity {
                    module: path.into(),
                    namespace,
                    name: name.clone(),
                },
                decl,
                definition,
                scheme,
                parent: None,
                blocked: None,
                at: None,
            });
        }
        interface
    }

    pub fn project(&self, actual: &Interface) -> Interface {
        match self {
            Self::Structure { requirements } => super::module_contract::project(
                &TypeDef {
                    kind: Kind::Module,
                    form: Form::ModuleContract {
                        requirements: requirements.clone(),
                    },
                },
                actual,
            ),
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
        fn replace(ty: &Type, replacements: &[(Type, Type)]) -> Type {
            if let Some((_, new)) = replacements.iter().find(|(old, _)| old == ty) {
                return new.clone();
            }
            map_module_children(ty, &mut |ty| replace(ty, replacements))
        }
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
        map_interface_types(&mut projected, &mut |ty| replace(ty, &replacements));
        path.clone_into(&mut projected.module);
        projected
    }

    pub fn accepts(&self, actual: &Interface) -> bool {
        match self {
            Self::Structure { requirements } => {
                actual.parameterized.is_none()
                    && super::module_contract::check(
                        &TypeDef {
                            kind: Kind::Module,
                            form: Form::ModuleContract {
                                requirements: requirements.clone(),
                            },
                        },
                        actual,
                    )
                    .is_empty()
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
                ar.is_subtype_of(&br)
            }
            (Self::Structure { .. }, Self::Structure { .. }) => {
                expected.accepts(&self.interface("$signature"))
            }
            _ => false,
        }
    }
}
