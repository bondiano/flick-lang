impl Checking<'_> {
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
            return Self::module_template(loaded);
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
