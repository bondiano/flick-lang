impl Checker<'_> {
    fn refresh_nominal_requirements(&self, signature: &mut super::ModuleSignature) {
        match signature {
            super::ModuleSignature::Parameterized {
                parameter, result, ..
            } => {
                self.refresh_nominal_requirements(parameter);
                self.refresh_nominal_requirements(result);
            }
            super::ModuleSignature::Structure { requirements } => {
                for requirement in requirements {
                    match requirement {
                        ModuleRequirement::Module { signature, .. } => {
                            self.refresh_nominal_requirements(signature)
                        }
                        ModuleRequirement::Nominal {
                            identity: Some(identity),
                            definition,
                            methods,
                            ..
                        } => {
                            if let Some(completed) =
                                self.env.type_def(&identity.module, &identity.name)
                            {
                                *methods = if let Form::Class { methods, .. } = &completed.form {
                                    methods
                                        .iter()
                                        .filter_map(|name| {
                                            let entity = Entity {
                                                module: identity.module.clone(),
                                                namespace: Namespace::Value,
                                                name: name.clone(),
                                            };
                                            self.env
                                                .value(&entity)
                                                .map(|ty| (name.clone(), ty.clone()))
                                        })
                                        .collect()
                                } else {
                                    Vec::new()
                                };
                                *definition = Some(Box::new(completed.clone()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn class_contract_requirement(
        &mut self,
        member: &Node,
        bound: &HashMap<String, Vec<Binder>>,
        requirements: &mut Vec<ModuleRequirement>,
    ) {
        if let Some(name) = self.con_token(member) {
            let params = bound.get(&name).cloned().unwrap_or_default();
            let (supers, fundeps, methods) = self.with_params(&params, |this| {
                let supers = member
                    .nodes()
                    .filter(|n| n.kind == K::Context)
                    .flat_map(Node::nodes)
                    .flat_map(|n| this.constraints(n, Free::Reject))
                    .collect();
                let fundeps = member
                    .nodes()
                    .filter(|n| n.kind == K::Fundep)
                    .filter_map(|n| this.fundep(n, &params))
                    .collect();
                let head = Type::apps(
                    Type::Var(name.clone()),
                    params.iter().map(|p| Type::Var(p.name.clone())),
                );
                let (methods, _) = this.class_members(member, head);
                let methods: Vec<_> = methods
                    .into_iter()
                    .map(|(name, ty)| (name, this.store.zonk(&quantify(&params, ty))))
                    .collect();
                (supers, fundeps, methods)
            });
            let method_names = methods.iter().map(|(n, _)| n.clone()).collect();
            requirements.push(ModuleRequirement::Nominal {
                name,
                category: ModuleNominalKind::Class,
                identity: None,
                definition: Some(Box::new(TypeDef {
                    kind: Kind::arrows(params.iter().map(|p| p.kind.clone()), Kind::Constraint),
                    form: Form::Class {
                        params: params.iter().map(|p| p.name.clone()).collect(),
                        supers,
                        fundeps,
                        methods: method_names,
                        defaults: Vec::new(),
                        default_sigs: Vec::new(),
                        derive: false,
                    },
                })),
                methods,
            });
        }
    }

    fn service_contract_requirement(
        &mut self,
        member: &Node,
        bound: &HashMap<String, Vec<Binder>>,
        requirements: &mut Vec<ModuleRequirement>,
    ) {
        if let Some(name) = self.con_token(member) {
            let params = bound.get(&name).cloned().unwrap_or_default();
            let operations = self.with_params(&params, |this| this.operations(member));
            let operations = operations
                .into_iter()
                .map(|(name, ty, class)| Operation { name, ty, class })
                .collect();
            requirements.push(ModuleRequirement::Nominal {
                name,
                category: ModuleNominalKind::Service,
                identity: None,
                definition: Some(Box::new(TypeDef {
                    kind: Kind::arrows(params.iter().map(|p| p.kind.clone()), Kind::Service),
                    form: Form::Service {
                        params: params.iter().map(|p| p.name.clone()).collect(),
                        operations,
                        effect: false,
                    },
                })),
                methods: Vec::new(),
            });
        }
    }

    fn validate_contract_dictionaries(
        &mut self,
        members: &[&Node],
        requirements: &[ModuleRequirement],
    ) {
        let dictionaries = requirements
            .iter()
            .filter_map(|requirement| match requirement {
                ModuleRequirement::Dictionary { head, from } => Some((head.clone(), from.clone())),
                _ => None,
            });
        for (member, (head, from)) in members
            .iter()
            .filter(|member| member.kind == K::ModuleInstanceReq)
            .zip(dictionaries)
        {
            let expected = super::module_contract::dictionary_type(&head);
            let found = requirements
                .iter()
                .find_map(|requirement| match requirement {
                    ModuleRequirement::Value { name, ty } if name == &from => Some(ty),
                    _ => None,
                });
            if !matches!((expected, found), (Some(expected), Some(found)) if api_diff::canonical_type(&expected) == api_diff::canonical_type(found))
            {
                self.error(
                    codes::T065,
                    self.span(member),
                    format!("`instance {} from {from}` requires a declared value `{from}` whose type is the matching class dictionary", self.show(&head)),
                );
            }
        }
    }

    fn default_contract_requirements(&mut self, requirements: &mut [ModuleRequirement]) {
        for requirement in requirements {
            match requirement {
                ModuleRequirement::Kind { definition, .. } => {
                    definition.kind = self.kinds.zonk(&definition.kind, true);
                }
                ModuleRequirement::Module { signature, .. } => {
                    signature.map_types(&mut |ty| self.default_kinds(ty));
                }
                ModuleRequirement::Type {
                    params, manifest, ..
                } => {
                    for param in params {
                        param.kind = self.kinds.zonk(&param.kind, true);
                    }
                    if let Some(ty) = manifest {
                        *ty = self.default_kinds(ty);
                    }
                }
                ModuleRequirement::Value { ty, .. } => *ty = self.default_kinds(ty),
                ModuleRequirement::Dictionary { head, .. } => *head = self.default_kinds(head),
                ModuleRequirement::Nominal {
                    definition,
                    methods,
                    ..
                } => {
                    for (_, ty) in methods {
                        *ty = self.default_kinds(ty);
                    }
                    if let Some(def) = definition {
                        def.kind = self.kinds.zonk(&def.kind, true);
                        match &mut def.form {
                            Form::Class {
                                supers,
                                default_sigs,
                                ..
                            } => {
                                for ty in supers
                                    .iter_mut()
                                    .chain(default_sigs.iter_mut().map(|(_, ty)| ty))
                                {
                                    *ty = self.default_kinds(ty);
                                }
                            }
                            Form::Service { operations, .. } => {
                                for operation in operations {
                                    operation.ty = self.default_kinds(&operation.ty);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn module_type_params(&mut self, member: &Node) -> Vec<Binder> {
        member
            .nodes()
            .filter(|n| n.kind == K::TypeParam)
            .filter_map(|param| {
                let name = param.child_tokens().find(|t| t.kind == K::Ident)?;
                let kind = match param.nodes().next() {
                    Some(node) => self.kind(node),
                    None => self.kinds.fresh(),
                };
                Some(Binder {
                    name: name.text(self.text).to_owned(),
                    kind,
                })
            })
            .collect()
    }

    pub fn con_token(&self, node: &Node) -> Option<String> {
        node.child_tokens()
            .find(|t| t.kind == K::Con)
            .map(|t| t.text(self.text).to_owned())
    }
}
