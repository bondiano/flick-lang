impl Checking<'_> {
    #[cfg(feature = "contracts")]
    fn proves_contracts(&self, loaded: &Loaded) -> bool {
        self.prove_contracts
            && loaded.has_contracts()
            && self.selected.is_none_or(|s| s.contains(&loaded.path))
    }

    #[cfg(not(feature = "contracts"))]
    fn proves_contracts(&self, _: &Loaded) -> bool {
        let _ = (self.prove_contracts, self.selected);
        false
    }

    fn lowers(&self, loaded: &Loaded) -> bool {
        self.lower && (!self.lower_packages_only || matches!(loaded.root, Root::Package(_)))
    }

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

    /// Resolves, checks and, when asked and clean, lowers module `i` against
    /// the interfaces known so far, or reuses the cached output; with a
    /// cache, also the key to cache it by.
    fn module(
        &self,
        i: usize,
        interfaces: &[Option<Interface>],
        comparable: &[Option<Arc<Interface>>],
        digests: &[Option<String>],
    ) -> Outcome {
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
        // a proof or contract reads the bodies of what it names, which no
        // interface shows: its module is checked again on every run, never
        // taken from a cache, as is a module whose laws a bundle of the run
        // may read
        let fresh = loaded.has_proofs()
            || (self.proving && loaded.has_laws())
            || self.proves_contracts(loaded);
        let Some(cache) = self.cache.filter(|_| !fresh) else {
            return (check(), None, None, Reason::Changed(Vec::new()));
        };
        #[cfg(feature = "native-project")]
        if let Some(store) = &cache.store {
            let (output, digest, reason) = self.stored(loaded, store, interfaces, digests, check);
            return (output, None, digest, reason);
        }
        #[cfg(not(feature = "native-project"))]
        let _ = digests;
        let mut key = self.memory_key(loaded, interfaces, comparable, &lookup, &contracts);
        // an output that was lowered serves a run that would not lower: an
        // error in another module changes nothing the module reads
        let entry = cache.entries.get(&loaded.path);
        key.lower |= entry.is_some_and(|(cached, _)| cached.lower);
        match entry {
            Some((cached, output)) if *cached == key || stale_reuse(cache, cached, &key) => {
                let mut output = output.clone();
                // re-exported names are declared where their module now says
                for import in &loaded.syntax.imports {
                    if let Some(interface) = known(&import.module) {
                        redeclare(&mut output.interface, interface);
                        redeclare(&mut output.exported, interface);
                    }
                }
                debug!(module = %loaded.name, "module reused");
                (output, Some(key), None, Reason::Reused)
            }
            _ => {
                let imports = (loaded.syntax.imports.iter())
                    .chain(&loaded.syntax.inline_imports)
                    .map(|i| i.module.as_str());
                let changed = (entry.filter(|_| cache.explain))
                    .map_or_else(Vec::new, |(cached, _)| cached.differs(&key, imports));
                key.lower = self.lowers(loaded);
                (check(), Some(key), None, Reason::Changed(changed))
            }
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
        // A parsed `InlineModuleDecl` always carries the parser's `ModulePath`,
        // so this is a defensive recovery: the structure it would check is
        // gone. Mark what importers see incomplete exactly when parsing failed,
        // as `check` does on the ordinary path, so they trust names the parser
        // may have lost. Do not fabricate exports or a parameterized signature.
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
            exported: Interface {
                incomplete: !loaded.parse.diagnostics.is_empty(),
                ..interface.clone()
            },
            interface: interface.clone(),
            ir: None,
            declarations: Arc::new(interface),
            proofs: Vec::new(),
            contracts: Vec::new(),
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
        let read = (self.project).map_or_else(Checked::default, |p| p.config.checked());
        let foreign_backend = self.foreign_backend_for(loaded);
        let options = types::Options {
            local: &local,
            limits: read.limits,
            no_let_assert: read.no_let_assert,
            thresholds: read.thresholds,
            watched: self.watched,
            stages_by_class: self.stages_by_class,
            foreign_backend: &foreign_backend,
            validate_foreign_metadata: self.env_ctx.validate_foreign_metadata,
            native_overrides: Some(self.env_ctx.native_overrides()),
            plugin_attributes: self.plugin_attributes,
        };
        let mut diagnostics = loaded.parse.diagnostics.clone();
        let resolved_cleanly = !resolved.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(resolved);
        diagnostics.extend(contract_errors);
        let broken = !loaded.parse.diagnostics.is_empty();
        let witnesses: Vec<_> = loaded
            .virtual_of
            .iter()
            .flat_map(|v| {
                v.capture.bindings.iter().map(|b| &b.argument).chain(
                    v.capture
                        .callee
                        .iter()
                        .flat_map(CalleeOrigin::contract_arguments),
                )
            })
            .filter_map(|argument| Self::argument_interface(argument, lookup, contracts))
            .collect();
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let (mut checked, mut typing) = types::check_typed_with_static_context(
            &module,
            &projected_lookup,
            types::builtins_of(self.stages_by_class),
            options,
            &mut interface,
            types::StaticContext {
                dictionaries: &local_dictionaries,
                witnesses: &witnesses,
                #[cfg(not(target_arch = "wasm32"))]
                ir_lookup: Some(lookup),
                skip_ir_types: !self.lower
                    || (self.cache.is_some() && self.cached_ir != CachedIr::Kept),
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
            if interface.parameterized.is_some() {
                interface.signature_explicit = loaded
                    .virtual_of
                    .as_ref()
                    .is_some_and(|v| Self::callee_signature_explicit(v, lookup, contracts));
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
        if self.env_ctx.validate_foreign_metadata {
            self.validate_official_foreigns(&foreign_backend, &module, &typing, &mut checked);
        }
        self.watched_calls(loaded, &typing, &mut checked);
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
                materialized_body: loaded
                    .syntax
                    .header
                    .as_ref()
                    .and_then(|header| header.materialized_body.clone()),
                capture: loaded.virtual_of.as_ref().map(|v| v.capture.clone()),
            });
        }
        diagnostics.extend(checked);
        // a module that checked is the only one whose proofs or contracts are about something
        let mut proofs = Vec::new();
        let mut verified_contracts = Vec::new();
        #[cfg(all(feature = "ide", not(target_arch = "wasm32")))]
        let mut proof_evidence = Vec::new();
        #[cfg(all(feature = "ide", not(target_arch = "wasm32")))]
        let mut evidence = self.observe_proofs.then_some(&mut proof_evidence);
        #[cfg(all(not(feature = "ide"), not(target_arch = "wasm32")))]
        let mut evidence = None;
        #[cfg(not(target_arch = "wasm32"))]
        let recovering_proofs = evidence.is_some() && !typing.proofs.is_empty();
        #[cfg(target_arch = "wasm32")]
        let recovering_proofs = false;
        if clean || recovering_proofs {
            use crate::project::builtins::builtin_laws;
            let stages = self.stages_by_class;
            let lowered = |name: &str| self.lowered.get(*self.by_name.get(name)?)?.as_ref();
            let run = ir::Run {
                builtins: &|| builtin_modules_of(stages),
                builtin_laws: &|| builtin_laws(stages),
                project: &|name| {
                    lowered(name)
                        .map(|(ir, _, _)| &**ir)
                        .or_else(|| self.precompiled.ir(name))
                },
                proven: &|name| lowered(name).map(|(_, _, p)| p.as_slice()),
                all_contracted: self.all_contracted,
                contracts: &|name| {
                    self.by_name
                        .get(name)
                        .and_then(|&idx| self.verified_contracts.get(idx))
                        .map_or(&[], Vec::as_slice)
                },
            };
            #[cfg(not(target_arch = "wasm32"))]
            let search = self
                .cache
                .and_then(|cache| cache.proof_search(&loaded.path));
            if clean {
                let (found, proven) = ir::check_proofs(
                    &module,
                    &typing,
                    &run,
                    #[cfg(not(target_arch = "wasm32"))]
                    evidence.take(),
                    #[cfg(not(target_arch = "wasm32"))]
                    self.project.is_some_and(|p| p.config.unused_proof_premises),
                    #[cfg(not(target_arch = "wasm32"))]
                    search,
                );
                diagnostics.extend(found);
                proofs = proven;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if !clean && let Some(evidence) = evidence {
                ir::observe_proofs(&module, &typing, &run, &diagnostics, evidence, search);
            }
            if clean && self.proves_contracts(loaded) {
                let (contract_diags, summaries) = ir::check_contracts(&module, &typing, &run);
                diagnostics.extend(contract_diags);
                verified_contracts = summaries;
            }
        }
        #[cfg(all(feature = "ide", not(target_arch = "wasm32")))]
        {
            typing.proof_evidence = proof_evidence;
        }
        interface.proofs = typing.proofs.iter().map(|p| p.name.clone()).collect();
        let mut declarations = typing.declarations.clone();
        let mut ir = self.lower_if_clean(
            loaded,
            &module,
            &typing,
            clean && interface.parameterized.is_none(),
            &mut declarations,
            &mut diagnostics,
        );
        #[cfg(not(target_arch = "wasm32"))]
        typing.clear_ir_types();
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
            exported: Interface {
                incomplete: broken,
                ..exported(&interface, clean)
            },
            interface,
            ir: ir
                .filter(|_| {
                    self.cache.is_none() || self.cached_ir != CachedIr::Dropped || self.proving
                })
                .map(Arc::new),
            declarations,
            proofs,
            contracts: verified_contracts,
        }
    }
}

/// Takes where `imported` declares the names `interface` re-exports.
fn redeclare(interface: &mut Interface, imported: &Interface) {
    for export in &mut interface.exports {
        if export.entity.module == interface.module {
            continue;
        }
        if let Some(found) = imported.exports.iter().find(|e| e.entity == export.entity) {
            export.at = found.at;
        }
    }
}

include!("checking_lower.rs");

include!("checking_foreign.rs");

include!("checking_inline_values.rs");

include!("checking_visibility.rs");

#[cfg(test)]
#[path = "checking_core_tests.rs"]
mod template_fallback_tests;
