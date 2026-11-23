impl Checking<'_> {
    fn foreign_meta_for(&self, loaded: &Loaded) -> &ForeignMeta {
        #[cfg(feature = "native-project")]
        let pkg_backend = loaded
            .definition_package()
            .and_then(|pkg| self.packages?.package_backend(pkg));
        #[cfg(not(feature = "native-project"))]
        let pkg_backend: Option<&str> = None;
        let module_when = loaded
            .syntax
            .header
            .as_ref()
            .and_then(|h| h.when.as_ref())
            .map(|(cond, _)| cond.as_str());
        self.env_ctx.foreign_meta_for(pkg_backend, module_when)
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
            adapter_diagnostics: Vec::new(),
            #[cfg(feature = "ide")]
            typing: Arc::new(types::Typing::default()),
            #[cfg(feature = "ide")]
            inline_ide: Vec::new(),
            #[cfg(feature = "ide")]
            foreign_meta: Arc::new(ForeignMeta::js()),
            exported: Interface {
                incomplete: !loaded.parse.diagnostics.is_empty(),
                ..interface.clone()
            },
            interface: interface.clone(),
            ir: None,
            declarations: Arc::new(interface),
            proofs: Vec::new(),
        }
    }

    fn lower_if_clean(
        &self,
        loaded: &Loaded,
        module: &resolve::Module<'_>,
        typing: &types::Typing,
        clean: bool,
        declarations: &mut Arc<Interface>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ir::Module> {
        let lowers = !self.lower_packages_only || matches!(loaded.root, Root::Package(_));
        if clean
            && self.lower
            && lowers
            && loaded.scope_of.is_none()
            && !Self::is_module_template(loaded)
        {
            #[cfg(not(target_arch = "wasm32"))]
            match ir::lower_with_local_schemes(module, typing) {
                Ok((lowered, local_schemes)) => {
                    if !local_schemes.is_empty() {
                        Arc::make_mut(declarations).local_schemes = local_schemes;
                    }
                    return Some(lowered);
                }
                Err(d) => diagnostics.extend(d),
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = declarations;
                match ir::lower(module, typing) {
                    Ok(lowered) => return Some(lowered),
                    Err(d) => diagnostics.extend(d),
                }
            }
        }
        None
    }
}

/// Official-backend fallback validation for JS/Go backend-owned foreign
/// adapter words (used when a backend runner does not execute `check-target`,
/// such as standalone `check_file`, `flick-wasm`, in-memory hosts, or modules
/// with other errors that prevented lowering).
fn official_adapter_diagnostics(typing: &types::Typing, meta: &ForeignMeta) -> Vec<Diagnostic> {
    if typing.foreigns.is_empty() {
        return Vec::new();
    }
    let mut entries: Vec<_> = typing.foreigns.iter().collect();
    entries.sort_by_key(|(range, _)| *range);
    let mut out = Vec::new();
    let code = |c: &str| match c {
        "F001" => codes::F001,
        "S011" => codes::S011,
        _ => codes::P012,
    };
    for (_, info) in entries {
        if info.adapter.words.is_empty() {
            continue;
        }
        match meta.backend_id.as_str() {
            "flick/js" => {
                for d in js_adapter::validate_foreign(
                    info.mode,
                    info.js_arity(),
                    &info.adapter.words,
                    info.last_param_is_list,
                ) {
                    out.push(Diagnostic::error(code(d.code), d.message, d.span));
                }
            }
            #[cfg(feature = "native-project")]
            "flick/go" => {
                for d in go_adapter::validate_foreign(
                    info.mode,
                    info.js_arity(),
                    &info.adapter.words,
                    info.last_param_is_list,
                ) {
                    out.push(Diagnostic::error(code(d.code), d.message, d.span));
                }
            }
            _ => {}
        }
    }
    out
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
