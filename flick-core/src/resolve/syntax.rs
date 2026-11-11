//! The module header and imports of a parsed file.

use std::sync::Arc;

use crate::plugin::blocks_under;
use crate::plugin::first_party::{BARE, DECLARATIONS, HEADS};
use crate::source::{FileId, Span};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug, Clone)]
pub struct ModuleSyntax {
    pub header: Option<Header>,
    pub imports: Vec<Import>,
    /// Imports written inside inline module bodies, used for dependency
    /// loading and cache invalidation, not the enclosing module's scope.
    pub inline_imports: Vec<Import>,
    /// Named parameterized module applications declared by this file.
    pub applications: Vec<ModuleApplication>,
    /// The heads of std's plugin its imports expose that a block of it
    /// writes as only an import lets it, without `{`: `cli Main`.
    pub bodiless: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub name: String,
    pub private: bool,
    /// Span of the module path.
    pub span: Span,
    /// `None` without `exposing`: the module exports nothing.
    pub exposing: Option<Vec<Item>>,
    /// Static module parameters of a functor source file.
    pub parameters: Vec<ModuleParam>,
    pub result: Option<Node>,
    pub parameter_groups: Vec<usize>,
    /// An expression-bodied file functor; structure bodies remain top-level declarations.
    pub body: Option<Node>,
    /// Concrete value selected by elaborating an expression-bodied module.
    pub materialized_body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModuleParam {
    pub name: String,
    pub span: Span,
    pub signature: Node,
}

#[derive(Debug, Clone)]
pub struct ModuleApplication {
    pub expression: Node,
    pub annotation: Option<Node>,
    pub argument_groups: Vec<usize>,
    pub name: String,
    pub template: String,
    pub template_span: Span,
    pub arguments: Vec<String>,
    pub argument_spans: Vec<Span>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    /// Span of the module path.
    pub span: Span,
    pub alias: Option<String>,
    pub exposing: Vec<Item>,
    /// The heads of plugin blocks it exposes, `plugin head`, each with the
    /// span of its item.
    pub plugins: Vec<Item>,
}

impl ModuleSyntax {
    /// The heads of plugin blocks its imports expose.
    pub fn heads(&self) -> Arc<[String]> {
        (self.imports.iter())
            .flat_map(|i| i.plugins.iter().map(|p| p.name.clone()))
            .collect()
    }

    /// The modules its imports name only by an alias, each with it: where
    /// generated code, which names a module by its path, says the alias.
    pub fn aliases(&self) -> Vec<(String, String)> {
        let imports = &self.imports;
        (imports.iter())
            .filter(|i| !(imports.iter()).any(|j| j.module == i.module && j.alias.is_none()))
            .filter_map(|i| Some((i.module.clone(), i.alias.clone()?)))
            .collect()
    }
}

impl Import {
    /// The prefix of qualified names: the alias, else the module path.
    pub fn qualifier(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.module)
    }
}

/// One entry of an `exposing (…)` list.
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: ItemKind,
    /// Empty for `..`.
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// `name`
    Value,
    /// `Type`, or a constructor
    Con,
    /// `Type(..)`
    ConAll,
    /// `..`
    All,
}

