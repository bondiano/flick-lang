impl Loaded {
    fn namespace_access(&self) -> Vec<String> {
        let mut access = self
            .virtual_of
            .as_ref()
            .map_or_else(Vec::new, |v| v.capture.access_modules.clone());
        access.push(self.name.clone());
        access.sort();
        access.dedup();
        access
    }

    fn can_access_member(&self, member: &StaticMember) -> bool {
        self.name == member.parent
            || self.virtual_of.as_ref().is_some_and(|context| {
                context.capture.access_modules.contains(&member.parent)
                    || context.capture.bindings.iter().any(|binding| {
                        binding.argument.module == format!("{}.{}", member.parent, member.name)
                            && binding.argument.access_modules.contains(&member.parent)
                    })
            })
    }
    fn can_access(&self, owner: &Root, name: &str) -> bool {
        self.root.same_package(owner)
            || self.virtual_of.as_ref().is_some_and(|context| {
                let capture = &context.capture;
                matches!(owner, Root::Package(package) if capture.access_packages.contains(package))
                    || capture
                        .bindings
                        .iter()
                        .any(|binding| binding.argument.module == name)
            })
    }

    fn definition_package(&self) -> Option<&str> {
        self.virtual_of
            .as_ref()
            .and_then(|v| v.capture.definition_package.as_deref())
            .or(match &self.root {
                Root::Package(package) => Some(package.as_str()),
                _ => None,
            })
    }
}

impl Loaded {
    /// A proof module, or one with a proof declaration.
    fn has_proofs(&self) -> bool {
        use crate::syntax::kind::SyntaxKind;
        self.root == Root::Proof || (self.parse.tree.nodes()).any(|n| n.kind == SyntaxKind::ProofDecl)
    }
}

impl Loader<'_> {
    /// Makes room for the IR of every module. A proof reads the IR of the
    /// modules it names, so a run with proofs lowers them.
    fn size_lowered(&mut self) {
        if self.modules.iter().any(Loaded::has_proofs) {
            self.lowered.get_or_insert_with(Vec::new);
            self.lower_packages_only = false;
        }
        if let Some(lowered) = &mut self.lowered {
            lowered.resize_with(self.modules.len(), || None);
        }
    }
}
