//! `@derive` before a class: the plugin of a package derives its instances
//! for the types that name it in `deriving`, as a block's plugin gives the
//! block's code.

use flick_plugin::{Block, CaseTarget, DeriveTarget, Span as BlockSpan};

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use flick_plugin::Place;

use super::expansion::{Expanded, Expansion};
use super::render::render;
use super::source::BlockSource;
use super::{Failure, blocks_under, first_party, placed};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::ModuleSyntax;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;
use crate::types::{builtin_sources, is_std_module, is_type};

/// A class a module declares with `@derive`, or a type such as `Schema`
/// whose companions a plugin derives, and the package its attribute names,
/// `None` for the class's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub class: String,
    pub package: Option<String>,
}

/// The classes the built-in module `module` marks `@derive`, read once.
pub fn builtin_derived(module: &str) -> &'static [Derived] {
    static ALL: OnceLock<HashMap<&'static str, Vec<Derived>>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        (builtin_sources().iter())
            .filter(|(_, text)| text.contains("@derive"))
            .map(|(name, text)| {
                (
                    *name,
                    derived_classes(&parse(text, FileId::default()).tree, text),
                )
            })
            .collect()
    });
    all.get(module).map_or(&[], Vec::as_slice)
}

/// The classes of `tree` marked `@derive`, in order.
pub fn derived_classes(tree: &Node, text: &str) -> Vec<Derived> {
    let mut out = Vec::new();
    let mut pending: Option<Option<String>> = None;
    for child in tree.nodes() {
        if child.kind == K::Attribute {
            let words: Vec<&str> = (child.child_tokens())
                .filter(|t| t.kind == K::Ident)
                .map(|t| t.text(text))
                .collect();
            pending = match words[..] {
                ["derive"] => Some(None),
                ["derive", package] => Some(Some(package.to_owned())),
                _ => pending,
            };
            continue;
        }
        if let Some(package) = pending.take()
            && matches!(child.kind, K::ClassDecl | K::TypeDecl)
            && let Some(class) = child.child_tokens().find(|t| t.kind == K::Con)
        {
            out.push(Derived {
                class: class.text(text).to_owned(),
                package,
            });
        }
    }
    out
}

/// What `decl`, a `TypeDecl` read as `block`, declares, for a derive: its
/// spans count from its start. `qualifier` names the module of a type of
/// another module, which qualifies its names and makes no holes.
pub fn target(
    decl: &Node,
    text: &str,
    block: Block,
    qualifier: Option<&str>,
) -> Option<DeriveTarget> {
    let start = decl.range().0;
    let span = |n: &Node| {
        let (s, e) = n.range();
        BlockSpan {
            start: s - start,
            end: e - start,
        }
    };
    let named = |name: &str| match qualifier {
        Some(q) => format!("{q}.{name}"),
        None => name.to_owned(),
    };
    let name = decl.child_tokens().find(|t| t.kind == K::Con)?.text(text);
    let params = (decl.nodes())
        .filter(|n| n.kind == K::TypeParam)
        .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Ident))
        .map(|t| t.text(text).to_owned())
        .collect();
    let cases = (decl.nodes())
        .filter(|n| n.kind == K::Variant)
        .filter_map(|v| {
            let con = v.child_tokens().find(|t| t.kind == K::Con)?.text(text);
            let fields = v.nodes().filter(|n| is_type(n.kind)).map(span).collect();
            Some(CaseTarget {
                name: named(con),
                fields,
                literal: v.nodes().find(|n| n.kind == K::LitPat).map(span),
                refined: v.nodes().any(|n| n.kind == K::VariantResult),
                constrained: v.nodes().any(|n| n.kind == K::Context),
            })
        })
        .collect();
    let body = decl
        .nodes()
        .find(|n| is_type(n.kind) && n.kind != K::TypeParam);
    let alias = decl
        .child_tokens()
        .any(|t| t.kind == K::Ident && t.text(text) == "alias");
    let refined = decl.has_token(K::RefinedKw);
    Some(DeriveTarget {
        decl: block,
        name: named(name),
        params,
        cases,
        alias: body.filter(|_| alias).map(span),
        refined: body.filter(|_| refined).map(span),
        local: qualifier.is_none(),
    })
}

