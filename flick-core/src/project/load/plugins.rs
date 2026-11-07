//! The plugins the loaded modules import and the blocks they expand. Each
//! plugin learns from all its blocks of a group, the project's sources, its
//! tests or a package, then gives the code of each block; the module is
//! checked with that code in place of its blocks.

use std::collections::{BTreeMap, HashMap};
use std::mem;
use std::path::Path;
use std::sync::Arc;

use flick_plugin::Block;

mod derive;
mod enums;
mod groups;
mod published;
mod results;

use published::Publications;
use results::Results;

use super::unimported::unimported;
use super::{Loaded, Loader, Root};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::expansion::{Expanded, Expansion};
use crate::plugin::reach::{Gate, Reach};
use crate::plugin::render::{Rendered, render};
use crate::plugin::source::BlockSource;
use crate::plugin::{
    EditorBlock, Expander, Failure, blocks_under, first_party, head_token, placed,
};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types;

/// How many blocks a block may be nested in, in the user's code of others.
const ROUNDS: usize = 8;

/// The plugin of each package a module imports, or why it does not load.
type Plugins = HashMap<String, Result<Arc<dyn Expander>, String>>;

/// A group collected: its plugin's package, its members, their blocks,
/// its state and its gate.
type Collected = (String, Vec<usize>, Vec<Block>, Vec<u8>, Option<Arc<Gate>>);

/// A block of a loaded module and the package of its plugin.
struct Found {
    module: usize,
    node: Node,
    source: BlockSource,
    package: String,
    /// A type declaration whose derived code goes after it.
    keep: bool,
    /// What the whole block stands for in the module: the block, or the
    /// class a `deriving` names.
    whole: Span,
}

