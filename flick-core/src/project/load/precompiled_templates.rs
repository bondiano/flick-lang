// Restoring checked static templates and their lexical source owners.

impl Loader<'_> {
    #[cfg(feature = "native-project")]
    fn restore_template(&mut self, name: &str) -> bool {
        let (package, module) = &self.precompiled.saved[name];
        let Some(template) = &module.interface.template else {
            return false;
        };
        let path = self.precompiled.packages[package].1.join(&module.path);
        #[cfg(feature = "native-project")]
        if self.host.read(&path).ok().is_some_and(|bytes| {
            str::from_utf8(&bytes).map_or(true, |text| hash(text) != module.source)
        }) {
            self.diagnostics.push(Diagnostic::new(codes::P034, Severity::Error,
                format!("`{}` changed since its package was checked: `flick install` restores the package store", self.host.display(&path))));
        }
        let file = self
            .sources
            .add(self.host.display(&path), template.source.clone());
        let parse = Arc::new(syntax::parse(&template.source, file).into_module());
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, &template.source, file));
        let at = syntax
            .header
            .as_ref()
            .map_or(Span::new(file, 0, 0), |h| h.span);
        let virtual_of = template.capture.clone().map(|mut capture| {
            capture
                .definition_package
                .get_or_insert_with(|| package.clone());
            Virtual { at, capture }
        });
        let index = self.modules.len();
        self.by_name.insert(name.to_owned(), index);
        self.modules.push(Loaded {
            name: name.to_owned(),
            path,
            file,
            root: Root::Package(package.clone()),
            parse,
            syntax,
            virtual_of,
            member_of: module.interface.member_of.clone(),
            scope_of: None,
            expansion: None,
            editing: Vec::new(),
            loader: None,
        });
        if let Some(target) = &template.materialized_body {
            self.select_expression_body(index, target.clone(), at);
        }
        true
    }

    /// A saved ordinary module can own a parameterized module's lexical declarations even
    /// when only the nested template is restored as a loaded source module.
    #[cfg(feature = "native-project")]
    pub(super) fn scope_source(&mut self, name: &str) -> Option<Loaded> {
        let (package, saved) = self.precompiled.saved.get(name)?;
        let source = saved
            .interface
            .template
            .as_ref()
            .map(|t| &t.source)
            .or(saved.source_text.as_ref())?
            .clone();
        let path = self.precompiled.packages[package].1.join(&saved.path);
        if self.host.read(&path).ok().is_some_and(|bytes| {
            str::from_utf8(&bytes).map_or(true, |text| hash(text) != saved.source)
        }) {
            self.diagnostics.push(Diagnostic::new(codes::P034, Severity::Error, format!("`{}` changed since its package was checked: `flick install` restores the package store", self.host.display(&path))));
        }
        let file = self.sources.add(self.host.display(&path), source.clone());
        let parse = Arc::new(syntax::parse(&source, file).into_module());
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, &source, file));
        Some(Loaded {
            name: name.into(),
            path,
            file,
            root: Root::Package(package.clone()),
            parse,
            syntax,
            virtual_of: None,
            member_of: None,
            scope_of: None,
            expansion: None,
            editing: Vec::new(),
            loader: None,
        })
    }
}
