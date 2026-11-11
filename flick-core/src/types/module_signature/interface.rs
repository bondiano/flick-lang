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
            interface.functor = Some(self.clone());
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
}
