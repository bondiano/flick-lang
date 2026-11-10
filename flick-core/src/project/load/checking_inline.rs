#[cfg(feature = "ide")]
fn merge_inline_ide(output: &mut Output, child_output: &Output, child: &Loaded) {
    if let Some(typing) = Arc::get_mut(&mut output.typing) {
        let child_typing = &child_output.typing;
        typing.names.extend(child_typing.names.clone());
        typing.binders.extend(child_typing.binders.clone());
        typing.types.extend(child_typing.types.clone());
        typing
            .signature_types
            .extend(child_typing.signature_types.clone());
        typing.type_names.extend(child_typing.type_names.clone());
        typing.fields.extend(child_typing.fields.clone());
        typing.refined.extend(child_typing.refined.clone());
        typing.values.extend(child_typing.values.clone());
    }
    let mut analyzed_child = Analyzed::new(
        child,
        child_output.typing.clone(),
        child_output.interface.clone(),
    );
    analyzed_child.inline = true;
    output.inline_ide.push(analyzed_child);
    output.inline_ide.extend(child_output.inline_ide.clone());
}

fn merge_inline_lowering(
    output: &mut Output,
    child_output: &mut Output,
    loaded: &Loaded,
    units: &InlineUnits,
) {
    // Invalid child interfaces may contain unresolved error types, which are
    // not serializable. A module with errors cannot contribute executable IR
    // or model declarations to the compilation.
    if output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return;
    }
    if let (Some(parent_ir), Some(mut child_ir)) = (&mut output.ir, child_output.ir.take()) {
        relocate_ir(parent_ir, &units.name, &loaded.name, &units.local);
        relocate_ir(&mut child_ir, &units.name, &loaded.name, &units.local);
        for (label, value) in child_ir.labels {
            parent_ir
                .labels
                .insert(format!("{}${label}", units.local), value);
        }
        parent_ir
            .items
            .extend(child_ir.items.into_iter().map(|mut item| {
                rename_item(&mut item, &units.local);
                item
            }));
    }
    let child_declarations = relocate_declarations(
        &child_output.declarations,
        &units.name,
        &loaded.name,
        &units.local,
    );
    let declarations = Arc::make_mut(&mut output.declarations);
    declarations.exports.extend(child_declarations.exports);
    declarations.instances.extend(child_declarations.instances);
}

fn unsupported_inline_applications(inline: &Node, text: &str, file: FileId) -> Vec<Diagnostic> {
    inline
        .nodes()
        .filter(|node| node.kind == K::Members)
        .flat_map(Node::nodes)
        .filter(|node| node.kind == K::ModuleApplyDecl)
        .filter_map(|node| {
            let name = node.child_tokens().find(|token| token.kind == K::Con)?;
            Some(Diagnostic::error(
                codes::R019,
                format!(
                    "static module application `{}` cannot be declared inside an inline module; declare it in a file module and import its result",
                    name.text(text)
                ),
                Span::new(file, name.start, name.end),
            ))
        })
        .collect()
}

struct InlineUnits {
    prefix: Loaded,
    child: Loaded,
    parent: Loaded,
    local: String,
    name: String,
    at: Span,
}

