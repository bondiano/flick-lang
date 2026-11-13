//! `@derive` before a class: the plugin of a package derives its instances
//! for the types that name it in `deriving`, as a block's plugin gives the
//! block's code.

mod declared;
mod deriving;

pub use declared::{Declared, declared};
use deriving::Deriving;

use flick_plugin::{Block, CaseTarget, DeriveTarget, Span as BlockSpan};

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use super::expansion::{Expanded, Expansion};
use super::render::render;
use super::source::BlockSource;
use super::written::{user_type, written};
use super::{EditorBlock, Expander, Failure, ROUNDS, blocks_under, first_party, placed};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::ModuleSyntax;
use crate::source::{FileId, Span};
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;
use crate::types::{Thresholds, builtin_sources, is_type};

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
    let ty = |n: &Node| written(n, text, start);
    let named = |name: &str| match qualifier {
        Some(q) => format!("{q}.{name}"),
        None => name.to_owned(),
    };
    let name = decl.child_tokens().find(|t| t.kind == K::Con)?.text(text);
    let (params, kinds) = (decl.nodes())
        .filter(|n| n.kind == K::TypeParam)
        .filter_map(|p| {
            let name = p.child_tokens().find(|t| t.kind == K::Ident)?.text(text);
            // Canonical token spacing, independent of how the kind was written.
            let kind = p.nodes().next().map_or_else(String::new, |k| {
                k.tokens()
                    .iter()
                    .filter(|t| !t.kind.is_trivia())
                    .map(|t| t.text(text))
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            Some((name.to_owned(), kind))
        })
        .unzip();
    // the words before its name: `type`, `alias`, `opaque`, `unboxed`, `error`
    let modifiers = (decl.child_tokens())
        .take_while(|t| t.kind != K::Con)
        .map(|t| t.text(text).to_owned())
        .collect();
    let deriving = (decl.nodes())
        .filter(|n| n.kind == K::DerivingClause)
        .flat_map(|clause| named_classes(clause, text))
        .map(|n| {
            if n.qualifier.is_empty() {
                n.name
            } else {
                format!("{}.{}", n.qualifier, n.name)
            }
        })
        .collect();
    let cases = (decl.nodes())
        .filter(|n| n.kind == K::Variant)
        .filter_map(|v| {
            let con = v.child_tokens().find(|t| t.kind == K::Con)?.text(text);
            let fields = v.nodes().filter(|n| is_type(n.kind)).map(ty).collect();
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
    Some(DeriveTarget {
        decl: block,
        name: named(name),
        params,
        kinds,
        cases,
        alias: body.filter(|_| alias).map(ty),
        wraps: None,
        modifiers,
        deriving,
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
    named_classes(node, text)
}

fn named_classes<'n>(node: &'n Node, text: &str) -> Vec<Named<'n>> {
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
/// after the type's declaration, and for each of its blocks, as `cli` and
/// `server`; `None` without one.
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
    let mut summary: Vec<DeriveTarget> = (declared.iter())
        .filter_map(|d| {
            let source =
                BlockSource::declaration(Arc::clone(&shared), file, d, Arc::clone(&heads), true);
            target(d, text, source.block(), None)
        })
        .collect();
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    // the blocks first: the types their code declares are the module's too
    let typed = std_blocks(
        &shared,
        file,
        tree,
        module,
        syntax,
        &mut blocks,
        &mut diagnostics,
    );
    for declared in typed.iter().flat_map(|(_, _, types)| types) {
        if let Ok(target) = &declared.target {
            summary.push(target.clone());
        }
    }
    let deriving = Deriving {
        module,
        syntax,
        summary: &summary,
    };
    for decl in declared {
        let Some(clause) = decl.nodes().find(|c| c.kind == K::DerivingClause) else {
            continue;
        };
        for class in named(clause, text) {
            let source =
                BlockSource::declaration(Arc::clone(&shared), file, decl, Arc::clone(&heads), true);
            let Some(target) = target(decl, text, source.block(), None) else {
                continue;
            };
            let of = (class.qualifier.as_str(), class.name.as_str());
            let made = deriving.run(
                decl,
                &target,
                source,
                of,
                class.span,
                true,
                &mut diagnostics,
            );
            blocks.extend(made);
        }
    }
    // the classes of the types the blocks declared: the block is the
    // declaration each derive reads
    for (node, whole, types) in &typed {
        for (declared, class) in (types.iter()).flat_map(|d| d.classes.iter().map(move |c| (d, c)))
        {
            let source = BlockSource::new(Arc::clone(&shared), file, node, Arc::clone(&heads))
                .map(|s| s.in_module(module, tree));
            let (Ok(target), Some(source)) = (&declared.target, source) else {
                continue;
            };
            let of = class.rsplit_once('.').unwrap_or(("", class));
            let made = deriving.run(node, target, source, of, *whole, false, &mut diagnostics);
            blocks.extend(made);
        }
    }
    if blocks.is_empty() {
        return (None, diagnostics);
    }
    blocks.sort_by_key(|b| b.node.range());
    checked_expansion(text, file, tree, blocks, diagnostics)
}

/// A block, what it stands for in the module, and the types its code
/// declares.
type Typed = (Node, (u32, u32), Vec<Declared>);

fn checked_expansion(
    text: &str,
    file: FileId,
    tree: &Node,
    blocks: Vec<Expanded>,
    mut diagnostics: Vec<Diagnostic>,
) -> (Option<Expansion>, Vec<Diagnostic>) {
    let editing = source_checks(&blocks);
    let (expansion, failed) = Expansion::build(text, file, tree, blocks);
    diagnostics.extend(failed);
    if let Some(expansion) = &expansion {
        diagnostics.extend(expansion.check_blocks(&editing, &[], Thresholds::default()));
    }
    (expansion, diagnostics)
}

/// Standalone checks also run source hooks, which need no solved wildcards.
fn source_checks(blocks: &[Expanded]) -> Vec<EditorBlock> {
    blocks
        .iter()
        .filter_map(|b| {
            let plugin = first_party::plugin(&b.plugin)?;
            Some(EditorBlock::new(&b.source, &[], &plugin))
        })
        .collect()
}

/// The code of each block of std's plugin in `tree`, as the loader expands
/// it, into `blocks`: the blocks of the module, then those in the user's
/// code of the ones the round before expanded, at most [`ROUNDS`] deep. One
/// not closed, or nested deeper, is `P036`. Gives each block whose code
/// declares types, with those.
fn std_blocks(
    shared: &Arc<str>,
    file: FileId,
    tree: &Node,
    module: &str,
    syntax: &ModuleSyntax,
    blocks: &mut Vec<Expanded>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Typed> {
    let mut typed = Vec::new();
    let Some(plugin) = first_party::plugin(first_party::PACKAGE) else {
        return typed;
    };
    let (heads, aliases) = (&syntax.heads(), syntax.aliases());
    let own = |n: &Node| {
        let head = super::head_token(n);
        head.is_some_and(|h| {
            first_party::HEADS
                .iter()
                .any(|(head, _)| *head == h.text(shared))
        })
    };
    let unexpanded = |n: &Node, why: &str| {
        let (start, end) = n.range();
        Diagnostic::error(codes::P036, why, Span::new(file, start, end))
    };
    // the closed blocks of std's plugin among `nodes`, each read by `source`
    let take = |nodes: Vec<&Node>,
                source: &dyn Fn(&Node) -> Option<BlockSource>,
                diagnostics: &mut Vec<Diagnostic>| {
        let mut round = Vec::new();
        for n in nodes.into_iter().filter(|n| own(n)) {
            match source(n) {
                Some(s) if s.closed() => round.push((n.clone(), s)),
                Some(_) => diagnostics.push(unexpanded(n, "this block is not closed")),
                None => {}
            }
        }
        round
    };
    let mut nodes = Vec::new();
    blocks_under(tree, &mut nodes);
    let top = |n: &Node| {
        let source = BlockSource::new(Arc::clone(shared), file, n, Arc::clone(heads))?;
        Some(source.in_module(module, tree))
    };
    let mut round = take(nodes, &top, diagnostics);
    for _ in 0..=ROUNDS {
        if round.is_empty() {
            return typed;
        }
        let expanded = std_round(plugin.as_ref(), module, round, &aliases, diagnostics);
        let mut next = Vec::new();
        for (block, types) in expanded {
            if !types.is_empty() {
                typed.push((block.node.clone(), block.whole, types));
            }
            let trees: Vec<Node> = (block.rendered.holes.iter())
                .filter_map(|&(_, hole)| block.source.hole_tree(hole))
                .collect();
            let mut nested = Vec::new();
            for tree in &trees {
                if matches!(tree.kind, K::PluginExpr | K::PluginDecl) {
                    nested.push(tree);
                } else {
                    blocks_under(tree, &mut nested);
                }
            }
            next.extend(take(nested, &|n| block.source.nested(n), diagnostics));
            blocks.push(block);
        }
        round = next;
    }
    let deep = format!("this block is nested in more than {ROUNDS} blocks");
    diagnostics.extend(round.iter().map(|(n, _)| unexpanded(n, &deep)));
    typed
}

/// The code std's plugin gives for each block of `round`, collected as one
/// group; a block it gives none is `P036`.
fn std_round(
    plugin: &dyn Expander,
    module: &str,
    round: Vec<(Node, BlockSource)>,
    aliases: &[(String, String)],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(Expanded, Vec<Declared>)> {
    let file = round[0].1.file();
    let failed = |why: String, node: &Node| {
        let (start, end) = node.range();
        Diagnostic::error(codes::P036, why, Span::new(file, start, end))
    };
    let read: Vec<Block> = round.iter().map(|(_, s)| s.block()).collect();
    let state = match plugin.collect(&read, &mut BlockSource::default()) {
        Ok(state) => state,
        Err(why) => {
            diagnostics.push(failed(why, &round[0].0));
            return Vec::new();
        }
    };
    // what the plugin publishes of the module's own names, as the loader
    // gives every block of a group; one that fails to publish expands none
    let published = match plugin.publish(&state) {
        Ok(published) => published,
        Err(why) => {
            diagnostics.push(failed(why, &round[0].0));
            return Vec::new();
        }
    };
    let published: BTreeMap<String, Vec<u8>> = (published.into_iter())
        .filter(|p| p.module == module)
        .map(|p| (p.name, p.data))
        .collect();
    let published = Arc::new(published);
    let mut out = Vec::new();
    for ((node, mut source), block) in round.into_iter().zip(&read) {
        let (start, end) = node.range();
        let whole = Span::new(file, start, end);
        source.see_published(Arc::clone(&published));
        let made = plugin.expand(block, &state, &mut source);
        diagnostics.append(&mut source.diagnostics);
        let Some(arena) = made_or(made, first_party::PACKAGE, &source, whole, diagnostics) else {
            continue;
        };
        let types = declared(&arena, block, &|span| user_type(&source, span));
        match render(&arena, block.whole().end, source.place(), aliases) {
            Ok(rendered) => out.push((
                Expanded {
                    node,
                    rendered,
                    source,
                    plugin: first_party::PACKAGE.into(),
                    keep: false,
                    whole: (start, end),
                },
                types,
            )),
            Err(why) => diagnostics.push(Diagnostic::error(codes::P036, why, whole)),
        }
    }
    out
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
