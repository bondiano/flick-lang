impl Checking<'_> {
    fn binding_signature(
        origin: &BindingOrigin,
        lookup: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        let mut signature = lookup(&origin.module)?.functor?;
        for argument in &origin.prefix {
            let types::ModuleSignature::Functor {
                binder, mut result, ..
            } = signature
            else {
                return None;
            };
            result.rename_parameter(&binder, &lookup(argument)?.module);
            signature = *result;
        }
        let types::ModuleSignature::Functor { parameter, .. } = signature else {
            return None;
        };
        Some(*parameter)
    }

    fn callee_signature(
        context: &Virtual,
        lookup: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        match context.callee.as_ref()? {
            CalleeOrigin::Module(name) => lookup(name)?.functor,
            CalleeOrigin::Parameter { binding, members } => {
                let mut signature = Self::binding_signature(binding, lookup)?;
                for name in members {
                    let types::ModuleSignature::Structure { requirements } = signature else {
                        return None;
                    };
                    signature = requirements.into_iter().find_map(|r| match r {
                        types::ModuleRequirement::Module {
                            name: member,
                            signature,
                        } if member == *name => Some(*signature),
                        _ => None,
                    })?;
                }
                Some(signature)
            }
        }
    }

    fn virtual_result_signature(
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
    ) -> Option<types::ModuleSignature> {
        let context = loaded.virtual_of.as_ref()?;
        let mut signature = Self::callee_signature(context, lookup)?;
        for argument in &context.arguments {
            let types::ModuleSignature::Functor {
                binder, mut result, ..
            } = signature
            else {
                return None;
            };
            result.rename_parameter(&binder, &lookup(argument)?.module);
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

    fn virtual_contracts(
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
    ) -> ContractView {
        let mut projections = BTreeMap::new();
        let mut dictionaries = Vec::new();
        let mut errors = Vec::new();
        let Some(context) = &loaded.virtual_of else {
            return (projections, dictionaries, errors);
        };
        for ((_, _, argument), origin) in context.parameters.iter().zip(&context.bindings) {
            let Some(signature) = Self::binding_signature(origin, lookup) else {
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
                    if dictionaries.iter().any(|(known, _)| *known == head) {
                        errors.push(Diagnostic::error(
                            codes::T066,
                            "more than one static argument supplies the same local dictionary in this parameterized module",
                            context.at,
                        ));
                    } else {
                        dictionaries.push((head, dictionary));
                    }
                }
                projections.insert(argument.clone(), signature.project(&actual));
            }
        }
        if let Some(mut signature) = Self::callee_signature(context, lookup) {
            for argument in &context.arguments {
                let types::ModuleSignature::Functor {
                    binder,
                    parameter,
                    mut result,
                    ..
                } = signature
                else {
                    errors.push(Diagnostic::error(
                        codes::T002,
                        "module expression is not a functor",
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
                result.rename_parameter(&binder, &actual.module);
                signature = *result;
            }
        }
        let mut reported = Vec::new();
        errors.retain(|diagnostic| {
            let key = (diagnostic.code, diagnostic.message.clone(), diagnostic.span);
            if reported.contains(&key) { false } else { reported.push(key); true }
        });
        (projections, dictionaries, errors)
    }
}
