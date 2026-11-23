impl Checking<'_> {
    /// Runs the official JS/Go backend's own foreign adapter validator in-process
    /// over finalized `foreign` declarations so `flick check`, standalone files,
    /// LSP, and the browser playground report exact backend adapter diagnostics
    /// even when other items in the same module have errors.
    fn validate_official_foreign_adapters(
        &self,
        typing: &types::Typing,
        checked: &mut Vec<Diagnostic>,
    ) {
        let backend_id = self.foreign_backend.backend_id.as_str();
        if !matches!(backend_id, "flick/js" | "flick/go") || typing.foreigns.is_empty() {
            return;
        }
        let mut entries: Vec<_> = typing.foreigns.iter().collect();
        entries.sort_by_key(|(range, _)| *range);
        for (_, info) in entries {
            if info.adapter.words.is_empty() {
                continue;
            }
            let view = flick_ir::Foreign {
                name: String::new(),
                source_name: None,
                mode: info.mode,
                binding: flick_ir::Binding {
                    from: None,
                    name: String::new(),
                },
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
    }
}
