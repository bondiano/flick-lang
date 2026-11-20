impl Checker<'_> {
    /// The order bodies are converted in, and how many of them are the aliases
    /// it starts with: aliases first, each after the aliases it names, so that
    /// every later declaration sees what an alias stands for. `= {}` keeps its
    /// place: the uses before it decide whether it is a record.
    fn conversion_order(&self, pending: &[Pending]) -> (Vec<usize>, usize) {
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
        let early = order.len();
        order.extend((0..pending.len()).filter(|&i| !seen[i]));
        (order, early)
    }

    /// Converts every body in [`Self::conversion_order`]. An operation's
    /// parameters are walked through the aliases they name, so source alias
    /// cycles break once the aliases are in; generated ones break later.
    fn convert_bodies(&mut self, pending: &[Pending]) -> Vec<Body> {
        let mut bodies: Vec<Body> = pending.iter().map(|_| Body::None).collect();
        let (order, aliases) = self.conversion_order(pending);
        for (step, i) in order.into_iter().enumerate() {
            if step == aliases {
                self.break_alias_cycles(pending);
            }
            let p = &pending[i];
            bodies[i] = self.with_params(&p.params, |this| this.body(p));
            if let Some(def) = self.env.type_def_mut(&p.name) {
                match &bodies[i] {
                    Body::ModuleContract(requirements) => {
                        def.form = Form::ModuleContract {
                            requirements: requirements.clone(),
                        }
                    }
                    Body::ModuleSignature(signature) => {
                        def.form = Form::ModuleSignature {
                            signature: Box::new(signature.clone()),
                        }
                    }
                    _ => {}
                }
            }
        }
        bodies
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

    /// Source alias cycles are reported by resolution. Generated service
    /// implementation aliases are known only here, so report their cycles
    /// here too. Cyclic members become nominal to keep error recovery finite.
    fn break_alias_cycles(&mut self, pending: &[Pending]) {
        let module = self.env.module.clone();
        let aliases: Vec<String> = pending
            .iter()
            .map(|p| p.name.clone())
            .chain(
                pending
                    .iter()
                    .filter(|p| p.node.kind == K::ServiceDecl)
                    .map(|p| format!("{}.Impl", p.name)),
            )
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
        self.report_implementation_cycles(pending, &aliases, &edges);
        let cyclic: Vec<String> = (0..aliases.len())
            .filter(|&i| resolve::shortest_cycle(i, &edges).is_some())
            .map(|i| aliases[i].clone())
            .collect();
        for name in cyclic {
            if let Some(def) = self.env.type_def_mut(&name) {
                def.form = Form::NOMINAL;
            }
        }
    }

    fn report_implementation_cycles(
        &mut self,
        pending: &[Pending],
        aliases: &[String],
        edges: &[Vec<usize>],
    ) {
        let mut reported = vec![false; aliases.len()];
        for p in pending.iter().filter(|p| p.node.kind == K::ServiceDecl) {
            let name = format!("{}.Impl", p.name);
            let Some(at) = aliases.iter().position(|alias| *alias == name) else {
                continue;
            };
            if reported[at] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(at, edges) else {
                continue;
            };
            for &member in &cycle {
                reported[member] = true;
            }
            let chain = cycle
                .iter()
                .chain([&at])
                .map(|&i| aliases[i].as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            let span = p
                .node
                .child_tokens()
                .find(|token| token.kind == K::Con)
                .map_or_else(|| self.span(p.node), |token| self.token_span(token));
            self.error(
                codes::R007,
                span,
                format!("service implementation type `{name}` is cyclic: {chain}"),
            );
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
                variance: Vec::new(),
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
                        variance: Vec::new(),
                    },
                );
                self.env.declare(Namespace::Type, &member);
            }
        }
    }
}
