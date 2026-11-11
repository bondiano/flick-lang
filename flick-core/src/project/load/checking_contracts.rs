impl Checking<'_> {
    fn virtual_contract_dependencies(
        &self,
        loaded: &Loaded,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Vec<String> {
        let Some(v) = &loaded.virtual_of else {
            return Vec::new();
        };
        let mut names: Vec<_> = v
            .capture
            .bindings
            .iter()
            .map(|b| b.origin.module.clone())
            .collect();
        match &v.capture.callee {
            Some(CalleeOrigin::Module(name)) => names.push(name.clone()),
            Some(CalleeOrigin::Member(member)) => names.push(member.parent.clone()),
            Some(CalleeOrigin::Parameter { binding, .. }) => {
                names.push(binding.module.clone());
            }
            None => {}
        }
        let caller = loaded
            .member_of
            .as_ref()
            .and_then(|member| self.by_name.get(&member.parent))
            .map_or(loaded, |&index| &self.modules[index]);
        let mut index = 0;
        while index < names.len() {
            if let Some(member) = contracts(&names[index]).and_then(|i| i.member_of)
                && !caller.can_access_member(&member)
                && !names.contains(&member.parent)
            {
                names.push(member.parent);
            }
            index += 1;
        }
        names
    }

    fn public_contract_interface(
        &self,
        loaded: &Loaded,
        name: &str,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<Interface> {
        let actual = contracts(name)?;
        let caller = loaded
            .member_of
            .as_ref()
            .and_then(|member| self.by_name.get(&member.parent))
            .map_or(loaded, |&index| &self.modules[index]);
        if let Some(member) = &actual.member_of
            && !caller.can_access_member(member)
        {
            return self
                .public_contract_interface(loaded, &member.parent, contracts)?
                .modules
                .remove(&member.name);
        }
        Some(actual)
    }

    fn binding_view_name(loaded: &Loaded, alias: &str) -> String {
        format!("$binding:{}:{alias}", loaded.name)
    }
    fn binding_signature(
        origin: &BindingOrigin,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        let mut signature = contracts(&origin.module)?.parameterized?;
        for argument in &origin.prefix {
            let types::ModuleSignature::Parameterized {
                binder, mut result, ..
            } = signature
            else {
                return None;
            };
            result.instantiate_parameter(&binder, &lookup(argument)?);
            signature = *result;
        }
        let types::ModuleSignature::Parameterized { parameter, .. } = signature else {
            return None;
        };
        Some(*parameter)
    }

    fn callee_signature(
        &self,
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        let context = loaded.virtual_of.as_ref()?;
        match context.capture.callee.as_ref()? {
            CalleeOrigin::Module(name) => {
                let actual = contracts(name)?;
                Some(
                    actual
                        .parameterized
                        .clone()
                        .unwrap_or_else(|| types::ModuleSignature::from_interface(&actual)),
                )
            }
            CalleeOrigin::Member(member) => {
                let actual = self
                    .public_contract_interface(loaded, &member.parent, contracts)?
                    .modules
                    .get(&member.name)?
                    .clone();
                Some(
                    actual
                        .parameterized
                        .clone()
                        .unwrap_or_else(|| types::ModuleSignature::from_interface(&actual)),
                )
            }
            CalleeOrigin::Parameter { binding, members } => {
                let signature = Self::binding_signature(binding, lookup, contracts)?;
                let argument = context
                    .capture
                    .bindings
                    .iter()
                    .rev()
                    .find(|b| &b.origin == binding)?;
                let mut actual = signature.project(&lookup(&argument.argument)?);
                for name in members {
                    actual = actual.modules.remove(name)?;
                }
                Some(
                    actual
                        .parameterized
                        .clone()
                        .unwrap_or_else(|| types::ModuleSignature::from_interface(&actual)),
                )
            }
        }
    }

    fn callee_has_explicit_result(
        &self,
        loaded: &Loaded,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> bool {
        loaded
            .virtual_of
            .as_ref()
            .is_some_and(|v| match &v.capture.callee {
                Some(CalleeOrigin::Module(name)) => {
                    contracts(name).is_some_and(|i| i.signature_explicit)
                }
                Some(CalleeOrigin::Member(member)) => self
                    .public_contract_interface(loaded, &member.parent, contracts)
                    .and_then(|i| i.modules.get(&member.name).map(|m| m.signature_explicit))
                    .unwrap_or(false),
                Some(CalleeOrigin::Parameter { .. }) => true,
                None => false,
            })
    }

    fn virtual_result_signature(
        &self,
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        let context = loaded.virtual_of.as_ref()?;
        let mut signature = self.callee_signature(loaded, lookup, contracts)?;
        for argument in &context.capture.arguments {
            let types::ModuleSignature::Parameterized {
                binder, mut result, ..
            } = signature
            else {
                return None;
            };
            result.instantiate_parameter(&binder, &lookup(argument)?);
            signature = *result;
        }
        Some(signature)
    }

    fn module_contract_errors(
        signature: &types::ModuleSignature,
        actual: &Interface,
        at: Span,
        label: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        let mismatches = types::check_module_contract(&signature.definition(), actual);
        let valid = mismatches.is_empty();
        for mismatch in mismatches {
            let (code, member, detail) = match mismatch {
                types::ModuleMismatch::Missing(name) => (codes::T060, name, "is not exported"),
                types::ModuleMismatch::NominalIdentity(name) => {
                    (codes::T062, name, "has the wrong nominal identity")
                }
                types::ModuleMismatch::Kind(name)
                | types::ModuleMismatch::TypeEquality(name)
                | types::ModuleMismatch::ValueType(name)
                | types::ModuleMismatch::Shape(name) => {
                    (codes::T061, name, "has the wrong kind, type or shape")
                }
            };
            diagnostics.push(Diagnostic::error(
                code,
                format!("{label}: `{member}` {detail}"),
                at,
            ));
        }
        valid
    }

    fn virtual_result_errors(
        expected: &mut types::ModuleSignature,
        actual: &Interface,
        context: &Virtual,
        witnesses: &[Interface],
        errors: &mut Vec<Diagnostic>,
    ) {
        let mut actual = actual.clone();
        for witness in witnesses {
            expected.normalize_in(witness);
            types::normalize_module_interface(&mut actual, witness);
        }
        Self::module_contract_errors(
            expected,
            &actual,
            context.at,
            "parameterized module result",
            errors,
        );
    }

    fn virtual_contracts(
        &self,
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> ContractView {
        let mut projections = BTreeMap::new();
        let mut dictionaries = Vec::new();
        let mut dictionary_scopes = Vec::new();
        let mut errors = Vec::new();
        let Some(context) = &loaded.virtual_of else {
            return (projections, dictionaries, errors);
        };
        for binding in &context.capture.bindings {
            let (alias, argument, origin) = (&binding.alias, &binding.argument, &binding.origin);
            let Some(signature) = Self::binding_signature(origin, lookup, contracts) else {
                continue;
            };
            let Some(actual) = lookup(argument) else {
                continue;
            };
            if Self::module_contract_errors(&signature, &actual, context.at, argument, &mut errors)
            {
                for (head, dictionary) in
                    types::module_local_dictionaries(&signature.definition(), &actual)
                {
                    let key = (origin.module.clone(), origin.group, head.clone());
                    if dictionary_scopes.contains(&key) {
                        errors.push(Diagnostic::error(
                            codes::T066,
                            "more than one static argument supplies the same local dictionary in this parameterized module",
                            context.at,
                        ));
                    } else {
                        dictionary_scopes.push(key);
                        dictionaries.push((head, dictionary));
                    }
                }
                projections.insert(
                    Self::binding_view_name(loaded, alias),
                    signature.project(&actual),
                );
            }
        }
        if let Some(mut signature) = self.callee_signature(loaded, lookup, contracts) {
            for argument in &context.capture.arguments {
                let types::ModuleSignature::Parameterized {
                    binder,
                    parameter,
                    mut result,
                    ..
                } = signature
                else {
                    errors.push(Diagnostic::error(
                        codes::T002,
                        "module expression is not a parameterized module",
                        context.at,
                    ));
                    break;
                };
                let Some(actual) = lookup(argument) else {
                    break;
                };
                Self::module_contract_errors(
                    &parameter,
                    &actual,
                    context.at,
                    argument,
                    &mut errors,
                );
                result.instantiate_parameter(&binder, &actual);
                signature = *result;
            }
        }
        let mut reported = Vec::new();
        errors.retain(|diagnostic| {
            let key = (diagnostic.code, diagnostic.message.clone(), diagnostic.span);
            if reported.contains(&key) {
                false
            } else {
                reported.push(key);
                true
            }
        });
        (projections, dictionaries, errors)
    }
}
