//! The module header and imports of a parsed file.

use std::sync::Arc;

use crate::plugin::blocks_under;
use crate::plugin::first_party::{BARE, DECLARATIONS, HEADS};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

#[derive(Debug, Clone)]
pub struct ModuleSyntax {
    pub header: Option<Header>,
    pub imports: Vec<Import>,
    /// The heads of std's plugin its imports expose that a block of it
    /// writes as only an import lets it, without `{`: `cli Main`.
    pub bodiless: Vec<String>,
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
    let bodiless = bodiless(tree, text, &imports);
    ModuleSyntax {
        header,
        imports,
        bodiless,
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
