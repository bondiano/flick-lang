impl Checker<'_> {
    /// Manifest equalities are transparent aliases, so their dependencies must
    /// be acyclic before constructing a parameter's symbolic interface.
    fn reject_contract_cycles(
        &mut self,
        members: &[&Node],
        requirements: &mut [ModuleRequirement],
    ) {
        let types: Vec<_> = requirements
            .iter()
            .filter_map(|r| match r {
                ModuleRequirement::Type {
                    name,
                    params,
                    manifest,
                } => Some((name.clone(), params, manifest)),
                _ => None,
            })
            .collect();
        let index: HashMap<_, _> = types
            .iter()
            .enumerate()
            .map(|(i, (name, _, _))| (name.as_str(), i))
            .collect();
        let edges: Vec<Vec<usize>> = types
            .iter()
            .map(|(_, params, manifest)| {
                let mut used = Vec::new();
                if let Some(ty) = manifest {
                    ty.visit(&mut |part| {
                        if let Type::Var(name) = part
                            && !params.iter().any(|p| p.name == *name)
                            && let Some(&at) = index.get(name.as_str())
                        {
                            used.push(at);
                        }
                    });
                }
                used
            })
            .collect();
        let mut cyclic = vec![false; types.len()];
        for at in 0..types.len() {
            if cyclic[at] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(at, &edges) else {
                continue;
            };
            for &member in &cycle {
                cyclic[member] = true;
            }
            let name = &types[at].0;
            let chain = cycle
                .iter()
                .chain([&at])
                .map(|&i| types[i].0.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            if let Some(member) = members
                .iter()
                .find(|member| self.con_token(member).as_ref() == Some(name))
            {
                let span = member
                    .child_tokens()
                    .find(|t| t.kind == K::Con)
                    .map_or_else(|| self.span(member), |t| self.token_span(t));
                self.error(
                    codes::R007,
                    span,
                    format!("module type equality `{name}` is cyclic: {chain}"),
                );
            }
        }
        let names: Vec<_> = types
            .iter()
            .enumerate()
            .filter(|(i, _)| cyclic[*i])
            .map(|(_, (name, _, _))| name.clone())
            .collect();
        for requirement in requirements {
            if let ModuleRequirement::Type { name, manifest, .. } = requirement
                && names.contains(name)
            {
                *manifest = None;
            }
        }
    }
}
