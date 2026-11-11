impl Loader<'_> {
    pub(super) fn instantiate(&mut self, parent: usize, application: &ModuleApplication) {
        let name = format!("{}.{}", self.modules[parent].name, application.name);
        if let Some(&existing) = self.by_name.get(&name) {
            let expected = PathBuf::from(format!(
                "{}#{}",
                self.modules[parent].path.display(),
                application.name
            ));
            if self.modules[existing].path != expected {
                self.diagnostics.push(Diagnostic::error(
                    codes::R002,
                    format!(
                        "virtual module `{name}` conflicts with `{}`",
                        self.host.display(&self.modules[existing].path)
                    ),
                    application.span,
                ));
            }
            return;
        }
        if !self.expressions_active.insert(name.clone()) {
            self.diagnostics.push(Diagnostic::error(
                codes::R006,
                "cyclic module expression",
                application.span,
            ));
            return;
        }
        if let Some(annotation) = &application.annotation {
            // Ascription follows evaluation of the underlying expression. In
            // particular, a parameter alias must first acquire its restricted
            // parameter view before its own result annotation is checked.
            let mut unannotated = application.clone();
            unannotated.annotation = None;
            if let Some(target) =
                self.module_expression(parent, &application.expression, None, &unannotated)
            {
                self.alias_expression(
                    parent,
                    Some(&application.name),
                    &target,
                    Some(annotation),
                    application.span,
                    None,
                );
            }
        } else {
            self.module_expression(
                parent,
                &application.expression,
                Some(&application.name),
                application,
            );
        }
        self.expressions_active.remove(&name);
    }

    pub(super) fn materialize_expression_body(&mut self, module: usize) {
        let Some(header) = self.modules[module].syntax.header.clone() else {
            return;
        };
        if !header.parameters.is_empty() || header.materialized_body.is_some() {
            return;
        }
        let Some(body) = header.body else {
            return;
        };
        // Mark entry before resolving the expression, so a self-reference is
        // diagnosed rather than recursively materialized.
        Arc::make_mut(&mut self.modules[module].syntax)
            .header
            .as_mut()
            .expect("header")
            .materialized_body = Some(self.modules[module].name.clone());
        let application = ModuleApplication {
            expression: body.clone(),
            annotation: None,
            name: String::new(),
            template: String::new(),
            template_span: header.span,
            arguments: Vec::new(),
            argument_spans: Vec::new(),
            span: header.span,
        };
        let Some(target) = self.module_expression(module, &body, None, &application) else {
            return;
        };
        if target == self.modules[module].name {
            self.diagnostics.push(Diagnostic::error(
                codes::R006,
                "cyclic module expression",
                header.span,
            ));
            return;
        }
        let syntax = Arc::make_mut(&mut self.modules[module].syntax);
        syntax.header.as_mut().expect("header").materialized_body = Some(target.clone());
        syntax.imports.push(Import {
            module: target,
            alias: None,
            exposing: vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span: header.span,
            }],
            plugins: Vec::new(),
            span: header.span,
        });
    }

    pub(super) fn callable_template(&mut self, mut template: usize, at: Span) -> Option<usize> {
        let mut aliases = Vec::new();
        loop {
            if aliases.contains(&template) {
                self.diagnostics
                    .push(Diagnostic::error(codes::R006, "cyclic template alias", at));
                return None;
            }
            aliases.push(template);
            self.materialize_expression_body(template);
            let Some(target) = self.modules[template]
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.materialized_body.clone())
            else {
                break;
            };
            let actual = self.find_module(template, &target, at)?;
            template = actual;
        }
        Some(template)
    }
}
