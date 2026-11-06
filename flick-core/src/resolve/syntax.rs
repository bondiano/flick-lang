//! The module header and imports of a parsed file.

use std::sync::Arc;

use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug, Clone)]
pub struct ModuleSyntax {
    pub header: Option<Header>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub name: String,
    /// Span of the module path.
    pub span: Span,
    /// `None` without `exposing`: the module exports nothing.
    pub exposing: Option<Vec<Item>>,
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

pub fn module_syntax(tree: &Node, text: &str, file: FileId) -> ModuleSyntax {
    let mut header = None;
    let mut imports = Vec::new();
    for node in tree.nodes() {
        let Some(path) = node.nodes().find(|n| n.kind == K::ModulePath) else {
            continue;
        };
        let name = path.text(text).split_whitespace().collect();
        let span = span(path, file);
        let exposing = node.nodes().find(|n| n.kind == K::Exposing);
        match node.kind {
            K::ModuleHeader if header.is_none() => {
                header = Some(Header {
                    name,
                    span,
                    exposing: exposing.map(|e| items(e, text, file)),
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
    ModuleSyntax { header, imports }
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
            // `sql` is a keyword still, of the queries the compiler reads
            let head = tokens
                .next()
                .filter(|t| matches!(t.kind, K::Ident | K::SqlKw))?;
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
