impl Checker<'_> {
    pub(super) fn static_module_signature(&mut self, node: &Node) -> super::ModuleSignature {
        use super::ModuleSignature as S;
        let children: Vec<_> = node.nodes().collect();
        match node.kind {
            K::ModuleSignatureArrow => {
                let Some(input) = children.first() else {
                    return S::Structure {
                        requirements: Vec::new(),
                    };
                };
                let binders: Vec<_> = if input.kind == K::ModuleSignatureParen
                    && input.nodes().all(|n| n.kind == K::ModuleSignatureBinder)
                {
                    input.nodes().collect()
                } else {
                    vec![*input]
                };
                let mark = self.opened_modules.len();
                let mut parameters = Vec::new();
                for input in binders {
                    let (alias, parameter) = if input.kind == K::ModuleSignatureBinder {
                        (
                            self.con_token(input).unwrap_or_default(),
                            self.static_module_signature(
                                input.nodes().next().expect("binder signature"),
                            ),
                        )
                    } else {
                        (String::new(), self.static_module_signature(input))
                    };
                    let binder = self.static_binder(&alias, input.range().0);
                    self.bind_static_witness(&alias, &format!("$module:{binder}"), &parameter);
                    parameters.push((binder, parameter));
                }
                let mut result = self.static_module_signature(children[1]);
                self.opened_modules.truncate(mark);
                for (index, (binder, parameter)) in parameters.into_iter().rev().enumerate() {
                    result = S::Parameterized {
                        group_size: index + 1,
                        binder,
                        parameter: Box::new(parameter),
                        result: Box::new(result),
                    };
                }
                result
            }
            K::ModuleSignatureParen | K::ModuleSignatureBinder | K::ModuleResult => {
                self.static_module_signature(children[0])
            }
            K::ModuleSignatureRefinement => {
                let mut signature = self.static_module_signature(children[0]);
                for equality in &children[1..] {
                    let mut nodes = equality.nodes();
                    let Some(path) = nodes.next() else {
                        continue;
                    };
                    let Some(rhs) = nodes.next() else {
                        continue;
                    };
                    let parts: Vec<_> = path
                        .child_tokens()
                        .filter(|t| t.kind == K::Con)
                        .map(|t| t.text(self.text).to_owned())
                        .collect();
                    let ty = self.ty(rhs, &Kind::Type, Free::Reject);
                    if let Err(detail) = signature.refine(&parts, ty) {
                        self.error(codes::T061, self.span(equality), detail);
                    }
                }
                signature
            }
            _ => {
                let ty = self.ty(node, &Kind::Module, Free::Reject);
                if let Type::Con { module, name } = ty {
                    match self.env.type_def(&module, &name).map(|d| &d.form) {
                        Some(Form::ModuleContract { requirements }) => {
                            return S::Structure {
                                requirements: requirements.clone(),
                            };
                        }
                        Some(Form::ModuleSignature { signature }) => return (**signature).clone(),
                        _ => {}
                    }
                }
                self.error(codes::T002, self.span(node), "expected a module signature");
                S::Structure {
                    requirements: Vec::new(),
                }
            }
        }
    }

    pub(super) fn static_binder(&self, alias: &str, start: u32) -> String {
        if alias.is_empty() {
            return String::new();
        }
        format!("{}@{start}#{alias}", self.env.module)
    }

    fn bind_static_witness(
        &mut self,
        alias: &str,
        witness: &str,
        signature: &super::ModuleSignature,
    ) {
        if alias.is_empty() {
            return;
        }
        let interface = signature.interface(witness);
        if interface.parameterized.is_some() {
            return;
        }
        for (name, child) in &interface.modules {
            let child_signature = super::ModuleSignature::from_interface(child);
            self.bind_static_witness(&format!("{alias}.{name}"), &child.module, &child_signature);
        }
        let types = interface
            .exports
            .iter()
            .filter_map(|export| {
                if export.entity.namespace != Namespace::Type {
                    return None;
                }
                let definition = export.definition.as_ref()?;
                let ty = match &definition.form {
                    Form::Alias { params, body, .. } if params.is_empty() => body.clone(),
                    _ => Type::con(&export.entity.module, &export.entity.name),
                };
                Some((export.entity.name.clone(), ty, definition.kind.clone()))
            })
            .collect();
        self.opened_modules.push(super::module_contract::Opened {
            alias: alias.to_owned(),
            types,
            values: Vec::new(),
        });
    }

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
}

include!("decls_contract_members.rs");
