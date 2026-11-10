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
                    result = S::Functor {
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

    fn bind_static_signature(&mut self, alias: &str, signature: &super::ModuleSignature) {
        self.bind_static_witness(alias, &format!("$module:{alias}"), signature);
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
        if interface.functor.is_some() {
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
                    Form::Alias { body, .. } => body.clone(),
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
}
