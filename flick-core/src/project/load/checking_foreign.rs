impl Checking<'_> {
    /// Selects the foreign metadata specification for `loaded`: if its module
    /// or package `@when` condition is inactive under the active environment
    /// and uniquely matches another known backend, uses that backend's spec.
    fn foreign_backend_for(&self, loaded: &Loaded) -> types::ForeignBackendSpec {
        let module_when = loaded
            .syntax
            .header
            .as_ref()
            .and_then(|h| h.when.as_ref())
            .map(|(cond, _)| cond.as_str());
        #[cfg(not(target_arch = "wasm32"))]
        let pkg_when = match loaded.definition_package() {
            Some(pkg) => self.package_when.get(pkg).map(String::as_str),
            None => self.own_package_when,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let cond = match (module_when, pkg_when) {
            (Some(m), Some(p)) => Some(Cow::Owned(format!("({p}) && ({m})"))),
            (Some(m), None) => Some(Cow::Borrowed(m)),
            (None, Some(p)) => Some(Cow::Borrowed(p)),
            (None, None) => None,
        };
        #[cfg(target_arch = "wasm32")]
        let cond = module_when.map(Cow::Borrowed);
        self.env_ctx.foreign_backend_for(cond.as_deref())
    }

    /// Runs official JS/Go adapter and inline code validators on both `foreign`
    /// declarations and `@native` overrides.
    fn validate_official_foreigns(
        &self,
        foreign_backend: &types::ForeignBackendSpec,
        module: &resolve::Module<'_>,
        typing: &types::Typing,
        checked: &mut Vec<Diagnostic>,
    ) {
        Self::validate_official_foreign_adapters(foreign_backend, module, typing, checked);
        self.validate_official_inline_foreign(foreign_backend, module, typing, checked);
    }

    /// Runs the official JS/Go backend's own foreign adapter validator in-process
    /// over finalized `foreign` declarations and selected `@native` overrides so
    /// `flick check`, standalone files, LSP, and the browser playground report
    /// exact backend adapter diagnostics even when other items in the same module
    /// have errors.
    fn validate_official_foreign_adapters(
        foreign_backend: &types::ForeignBackendSpec,
        module: &resolve::Module<'_>,
        typing: &types::Typing,
        checked: &mut Vec<Diagnostic>,
    ) {
        let mut entries: Vec<_> = typing.foreigns.iter().collect();
        entries.sort_by_key(|(range, _)| *range);
        let backend_id = foreign_backend.backend_id.as_str();
        if matches!(backend_id, "flick/js" | "flick/go") {
            for (range, info) in entries {
                if info.adapter.words.is_empty() {
                    continue;
                }
                let is_inline = module
                    .tree
                    .nodes()
                    .find(|n| n.kind == K::ForeignDecl && n.range() == *range)
                    .and_then(|n| n.nodes().find(|c| c.kind == K::ForeignSource))
                    .is_some_and(|src| {
                        src.child_tokens()
                            .any(|t| t.kind == K::Ident && t.text(module.text) == "inline")
                    });
                Self::validate_one_official_adapter(
                    backend_id,
                    module.file,
                    *range,
                    is_inline,
                    info,
                    checked,
                );
            }
        }
        let mut overrides: Vec<_> = typing.native_overrides.iter().collect();
        overrides.sort_by_key(|(range, _)| *range);
        for (_, ov) in overrides {
            if ov.info.adapter.words.is_empty()
                || !matches!(ov.backend_id.as_str(), "flick/js" | "flick/go")
            {
                continue;
            }
            let is_inline = find_native_override(module.tree, ov.override_range)
                .and_then(|n| n.nodes().find(|c| c.kind == K::ForeignSource))
                .is_some_and(|src| {
                    src.child_tokens()
                        .any(|t| t.kind == K::Ident && t.text(module.text) == "inline")
                });
            Self::validate_one_official_adapter(
                &ov.backend_id,
                module.file,
                ov.override_range,
                is_inline,
                &ov.info,
                checked,
            );
        }
    }

    fn validate_one_official_adapter(
        backend_id: &str,
        file: FileId,
        range: (u32, u32),
        is_inline: bool,
        info: &types::ForeignInfo,
        checked: &mut Vec<Diagnostic>,
    ) {
        let binding = if is_inline {
            flick_ir::Binding::Inline(flick_ir::InlineCode {
                code: String::new(),
                span: Span::new(file, range.0, range.1),
            })
        } else {
            flick_ir::Binding::Global {
                name: String::new(),
            }
        };
        let view = flick_ir::Foreign {
            name: String::new(),
            source_name: None,
            mode: info.mode,
            binding,
            stages: info.stages.clone(),
            dicts: info.dicts,
            arity: info.js_arity(),
            adapter: info.adapter.clone(),
            catch: None,
            type_params: Vec::new(),
            ty: None,
        };
        match backend_id {
            "flick/js" => {
                let mut found = Vec::new();
                js_foreign_meta::validate_foreign(
                    &view,
                    ("Prelude", "List"),
                    Some(&info.list_params),
                    &mut found,
                );
                for d in found {
                    let code = if d.code == "S011" {
                        codes::S011
                    } else {
                        codes::F001
                    };
                    checked.push(Diagnostic::error(code, d.message, d.span));
                }
            }
            #[cfg(feature = "native-project")]
            "flick/go" => {
                let mut found = Vec::new();
                go_foreign_meta::validate_foreign(
                    &view,
                    ("Prelude", "List"),
                    Some(&info.list_params),
                    &mut found,
                );
                for d in found {
                    let code = if d.code == "S011" {
                        codes::S011
                    } else {
                        codes::F001
                    };
                    checked.push(Diagnostic::error(code, d.message, d.span));
                }
            }
            _ => {}
        }
    }

    /// Runs the official JS/Go backend's inline foreign validator in-process
    /// over `foreign module inline` preambles, `from inline` bindings, and
    /// `@native inline` overrides so `flick check`, LSP, and the browser
    /// playground report span-mapped `FLICK-F011` diagnostics directly.
    fn validate_official_inline_foreign(
        &self,
        foreign_backend: &types::ForeignBackendSpec,
        module: &resolve::Module<'_>,
        typing: &types::Typing,
        checked: &mut Vec<Diagnostic>,
    ) {
        let backend_id = foreign_backend.backend_id.as_str();
        if matches!(backend_id, "flick/js" | "flick/go") {
            for decl in module.tree.nodes() {
                if decl.kind == K::ForeignPreamble {
                    let Some(tok) = decl.child_tokens().find(|t| t.kind == K::String) else {
                        continue;
                    };
                    let code = string_value(tok.text(module.text));
                    let span = Span::new(module.file, tok.start, tok.end);
                    Self::check_inline_preamble(backend_id, &code, span, module.text, checked);
                } else if decl.kind == K::ForeignDecl {
                    Self::check_inline_source(backend_id, decl, module, checked);
                }
            }
        }
        // Validate `@native inline "..."` clauses for their selected or uniquely matched backend.
        let mut overrides = Vec::new();
        collect_native_overrides(module.tree, &mut overrides);
        for (decl_range, ov) in overrides {
            let selected_backend = typing
                .native_overrides
                .get(&decl_range)
                .filter(|n| n.override_range == ov.range())
                .map(|n| n.backend_id.as_str());
            let target_backend = selected_backend.or_else(|| {
                let cond_text = ov.nodes().find(|n| n.kind == K::WhenCond)?.text(module.text);
                let expr = when::parse(cond_text.trim()).ok()?;
                self.env_ctx
                    .native_overrides
                    .unique_backend_for(&expr)
                    .map(|s| s.backend_id.as_str())
            });
            if let Some(id @ ("flick/js" | "flick/go")) = target_backend {
                Self::check_inline_source(id, ov, module, checked);
            }
        }
    }

    fn check_inline_preamble(
        backend_id: &str,
        code: &str,
        span: Span,
        source: &str,
        checked: &mut Vec<Diagnostic>,
    ) {
        let found = match backend_id {
            "flick/js" => js_inline::validate_preamble(code, span, source),
            #[cfg(feature = "native-project")]
            "flick/go" => go_inline::validate_preamble(code, span, source).map(|d| {
                js_foreign_meta::ForeignDiagnostic {
                    code: d.code,
                    message: d.message,
                    span: d.span,
                }
            }),
            _ => None,
        };
        if let Some(d) = found {
            checked.push(Diagnostic::error(codes::F011, d.message, d.span));
        }
    }

    fn check_inline_source(
        backend_id: &str,
        parent: &Node,
        module: &resolve::Module<'_>,
        checked: &mut Vec<Diagnostic>,
    ) {
        let Some(source) = parent.nodes().find(|n| n.kind == K::ForeignSource) else {
            return;
        };
        let is_inline = source
            .child_tokens()
            .any(|t| t.kind == K::Ident && t.text(module.text) == "inline");
        if !is_inline {
            return;
        }
        let Some(tok) = source.child_tokens().find(|t| t.kind == K::String) else {
            return;
        };
        let code = string_value(tok.text(module.text));
        let span = Span::new(module.file, tok.start, tok.end);
        let found = match backend_id {
            "flick/js" => js_inline::validate_binding(&code, span, module.text),
            #[cfg(feature = "native-project")]
            "flick/go" => go_inline::validate_binding(&code, span, module.text).map(|d| {
                js_foreign_meta::ForeignDiagnostic {
                    code: d.code,
                    message: d.message,
                    span: d.span,
                }
            }),
            _ => None,
        };
        if let Some(d) = found {
            checked.push(Diagnostic::error(codes::F011, d.message, d.span));
        }
    }
}

fn find_native_override(tree: &Node, target: (u32, u32)) -> Option<&Node> {
    let mut found = Vec::new();
    collect_native_overrides(tree, &mut found);
    found
        .into_iter()
        .find(|(_, ov)| ov.range() == target)
        .map(|(_, ov)| ov)
}

fn collect_native_overrides<'a>(tree: &'a Node, out: &mut Vec<((u32, u32), &'a Node)>) {
    for decl in tree.nodes() {
        match decl.kind {
            K::LetDecl => {
                for ov in decl.nodes().filter(|n| n.kind == K::NativeOverride) {
                    out.push((decl.range(), ov));
                }
            }
            K::RecGroup => {
                for member in decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                {
                    if member.kind == K::LetDecl {
                        for ov in member.nodes().filter(|n| n.kind == K::NativeOverride) {
                            out.push((member.range(), ov));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
