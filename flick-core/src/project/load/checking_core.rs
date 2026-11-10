impl Checking<'_> {
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
        let known = |name: &str| {
            let (interface, owner) = match self.by_name.get(name) {
                Some(&j) => (interfaces[j].as_ref(), Some(self.modules[j].root.clone())),
                None => (
                    self.from_packages.get(name),
                    self.precompiled
                        .package_of(name)
                        .map(|p| Root::Package(p.to_owned())),
                ),
            };
            interface.filter(|interface| {
                !interface.private || owner.as_ref().is_some_and(|r| loaded.root.same_package(r))
            })
        };
        let lookup = |name: &str| known(name).cloned();
        let Some(cache) = self.cache else {
            return (self.timed(loaded, &lookup), None);
        };
        let key = Key {
            file: loaded.file,
            // the code of its blocks too, which other modules' blocks change
            text: self.text_of(loaded).to_owned(),
            config: self.project.map(|p| p.config.clone()),
            lower: self.lower,
            keeps_ir: self.keeps_ir,
            imports: loaded
                .syntax
                .imports
                .iter()
                .chain(&loaded.syntax.inline_imports)
                .map(|i| {
                    let interface = match self.by_name.get(&i.module) {
                        Some(&j) => comparable[j].clone(),
                        None => self.from_packages.get(&i.module).map(comparable_copy),
                    };
                    (interface, self.by_name.contains_key(&i.module))
                })
                .collect(),
            virtual_contracts: loaded
                .virtual_of
                .iter()
                .flat_map(|virtual_of| &virtual_of.parameters)
                .map(|(_, contract, _)| {
                    let module = contract.rsplit_once('.')?.0;
                    match self.by_name.get(module) {
                        Some(&j) => comparable[j].clone(),
                        None => self.from_packages.get(module).map(comparable_copy),
                    }
                })
                .collect(),
            virtual_parameters: loaded
                .virtual_of
                .as_ref()
                .map_or_else(Vec::new, |virtual_of| virtual_of.parameters.clone()),
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
            _ => (self.timed(loaded, &lookup), Some(key)),
        }
    }

    /// [`Checking::check`], logging how long it took.
    fn timed(&self, loaded: &Loaded, lookup: &dyn Fn(&str) -> Option<Interface>) -> Output {
        let (output, elapsed) = timed(|| self.check(loaded, lookup));
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

    fn virtual_contracts(
        loaded: &Loaded,
        lookup: &dyn Fn(&str) -> Option<Interface>,
    ) -> ContractView {
        let mut projections = BTreeMap::new();
        let mut local_dictionaries = Vec::new();
        let mut contract_errors = Vec::new();
        if let Some(virtual_of) = &loaded.virtual_of {
            for (_, contract_path, argument) in &virtual_of.parameters {
                let Some((contract_module, contract_name)) = contract_path.rsplit_once('.') else {
                    contract_errors.push(Diagnostic::error(
                        codes::T001,
                        format!("`{contract_path}` must name a `type module` in a module"),
                        virtual_of.at,
                    ));
                    continue;
                };
                let contract = lookup(contract_module)
                    .and_then(|interface| interface.find(Namespace::Type, contract_name).cloned())
                    .and_then(|export| export.definition);
                let Some(contract) = contract else {
                    contract_errors.push(Diagnostic::error(
                        codes::T001,
                        format!("unknown module contract `{contract_path}`"),
                        virtual_of.at,
                    ));
                    continue;
                };
                let Some(actual) = lookup(argument) else {
                    continue;
                };
                let mismatches = types::check_module_contract(&contract, &actual);
                for mismatch in &mismatches {
                    let (code, member, detail) = match mismatch {
                        types::ModuleMismatch::Missing(name) => {
                            (codes::T060, name, "is not exported")
                        }
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
                    contract_errors.push(Diagnostic::error(
                        code,
                        format!(
                            "`{argument}` does not satisfy `{contract_path}`: `{member}` {detail}"
                        ),
                        virtual_of.at,
                    ));
                }
                if mismatches.is_empty() {
                    for (head, dict) in types::module_local_dictionaries(&contract, &actual) {
                        if local_dictionaries
                            .iter()
                            .any(|(existing, _)| existing == &head)
                        {
                            contract_errors.push(Diagnostic::error(
                                codes::T066,
                                format!("more than one static module argument supplies a dictionary for `{}`; keep one `instance ... from ...` requirement in the scope of this parameterized module", types::show_type(&head)),
                                virtual_of.at,
                            ));
                        } else {
                            local_dictionaries.push((head, dict));
                        }
                    }
                    projections.insert(
                        argument.clone(),
                        types::project_module_contract(&contract, &actual),
                    );
                }
            }
        }
        (projections, local_dictionaries, contract_errors)
    }

    fn check(&self, loaded: &Loaded, lookup: &dyn Fn(&str) -> Option<Interface>) -> Output {
        if Self::is_module_template(loaded) {
            return Self::module_template(loaded);
        }
        let tree = loaded
            .expansion
            .as_ref()
            .map_or(&loaded.parse.tree, |e| &e.tree);
        if let Some((index, inline)) = tree
            .nodes()
            .enumerate()
            .find(|(_, n)| n.kind == K::InlineModuleDecl)
        {
            return self.check_inline(loaded, tree, index, inline, lookup);
        }
        let (projections, local_dictionaries, contract_errors) =
            Self::virtual_contracts(loaded, lookup);
        let projected_lookup = |name: &str| projections.get(name).cloned().or_else(|| lookup(name));
        let module = resolve::Module {
            name: &loaded.name,
            file: loaded.file,
            text: self.text_of(loaded),
            tree,
            syntax: &loaded.syntax,
            generated: loaded
                .expansion
                .as_ref()
                .map_or(resolve::NO_GENERATED, |e| e.len),
        };
        let (mut interface, resolved) = resolve::resolve(&module, &projected_lookup);
        // a library's modules are another package's: they own their own instances
        let local = |name: &str| match self.by_name.get(name) {
            _ if name == loaded.name => true,
            Some(&i) => package_of(&self.modules[i].root) == package_of(&loaded.root),
            // a saved module of the same package is as local as its source
            None => self
                .precompiled
                .package_of(name)
                .is_some_and(|p| package_of(&loaded.root) == Some(p)),
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
        let (mut checked, typing) = types::check_typed_with_module_dictionaries(
            &module,
            &projected_lookup,
            types::builtins_of(self.stages_by_class),
            options,
            &mut interface,
            &local_dictionaries,
        );
        // what the plugins find from the types of their code stops lowering too
        if let Some(expansion) = &loaded.expansion {
            checked.extend(expansion.check_blocks(&loaded.editing, &typing.wildcards, options.thresholds));
        }
        checked.extend(self.watched_calls(loaded, &typing));
        let checked = types::beside_syntax_errors(tree, checked);
        let clean =
            !broken && resolved_cleanly && !checked.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(checked);
        let declarations = typing.declarations.clone();
        let mut ir = self.lower_if_clean(loaded, &module, &typing, clean, &mut diagnostics);
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
            // editors read its diagnostics only, and keeping the IR in their
            // cache would copy all of it on every keystroke
            ir: ir.filter(|_| self.cache.is_none() || self.keeps_ir),
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
        if clean && self.lower && lowers {
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
    ) -> Output {
        let text = self.text_of(loaded);
        let Some(units) = inline_units(loaded, tree, index, inline, text) else {
            return Self::module_template(loaded);
        };
        let prefix_output = self.check(&units.prefix, lookup);
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
        let mut child_output = self.check(child, &prefix_lookup);
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
        let mut output = self.check(&units.parent, &child_lookup);
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
