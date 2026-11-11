impl Loader<'_> {
    fn virtual_source(
        &self,
        parent: usize,
        template: usize,
        name: &str,
        header: &resolve::Header,
        application: &ModuleApplication,
    ) -> Option<String> {
        let source = self.sources.get(self.modules[template].file).text.clone();
        let header_node = self.modules[template]
            .parse
            .tree
            .nodes()
            .find(|node| matches!(node.kind, K::ModuleHeader | K::ModuleApplyDecl))?;
        let exposed = header
            .exposing
            .as_ref()
            .map(|items| {
                let list = items
                    .iter()
                    .map(|item| match item.kind {
                        ItemKind::All => "..".to_owned(),
                        ItemKind::ConAll => format!("{}(..)", item.name),
                        _ => item.name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(" exposing ({list})")
            })
            .unwrap_or_default();
        let visibility = self.expression_visibility(parent, Some(&application.name));
        let result = header
            .result
            .as_ref()
            .map_or(String::new(), |r| format!(" : {}", r.text(&source)));
        let groups = header_node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .skip(application.argument_groups.len())
            .map(|n| n.text(&source))
            .collect::<String>();
        let expression_body = header
            .body
            .as_ref()
            .map_or(String::new(), |body| format!(" = {}", body.text(&source)));
        let mut generated =
            format!("{visibility}module {name}{groups}{result}{exposed}{expression_body}\n");
        for (parameter, argument) in header.parameters.iter().zip(&application.arguments) {
            generated.push_str("import ");
            generated.push_str(argument);
            generated.push_str(" as ");
            generated.push_str(&parameter.name);
            generated.push('\n');
        }
        let suffix = &source[header_node.range().1 as usize..];
        if header_node.has_token(K::Eq) && header.body.is_none() {
            let indent = self.modules[functor]
                .parse
                .tree
                .nodes()
                .find(|node| node.range().0 > header_node.range().1)
                .map_or(0, |node| {
                    source[..node.range().0 as usize]
                        .rsplit('\n')
                        .next()
                        .map_or(0, str::len)
                });
            for line in suffix.split_inclusive('\n') {
                let spaces = line.bytes().take_while(|b| *b == b' ').count().min(indent);
                generated.push_str(&line[spaces..]);
            }
        } else {
            generated.push_str(suffix);
        }
        Some(generated)
    }

    pub(super) fn instantiate_named(
        &mut self,
        parent: usize,
        application: &ModuleApplication,
        callee: CalleeOrigin,
    ) {
        let name = format!("{}.{}", self.modules[parent].name, application.name);
        let path = PathBuf::from(format!(
            "{}#{}",
            self.modules[parent].path.display(),
            application.name
        ));
        if let Some(&existing) = self.by_name.get(&name) {
            if self.modules[existing].path != path {
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
        let Some(template) =
            self.find_module(parent, &application.template, application.template_span)
        else {
            return;
        };
        let Some(template) = self.callable_template(template, application.span) else {
            return;
        };
        let Some(header) = self.modules[template].syntax.header.clone() else {
            return;
        };
        if header.parameters.is_empty()
            || application.argument_groups.is_empty()
            || !header
                .parameter_groups
                .starts_with(&application.argument_groups)
            || application.arguments.len() != application.argument_groups.iter().sum::<usize>()
        {
            self.diagnostics.push(Diagnostic::error(
                codes::T002,
                format!(
                    "`{}` expects {} static module argument(s), found {}",
                    application.template,
                    header.parameters.len(),
                    application.arguments.len()
                ),
                application.span,
            ));
            return;
        }
        let Some((template, trace)) = self.expansion_trace(parent, functor, application.span)
        else {
            return;
        };
        let mut normalized = application.clone();
        for (argument, span) in normalized
            .arguments
            .iter_mut()
            .zip(&application.argument_spans)
        {
            let Some(name) = self.module_reference(parent, argument, *span) else {
                return;
            };
            *argument = name;
        }
        let application = &normalized;
        let Some(generated) = self.virtual_source(parent, template, &name, &header, application)
        else {
            return;
        };
        let file = self
            .sources
            .add(format!("<parameterized module {name}>"), generated);
        let generated = &self.sources.get(file).text;
        let parse = Arc::new(syntax::parse(generated, file));
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, generated, file));
        let index = self.modules.len();
        self.by_name.insert(name.clone(), index);
        let context =
            self.application_context(functor, &header, application, callee, template, trace);
        self.modules.push(Loaded {
            path,
            file,
            root: self.modules[parent].root.clone(),
            parse,
            syntax,
            name,
            expansion: None,
            editing: Vec::new(),
            loader: None,
            virtual_of: Some(context),
            member_of: Some(StaticMember {
                parent: self.modules[parent].name.clone(),
                name: application.name.clone(),
            }),
            scope_of: None,
        });
    }

    fn application_context(
        &self,
        functor: usize,
        header: &resolve::Header,
        application: &ModuleApplication,
        callee: CalleeOrigin,
        template: String,
        trace: Vec<String>,
    ) -> Virtual {
        let inherited = self.modules[functor].virtual_of.as_ref();
        let mut capture = inherited.map_or_else(StaticCapture::default, |v| v.capture.clone());
        if !capture.access_modules.contains(&self.modules[functor].name) {
            capture
                .access_modules
                .push(self.modules[functor].name.clone());
        }
        for (index, (parameter, argument)) in header
            .parameters
            .iter()
            .zip(&application.arguments)
            .enumerate()
        {
            capture.bindings.push(StaticBinding {
                alias: parameter.name.clone(),
                argument: argument.clone(),
                origin: BindingOrigin {
                    module: self.modules[functor].name.clone(),
                    parameter: index,
                    group: header
                        .parameter_groups
                        .iter()
                        .scan(0, |start, size| {
                            *start += size;
                            Some(*start)
                        })
                        .position(|end| end > index)
                        .unwrap_or(0),
                    prefix: application.arguments[..index].to_vec(),
                },
            });
        }
        if let Root::Package(package) = &self.modules[functor].root
            && !capture.access_packages.contains(package)
        {
            capture.access_packages.push(package.clone());
        }
        capture.definition_package = self.modules[functor]
            .definition_package()
            .map(str::to_owned);
        capture.callee = Some(callee);
        capture.inherited_imports += application.arguments.len();
        capture.arguments.clone_from(&application.arguments);
        capture.template = Some(template);
        capture.trace = trace;
        Virtual {
            at: application.span,
            capture,
        }
    }

    fn expansion_trace(
        &mut self,
        parent: usize,
        functor: usize,
        at: Span,
    ) -> Option<(String, Vec<String>)> {
        let context = self.modules[functor].virtual_of.as_ref();
        let template = context
            .and_then(|v| v.capture.template.clone())
            .unwrap_or_else(|| self.modules[functor].name.clone());
        let mut trace = self.modules[parent]
            .virtual_of
            .as_ref()
            .map_or_else(Vec::new, |v| v.capture.trace.clone());
        if let Some(context) = context {
            let mut captured = context.capture.trace.as_slice();
            // A partial application resumes its own last expansion. Earlier
            // captured expansions remain active in its lexical environment.
            if context.capture.template.is_some() && captured.last() == Some(&template) {
                captured = &captured[..captured.len() - 1];
            }
            trace.extend_from_slice(captured);
        }
        if trace.contains(&template) {
            trace.push(template);
            self.diagnostics.push(Diagnostic::error(
                codes::R006,
                format!("recursive static module expansion: {}", trace.join(" -> ")),
                at,
            ));
            return None;
        }
        trace.push(template.clone());
        Some((template, trace))
    }
}
