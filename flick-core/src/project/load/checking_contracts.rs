impl Checking<'_> {
    fn virtual_contract_inputs(
        &self,
        loaded: &Loaded,
        comparable: &[Option<Arc<Interface>>],
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Vec<Option<Arc<Interface>>> {
        let mut inputs = Vec::new();
        if let Some(context) = &loaded.virtual_of {
            let origins = context
                .capture
                .bindings
                .iter()
                .map(|b| b.origin.module.as_str());
            let callee = context.capture.callee.as_ref();
            for owner in origins.chain(callee.map(CalleeOrigin::signature_owner)) {
                inputs.push(match self.by_name.get(owner) {
                    Some(&j) => comparable[j].clone(),
                    None => self.from_packages.get(owner).map(comparable_copy),
                });
            }
            for argument in callee
                .into_iter()
                .flat_map(CalleeOrigin::contract_arguments)
            {
                // Include the caller's view through all ancestor ascriptions.
                inputs.push(
                    Self::argument_interface(argument, lookup, contracts)
                        .as_ref()
                        .map(comparable_copy),
                );
            }
        }
        inputs
    }

    fn argument_interface(
        argument: &StaticArgument,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<Interface> {
        fn view(
            module: &str,
            access: &[String],
            lookup: &dyn Fn(&str) -> Option<Interface>,
            contracts: &dyn Fn(&str) -> Option<Interface>,
        ) -> Option<Interface> {
            let raw = contracts(module).or_else(|| lookup(module))?;
            if let Some(member) = &raw.member_of
                && !access.contains(&member.parent)
            {
                return view(&member.parent, access, lookup, contracts)?
                    .modules
                    .remove(&member.name);
            }
            Some(raw)
        }
        view(
            &argument.module,
            &argument.access_modules,
            lookup,
            contracts,
        )
    }

    fn callee_signature_explicit(
        context: &Virtual,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> bool {
        match &context.capture.callee {
            Some(CalleeOrigin::Module(name)) => {
                contracts(name).is_some_and(|i| i.signature_explicit)
            }
            Some(CalleeOrigin::Member(member)) => {
                Self::argument_interface(member, lookup, contracts)
                    .is_some_and(|i| i.signature_explicit)
            }
            Some(CalleeOrigin::Parameter { .. }) => true,
            None => false,
        }
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
            result.instantiate_parameter(
                &binder,
                &Self::argument_interface(argument, lookup, contracts)?,
            );
            signature = *result;
        }
        let types::ModuleSignature::Parameterized { parameter, .. } = signature else {
            return None;
        };
        Some(*parameter)
    }

    fn callee_signature(
        context: &Virtual,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
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
                let actual = Self::argument_interface(member, lookup, contracts)?;
                Some(
                    actual
                        .parameterized
                        .clone()
                        .unwrap_or_else(|| types::ModuleSignature::from_interface(&actual)),
                )
            }
            CalleeOrigin::Parameter {
                binding,
                argument,
                members,
            } => {
                let signature = Self::binding_signature(binding, lookup, contracts)?;
                let mut actual =
                    signature.project(&Self::argument_interface(argument, lookup, contracts)?);
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

    fn virtual_result_signature(
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        let context = loaded.virtual_of.as_ref()?;
        // An inferred shape validates applications, but does not introduce an
        // abstraction boundary. In particular it must not erase ADT constructors
        // explicitly exposed by an unannotated private factory's source body.
        if !Self::callee_signature_explicit(context, lookup, contracts) {
            return None;
        }
        let mut signature = Self::callee_signature(context, lookup, contracts)?;
        for argument in &context.capture.arguments {
            let types::ModuleSignature::Parameterized {
                binder, mut result, ..
            } = signature
            else {
                return None;
            };
            result.instantiate_parameter(
                &binder,
                &Self::argument_interface(argument, lookup, contracts)?,
            );
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
            let Some(actual) = Self::argument_interface(argument, lookup, contracts) else {
                continue;
            };
            if Self::module_contract_errors(
                &signature,
                &actual,
                context.at,
                &argument.module,
                &mut errors,
            ) {
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
        if let Some(mut signature) = Self::callee_signature(context, lookup, contracts) {
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
                let Some(actual) = Self::argument_interface(argument, lookup, contracts) else {
                    break;
                };
                Self::module_contract_errors(
                    &parameter,
                    &actual,
                    context.at,
                    &argument.module,
                    &mut errors,
                );
                result.instantiate_parameter(&binder, &actual);
                signature = *result;
            }
        } else if let Some(CalleeOrigin::Member(member)) = &context.capture.callee {
            errors.push(Diagnostic::error(
                codes::R018,
                format!(
                    "module `{}` is not exported through this namespace view",
                    member.module
                ),
                context.at,
            ));
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