impl Loader<'_> {
    /// Expands the plugin blocks of every loaded module, in rounds: the
    /// blocks of the modules first, then those inside the user's code of
    /// the blocks the round before expanded, at most [`ROUNDS`] deep.
    pub(super) fn expand_plugins(&mut self) {
        let mut plugins: Plugins = HashMap::new();
        let mut diagnostics = Vec::new();
        let mut round = Vec::new();
        let mut editing = Vec::new();
        let mut heads = Vec::new();
        for (i, m) in self.modules.iter().enumerate() {
            heads.push(self.heads(m, &mut plugins, &mut diagnostics));
            let mut blocks = Vec::new();
            blocks_under(&m.parse.tree, &mut blocks);
            if blocks.is_empty() {
                continue;
            }
            let new = self.block_sources(m);
            let mut sink = Sink {
                plugins: &plugins,
                heads: &heads[i],
                found: &mut round,
                editing: &mut editing,
            };
            for node in blocks {
                sink.take(self, i, node, new(node), &mut diagnostics);
            }
        }
        let previous = self.cache.as_mut().map(|c| mem::take(&mut c.plugins));
        let mut results = Results {
            host: self.host,
            previous: previous.unwrap_or_default(),
            kept: HashMap::new(),
        };
        let mut expanded = Vec::new();
        let mut publications = Publications::new();
        // the blocks of the modules, then those nested in 1 to ROUNDS blocks
        for _ in 0..=ROUNDS {
            if round.is_empty() {
                break;
            }
            let rendered = self.expand_found(
                &plugins,
                &mut results,
                &mut publications,
                &mut round,
                &mut diagnostics,
                &mut editing,
            );
            let mut next = Vec::new();
            for (f, rendered) in round.iter().zip(&rendered) {
                let Some(rendered) = rendered else {
                    continue;
                };
                // the user's code of the block, where blocks of its own are
                let trees: Vec<Node> = (rendered.holes.iter())
                    .filter_map(|&(_, hole)| f.source.hole_tree(hole))
                    .collect();
                let mut blocks = Vec::new();
                for tree in &trees {
                    if matches!(tree.kind, K::PluginExpr | K::PluginDecl) {
                        blocks.push(tree);
                    } else {
                        blocks_under(tree, &mut blocks);
                    }
                }
                let mut sink = Sink {
                    plugins: &plugins,
                    heads: &heads[f.module],
                    found: &mut next,
                    editing: &mut editing,
                };
                for node in blocks {
                    sink.take(
                        self,
                        f.module,
                        node,
                        f.source.nested(node),
                        &mut diagnostics,
                    );
                }
            }
            expanded.extend(mem::take(&mut round).into_iter().zip(rendered));
            round = next;
        }
        expanded.extend(self.derive_blocks(&mut plugins, &mut results, &mut diagnostics));
        diagnostics.extend(round.iter().map(|f| {
            failure(
                f,
                &f.package,
                &format!("is nested in more than {ROUNDS} blocks"),
            )
        }));
        if let Some(cache) = &mut self.cache {
            cache.plugins = results.kept;
        }
        for (i, block) in editing {
            self.modules[i].editing.push(block);
        }
        self.place_expanded(expanded, &mut diagnostics);
        self.diagnostics.extend(diagnostics);
    }

    /// The source of each block of `m`: its text, its module and what that
    /// module imports and sees.
    fn block_sources<'m>(&self, m: &'m Loaded) -> impl Fn(&Node) -> Option<BlockSource> + 'm {
        let shared: Arc<str> = Arc::from(self.sources.get(m.file).text.as_str());
        let imported = m.syntax.heads();
        let enums = Arc::new(self.enums_seen(m));
        move |node: &Node| {
            let source = BlockSource::new(Arc::clone(&shared), m.file, node, Arc::clone(&imported));
            source.map(|s| {
                s.in_module(&m.name, &m.parse.tree)
                    .with_enums(Arc::clone(&enums))
            })
        }
    }

    /// The expansion of each module with blocks `expanded` gave code for.
    fn place_expanded(
        &mut self,
        expanded: Vec<(Found, Option<Rendered>)>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut by_module: BTreeMap<usize, Vec<Expanded>> = BTreeMap::new();
        for (f, rendered) in expanded {
            if let Some(rendered) = rendered {
                by_module.entry(f.module).or_default().push(Expanded {
                    node: f.node,
                    rendered,
                    source: f.source,
                    plugin: f.package,
                    keep: f.keep,
                    whole: (f.whole.start, f.whole.end),
                });
            }
        }
        for (i, mut blocks) in by_module {
            blocks.sort_by_key(|b| b.node.range());
            let m = &self.modules[i];
            let text = &self.sources.get(m.file).text;
            let (expansion, failed) = Expansion::build(text, m.file, &m.parse.tree, blocks);
            diagnostics.extend(failed);
            self.modules[i].expansion = expansion.map(Arc::new);
        }
    }

    /// The heads the imports of `m` expose, each with the package of its
    /// plugin, loading each plugin once; why one does not load goes to
    /// `diagnostics`.
    fn heads(
        &self,
        m: &Loaded,
        plugins: &mut Plugins,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> HashMap<String, String> {
        let mut heads: HashMap<String, String> = HashMap::new();
        for import in m.syntax.imports.iter().filter(|i| !i.plugins.is_empty()) {
            let module = &import.module;
            let refuse = |why: String| {
                (import.plugins.iter())
                    .map(move |item| Diagnostic::error(codes::P035, why.clone(), item.span))
            };
            // a module of a package may come from the package cache
            let saved = self.precompiled.package_of(module);
            let root = self.by_name.get(module).map(|&j| &self.modules[j].root);
            let implicit = types::implicit_package_of(module);
            let package = match (root, saved) {
                (Some(Root::Package(name)), _) => name.clone(),
                (None, Some(name)) => name.to_owned(),
                (None, None) if implicit.is_some() => implicit.unwrap_or_default().to_owned(),
                (Some(_), _) => {
                    diagnostics.extend(refuse(format!(
                        "`{module}` is a module of this project: only a package provides a plugin"
                    )));
                    continue;
                }
                (None, None) if self.builtins().iter().any(|b| b.module == *module) => {
                    diagnostics.extend(refuse(format!(
                        "`{module}` is a built-in module, which provides no plugin"
                    )));
                    continue;
                }
                // the module is reported as unknown
                (None, None) => continue,
            };
            let plugin =
                plugins.entry(package.clone()).or_insert_with(|| {
                    match first_party::plugin(&package) {
                        Some(plugin) => Ok(plugin),
                        None => self.host.plugin(&package, self.component(&package)),
                    }
                });
            if let Err(why) = plugin {
                diagnostics.extend(refuse(format!(
                    "the plugin of package `{package}` cannot be loaded: {why}"
                )));
                continue;
            }
            for item in &import.plugins {
                heads.insert(item.name.clone(), package.clone());
            }
        }
        heads
    }

    /// The code each block of `found` stands for, from its plugin: each
    /// plugin collects from its blocks of a group, in the order of their
    /// modules' names and places, and publishes what it learned; then it
    /// expands each block. A call whose result this or an earlier run
    /// already has is not made again.
    #[allow(clippy::too_many_arguments)]
    fn expand_found(
        &self,
        plugins: &Plugins,
        results: &mut Results,
        publications: &mut Publications,
        found: &mut [Found],
        diagnostics: &mut Vec<Diagnostic>,
        editing: &mut Vec<(usize, EditorBlock)>,
    ) -> Vec<Option<Rendered>> {
        let mut rendered: Vec<Option<Rendered>> = found.iter().map(|_| None).collect();
        let states = self.collect_groups(plugins, results, publications, found, diagnostics);
        for (package, members, blocks, state, gate) in states {
            let Some(Ok(plugin)) = plugins.get(&package) else {
                continue;
            };
            let theirs = publications.get(&package);
            let mut seen = HashMap::new();
            for &j in &members {
                let f = &mut found[j];
                if let Some(theirs) = theirs {
                    let module = &self.modules[f.module];
                    let view = seen
                        .entry(f.module)
                        .or_insert_with(|| Arc::new(published::seen(module, theirs)));
                    f.source.see_published(Arc::clone(view));
                }
                let editor = EditorBlock::new(&f.source, &state, plugin);
                editing.push((f.module, editor));
            }
            for (&j, block) in members.iter().zip(&blocks) {
                let f = &mut found[j];
                f.source.reach = Reach::new(gate.clone());
                let expanded = results.expand(plugin.as_ref(), block, &state, &mut f.source);
                // what the host refused, whatever the plugin made of it
                for (code, message) in f.source.reach.problems.drain(..) {
                    diagnostics.push(Diagnostic::error(code, message, f.whole));
                }
                match expanded {
                    Ok(arena) => {
                        let len = u32::try_from(block.text.len()).expect("source fits in u32");
                        let aliases = self.modules[f.module].syntax.aliases();
                        match render(&arena, len, block.place, &aliases) {
                            Ok(code) => rendered[j] = Some(code),
                            Err(why) => diagnostics.push(failure(
                                f,
                                &package,
                                &format!("gave code that is not valid: {why}"),
                            )),
                        }
                    }
                    Err(Failure::Refused(refused)) => {
                        if refused.is_empty() && f.source.diagnostics.is_empty() {
                            diagnostics.push(failure(f, &package, "gave no code for this block"));
                        }
                        // the errors the compiler found in its holes say why
                        let parsed = f.source.diagnostics.is_empty();
                        diagnostics.extend(
                            (refused.iter())
                                .filter(|d| parsed || d.code.is_some())
                                .map(|d| refusal(f, &package, d)),
                        );
                    }
                    Err(Failure::Broken(why)) => {
                        diagnostics.push(failure(f, &package, &format!("failed: {why}")));
                    }
                }
                diagnostics.append(&mut f.source.diagnostics);
            }
        }
        rendered
    }
}

