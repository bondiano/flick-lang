impl Checker<'_> {
    fn static_module_expression(
        &mut self,
        node: &Node,
        module: &Module,
        path: &str,
    ) -> Option<Interface> {
        match node.kind {
            K::ModulePath => {
                let name: String = node.text(self.text).split_whitespace().collect();
                self.env
                    .qualified_module(&name)
                    .and_then(|(_, interface)| interface.cloned())
            }
            K::ModuleParenExpr => self.static_module_expression(node.nodes().next()?, module, path),
            K::ModuleApplyExpr => {
                let mut nodes = node.nodes();
                let function = self.static_module_expression(nodes.next()?, module, path)?;
                let explicit = function.signature_explicit;
                let Some(mut signature) = function.parameterized else {
                    self.error(
                        codes::T002,
                        self.span(node),
                        "module expression is not a parameterized module",
                    );
                    return None;
                };
                if let ModuleSignature::Parameterized { group_size, .. } = &signature
                    && *group_size != node.nodes().skip(1).count()
                {
                    self.error(
                        codes::T002,
                        self.span(node),
                        format!(
                            "parameterized requires {group_size} arguments in this parameter group"
                        ),
                    );
                    return None;
                }
                for argument in nodes {
                    let actual = self.static_module_expression(
                        argument,
                        module,
                        &format!("{path}.Argument{}", argument.range().0),
                    )?;
                    let ModuleSignature::Parameterized {
                        binder,
                        parameter,
                        mut result,
                        ..
                    } = signature
                    else {
                        self.error(
                            codes::T002,
                            self.span(node),
                            "module expression is not a parameterized module",
                        );
                        return None;
                    };
                    if !parameter.accepts(&actual) {
                        self.error(
                            codes::T061,
                            self.span(argument),
                            "module argument does not satisfy the parameterized signature",
                        );
                        return None;
                    }
                    result.instantiate_parameter(&binder, &actual);
                    signature = *result;
                }
                let mut result = signature.interface(path);
                result.signature_explicit = explicit;
                Some(result)
            }
            K::ModuleAnnotExpr => {
                let mut nodes = node.nodes();
                let actual = self.static_module_expression(nodes.next()?, module, path)?;
                let signature = self.static_module_signature(nodes.next()?);
                if !signature.accepts(&actual) {
                    self.error(
                        codes::T061,
                        self.span(node),
                        "module expression does not satisfy its annotation",
                    );
                    return None;
                }
                Some(signature.seal(&actual, path))
            }
            K::ModuleParamExpr => self.static_anonymous_parameterized(node, module, path),
            _ => None,
        }
    }

    fn static_parameterized_header(&self, node: &Node, path: &str, body: &Node) -> Header {
        let groups: Vec<_> = node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .collect();
        let span = self.span(node);
        Header {
            name: path.to_owned(),
            private: true,
            span,
            exposing: (!node.nodes().any(|n| n.kind == K::ModuleResult)).then(|| {
                vec![Item {
                    kind: ItemKind::All,
                    name: String::new(),
                    span,
                }]
            }),
            parameters: groups
                .iter()
                .flat_map(|g| g.nodes())
                .filter_map(|p| {
                    let name = p.child_tokens().find(|t| t.kind == K::Con)?;
                    let signature = p.nodes().next()?;
                    Some(ModuleParam {
                        name: name.text(self.text).to_owned(),
                        span: Span::new(self.file, name.start, name.end),
                        signature: signature.clone(),
                    })
                })
                .collect(),
            parameter_groups: groups.iter().map(|g| g.nodes().count()).collect(),
            result: node
                .nodes()
                .find(|n| n.kind == K::ModuleResult)
                .and_then(|n| n.nodes().next())
                .cloned(),
            body: (body.kind != K::Members).then(|| body.clone()),
            materialized_body: None,
        }
    }

    fn static_anonymous_parameterized(
        &mut self,
        node: &Node,
        parent: &Module,
        path: &str,
    ) -> Option<Interface> {
        let body = node
            .nodes()
            .find(|n| !matches!(n.kind, K::ModuleParamGroup | K::ModuleResult))?;
        let tree = Node {
            kind: K::SourceFile,
            children: if body.kind == K::Members {
                body.children.clone()
            } else {
                Vec::new()
            },
        };
        let mut syntax = resolve::module_syntax(&tree, self.text, self.file);
        let own_imports = syntax.imports.clone();
        syntax.imports.extend(parent.syntax.imports.clone());
        let mut captured = HashMap::new();
        for import in &syntax.imports {
            if let Some((_, Some(interface))) = self.env.qualified_module(import.qualifier()) {
                captured.insert(import.module.clone(), interface.clone());
            }
        }
        captured.insert(parent.name.to_owned(), self.declarations());
        let span = self.span(node);
        syntax.imports.push(Import {
            module: parent.name.to_owned(),
            alias: None,
            exposing: vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span,
            }],
            plugins: Vec::new(),
            span,
        });
        syntax.header = Some(self.static_parameterized_header(node, path, body));
        let parameters = &syntax.header.as_ref().expect("anonymous header").parameters;
        let names: Vec<_> = parameters.iter().map(|p| p.name.as_str()).collect();
        syntax
            .imports
            .retain(|import| !names.contains(&import.qualifier()));
        for parameter in parameters {
            syntax.imports.push(Import {
                module: format!("$module:{}", parameter.name),
                alias: Some(parameter.name.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: parameter.span,
            });
        }
        for application in &syntax.applications {
            syntax.imports.push(Import {
                module: format!("{path}.{}", application.name),
                alias: Some(application.name.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: application.span,
            });
        }
        let env = &self.env;
        let lookup = |name: &str| {
            captured
                .get(name)
                .cloned()
                .or_else(|| env.static_known(name))
        };
        let child = Module {
            name: path,
            file: self.file,
            text: self.text,
            tree: &tree,
            syntax: &syntax,
            generated: parent.generated,
        };
        let (mut interface, resolved) = resolve::resolve(&child, &lookup);
        self.diagnostics.extend(resolved.into_iter().filter(|d| {
            d.code != codes::R012 || own_imports.iter().any(|i| Some(i.span) == d.span)
        }));
        let dictionaries = self.classes.local.clone();
        let (checked, _) = super::check_typed_with_static_context(
            &child,
            &lookup,
            super::builtins_of(self.options.stages_by_class),
            self.options,
            &mut interface,
            super::StaticContext {
                dictionaries: &dictionaries,
                witnesses: self.static_witnesses,
            },
        );
        self.diagnostics.extend(checked);
        Some(interface)
    }
}
