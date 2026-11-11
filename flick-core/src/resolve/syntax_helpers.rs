fn collect_inline_imports(node: &Node, text: &str, file: FileId, out: &mut Vec<Import>) {
    for child in node
        .nodes()
        .filter(|n| n.kind == K::Members)
        .flat_map(Node::nodes)
    {
        if child.kind == K::ImportDecl {
            let tree = Node {
                kind: K::SourceFile,
                children: vec![Element::Node(child.clone())],
            };
            out.extend(module_syntax(&tree, text, file).imports);
        } else if child.kind == K::InlineModuleDecl {
            collect_inline_imports(child, text, file, out);
        }
    }
}

/// The heads of std's plugin `imports` expose that a block under `tree`
/// writes in a form the parser reads only after an import: an expression
/// without `{`, other than [`BARE`] before a name, or a declaration that is
/// none of [`DECLARATIONS`]. The body of a block is not parsed yet, so a head
/// written there counts too, whatever its form: the import then stays.
fn bodiless(tree: &Node, text: &str, imports: &[Import]) -> Vec<String> {
    let implicit = |head: &str| HEADS.iter().any(|(h, _)| *h == head);
    let imported = |head: &str| (imports.iter()).any(|i| i.plugins.iter().any(|p| p.name == head));
    if !HEADS.iter().any(|(h, _)| imported(h)) {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    blocks_under(tree, &mut blocks);
    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        let mut words = (block.child_tokens()).filter(|t| !t.kind.is_trivia());
        let Some(head) = words.next().filter(|t| t.kind == K::Ident) else {
            continue;
        };
        let at = head.start;
        let head = head.text(text);
        let next = words.next();
        let free = match block.kind {
            K::PluginExpr => {
                block.nodes().any(|n| n.kind == K::Block)
                    || BARE.contains(&head)
                        && next.is_some_and(|t| matches!(t.kind, K::Ident | K::Con))
            }
            _ => next.is_some_and(|t| {
                (DECLARATIONS.iter()).any(|(h, w)| *h == head && *w == t.text(text))
            }),
        };
        // a block in its body is read only as it expands, so any word of a head counts
        let inner = (block.tokens().into_iter())
            .filter(|t| t.kind == K::Ident && t.start > at)
            .map(|t| t.text(text))
            .filter(|w| implicit(w) && imported(w));
        for word in inner {
            if !out.iter().any(|h| h == word) {
                out.push(word.to_owned());
            }
        }
        if !free && implicit(head) && imported(head) && !out.iter().any(|h| h == head) {
            out.push(head.to_owned());
        }
    }
    out
}

fn items(exposing: &Node, text: &str, file: FileId) -> Vec<Item> {
    exposing
        .nodes()
        .filter_map(|item| {
            let first = item.child_tokens().next()?;
            let kind = match first.kind {
                K::Ident => ItemKind::Value,
                K::DotDot => ItemKind::All,
                K::Con if item.has_token(K::DotDot) => ItemKind::ConAll,
                K::Con => ItemKind::Con,
                _ => return None,
            };
            let name = match kind {
                ItemKind::All => String::new(),
                _ => first.text(text).to_owned(),
            };
            Some(Item {
                kind,
                name,
                span: span(item, file),
            })
        })
        .collect()
}

/// The items `plugin head` of `exposing`, by head.
fn plugins(exposing: &Node, text: &str, file: FileId) -> Vec<Item> {
    exposing
        .nodes()
        .filter_map(|item| {
            let mut tokens = item.child_tokens();
            tokens.next().filter(|t| t.kind == K::PluginKw)?;
            let head = tokens.next().filter(|t| t.kind == K::Ident)?;
            Some(Item {
                kind: ItemKind::Value,
                name: head.text(text).to_owned(),
                span: span(item, file),
            })
        })
        .collect()
}

pub(super) fn span(node: &Node, file: FileId) -> Span {
    let (start, end) = node.range();
    Span::new(file, start, end)
}
