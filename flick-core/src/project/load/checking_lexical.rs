fn lexical_scope_output(mut output: Output) -> Output {
    let failed = output
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    let parameterized = output.interface.parameterized.take();
    output.interface = if failed {
        Interface {
            module: output.interface.module.clone(),
            ..Interface::default()
        }
    } else {
        (*output.declarations).clone()
    };
    output.interface.parameterized =
        parameterized.map(|s| if failed { s.inputs_only() } else { s });
    output.interface.private = true;
    output.exported = output.interface.clone();
    output.ir = None;
    output.diagnostics.retain(|d| d.severity == Severity::Error);
    output
}

impl Checking<'_> {
    /// Combine outer-to-inner definition scopes without changing entity
    /// owners. Only this private lookup view receives unexported declarations.
    fn lexical_environment(
        &self,
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        syntax: &mut ModuleSyntax,
    ) -> (BTreeMap<String, Interface>, Vec<(types::Type, types::Dict)>) {
        let mut views = BTreeMap::new();
        let mut dictionaries = Vec::new();
        let mut scope = Interface {
            module: format!("$lexical:{}", loaded.name),
            ..Interface::default()
        };
        for captured in loaded.virtual_of.iter().flat_map(|v| &v.capture.scopes) {
            let Some(prefix) = lookup(&captured.key()) else {
                continue;
            };
            for export in &prefix.exports {
                scope.exports.retain(|known| {
                    (known.entity.namespace, &known.entity.name)
                        != (export.entity.namespace, &export.entity.name)
                });
                scope.exports.push(export.clone());
            }
            scope.modules.extend(prefix.modules.clone());
            scope.instances.extend(prefix.instances.clone());
            let mut arrow = prefix.parameterized.as_ref();
            if let Some(header) = self
                .by_name
                .get(&captured.key())
                .and_then(|&i| self.modules[i].syntax.header.as_ref())
            {
                for parameter in &header.parameters {
                    if let Some(types::ModuleSignature::Parameterized {
                        parameter: input,
                        result,
                        ..
                    }) = arrow
                    {
                        if let Some(actual) = prefix.modules.get(&parameter.name) {
                            dictionaries.extend(types::module_local_dictionaries(
                                &input.definition(),
                                actual,
                            ));
                        }
                        arrow = Some(result);
                    }
                }
            }
        }
        if scope.exports.is_empty() && scope.modules.is_empty() {
            return (views, dictionaries);
        }
        let at = syntax
            .header
            .as_ref()
            .map_or(Span::new(loaded.file, 0, 0), |h| h.span);
        for (alias, child) in &scope.modules {
            if syntax.imports.iter().any(|i| i.qualifier() == alias) {
                continue;
            }
            let key = format!("{}:{alias}", scope.module);
            views.insert(key.clone(), child.clone());
            syntax.imports.push(Import {
                module: key,
                alias: Some(alias.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: at,
            });
        }
        syntax.imports.push(Import {
            module: scope.module.clone(),
            alias: None,
            exposing: vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span: at,
            }],
            plugins: Vec::new(),
            span: at,
        });
        views.insert(scope.module.clone(), scope);
        (views, dictionaries)
    }
}
