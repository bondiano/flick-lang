impl Checker<'_> {
    fn module_contract(&mut self, decl: &Node) -> Vec<ModuleRequirement> {
        let members: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .collect();
        self.contract_duplicates(&members);
        let mark = self.contract_types.len();
        let module_mark = self.opened_modules.len();
        let bound = self.bind_contract_members(&members);
        let mut requirements = self.contract_requirements(&members, &bound);
        self.validate_contract_dictionaries(&members, &requirements);
        self.default_contract_requirements(&mut requirements);
        self.contract_types.truncate(mark);
        self.opened_modules.truncate(module_mark);
        requirements
    }

    fn contract_duplicates(&mut self, members: &[&Node]) {
        let mut seen = HashMap::new();
        for member in members {
            if member.kind == K::ModuleInstanceReq {
                continue;
            }
            let namespace = if member.kind == K::Signature {
                Namespace::Value
            } else {
                Namespace::Type
            };
            let token = member.child_tokens().find(|t| {
                t.kind
                    == if namespace == Namespace::Value {
                        K::Ident
                    } else {
                        K::Con
                    }
            });
            if let Some(token) = token {
                let name = token.text(self.text).to_owned();
                let span = self.token_span(token);
                let key = (namespace, name.clone());
                if let Some(first) = seen.get(&key) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            codes::R008,
                            format!("`{name}` is required more than once by this module contract"),
                            span,
                        )
                        .with_related(*first, "first requirement here"),
                    );
                } else {
                    seen.insert(key, span);
                }
            }
        }
    }

    fn bind_contract_members(&mut self, members: &[&Node]) -> HashMap<String, Vec<Binder>> {
        let mut bound = HashMap::new();
        for member in members {
            if matches!(
                member.kind,
                K::ModuleTypeMember | K::ClassDecl | K::ServiceDecl
            ) && let Some(name) = self.con_token(member)
            {
                let params = self.module_type_params(member);
                let result = match member.kind {
                    K::ClassDecl => Kind::Constraint,
                    K::ServiceDecl => Kind::Service,
                    _ => Kind::Type,
                };
                let kind = Kind::arrows(params.iter().map(|p| p.kind.clone()), result);
                bound.insert(name.clone(), params);
                self.contract_types
                    .push((name.clone(), Type::Var(name), kind));
            } else if member.kind == K::ModuleNominalMember
                && let Some(local) = self.con_token(member)
                && let Some(path) = member.nodes().find(|n| n.kind == K::ModulePath)
            {
                let parts: Vec<&str> = path
                    .child_tokens()
                    .filter(|t| t.kind == K::Con)
                    .map(|t| t.text(self.text))
                    .collect();
                if let Some((actual, qualifier)) = parts.split_last()
                    && let Some((module, def)) = self.find_type(path, &qualifier.join("."), actual)
                {
                    self.contract_types
                        .push((local, Type::con(&module, actual), def.kind));
                }
            }
        }
        for member in members.iter().filter(|m| m.kind == K::ModuleMember) {
            if let (Some(name), Some(node)) = (self.con_token(member), member.nodes().next()) {
                let signature = self.static_module_signature(node);
                let binder = self.static_binder(&name, member.range().0);
                self.bind_static_witness(&name, &format!("$module:{binder}"), &signature);
            }
        }
        bound
    }

    fn contract_requirements(
        &mut self,
        members: &[&Node],
        bound: &HashMap<String, Vec<Binder>>,
    ) -> Vec<ModuleRequirement> {
        let mut requirements = Vec::new();
        for member in members {
            match member.kind {
                K::ModuleMember => {
                    if let (Some(name), Some(node)) =
                        (self.con_token(member), member.nodes().next())
                    {
                        let signature = self.static_module_signature(node);
                        requirements.push(ModuleRequirement::Module {
                            binder: self.static_binder(&name, member.range().0),
                            name,
                            signature: Box::new(signature),
                        });
                    }
                }
                K::ModuleTypeMember => {
                    if let Some(name) = self.con_token(member) {
                        let params = bound.get(&name).cloned().unwrap_or_default();
                        let manifest = member.nodes().find(|n| is_type(n.kind)).map(|n| {
                            self.with_params(&params, |this| this.ty(n, &Kind::Type, Free::Reject))
                        });
                        requirements.push(ModuleRequirement::Type {
                            name,
                            params,
                            manifest,
                        });
                    }
                }
                K::Signature => {
                    let name = member
                        .child_tokens()
                        .find(|t| t.kind == K::Ident)
                        .map(|t| t.text(self.text).to_owned());
                    if let (Some(name), Some(ty)) = (name, member.nodes().next()) {
                        let (ty, _) = self.signature(ty);
                        requirements.push(ModuleRequirement::Value { name, ty });
                    }
                }
                K::ModuleNominalMember => {
                    self.manifest_contract_requirement(member, &mut requirements);
                }
                K::ModuleInstanceReq => {
                    let head = member
                        .nodes()
                        .next()
                        .map(|n| self.ty(n, &Kind::Constraint, Free::Reject));
                    let from = member
                        .child_tokens()
                        .find(|t| t.kind == K::Ident)
                        .map(|t| t.text(self.text).to_owned());
                    if let (Some(head), Some(from)) = (head, from) {
                        requirements.push(ModuleRequirement::Dictionary { head, from });
                    }
                }
                K::ClassDecl => self.class_contract_requirement(member, bound, &mut requirements),
                K::ServiceDecl => {
                    self.service_contract_requirement(member, bound, &mut requirements);
                }
                _ => {}
            }
        }
        requirements
    }

    fn manifest_contract_requirement(
        &mut self,
        member: &Node,
        requirements: &mut Vec<ModuleRequirement>,
    ) {
        let name = self.con_token(member);
        let path = member.nodes().find(|n| n.kind == K::ModulePath);
        if let (Some(name), Some(path)) = (name, path) {
            let parts: Vec<&str> = path
                .child_tokens()
                .filter(|t| t.kind == K::Con)
                .map(|t| t.text(self.text))
                .collect();
            if let Some((actual, qualifier)) = parts.split_last() {
                let qualifier = qualifier.join(".");
                let category = if member.has_token(K::ClassKw) {
                    ModuleNominalKind::Class
                } else {
                    ModuleNominalKind::Service
                };
                let resolved = self.find_type(path, &qualifier, actual).and_then(
                                |(module, def)| {
                                    let matches = matches!(
                                        (category, &def.form),
                                        (ModuleNominalKind::Class, Form::Class { .. })
                                            | (ModuleNominalKind::Service, Form::Service { .. })
                                    );
                                    if !matches {
                                        let word = match category {
                                            ModuleNominalKind::Class => "class",
                                            ModuleNominalKind::Service => "service",
                                        };
                                        self.error(
                                            codes::T002,
                                            self.span(path),
                                            format!(
                                                "`{}` is not a {word}; this manifest requirement must name an existing {word}",
                                                self.text_of(path)
                                            ),
                                        );
                                        return None;
                                    }
                                    Some((Entity {
                                        module,
                                        namespace: Namespace::Type,
                                        name: (*actual).to_owned(),
                                    }, def))
                                },
                            );
                let (identity, definition, methods) = match resolved {
                    Some((identity, definition)) => {
                        let methods = match &definition.form {
                            Form::Class { methods, .. } => methods
                                .iter()
                                .filter_map(|name| {
                                    let entity = Entity {
                                        module: identity.module.clone(),
                                        namespace: Namespace::Value,
                                        name: name.clone(),
                                    };
                                    self.env.value(&entity).map(|ty| (name.clone(), ty.clone()))
                                })
                                .collect(),
                            _ => Vec::new(),
                        };
                        (Some(identity), Some(Box::new(definition)), methods)
                    }
                    None => (None, None, Vec::new()),
                };
                requirements.push(ModuleRequirement::Nominal {
                    name,
                    category,
                    identity,
                    definition,
                    methods,
                });
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
