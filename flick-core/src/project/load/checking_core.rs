impl Checking<'_> {
    fn raw_interface<'i>(
        &'i self,
        interfaces: &'i [Option<Interface>],
        name: &str,
    ) -> Option<&'i Interface> {
        match self.by_name.get(name) {
            Some(&i) => interfaces[i].as_ref(),
            None => self.from_packages.get(name),
        }
    }

    fn visible_interface<'i>(
        &'i self,
        loaded: &Loaded,
        interfaces: &'i [Option<Interface>],
        name: &str,
    ) -> Option<&'i Interface> {
        let raw = self.raw_interface(interfaces, name)?;
        if let Some(member) = &raw.member_of
            && !loaded.can_access_member(member)
        {
            return self
                .visible_interface(loaded, interfaces, &member.parent)?
                .modules
                .get(&member.name);
        }
        let owner = self
            .by_name
            .get(name)
            .map(|&i| self.modules[i].root.clone())
            .or_else(|| {
                self.precompiled
                    .package_of(name)
                    .map(|p| Root::Package(p.to_owned()))
            });
        (!raw.private || owner.as_ref().is_some_and(|r| loaded.can_access(r, name))).then_some(raw)
    }

    fn namespace_import_errors(
        &self,
        loaded: &Loaded,
        interfaces: &[Option<Interface>],
    ) -> Vec<Diagnostic> {
        let known = |name: &str| self.visible_interface(loaded, interfaces, name);
        let mut errors = Vec::new();
        for import in loaded
            .syntax
            .imports
            .iter()
            .chain(&loaded.syntax.inline_imports)
        {
            if let Some(member) = self
                .raw_interface(interfaces, &import.module)
                .and_then(|raw| raw.member_of.as_ref())
                && !loaded.can_access_member(member)
                && known(&import.module).is_none()
                && known(&member.parent).is_some()
            {
                errors.push(Diagnostic::error(
                    codes::R018,
                    format!(
                        "module `{}` is not exported by `{}`",
                        import.module, member.parent
                    ),
                    import.span,
                ));
            }
        }
        errors
    }

    /// Resolves, checks and, when asked and clean, lowers module `i` against
    /// the interfaces known so far, or reuses the cached output; with a
    /// cache, also the key to cache it by.
    fn module(
        &self,
        i: usize,
        interfaces: &[Option<Interface>],
        comparable: &[Option<Arc<Interface>>],
    ) -> (Output, Option<Key>) {
        let loaded = &self.modules[i];
        let known = |name: &str| self.visible_interface(loaded, interfaces, name);
        let lookup = |name: &str| known(name).cloned();
        let contracts = |name: &str| self.raw_interface(interfaces, name).cloned();
        let check = || {
            let mut output = self.timed(loaded, &lookup, &contracts);
            if loaded.scope_of.is_some() {
                output = lexical_scope_output(output);
            }
            output
                .diagnostics
                .extend(self.namespace_import_errors(loaded, interfaces));
            output
        };
        let Some(cache) = self.cache else {
            return (check(), None);
        };
        let key = Key {
            file: loaded.file,
            // the code of its blocks too, which other modules' blocks change
            text: self.text_of(loaded).to_owned(),
            config: self.project.map(|p| p.config.clone()),
            lower: self.lower,
            cached_ir: self.cached_ir,
            imports: loaded
                .syntax
                .imports
                .iter()
                .chain(&loaded.syntax.inline_imports)
                .map(|i| {
                    let interface =
                        if self
                            .raw_interface(interfaces, &i.module)
                            .is_some_and(|raw| {
                                raw.member_of
                                    .as_ref()
                                    .is_some_and(|member| !loaded.can_access_member(member))
                            })
                        {
                            known(&i.module).map(comparable_copy)
                        } else {
                            match self.by_name.get(&i.module) {
                                Some(&j) => comparable[j].clone(),
                                None => self.from_packages.get(&i.module).map(comparable_copy),
                            }
                        };
                    (interface, self.by_name.contains_key(&i.module))
                })
                .collect(),
            virtual_contracts: loaded
                .virtual_of
                .iter()
                .flat_map(|v| {
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
                })
                .map(|module| match self.by_name.get(&module) {
                    Some(&j) => comparable[j].clone(),
                    None => self.from_packages.get(&module).map(comparable_copy),
                })
                .collect(),
            virtual_capture: loaded.virtual_of.as_ref().map(|v| v.capture.clone()),
            watching: (self.watching.iter())
                .map(|w| w.plugin.id().to_owned())
                .collect(),
        };
        match cache.entries.get(&loaded.path) {
            Some((cached, output)) if *cached == key => {
                let mut output = output.clone();
                // re-exported names are declared where their module now says
                for import in &loaded.syntax.imports {
                    if let Some(interface) = known(&import.module) {
                        redeclare(&mut output.interface, interface);
                        redeclare(&mut output.exported, interface);
                    }
                }
                debug!(module = %loaded.name, "module reused");
                (output, Some(key))
            }
            _ => (check(), Some(key)),
        }
    }

    /// [`Checking::check`], logging how long it took.
    fn timed(
        &self,
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Output {
        let (output, elapsed) = timed(|| self.check(loaded, lookup, contracts));
        debug!(module = %loaded.name, ?elapsed, micros = micros(elapsed), "module checked");
        output
    }

    /// The text checking reads: the module's own, followed by the code of
    /// its plugin blocks.
    fn text_of<'l>(&'l self, loaded: &'l Loaded) -> &'l str {
        loaded
            .expansion
            .as_ref()
            .map_or(&self.sources.get(loaded.file).text, |e| &e.text)
    }

    fn is_module_template(loaded: &Loaded) -> bool {
        loaded
            .syntax
            .header
            .as_ref()
            .is_some_and(|header| !header.parameters.is_empty())
    }

    fn module_template(loaded: &Loaded) -> Output {
        let interface = Interface {
            module: loaded.name.clone(),
            ..Interface::default()
        };
        Output {
            diagnostics: loaded.parse.diagnostics.clone(),
            #[cfg(feature = "ide")]
            typing: Arc::new(types::Typing::default()),
            #[cfg(feature = "ide")]
            inline_ide: Vec::new(),
            exported: interface.clone(),
            interface: interface.clone(),
            ir: None,
            declarations: Arc::new(interface),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check(
        &self,
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Output {
        let tree = loaded
            .expansion
            .as_ref()
            .map_or(&loaded.parse.tree, |e| &e.tree);
        if let Some((index, inline)) = tree.nodes().enumerate().find(|(_, n)| {
            n.kind == K::InlineModuleDecl && !n.nodes().any(|n| n.kind == K::ModuleParamGroup)
        }) {
            return self.check_inline(loaded, tree, index, inline, lookup, contracts);
        }
        let (mut projections, mut local_dictionaries, contract_errors) =
            self.virtual_contracts(loaded, lookup, contracts);
        let mut projected_syntax = (*loaded.syntax).clone();
        let (lexical, mut lexical_dictionaries) =
            self.lexical_environment(loaded, lookup, &mut projected_syntax);
        projections.extend(lexical);
        lexical_dictionaries.append(&mut local_dictionaries);
        let local_dictionaries = lexical_dictionaries;
        let projected_lookup = |name: &str| projections.get(name).cloned().or_else(|| lookup(name));
        let result_signature = self.virtual_result_signature(loaded, lookup, contracts);
        for import in &mut projected_syntax.imports {
            let key = Self::binding_view_name(loaded, import.qualifier());
            if projections.contains_key(&key) {
                import.module = key;
            }
        }
        let module = resolve::Module {
            name: &loaded.name,
            file: loaded.file,
            text: self.text_of(loaded),
            tree,
            syntax: &projected_syntax,
            generated: loaded
                .expansion
                .as_ref()
                .map_or(resolve::NO_GENERATED, |e| e.len),
        };
        let (mut interface, mut resolved) = resolve::resolve(&module, &projected_lookup);
        resolved.retain(|diagnostic| {
            diagnostic.code != codes::R012
                || !loaded.virtual_of.as_ref().is_some_and(|context| {
                    loaded
                        .syntax
                        .imports
                        .iter()
                        .enumerate()
                        .any(|(index, import)| {
                            (Some(import.span) == diagnostic.span
                                || (index < context.capture.inherited_imports
                                    && import
                                        .exposing
                                        .iter()
                                        .chain(&import.plugins)
                                        .any(|item| Some(item.span) == diagnostic.span)))
                                && (index < context.capture.inherited_imports
                                    || context
                                        .capture
                                        .bindings
                                        .iter()
                                        .any(|b| import.alias.as_ref() == Some(&b.alias)))
                        })
                })
        });
        // a library's modules are another package's: they own their own instances
        let local = |name: &str| match self.by_name.get(name) {
            _ if name == loaded.name => true,
            Some(&i) => self.modules[i].definition_package() == loaded.definition_package(),
            // a saved module of the same package is as local as its source
            None => self
                .precompiled
                .package_of(name)
                .is_some_and(|p| loaded.definition_package() == Some(p)),
        };
        let read = (self.project).map_or_else(Checked::default, |p| p.config.checked());
        let options = types::Options {
            local: &local,
            limits: read.limits,
            no_let_assert: read.no_let_assert,
            thresholds: read.thresholds,
            watched: self.watched,
            effects: read.effects,
            stages_by_class: self.stages_by_class,
        };
        let mut diagnostics = loaded.parse.diagnostics.clone();
        let resolved_cleanly = !resolved.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(resolved);
        diagnostics.extend(contract_errors);
        let broken = !loaded.parse.diagnostics.is_empty();
        let witnesses: Vec<_> = loaded
            .virtual_of
            .iter()
            .flat_map(|v| &v.capture.bindings)
            .filter_map(|b| lookup(&b.argument))
            .collect();
        let (mut checked, typing) = types::check_typed_with_static_context(
            &module,
            &projected_lookup,
            types::builtins_of(self.stages_by_class),
            options,
            &mut interface,
            types::StaticContext {
                dictionaries: &local_dictionaries,
                witnesses: &witnesses,
            },
        );
        if let Some(mut expected) = result_signature {
            if let Some(context) = &loaded.virtual_of {
                // Publish equalities using the caller-visible transparent
                // definitions; the body was checked through abstract imports.
                Self::virtual_result_errors(
                    &mut expected,
                    &interface,
                    context,
                    &witnesses,
                    &mut checked,
                );
            }
            let explicit = self.callee_has_explicit_result(loaded, contracts);
            // An inferred private factory exports its ordinary checked declarations,
            // including ADT constructors. Only an advertised signature seals them.
            if explicit {
                interface = if loaded
                    .virtual_of
                    .as_ref()
                    .is_some_and(|v| v.capture.arguments.is_empty())
                {
                    expected.project(&interface)
                } else {
                    expected.seal(&interface, &loaded.name)
                };
            }
            if interface.parameterized.is_some() {
                interface.signature_explicit = explicit;
            }
        }
        // what the plugins find from the types of their code stops lowering too
        if let Some(expansion) = &loaded.expansion {
            checked.extend(expansion.check_blocks(
                &loaded.editing,
                &typing.wildcards,
                options.thresholds,
            ));
        }
        checked.extend(self.watched_calls(loaded, &typing));
        let checked = types::beside_syntax_errors(tree, checked);
        let clean = !broken
            && resolved_cleanly
            && !checked.iter().any(|d| d.severity == Severity::Error)
            && !diagnostics.iter().any(|d| d.severity == Severity::Error);
        interface.private = loaded.syntax.header.as_ref().is_some_and(|h| h.private);
        interface.member_of.clone_from(&loaded.member_of);
        #[cfg(feature = "native-project")]
        if clean && interface.parameterized.is_some() {
            interface.template = Some(ModuleTemplate {
                source: self.text_of(loaded).to_owned(),
                capture: loaded.virtual_of.as_ref().map(|v| v.capture.clone()),
            });
        }
        diagnostics.extend(checked);
        let declarations = typing.declarations.clone();
        let mut ir = self.lower_if_clean(
            loaded,
            &module,
            &typing,
            clean && interface.parameterized.is_none(),
            &mut diagnostics,
        );
        if let Some(ir) = &mut ir {
            for scope in self
                .modules
                .iter()
                .filter_map(|m| m.scope_of.as_ref())
                .filter(|s| s.module == loaded.name)
            {
                if let Some(prefix) = lookup(&scope.key()) {
                    let mut bindings: Vec<_> = prefix
                        .exports
                        .iter()
                        .filter(|e| {
                            e.entity.namespace != Namespace::Type || e.decl == Decl::Service
                        })
                        .map(|e| e.entity.name.clone())
                        .collect();
                    bindings.extend(
                        prefix
                            .instances
                            .iter()
                            .filter(|i| i.module == loaded.name)
                            .map(|i| i.name.clone()),
                    );
                    bindings.sort();
                    bindings.dedup();
                    ir.initialization.prefixes.push(flick_ir::Prefix {
                        before: scope.before,
                        bindings,
                    });
                }
            }
            ir.initialization.prefixes.sort_by_key(|p| p.before);
            ir.initialization.prefixes.dedup_by_key(|p| p.before);
        }
        if let Some(expansion) = &loaded.expansion {
            expansion.restore(&mut diagnostics, ir.as_mut());
            expansion.place_interface(&mut interface);
        }
        #[cfg(feature = "ide")]
        let typing = match &loaded.expansion {
            Some(expansion) => {
                let own = &self.sources.get(loaded.file).text;
                editor_names(typing, expansion, loaded.file, own)
            }
            None => typing,
        };
        Output {
            diagnostics,
            #[cfg(feature = "ide")]
            typing: Arc::new(typing),
            #[cfg(feature = "ide")]
            inline_ide: Vec::new(),
            exported: exported(&interface, clean),
            interface,
            ir: ir.filter(|_| self.cache.is_none() || self.cached_ir == CachedIr::Kept),
            declarations,
        }
    }
}

include!("checking_lower.rs");
