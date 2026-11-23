impl Checking<'_> {
    fn lower_if_clean(
        &self,
        loaded: &Loaded,
        module: &resolve::Module<'_>,
        typing: &types::Typing,
        clean: bool,
        declarations: &mut Arc<Interface>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ir::Module> {
        if clean
            && self.lowers(loaded)
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
