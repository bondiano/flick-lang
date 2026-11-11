/// Recursive static signatures have no finite structural contract. Follow only
/// signature dependencies: ordinary member types may shadow a contract name.
pub(super) fn module_signature_cycles(tree: &Node, text: &str, file: FileId) -> Vec<Diagnostic> {
    fn names(node: &Node, text: &str, out: &mut Vec<String>) {
        match node.kind {
            K::Members => {
                for member in node.nodes().filter(|n| n.kind == K::ModuleMember) {
                    for signature in member.nodes() {
                        names(signature, text, out);
                    }
                }
            }
            K::ModuleSignatureRefinement => {
                if let Some(signature) = node.nodes().next() {
                    names(signature, text, out);
                }
            }
            K::NameType if !node.has_token(K::Dot) => {
                out.extend(
                    node.child_tokens()
                        .filter(|t| t.kind == K::Con)
                        .map(|t| t.text(text).to_owned()),
                );
            }
            _ => {
                for child in node.nodes() {
                    names(child, text, out);
                }
            }
        }
    }
    let contracts: Vec<_> = tree
        .nodes()
        .filter(|n| n.kind == K::ModuleTypeDecl)
        .filter_map(|decl| {
            let name = decl.child_tokens().find(|t| t.kind == K::Con)?;
            let mut dependencies = Vec::new();
            names(decl, text, &mut dependencies);
            Some((
                name.text(text),
                Span::new(file, name.start, name.end),
                dependencies,
            ))
        })
        .collect();
    let edges: Vec<Vec<usize>> = contracts
        .iter()
        .map(|(_, _, names)| {
            names
                .iter()
                .filter_map(|name| {
                    contracts
                        .iter()
                        .position(|(candidate, _, _)| candidate == name)
                })
                .collect()
        })
        .collect();
    let mut reported = vec![false; contracts.len()];
    let mut diagnostics = Vec::new();
    for start in 0..contracts.len() {
        if reported[start] {
            continue;
        }
        let Some(cycle) = shortest_cycle(start, &edges) else {
            continue;
        };
        for &index in &cycle {
            reported[index] = true;
        }
        let chain: Vec<_> = cycle
            .iter()
            .chain([&start])
            .map(|&index| contracts[index].0)
            .collect();
        let (name, span, _) = &contracts[start];
        diagnostics.push(Diagnostic::error(
            codes::R007,
            format!(
                "module signature `{name}` is cyclic: {}",
                chain.join(" -> ")
            ),
            *span,
        ));
    }
    diagnostics
}
