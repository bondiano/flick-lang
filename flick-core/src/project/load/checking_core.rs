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
        let Some(cache) = self.cache else {
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
                key.lower = self.lower;
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
        let options = types::Options {
            local: &local,
            limits: read.limits,
            no_let_assert: read.no_let_assert,
            thresholds: read.thresholds,
            watched: self.watched,
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
        // a module that checked is the only one whose proofs are about something
        if clean {
            diagnostics.extend(ir::check_proofs(&module, &typing));
        }
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
            exported: Interface {
                incomplete: broken,
                ..exported(&interface, clean)
            },
            interface,
            ir: ir.filter(|_| self.cache.is_none() || self.cached_ir == CachedIr::Kept),
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
            && !Self::is_module_template(loaded)
        {
            match ir::lower(module, typing) {
                Ok(lowered) => return Some(lowered),
                Err(d) => diagnostics.extend(d),
            }
        }
        None
    }
}

include!("checking_inline_values.rs");

include!("checking_visibility.rs");

#[cfg(test)]
mod template_fallback_tests {
    use super::*;
    use crate::syntax;

    fn loaded(source: &str, broken: bool) -> Loaded {
        let mut sources = SourceMap::default();
        let file = sources.add("A/Parent.fl", source);
        let parse = syntax::parse(source, file).into_module();
        let syntax = resolve::module_syntax(&parse.tree, source, file);
        let diagnostics = if broken {
            vec![Diagnostic::error(
                codes::S007,
                "unexpected `)`",
                Span::new(file, 6, 7),
            )]
        } else {
            Vec::new()
        };
        Loaded {
            path: PathBuf::from("A/Parent.fl"),
            file,
            root: Root::Other(None),
            parse: Arc::new(Parse {
                tree: parse.tree,
                diagnostics,
            }),
            syntax: Arc::new(syntax),
            name: "A.Parent".into(),
            expansion: None,
            editing: Vec::new(),
            loader: None,
            virtual_of: None,
            member_of: None,
            scope_of: None,
        }
    }

    /// A template fallback has no structure to export, so it must not invent
    /// names; it marks the module incomplete exactly when parsing failed.
    #[test]
    fn the_template_fallback_is_incomplete_only_when_the_parser_reported() {
        let source = "module A.Parent exposing (x)\nlet x = 1\n";
        let broken = Checking::module_template(&loaded(source, true));
        assert!(broken.exported.incomplete);
        assert!(!broken.interface.incomplete);
        assert!(broken.exported.exports.is_empty());
        assert!(broken.exported.parameterized.is_none());
        assert_eq!(broken.diagnostics.len(), 1);
        assert_eq!(broken.diagnostics[0].code, codes::S007);

        let clean = Checking::module_template(&loaded(source, false));
        assert!(!clean.exported.incomplete);
        assert!(!clean.interface.incomplete);
        assert!(clean.exported.exports.is_empty());
        assert!(clean.diagnostics.is_empty());
    }
}
