impl Checking<'_> {
    fn visible_interface<'i>(
        &'i self,
        loaded: &Loaded,
        interfaces: &'i [Option<Interface>],
        name: &str,
    ) -> Option<&'i Interface> {
        if companion_of(&loaded.name) == Some(name)
            && let Some(view) = self.privates.get(name)
        {
            return Some(view);
        }
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
}

/// What a test companion sees of its module: its exports and every other
/// top-level declaration, constructors of exported types included.
fn private_view(exported: &Interface, declarations: &Interface) -> Interface {
    let mut view = exported.clone();
    for export in &declarations.exports {
        if !view.exports.iter().any(|e| e.entity == export.entity) {
            view.exports.push(export.clone());
        }
    }
    view.hidden.clear();
    view
}
