impl Loader<'_> {
    fn callee_origin(&self, parent: usize, expression: &Node, actual: &str) -> CalleeOrigin {
        fn path(node: &Node, source: &str) -> Option<String> {
            match node.kind {
                K::ModulePath => Some(node.text(source).split_whitespace().collect()),
                K::ModuleParenExpr => path(node.nodes().next()?, source),
                _ => None,
            }
        }
        let source = &self.sources.get(self.modules[parent].file).text;
        let mut reference = path(expression, source);
        let mut seen = Vec::new();
        while let Some(name) = reference.take() {
            if seen.contains(&name) {
                break;
            }
            seen.push(name.clone());
            let (head, tail) = name.split_once('.').unwrap_or((&name, ""));
            if let Some(context) = &self.modules[parent].virtual_of
                && let Some(binding) = context
                    .capture
                    .bindings
                    .iter()
                    .rev()
                    .find(|b| b.alias == head)
            {
                return CalleeOrigin::Parameter {
                    binding: binding.origin.clone(),
                    members: tail
                        .split('.')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                };
            }
            reference = self.modules[parent]
                .syntax
                .applications
                .iter()
                .find(|a| a.name == head && a.annotation.is_none())
                .and_then(|a| path(&a.expression, source))
                .map(|target| {
                    if tail.is_empty() {
                        target
                    } else {
                        format!("{target}.{tail}")
                    }
                });
        }
        if let Some(member) = self
            .by_name
            .get(actual)
            .and_then(|&i| self.modules[i].member_of.as_ref())
            && !self.modules[parent].can_access_member(member)
        {
            return CalleeOrigin::Member(member.clone());
        }
        CalleeOrigin::Module(actual.to_owned())
    }

    fn expression_captures(&self, parent: usize, functor: Option<&Node>) -> (String, usize) {
        let loaded = &self.modules[parent];
        let source = &self.sources.get(loaded.file).text;
        let tokens: Vec<_> = functor
            .into_iter()
            .flat_map(Node::tokens)
            .filter(|t| !t.kind.is_trivia())
            .map(|t| t.text(source))
            .collect();
        let mut shadowed: Vec<_> = functor
            .into_iter()
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::ModuleParamGroup)
            .flat_map(Node::nodes)
            .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Con))
            .map(|t| t.text(source).to_owned())
            .collect();
        if let Some(body) = functor
            .into_iter()
            .flat_map(Node::nodes)
            .find(|n| n.kind == K::Members)
        {
            shadowed.extend(
                resolve::module_syntax(body, source, loaded.file)
                    .imports
                    .into_iter()
                    .map(|i| i.qualifier().to_owned()),
            );
        }
        let mut imports = String::new();
        let mut count = 0;
        for import in &loaded.syntax.imports {
            if import.module.starts_with('$') || shadowed.iter().any(|q| q == import.qualifier()) {
                continue;
            }
            // Written imports also bring lexical instances. Inferred signature
            // and module-expression imports are dependencies only when used;
            // copying the enclosing declaration's own binding creates a cycle.
            let written = loaded.parse.tree.nodes().any(|n| {
                let (start, end) = n.range();
                n.kind == K::ImportDecl && start <= import.span.start && import.span.end <= end
            });
            if !written {
                let qualifier = import
                    .qualifier()
                    .split('.')
                    .collect::<Vec<_>>()
                    .join(" . ");
                let path: Vec<_> = qualifier.split_whitespace().collect();
                if !tokens
                    .windows(path.len())
                    .enumerate()
                    .any(|(i, ts)| ts == path && (i == 0 || tokens[i - 1] != "."))
                {
                    continue;
                }
            }
            imports.push_str("import ");
            imports.push_str(&import.module);
            if let Some(alias) = &import.alias {
                imports.push_str(" as ");
                imports.push_str(alias);
            }
            let items: Vec<_> = import
                .exposing
                .iter()
                .map(|item| match item.kind {
                    ItemKind::All => "..".to_owned(),
                    ItemKind::ConAll => format!("{}(..)", item.name),
                    _ => item.name.clone(),
                })
                .chain(
                    import
                        .plugins
                        .iter()
                        .map(|item| format!("plugin {}", item.name)),
                )
                .collect();
            if !items.is_empty() {
                imports.push_str(" exposing (");
                imports.push_str(&items.join(", "));
                imports.push(')');
            }
            imports.push('\n');
            count += 1;
        }
        (imports, count)
    }
}

fn body_source(node: &Node, source: &str) -> String {
    let (start, end) = node.range();
    let indent = source[..start as usize]
        .rsplit('\n')
        .next()
        .map_or(0, str::len);
    source[start as usize..end as usize]
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line
            } else {
                let spaces = line.bytes().take_while(|b| *b == b' ').count().min(indent);
                &line[spaces..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
