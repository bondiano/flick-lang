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
        debug!(module = %loaded.name, ?elapsed, "module checked");
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

    fn is_functor_template(loaded: &Loaded) -> bool {
        loaded
            .syntax
            .header
            .as_ref()
            .is_some_and(|header| !header.parameters.is_empty())
    }

    fn functor_template(loaded: &Loaded) -> Output {
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
            Self::virtual_contracts(loaded, lookup, contracts);
        let mut projected_syntax = (*loaded.syntax).clone();
        let (lexical, mut lexical_dictionaries) =
            self.lexical_environment(loaded, lookup, &mut projected_syntax);
        projections.extend(lexical);
        lexical_dictionaries.append(&mut local_dictionaries);
        let local_dictionaries = lexical_dictionaries;
        let projected_lookup = |name: &str| projections.get(name).cloned().or_else(|| lookup(name));
        let result_signature = Self::virtual_result_signature(loaded, lookup, contracts);
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
        let build = self.project.map(|p| &p.config.build);
        let options = types::Options {
            local: &local,
            limits: build.map_or_else(types::Limits::default, |b| types::Limits {
                steps: b.solver_steps,
                depth: b.solver_depth,
            }),
            no_let_assert: self.project.is_some_and(|p| p.config.no_let_assert),
            thresholds: self
                .project
                .map_or_else(Default::default, |p| p.config.thresholds),
            watched: self.watched,
            effects: build.is_none_or(|b| b.effect != Backend::None),
            stages_by_class: self.stages_by_class,
        };
        let mut diagnostics = loaded.parse.diagnostics.clone();
        let resolved_cleanly = !resolved.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(resolved);
        diagnostics.extend(contract_errors);
        // Keep the resolved exports before checking: a failed module may leave
        // store-local type variables in its inferred schemes. Importers must
        // not read those variables with their own unification store.
        let broken = !loaded.parse.diagnostics.is_empty();
        let mut untyped = interface.clone();
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
            interface = if loaded
                .virtual_of
                .as_ref()
                .is_some_and(|v| v.capture.arguments.is_empty())
            {
                expected.project(&interface)
            } else {
                expected.seal(&interface, &loaded.name)
            };
            if interface.functor.is_some() {
                interface.signature_explicit = loaded.virtual_of.as_ref().is_some_and(|v| match &v
                    .capture
                    .callee
                {
                    Some(CalleeOrigin::Module(name)) => {
                        contracts(name).is_some_and(|i| i.signature_explicit)
                    }
                    Some(CalleeOrigin::Member(member)) => {
                        Self::public_contract_interface(&member.parent, contracts)
                            .and_then(|i| i.modules.get(&member.name).map(|m| m.signature_explicit))
                            .unwrap_or(false)
                    }
                    Some(CalleeOrigin::Parameter { .. }) => true,
                    None => false,
                });
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
        if !clean {
            untyped.functor = interface
                .functor
                .as_ref()
                .map(types::ModuleSignature::inputs_only);
        }
        interface.private = loaded.syntax.header.as_ref().is_some_and(|h| h.private);
        interface.member_of.clone_from(&loaded.member_of);
        untyped.member_of.clone_from(&loaded.member_of);
        untyped.private = interface.private;
        #[cfg(any(feature = "native-project", feature = "build-artifacts"))]
        if clean && interface.functor.is_some() {
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
            clean && interface.functor.is_none(),
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
            expansion.place_interface(&mut untyped);
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
            exported: if clean { interface.clone() } else { untyped },
            interface,
            // editors, the runs with a cache, read its diagnostics only, and
            // keeping the IR would copy all of it on every keystroke
            ir: ir.filter(|_| self.cache.is_none()),
            declarations,
        }
    }

    fn lower_if_clean(
        &self,
        loaded: &Loaded,
        module: &resolve::Module<'_>,
        typing: &types::Typing,
        clean: bool,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ir::Module> {
        let lowers = !self.lower_packages_only || matches!(loaded.root, Root::Package(_));
        if clean
            && self.lower
            && lowers
            && loaded.scope_of.is_none()
            && !Self::is_functor_template(loaded)
        {
            match ir::lower(module, typing) {
                Ok(lowered) => return Some(lowered),
                Err(d) => diagnostics.extend(d),
            }
        }
        None
    }

    fn check_inline(
        &self,
        loaded: &Loaded,
        tree: &Node,
        index: usize,
        inline: &Node,
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Output {
        let text = self.text_of(loaded);
        let Some(units) = inline_units(loaded, tree, index, inline, text) else {
            return Self::functor_template(loaded);
        };
        let prefix_output = self.check(&units.prefix, lookup, contracts);
        let child = &units.child;
        let child_module = resolve::Module {
            name: &child.name,
            file: child.file,
            text,
            tree: child
                .expansion
                .as_ref()
                .map_or(&child.parse.tree, |e| &e.tree),
            syntax: &child.syntax,
            generated: child
                .expansion
                .as_ref()
                .map_or(resolve::NO_GENERATED, |e| e.len),
        };
        let child_uses = resolve::referenced_names(&child_module);
        let child_defs = resolve::declarations(child_module.tree, text, loaded.file);
        let prefix_lookup = |module: &str| {
            if module == loaded.name {
                Some(prefix_output.interface.clone())
            } else {
                lookup(module)
            }
        };
        let mut child_output = self.check(child, &prefix_lookup, contracts);
        // The child sees its parent's prior instances, but importing the
        // child back must not import those instances a second time.
        child_output
            .interface
            .instances
            .retain(|i| i.module != loaded.name);
        child_output
            .exported
            .instances
            .retain(|i| i.module != loaded.name);
        let child_lookup = |module: &str| {
            if module == units.name {
                Some(child_output.exported.clone())
            } else {
                lookup(module)
            }
        };
        let mut output = self.check(&units.parent, &child_lookup, contracts);
        #[cfg(feature = "ide")]
        merge_inline_ide(&mut output, &child_output, child);
        output
            .diagnostics
            .extend(self.inline_name_errors(loaded, tree, index, text, &units));
        output
            .diagnostics
            .extend(unsupported_inline_applications(inline, text, loaded.file));
        output.diagnostics.retain(|diagnostic| {
            if diagnostic.code != codes::R014 {
                return true;
            }
            let Some(span) = diagnostic.span else {
                return true;
            };
            !prefix_output.interface.exports.iter().any(|export| {
                export.at == Some([span.start, span.end])
                    && child_uses.contains(&(export.entity.namespace, export.entity.name.clone()))
                    && !child_defs.iter().any(|def| {
                        def.namespace == export.entity.namespace && def.name == export.entity.name
                    })
            })
        });
        output.diagnostics.retain(|d| d.code != codes::R012);
        child_output.diagnostics.retain(|d| d.code != codes::R012);
        let source_module = resolve::Module {
            name: &loaded.name,
            file: loaded.file,
            text,
            tree,
            syntax: &loaded.syntax,
            generated: loaded
                .expansion
                .as_ref()
                .map_or(resolve::NO_GENERATED, |e| e.len),
        };
        output
            .diagnostics
            .extend(resolve::import_warnings(&source_module, lookup));
        output.diagnostics.extend(child_output.diagnostics.clone());
        merge_inline_lowering(&mut output, &mut child_output, loaded, &units);
        output
    }

    fn inline_name_errors(
        &self,
        loaded: &Loaded,
        tree: &Node,
        index: usize,
        text: &str,
        units: &InlineUnits,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if let Some(&other) = self.by_name.get(&units.name)
            && self.modules[other].path != loaded.path
        {
            diagnostics.push(Diagnostic::error(
                codes::R002,
                format!(
                    "inline module `{}` conflicts with the file module at `{}`",
                    units.name,
                    self.modules[other].path.display()
                ),
                units.at,
            ));
        }
        for later in tree
            .nodes()
            .skip(index + 1)
            .filter(|n| n.kind == K::InlineModuleDecl)
        {
            if let Some(later_path) = later.nodes().find(|n| n.kind == K::ModulePath)
                && later_path.text(text).split_whitespace().collect::<String>() == units.local
            {
                let (start, end) = later_path.range();
                diagnostics.push(
                    Diagnostic::error(
                        codes::R002,
                        format!(
                            "inline module `{}` is declared twice in this file",
                            units.local
                        ),
                        Span::new(loaded.file, start, end),
                    )
                    .with_related(units.at, "first declared here"),
                );
                break;
            }
        }
        diagnostics
    }
}
