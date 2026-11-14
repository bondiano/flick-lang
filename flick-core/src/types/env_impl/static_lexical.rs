impl Env<'_> {
    /// The canonical declaration kind; an implicit dictionary head derives
    /// its kind ONLY from an actual class declaration, never its spelling alone.
    pub(in crate::types) fn constructor_kind(&self, module: &str, name: &str) -> Option<Kind> {
        if let Some(definition) = self.type_def(module, name) {
            return Some(definition.kind.clone());
        }
        let class = name.strip_suffix("$Dict")?;
        let definition = self.type_def(module, class)?;
        matches!(definition.form, Form::Class { .. })
            .then(|| super::convert::as_value_kind(definition.kind.clone()))
    }

    /// A checked lexical prefix retains its defining entities, including
    /// private members. Only the temporary static-expression environment
    /// opens it; the enclosing module is still checked independently.
    pub(super) fn static_lexical(&mut self, interface: &Interface) -> StaticLexical {
        let mut saved = StaticLexical {
            names: Vec::new(),
            instances: self.instances.len(),
        };
        learn_exports(
            &mut self.types,
            &mut self.values,
            &mut self.parents,
            interface,
        );
        for instance in &interface.instances {
            if !self
                .builtins
                .seen
                .contains(&(instance.module.clone(), instance.name.clone()))
                && !self
                    .instances
                    .iter()
                    .any(|known| known.module == instance.module && known.name == instance.name)
            {
                self.instances.push(instance.clone());
            }
        }
        for export in &interface.exports {
            debug_assert_eq!(export.entity.module, self.module);
            let key = (export.entity.namespace, export.entity.name.clone());
            let ambiguous = self.ambiguous.remove(&key);
            let previous = self.scope.insert(key.clone(), export.entity.clone());
            saved.names.push((key, previous, ambiguous));
        }
        saved
    }

    pub(super) fn restore_static_lexical(&mut self, interface: &Interface, saved: StaticLexical) {
        for export in &interface.exports {
            let entity = &export.entity;
            self.types.remove(&entity.module, &entity.name);
            self.types
                .remove(&entity.module, &format!("{}.Impl", entity.name));
            self.values.remove(entity);
            self.parents.remove(entity);
        }
        self.instances.truncate(saved.instances);
        for (key, previous, ambiguous) in saved.names {
            if let Some(previous) = previous {
                self.scope.insert(key.clone(), previous);
            } else {
                self.scope.remove(&key);
            }
            if ambiguous {
                self.ambiguous.insert(key);
            }
        }
    }

    pub(super) fn static_known(&self, name: &str) -> Option<Interface> {
        self.interfaces
            .get(name)
            .and_then(Option::as_ref)
            .or_else(|| self.builtins.interfaces.get(name))
            .cloned()
    }
    pub(super) fn static_parameter(
        &mut self,
        alias: &str,
        interface: Interface,
    ) -> Result<(), String> {
        self.learn(&interface)?;
        // A symbolic result was unresolved during name resolution. Once its
        // checked view is installed, this checker owns missing-member errors.
        if self
            .qualified_module(alias)
            .is_none_or(|(_, i)| i.is_none())
            || interface.module.starts_with("$module:")
        {
            self.imported.remove(alias);
        }
        // Several aliases may expose different signatures of one module.
        // Keep their namespace views separate from its nominal identity.
        let view = format!("$static:{}:{alias}", self.module);
        self.qualifiers.insert(alias.to_owned(), view.clone());
        self.interfaces.insert(view, Some(interface));
        Ok(())
    }
}
