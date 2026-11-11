impl Checker<'_> {
    /// The order bodies are converted in: aliases first, each after the aliases
    /// it names, so that every later declaration sees what an alias stands for.
    /// `= {}` keeps its place: the uses before it decide whether it is a record.
    fn conversion_order(&self, pending: &[Pending]) -> Vec<usize> {
        let module = &self.env.module;
        let converted_early = |p: &Pending| {
            matches!(
                self.env.type_def(module, &p.name).map(|d| &d.form),
                Some(Form::Alias { .. })
            ) || p.node.kind == K::ModuleTypeDecl
        };
        let converted_early = |p: &Pending| {
            converted_early(p)
                && !p
                    .node
                    .nodes()
                    .any(|n| n.kind == K::RowType && n.nodes().next().is_none())
        };
        let aliases: HashMap<&str, usize> = pending
            .iter()
            .enumerate()
            .filter(|(_, p)| converted_early(p))
            .map(|(i, p)| (p.name.as_str(), i))
            .collect();
        let mut order = Vec::with_capacity(pending.len());
        let mut seen = vec![false; pending.len()];
        for (i, p) in pending.iter().enumerate() {
            if aliases.contains_key(p.name.as_str()) {
                self.after_named(i, pending, &aliases, &mut seen, &mut order);
            }
        }
        order.extend((0..pending.len()).filter(|&i| !seen[i]));
        order
    }

    /// Adds alias `i` after the aliases it names; a cycle, which name resolution
    /// reports, ends where it comes back.
    fn after_named(
        &self,
        i: usize,
        pending: &[Pending],
        aliases: &HashMap<&str, usize>,
        seen: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if mem::replace(&mut seen[i], true) {
            return;
        }
        for name in resolve::alias_names(pending[i].node, self.text) {
            if let Some(&named) = aliases.get(name.as_str()) {
                self.after_named(named, pending, aliases, seen, order);
            }
        }
        order.push(i);
    }

    /// A cycle of transparent aliases is reported by name resolution; its
    /// members become nominal here, so expanding them terminates.
    fn break_alias_cycles(&mut self, pending: &[Pending]) {
        let module = self.env.module.clone();
        let aliases: Vec<&str> = pending
            .iter()
            .map(|p| p.name.as_str())
            .filter(|name| {
                matches!(
                    self.env.type_def(&module, name).map(|d| &d.form),
                    Some(Form::Alias { opaque: false, .. })
                )
            })
            .collect();
        let edges: Vec<Vec<usize>> = aliases
            .iter()
            .map(|name| {
                let Some(Form::Alias { body, .. }) =
                    self.env.type_def(&module, name).map(|d| &d.form)
                else {
                    return Vec::new();
                };
                let mut out = Vec::new();
                body.visit(&mut |t| {
                    if let Type::Con { module: m, name: n } = t
                        && *m == module
                        && let Some(i) = aliases.iter().position(|a| a == n)
                    {
                        out.push(i);
                    }
                });
                out
            })
            .collect();
        let cyclic: Vec<String> = (0..aliases.len())
            .filter(|&i| resolve::shortest_cycle(i, &edges).is_some())
            .map(|i| aliases[i].to_owned())
            .collect();
        for name in cyclic {
            if let Some(def) = self.env.type_def_mut(&name) {
                def.form = Form::NOMINAL;
            }
        }
    }

    fn kind_decl(&mut self, decl: &Node) {
        let Some(name) = self.con_token(decl) else {
            return;
        };
        let kind = Kind::Named {
            module: self.env.module.clone(),
            name: name.clone(),
        };
        self.env.define_type(
            &name,
            TypeDef {
                kind: kind.clone(),
                form: Form::Kind,
            },
        );
        self.env.declare(Namespace::Type, &name);
        for variant in decl.nodes().filter(|n| n.kind == K::Variant) {
            if let Some(member) = self.con_token(variant) {
                self.env.define_type(
                    &member,
                    TypeDef {
                        kind: kind.clone(),
                        form: Form::KindMember,
                    },
                );
                self.env.declare(Namespace::Type, &member);
            }
        }
    }
}
