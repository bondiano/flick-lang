fn source_module_declarations(text: &str, owner: &str) -> HashMap<String, String> {
    fn collect(
        node: &Node,
        text: &str,
        scope: &str,
        owner: &str,
        out: &mut HashMap<String, String>,
    ) {
        for declaration in node.nodes() {
            if scope == owner
                && declaration.kind == K::ModuleTypeDecl
                && let Some(name) = declaration
                    .child_tokens()
                    .find(|token| token.kind == K::Con)
            {
                let (start, end) = declaration.range();
                out.insert(
                    name.text(text).to_owned(),
                    text[start as usize..end as usize].to_owned(),
                );
            }
            if declaration.kind == K::InlineModuleDecl
                && let Some(path) = declaration.nodes().find(|n| n.kind == K::ModulePath)
                && let Some(body) = declaration.nodes().find(|n| n.kind == K::Members)
            {
                let local: String = path.text(text).split_whitespace().collect();
                collect(body, text, &format!("{scope}.{local}"), owner, out);
            }
        }
    }
    let parsed = syntax::parse(text, FileId::default());
    let root = parsed
        .tree
        .nodes()
        .find(|n| n.kind == K::ModuleHeader)
        .and_then(|header| header.nodes().find(|n| n.kind == K::ModulePath))
        .map_or_else(
            || owner.to_owned(),
            |path| path.text(text).split_whitespace().collect(),
        );
    let mut declarations = HashMap::new();
    collect(&parsed.tree, text, &root, owner, &mut declarations);
    declarations
}

#[cfg(test)]
mod scoped_module_tests {
    use crate::ir::tests::check_text;

    #[test]
    fn review_source_contracts_stay_in_their_defining_scope() {
        let source = "module Api exposing (Input)\ntype module Input =\n  value : Int\nmodule Child exposing (Input) =\n  type module Input =\n    value : String\n";
        let (_, interface) = check_text("Api", source).expect("source checks");
        let page = super::page(&interface, source);
        assert!(page.contains("value : Int"), "{page}");
        assert!(!page.contains("value : String"), "{page}");
        let (_, child) = check_text(
            "Api.Child",
            "module Api.Child exposing (Input)\ntype module Input =\n  value : String\n",
        )
        .expect("child checks");
        let page = super::page(&child, source);
        assert!(page.contains("value : String"), "{page}");
        assert!(!page.contains("value : Int"), "{page}");
    }
}