fn inline_units(
    loaded: &Loaded,
    tree: &Node,
    index: usize,
    inline: &Node,
    text: &str,
) -> Option<InlineUnits> {
    let path = inline.nodes().find(|n| n.kind == K::ModulePath)?;
    let local: String = path.text(text).split_whitespace().collect();
    let name = format!("{}.{}", loaded.name, local);
    let (start, end) = path.range();
    let at = Span::new(loaded.file, start, end);
    let all = Item {
        kind: ItemKind::All,
        name: String::new(),
        span: at,
    };
    let prefix_tree = Node {
        kind: K::SourceFile,
        children: tree
            .nodes()
            .take(index)
            .cloned()
            .map(Element::Node)
            .collect(),
    };
    let mut prefix_syntax = (*loaded.syntax).clone();
    if let Some(header) = &mut prefix_syntax.header {
        header.exposing = Some(vec![all.clone()]);
    }
    let prefix = inline_unit(
        loaded,
        loaded.name.clone(),
        prefix_tree,
        prefix_syntax,
        false,
    );
    let mut child_syntax = (*loaded.syntax).clone();
    child_syntax.header = Some(Header {
        result: None,
            body: None,
            materialized_body: None,
        parameter_groups: Vec::new(),
        name: name.clone(),
        private: true,
        span: at,
        exposing: inline.nodes().find(|n| n.kind == K::Exposing).map(|_| {
            let header = Node {
                kind: K::SourceFile,
                children: vec![Element::Node(Node {
                    kind: K::ModuleHeader,
                    children: inline.children.clone(),
                })],
            };
            resolve::module_syntax(&header, text, loaded.file)
                .header
                .and_then(|h| h.exposing)
                .unwrap_or_default()
        }),
        parameters: Vec::new(),
    });
    child_syntax.imports.push(Import {
        module: loaded.name.clone(),
        span: at,
        alias: None,
        exposing: vec![all],
        plugins: Vec::new(),
    });
    let body = inline.nodes().find(|n| n.kind == K::Members);
    let child_tree = Node {
        kind: K::SourceFile,
        children: body
            .into_iter()
            .flat_map(Node::nodes)
            .cloned()
            .map(Element::Node)
            .collect(),
    };
    child_syntax
        .imports
        .extend(resolve::module_syntax(&child_tree, text, loaded.file).imports);
    let mut child = inline_unit(loaded, name.clone(), child_tree, child_syntax, false);
    set_inline_source_tree(&mut child, loaded, at);
    let parent = inline_parent(loaded, tree, inline, &name, &local, at);
    Some(InlineUnits {
        prefix,
        child,
        parent,
        local,
        name,
        at,
    })
}

/// The checker sees expanded plugin declarations. Editor queries must walk
/// original source tokens: generated tokens may lie beyond the file text.
fn set_inline_source_tree(child: &mut Loaded, loaded: &Loaded, at: Span) {
    let Some(expansion) = child.expansion.as_mut() else {
        return;
    };
    let Some(source_inline) = loaded.expansion.as_ref().and_then(|original| {
        original.source_tree.nodes().find(|decl| {
            decl.kind == K::InlineModuleDecl
                && decl
                    .nodes()
                    .find(|n| n.kind == K::ModulePath)
                    .is_some_and(|path| path.range().0 == at.start)
        })
    }) else {
        return;
    };
    let source_body = source_inline.nodes().find(|n| n.kind == K::Members);
    Arc::make_mut(expansion).source_tree = Node {
        kind: K::SourceFile,
        children: source_body
            .into_iter()
            .flat_map(Node::nodes)
            .cloned()
            .map(Element::Node)
            .collect(),
    };
}

fn inline_parent(
    loaded: &Loaded,
    tree: &Node,
    inline: &Node,
    name: &str,
    local: &str,
    at: Span,
) -> Loaded {
    let parent_tree = Node {
        kind: K::SourceFile,
        children: tree
            .nodes()
            .filter(|n| n.range() != inline.range())
            .cloned()
            .map(Element::Node)
            .collect(),
    };
    let mut parent_syntax = (*loaded.syntax).clone();
    parent_syntax.imports.push(Import {
        module: name.to_owned(),
        span: at,
        alias: Some(local.to_owned()),
        exposing: Vec::new(),
        plugins: Vec::new(),
    });
    inline_unit(
        loaded,
        loaded.name.clone(),
        parent_tree,
        parent_syntax,
        true,
    )
}

fn inline_unit(
    loaded: &Loaded,
    name: String,
    tree: Node,
    syntax: ModuleSyntax,
    diagnostics: bool,
) -> Loaded {
    let mut expansion = loaded.expansion.as_deref().cloned();
    if let Some(expansion) = &mut expansion {
        expansion.tree = tree.clone();
        if !diagnostics {
            expansion.source_tree = tree.clone();
        }
    }
    Loaded {
        path: loaded.path.clone(),
        file: loaded.file,
        root: loaded.root.clone(),
        parse: Arc::new(Parse {
            tree,
            diagnostics: if diagnostics {
                loaded.parse.diagnostics.clone()
            } else {
                Vec::new()
            },
        }),
        syntax: Arc::new(syntax),
        name,
        expansion: expansion.map(Arc::new),
        editing: loaded.editing.clone(),
        loader: loaded.loader.clone(),
        virtual_of: loaded.virtual_of.clone(),
    }
}