/// A class a `deriving` names, its arguments and where it is written.
pub struct Named<'n> {
    pub qualifier: String,
    pub name: String,
    pub args: Option<&'n Node>,
    pub span: (u32, u32),
}

/// The classes `node`, a `deriving` clause or a `deriving instance`, names.
pub fn named<'n>(node: &'n Node, text: &str) -> Vec<Named<'n>> {
    let mut out = Vec::new();
    let mut path: Vec<Token> = Vec::new();
    let mut finish = |path: &mut Vec<Token>, args: Option<&'n Node>| {
        let parts: Vec<&str> = (path.iter())
            .filter(|t| t.kind != K::Dot)
            .map(|t| t.text(text))
            .collect();
        if let (Some((name, qualifier)), Some(first), Some(last)) =
            (parts.split_last(), path.first(), path.last())
        {
            out.push(Named {
                qualifier: qualifier.join("."),
                name: (*name).to_owned(),
                args,
                span: (first.start, last.end),
            });
        }
        path.clear();
    };
    for element in &node.children {
        match element {
            Element::Token(t) if matches!(t.kind, K::Con | K::Dot) => path.push(*t),
            Element::Token(t) if t.kind == K::Comma => finish(&mut path, None),
            Element::Node(n) if n.kind == K::DerivingArgs => {
                finish(&mut path, Some(n));
            }
            // the type of `deriving instance` ends the class
            Element::Node(n) if is_type(n.kind) => break,
            _ => {}
        }
    }
    finish(&mut path, None);
    out
}

/// The expansion of a built-in module `module`'s derives and blocks: the
/// code std's plugin gives for each type that names a class it derives,
/// after the type's declaration, and for each of its blocks the module
/// imports, as `cli` and `server`; `None` without one.
pub fn builtin_expansion(
    text: &str,
    file: FileId,
    tree: &Node,
    module: &str,
    syntax: &ModuleSyntax,
) -> (Option<Expansion>, Vec<Diagnostic>) {
    let shared: Arc<str> = Arc::from(text);
    let heads = syntax.heads();
    let declared: Vec<&Node> = (tree.nodes())
        .filter(|n| matches!(n.kind, K::TypeDecl | K::ErrorDecl))
        .collect();
    let summary: Vec<DeriveTarget> = (declared.iter())
        .filter_map(|d| {
            let source =
                BlockSource::declaration(Arc::clone(&shared), file, d, Arc::clone(&heads), true);
            target(d, text, source.block(), None)
        })
        .collect();
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    for decl in declared {
        let Some(clause) = decl.nodes().find(|c| c.kind == K::DerivingClause) else {
            continue;
        };
        for class in named(clause, text) {
            // `@derive` alone: the plugin of the class's package
            let marked = |m: &str| {
                (builtin_derived(m).iter())
                    .find(|d| d.class == class.name)
                    .map(|d| {
                        let own = is_std_module(m).then(|| "std".to_owned());
                        (m.to_owned(), d.package.clone().or(own))
                    })
            };
            // as the loader finds it: the module, an import exposing it, the Prelude
            let found = if class.qualifier.is_empty() {
                marked(module)
                    .or_else(|| {
                        (syntax.imports.iter())
                            .filter(|i| {
                                (i.exposing.iter())
                                    .any(|e| e.name == class.name || e.name.is_empty())
                            })
                            .find_map(|i| marked(&i.module))
                    })
                    .or_else(|| marked("Prelude"))
            } else {
                let import = syntax
                    .imports
                    .iter()
                    .find(|i| i.qualifier() == class.qualifier);
                marked(import.map_or(class.qualifier.as_str(), |i| i.module.as_str()))
            };
            let Some((owner, Some(package))) = found else {
                continue;
            };
            let Some(plugin) = first_party::plugin(&package) else {
                continue;
            };
            let mut source =
                BlockSource::declaration(Arc::clone(&shared), file, decl, Arc::clone(&heads), true);
            let Some(target) = target(decl, text, source.block(), None) else {
                continue;
            };
            let full = format!("{owner}.{}", class.name);
            let whole = Span::new(file, class.span.0, class.span.1);
            let made = plugin.derive(&target, &full, None, &summary, &[], &mut source);
            let Some(arena) = made_or(made, &package, &source, whole, &mut diagnostics) else {
                continue;
            };
            let len = source.end() - source.start();
            match render(&arena, len, Place::Declaration, &syntax.aliases()) {
                Ok(rendered) => blocks.push(Expanded {
                    node: decl.clone(),
                    rendered,
                    source,
                    plugin: package,
                    keep: true,
                    whole: class.span,
                }),
                Err(why) => diagnostics.push(Diagnostic::error(codes::P036, why, whole)),
            }
        }
    }
    std_blocks(
        &shared,
        file,
        tree,
        module,
        syntax,
        &mut blocks,
        &mut diagnostics,
    );
    if blocks.is_empty() {
        return (None, diagnostics);
    }
    blocks.sort_by_key(|b| b.node.range());
    let (expansion, failed) = Expansion::build(text, file, tree, blocks);
    diagnostics.extend(failed);
    (expansion, diagnostics)
}