/// Where the blocks of a round go: those to expand, and those only editors
/// see while they are not closed.
struct Sink<'a> {
    plugins: &'a Plugins,
    /// The heads of the module the blocks are in, by the package of each.
    heads: &'a HashMap<String, String>,
    found: &'a mut Vec<Found>,
    editing: &'a mut Vec<(usize, EditorBlock)>,
}

impl Sink<'_> {
    /// The block `node` of the module `i`, read as `source`; a head of the
    /// compiler's own plugins the module does not import is `FLICK-P037`.
    fn take(
        &mut self,
        loader: &Loader,
        i: usize,
        node: &Node,
        source: Option<BlockSource>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let m = &loader.modules[i];
        let text = &loader.sources.get(m.file).text;
        let Some(token) = head_token(node) else {
            return;
        };
        let head = token.text(text);
        let Some(package) = self.heads.get(head) else {
            if let Some((_, module)) = first_party::HEADS.iter().find(|(h, _)| *h == head) {
                let span = (token.start, token.end);
                diagnostics.push(unimported(m.file, &m.parse.tree, span, head, module));
            }
            return;
        };
        let Some(source) = source else {
            return;
        };
        if source.closed() {
            let (start, end) = node.range();
            self.found.push(Found {
                module: i,
                node: node.clone(),
                whole: Span::new(source.file(), start, end),
                source,
                package: package.clone(),
                keep: false,
            });
        } else if let Some(Ok(plugin)) = self.plugins.get(package) {
            self.editing
                .push((i, EditorBlock::new(&source, &[], plugin)));
        }
    }
}

/// The group a module's blocks are collected with.
fn group(root: &Root) -> String {
    match root {
        Root::Source => "source".into(),
        Root::Test => "test".into(),
        Root::Package(name) => format!("package {name}"),
        Root::Other(_) => "other".into(),
    }
}

/// `P036` on the block of `found`: its plugin `package` `what`.
fn failure(found: &Found, package: &str, what: &str) -> Diagnostic {
    Diagnostic::error(
        codes::P036,
        format!("plugin `{package}` {what}"),
        found.whole,
    )
}

/// What the plugin `package` found wrong at a span of the block of `found`,
/// the whole of its frame meaning the whole block: under the plugin's own
/// code, or `P036` without one.
fn refusal(found: &Found, package: &str, d: &flick_plugin::Diagnostic) -> Diagnostic {
    let source = &found.source;
    let frame = Span::new(source.file(), source.start(), source.end());
    // a declaration of another module has no place here but the block
    placed(d, package, frame, found.whole, source.foreign())
}

impl Loader<'_> {
    /// The component of the plugin of the dependency `package`.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    fn component(&self, package: &str) -> Option<&Path> {
        #[cfg(feature = "native-project")]
        return self.packages.plugin(package);
        #[cfg(not(feature = "native-project"))]
        {
            let _ = package;
            None
        }
    }
}
