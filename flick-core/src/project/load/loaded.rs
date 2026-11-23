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

    /// A module with a class that states a law.
    fn has_laws(&self) -> bool {
        use crate::syntax::kind::SyntaxKind;
        (self.parse.tree.nodes())
            .filter(|n| n.kind == SyntaxKind::ClassDecl)
            .flat_map(|c| c.nodes().filter(|n| n.kind == SyntaxKind::Members))
            .any(|m| m.nodes().any(|n| n.kind == SyntaxKind::LawDecl))
    }

    /// A module with at least one function contract (`where require/ensure`),
    /// on a top-level `let`, inside a `rec { ... }` group, or in an inline
    /// module.
    fn has_contracts(&self) -> bool {
        use crate::syntax::cst::Node;
        use crate::syntax::kind::SyntaxKind as K;
        fn decl_has_contract(decl: &Node) -> bool {
            match decl.kind {
                K::LetDecl => decl.nodes().any(|n| n.kind == K::Contract),
                K::RecGroup => decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                    .filter(|n| n.kind == K::LetDecl)
                    .any(|m| m.nodes().any(|n| n.kind == K::Contract)),
                K::InlineModuleDecl => decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                    .any(decl_has_contract),
                _ => false,
            }
        }
        let tree = self
            .expansion
            .as_ref()
            .map_or(&self.parse.tree, |e| &e.tree);
        tree.nodes().any(decl_has_contract)
    }

    /// Collects every `(module_name, function_name)` declared with a `where`
    /// contract in this module (including `rec { ... }` groups and inline
    /// modules), so cross-module callers never unfold a contracted callee's
    /// body as an uncontracted helper.
    fn collect_contract_names(&self, text: &str, out: &mut HashSet<(String, String)>) {
        use crate::syntax::cst::Node;
        use crate::syntax::kind::SyntaxKind as K;
        fn walk(
            decl: &Node,
            module_name: &str,
            text: &str,
            out: &mut HashSet<(String, String)>,
        ) {
            match decl.kind {
                K::LetDecl => {
                    if decl.nodes().any(|n| n.kind == K::Contract)
                        && let Some(pat) = decl.nodes().find(|n| n.kind == K::VarPat)
                    {
                        let name = pat.text(text).trim().to_owned();
                        if !name.is_empty() {
                            out.insert((module_name.to_owned(), name));
                        }
                    }
                }
                K::RecGroup => {
                    for member in decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                        .filter(|n| n.kind == K::LetDecl)
                    {
                        walk(member, module_name, text, out);
                    }
                }
                K::InlineModuleDecl => {
                    let sub_name = decl
                        .nodes()
                        .find(|n| n.kind == K::ModulePath)
                        .map(|p| p.text(text).split_whitespace().collect::<String>())
                        .filter(|local| !local.is_empty())
                        .map(|local| {
                            if module_name.is_empty() {
                                local
                            } else {
                                format!("{module_name}.{local}")
                            }
                        });
                    if let Some(sub_name) = sub_name {
                        for member in decl
                            .nodes()
                            .filter(|n| n.kind == K::Members)
                            .flat_map(Node::nodes)
                        {
                            walk(member, &sub_name, text, out);
                        }
                    }
                }
                _ => {}
            }
        }
        let tree = self
            .expansion
            .as_ref()
            .map_or(&self.parse.tree, |e| &e.tree);
        for decl in tree.nodes() {
            walk(decl, &self.name, text, out);
        }
    }
}

impl Loader<'_> {
    /// Whether this run is in `Mode::Prove` and at least one selected root
    /// module declares a contract.
    fn has_selected_contracts(&self) -> bool {
        self.prove_contracts
            && self
                .modules
                .iter()
                .any(|m| m.has_contracts() && self.selected_roots.contains(&m.path))
    }

    /// Makes room for the IR of every module. A proof or contract reads the IR
    /// of the modules it names, so a run with proofs or selected contracts
    /// lowers them.
    fn size_lowered(&mut self) {
        self.all_contracted.clear();
        for m in &self.modules {
            let text = m
                .expansion
                .as_ref()
                .map_or(self.sources.get(m.file).text.as_str(), |e| e.text.as_str());
            m.collect_contract_names(text, &mut self.all_contracted);
        }
        self.verified_contracts
            .resize_with(self.modules.len(), Vec::new);
        if self.modules.iter().any(Loaded::has_proofs) || self.has_selected_contracts() {
            self.lowered.get_or_insert_with(Vec::new);
            self.lower_packages_only = false;
        }
        if let Some(lowered) = &mut self.lowered {
            lowered.resize_with(self.modules.len(), || None);
        }
    }
}
