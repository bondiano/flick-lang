impl super::Checker<'_> {
    pub(super) fn fresh_module_witness(&mut self) -> String {
        loop {
            let id = self.witness_counter;
            self.witness_counter = id
                .checked_add(1)
                .expect("module witness identity exhausted");
            let witness = format!("{}.$witness{id}", self.env.module);
            if !self.env.has_witness_namespace(&witness) {
                return witness;
            }
        }
    }

    pub(super) fn materialize_witness_view(
        &mut self,
        signature: &super::ModuleSignature,
        witness: &str,
    ) -> Result<Interface, String> {
        let view = signature.interface(witness);
        self.env.register_witness_types(witness, &view)?;
        Ok(view)
    }

    /// Source member aliases map to their declared identities, never alias bodies.
    pub(super) fn witness_bindings(
        signature: &super::ModuleSignature,
        view: &Interface,
        alias: &str,
    ) -> Vec<Opened> {
        let super::ModuleSignature::Structure { requirements } = signature else {
            return Vec::new();
        };
        let requirements = super::ModuleSignature::instantiate_members(requirements, view);
        let mut types = Vec::new();
        let mut nested = Vec::new();
        for requirement in &requirements {
            let (name, identity) = match requirement {
                ModuleRequirement::Type { name, .. } => (name, None),
                ModuleRequirement::Kind { name, identity, .. }
                | ModuleRequirement::Nominal { name, identity, .. } => (name, identity.as_ref()),
                ModuleRequirement::Module {
                    name, signature, ..
                } => {
                    if let Some(child) = view.modules.get(name) {
                        nested.extend(Self::witness_bindings(
                            signature,
                            child,
                            &format!("{alias}.{name}"),
                        ));
                    }
                    continue;
                }
                ModuleRequirement::Value { .. } | ModuleRequirement::Dictionary { .. } => continue,
            };
            let export = identity.map_or_else(
                || view.find(Namespace::Type, name),
                |identity| {
                    view.exports
                        .iter()
                        .find(|export| export.entity == *identity)
                },
            );
            if let Some(export) = export
                && let Some(definition) = &export.definition
            {
                types.push((
                    name.clone(),
                    Type::con(&export.entity.module, &export.entity.name),
                    definition.kind.clone(),
                ));
            }
        }
        let values = requirements
            .iter()
            .filter_map(|requirement| {
                let ModuleRequirement::Value { name, .. } = requirement else {
                    return None;
                };
                view.exports
                    .iter()
                    .find(|export| {
                        export.entity.namespace == Namespace::Value
                            && export.entity.module == view.module
                            && export.entity.name == *name
                    })
                    .and_then(|export| export.scheme.as_ref().map(|ty| (name.clone(), ty.clone())))
            })
            .collect();
        let mut opened = vec![Opened {
            alias: alias.into(),
            types,
            values,
        }];
        opened.extend(nested);
        opened
    }
    /// Resolve solved inference data before lexical closing. Unsolved kinds
    /// stay unsolved here; declaration-wide defaulting retains its timing.
    pub(super) fn resolved_witness_type(&mut self, ty: &Type) -> Type {
        fn kinds(checker: &mut super::Checker<'_>, ty: &Type) -> Type {
            let mut mapped = ty.map_children(&mut |child| kinds(checker, child));
            if let Type::Forall { vars, .. } = &mut mapped {
                for binder in vars {
                    binder.kind = checker.kinds.zonk(&binder.kind, false);
                }
            }
            mapped
        }
        let ty = self.zonk(ty);
        kinds(self, &ty)
    }

    /// Saturated transparent members do not make an otherwise closed result
    /// existential. Partial/cyclic aliases and abstract identities still do.
    fn witness_escapes(&self, ty: &Type, witness: &str) -> bool {
        fn visit(
            checker: &super::Checker<'_>,
            ty: &Type,
            witness: &str,
            aliases: &mut Vec<(String, String)>,
        ) -> bool {
            let ty = checker.zonk(ty);
            let (head, args) = ty.spine();
            let alias = if let Type::Con { module, name } = head {
                checker.env.type_def(module, name).and_then(|definition| {
                    matches!(&definition.form, Form::Alias { params, opaque: false, .. } if args.len() >= params.len())
                        .then(|| (module.clone(), name.clone()))
                })
            } else {
                None
            };
            if let Some(alias) = &alias {
                if aliases.contains(alias) {
                    return witness_scope_occurs(&ty, witness);
                }
                aliases.push(alias.clone());
            }
            let reduced = checker.witness_head(&ty);
            let mut escapes = witness_local_scope_occurs(&reduced, witness);
            reduced.for_each_child(&mut |child| escapes |= visit(checker, child, witness, aliases));
            if alias.is_some() {
                aliases.pop();
            }
            escapes
        }
        visit(self, ty, witness, &mut Vec::new())
    }
}