/// The code of each block of std's plugin in `tree` whose head the module
/// imports, as the loader expands it, into `blocks`.
fn std_blocks(
    shared: &Arc<str>,
    file: FileId,
    tree: &Node,
    module: &str,
    syntax: &ModuleSyntax,
    blocks: &mut Vec<Expanded>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(plugin) = first_party::plugin("std") else {
        return;
    };
    let (heads, aliases) = (&syntax.heads(), syntax.aliases());
    let mut nodes = Vec::new();
    blocks_under(tree, &mut nodes);
    let own = |n: &&Node| {
        let head = super::head_token(n);
        head.is_some_and(|h| {
            let head = h.text(shared);
            heads.iter().any(|i| i == head) && first_party::HEADS.iter().any(|(h, _)| *h == head)
        })
    };
    let mut found: Vec<(Node, BlockSource)> = (nodes.into_iter())
        .filter(own)
        .filter_map(|n| {
            let source = BlockSource::new(Arc::clone(shared), file, n, Arc::clone(heads))?;
            Some((n.clone(), source.in_module(module, tree)))
        })
        .filter(|(_, s)| s.closed())
        .collect();
    if found.is_empty() {
        return;
    }
    let read: Vec<Block> = found.iter().map(|(_, s)| s.block()).collect();
    let state = match plugin.collect(&read, &mut BlockSource::default()) {
        Ok(state) => state,
        Err(why) => {
            let (start, end) = found[0].0.range();
            diagnostics.push(Diagnostic::error(
                codes::P036,
                why,
                Span::new(file, start, end),
            ));
            return;
        }
    };
    for ((node, mut source), block) in found.drain(..).zip(&read) {
        let (start, end) = node.range();
        let whole = Span::new(file, start, end);
        let made = plugin.expand(block, &state, &mut source);
        diagnostics.append(&mut source.diagnostics);
        let Some(arena) = made_or(made, "std", &source, whole, diagnostics) else {
            continue;
        };
        match render(&arena, block.whole().end, source.place(), &aliases) {
            Ok(rendered) => blocks.push(Expanded {
                node,
                rendered,
                source,
                plugin: "std".into(),
                keep: false,
                whole: (start, end),
            }),
            Err(why) => diagnostics.push(Diagnostic::error(codes::P036, why, whole)),
        }
    }
}

/// The code a plugin of `package` gave, or what it said, under its
/// package's codes, at its spans in `source` or the whole of it at `whole`,
/// as the loader gives it.
fn made_or<T>(
    made: Result<T, Failure>,
    package: &str,
    source: &BlockSource,
    whole: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<T> {
    match made {
        Ok(made) => Some(made),
        Err(Failure::Refused(found)) => {
            let frame = Span::new(source.file(), source.start(), source.end());
            diagnostics.extend(
                found
                    .iter()
                    .map(|d| placed(d, package, frame, whole, false)),
            );
            None
        }
        Err(Failure::Broken(why)) => {
            diagnostics.push(Diagnostic::error(codes::P036, why, whole));
            None
        }
    }
}