#[allow(clippy::too_many_lines)]
pub fn module_syntax(tree: &Node, text: &str, file: FileId) -> ModuleSyntax {
    let mut header = None;
    let mut imports = Vec::new();
    let mut applications = Vec::new();
    for node in tree.nodes() {
        let file_expression = node.kind == K::ModuleApplyDecl
            && header.is_none()
            && node.nodes().any(|n| {
                n.kind == K::ModulePath
                    && (n.has_token(K::Dot) || node.nodes().any(|n| n.kind == K::ModuleParamGroup))
            });
        let nested_functor = matches!(node.kind, K::InlineModuleDecl | K::ModuleApplyDecl)
            && !file_expression
            && node.nodes().any(|n| n.kind == K::ModuleParamGroup);
        if (node.kind == K::ModuleApplyDecl && !file_expression) || nested_functor {
            fn paths<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
                if node.kind == K::ModulePath {
                    out.push(node);
                } else {
                    for child in node.nodes() {
                        paths(child, out);
                    }
                }
            }
            let mut named = Vec::new();
            for child in node.nodes().filter(|n| n.kind != K::ModuleResult) {
                paths(child, &mut named);
            }
            let local = node.child_tokens().find(|t| t.kind == K::Con);
            let (local_name, local_span) = if let Some(local) = local {
                (
                    local.text(text).to_owned(),
                    Span::new(file, local.start, local.end),
                )
            } else if let Some(local) = named.first() {
                let local = *local;
                named.remove(0);
                (
                    local.text(text).split_whitespace().collect(),
                    span(local, file),
                )
            } else {
                continue;
            };
            let nested_expression = nested_functor.then(|| Node {
                kind: K::ModuleFunctorExpr,
                children: node
                    .nodes()
                    .filter(|n| {
                        matches!(
                            n.kind,
                            K::ModuleParamGroup
                                | K::ModuleResult
                                | K::Members
                                | K::ModuleApplyExpr
                                | K::ModuleFunctorExpr
                                | K::ModuleParenExpr
                                | K::ModuleAnnotExpr
                        )
                    })
                    .cloned()
                    .map(Element::Node)
                    .collect(),
            });
            let expression = nested_expression.as_ref().or_else(|| {
                node.nodes()
                    .filter(|n| {
                        matches!(
                            n.kind,
                            K::ModuleApplyExpr
                                | K::ModuleFunctorExpr
                                | K::ModuleParenExpr
                                | K::ModuleAnnotExpr
                        )
                    })
                    .last()
            });
            if let Some(expression) = expression {
                fn groups(node: &Node, out: &mut Vec<usize>) {
                    if node.kind == K::ModuleApplyExpr {
                        if let Some(callee) = node.nodes().next() {
                            groups(callee, out);
                        }
                        out.push(node.nodes().count().saturating_sub(1));
                    }
                }
                let mut argument_groups = Vec::new();
                for child in node.nodes() {
                    groups(child, &mut argument_groups);
                }
                applications.push(ModuleApplication {
                    expression: expression.clone(),
                    annotation: (!nested_functor)
                        .then(|| {
                            node.nodes()
                                .find(|n| n.kind == K::ModuleResult)
                                .and_then(|n| n.nodes().next())
                                .cloned()
                        })
                        .flatten(),
                    argument_groups,
                    name: local_name,
                    template: named
                        .first()
                        .map_or(String::new(), |n| n.text(text).split_whitespace().collect()),
                    template_span: named.first().map_or(local_span, |n| span(n, file)),
                    arguments: named
                        .iter()
                        .skip(1)
                        .map(|n| n.text(text).split_whitespace().collect())
                        .collect(),
                    argument_spans: named.iter().skip(1).map(|n| span(n, file)).collect(),
                    span: local_span,
                });
            }
            continue;
        }
        let Some(path) = node.nodes().find(|n| n.kind == K::ModulePath) else {
            continue;
        };
        let name = path.text(text).split_whitespace().collect();
        let span = span(path, file);
        let exposing = node.nodes().find(|n| n.kind == K::Exposing);
        match node.kind {
            K::ModuleHeader | K::ModuleApplyDecl if header.is_none() => {
                header = Some(Header {
                    materialized_body: None,
                    body: file_expression
                        .then(|| node.nodes().last().cloned())
                        .flatten(),
                    parameter_groups: node
                        .nodes()
                        .filter(|n| n.kind == K::ModuleParamGroup)
                        .map(|g| g.nodes().count())
                        .collect(),
                    name,
                    private: node
                        .child_tokens()
                        .any(|t| t.kind == K::Ident && t.text(text) == "private"),
                    span,
                    exposing: exposing.map(|e| items(e, text, file)),
                    parameters: node
                        .nodes()
                        .filter(|n| n.kind == K::ModuleParamGroup)
                        .flat_map(Node::nodes)
                        .filter(|n| n.kind == K::ModuleParam)
                        .filter_map(|param| {
                            let name = param.child_tokens().find(|t| t.kind == K::Con)?;
                            let contract = param.nodes().next()?;
                            Some(ModuleParam {
                                name: name.text(text).to_owned(),
                                span: Span::new(file, name.start, name.end),
                                signature: contract.clone(),
                            })
                        })
                        .collect(),
                    result: node
                        .nodes()
                        .find(|n| n.kind == K::ModuleResult)
                        .and_then(|n| n.nodes().next())
                        .cloned(),
                });
            }
            K::ImportDecl => imports.push(Import {
                module: name,
                span,
                alias: node
                    .child_tokens()
                    .find(|t| t.kind == K::Con)
                    .map(|t| t.text(text).to_owned()),
                exposing: exposing.map(|e| items(e, text, file)).unwrap_or_default(),
                plugins: exposing.map(|e| plugins(e, text, file)).unwrap_or_default(),
            }),
            _ => {}
        }
    }
    let bodiless = bodiless(tree, text, &imports);
    let mut inline_imports = Vec::new();
    if let Some(header) = &header {
        fn signature_imports(node: &Node, text: &str, file: FileId, imports: &mut Vec<Import>) {
            if node.kind == K::NameType {
                let parts: Vec<_> = node
                    .child_tokens()
                    .filter(|t| t.kind == K::Con)
                    .map(|t| t.text(text))
                    .collect();
                if parts.len() > 1 {
                    let module = parts[..parts.len() - 1].join(".");
                    if !imports
                        .iter()
                        .any(|i| i.module == module || i.qualifier() == module)
                    {
                        imports.push(Import {
                            module,
                            alias: None,
                            exposing: Vec::new(),
                            plugins: Vec::new(),
                            span: span(node, file),
                        });
                    }
                }
            }
            if node.kind != K::ModuleTypeEquality {
                for child in node.nodes() {
                    signature_imports(child, text, file, imports);
                }
            }
        }
        fn expression_imports(
            node: &Node,
            text: &str,
            file: FileId,
            bound: &[String],
            imports: &mut Vec<Import>,
        ) {
            if matches!(node.kind, K::ModuleNominalMember | K::ModuleTypeEquality) {
                return;
            }
            let mut bound = bound.to_vec();
            if node.kind == K::ModuleFunctorExpr {
                for parameter in node
                    .nodes()
                    .filter(|n| n.kind == K::ModuleParamGroup)
                    .flat_map(Node::nodes)
                {
                    if let Some(name) = parameter.child_tokens().find(|t| t.kind == K::Con) {
                        bound.push(name.text(text).to_owned());
                    }
                    for signature in parameter.nodes() {
                        signature_imports(signature, text, file, imports);
                    }
                }
            }
            if node.kind == K::ModuleFunctorExpr
                && let Some(result) = node.nodes().find(|n| n.kind == K::ModuleResult)
            {
                signature_imports(result, text, file, imports);
            }
            if node.kind == K::ModuleAnnotExpr
                && let Some(signature) = node.nodes().last()
            {
                signature_imports(signature, text, file, imports);
            }
            if node.kind == K::ModulePath {
                let name: String = node.text(text).split_whitespace().collect();
                let root = name.split('.').next().unwrap_or(&name);
                if name.contains('.')
                    && !bound.iter().any(|n| n == root)
                    && !imports
                        .iter()
                        .any(|i| i.module == name || i.qualifier() == root)
                {
                    imports.push(Import {
                        module: name,
                        alias: None,
                        exposing: Vec::new(),
                        plugins: Vec::new(),
                        span: span(node, file),
                    });
                }
            }
            for child in node.nodes().filter(|n| n.kind != K::ModuleParamGroup) {
                expression_imports(child, text, file, &bound, imports);
            }
        }
        let bound: Vec<_> = header
            .parameters
            .iter()
            .map(|p| p.name.clone())
            .chain(applications.iter().map(|a| a.name.clone()))
            .collect();
        for application in &applications {
            expression_imports(&application.expression, text, file, &bound, &mut imports);
            if let Some(annotation) = &application.annotation {
                signature_imports(annotation, text, file, &mut imports);
            }
        }
        if let Some(body) = &header.body {
            expression_imports(body, text, file, &bound, &mut imports);
        }
        for parameter in &header.parameters {
            signature_imports(&parameter.signature, text, file, &mut imports);
        }
        if let Some(result) = &header.result {
            signature_imports(result, text, file, &mut imports);
        }
        for parameter in &header.parameters {
            imports.push(Import {
                module: format!("$module:{}", parameter.name),
                alias: Some(parameter.name.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: parameter.span,
            });
        }
        for application in &applications {
            imports.push(Import {
                module: format!("{}.{}", header.name, application.name),
                alias: Some(application.name.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: application.span,
            });
        }
    }
    for node in tree.nodes().filter(|n| n.kind == K::InlineModuleDecl) {
        collect_inline_imports(node, text, file, &mut inline_imports);
    }
    ModuleSyntax {
        header,
        imports,
        inline_imports,
        applications,
        bodiless,
    }
}

include!("syntax_helpers.rs");
