impl Checker<'_> {
    fn finish_module_contracts(&mut self, pending: &[Pending<'_>]) {
        // Manifest nominal requirements may have observed skeletons while
        // contracts were converted. Publish snapshots of completed definitions.
        for p in pending.iter().filter(|p| p.node.kind == K::ModuleTypeDecl) {
            let Some(mut definition) = self.env.type_def(&self.env.module, &p.name).cloned() else {
                continue;
            };
            match &mut definition.form {
                Form::ModuleContract { requirements } => {
                    let mut signature = super::ModuleSignature::Structure {
                        requirements: mem::take(requirements),
                    };
                    self.refresh_nominal_requirements(&mut signature);
                    let super::ModuleSignature::Structure {
                        requirements: completed,
                    } = signature
                    else {
                        unreachable!()
                    };
                    *requirements = completed;
                }
                Form::ModuleSignature { signature } => self.refresh_nominal_requirements(signature),
                _ => {}
            }
            self.env.define_type(&p.name, definition);
        }
    }
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
                            self.refresh_nominal_requirements(signature);
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
}
