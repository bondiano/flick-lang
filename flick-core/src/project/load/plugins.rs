//! The plugins the loaded modules import and the blocks they expand. Each
//! plugin learns from all its blocks of a group, the project's sources, its
//! tests or a package, then gives the code of each block; the module is
//! checked with that code in place of its blocks.

use std::collections::{BTreeMap, HashMap};
use std::mem;
use std::path::Path;
use std::sync::Arc;

use flick_plugin::{Arena, Block, VERSION};

use super::unimported::unimported;
use super::{Loaded, Loader, Root};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::expansion::{Expanded, Expansion};
use crate::plugin::render::{Rendered, render};
use crate::plugin::source::BlockSource;
use crate::plugin::{EditorBlock, Expander, Failure, blocks_under, first_party};
use crate::project::Host;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types;

/// The plugin of each package a module imports, or why it does not load.
type Plugins = HashMap<String, Result<Arc<dyn Expander>, String>>;

/// A block of a loaded module and the package of its plugin.
struct Found {
    module: usize,
    node: Node,
    source: BlockSource,
    package: String,
}

impl Loader<'_> {
    /// Expands the plugin blocks of every loaded module.
    pub(super) fn expand_plugins(&mut self) {
        let mut plugins: Plugins = HashMap::new();
        let mut diagnostics = Vec::new();
        let mut found = Vec::new();
        let mut editing = Vec::new();
        for (i, m) in self.modules.iter().enumerate() {
            let text = &self.sources.get(m.file).text;
            let heads = self.heads(m, &mut plugins, &mut diagnostics);
            let mut blocks = Vec::new();
            blocks_under(&m.parse.tree, &mut blocks);
            if blocks.is_empty() {
                continue;
            }
            let shared: Arc<str> = Arc::from(text.as_str());
            for node in blocks {
                let Some(token) = node.child_tokens().find(|t| t.kind == K::Ident) else {
                    continue;
                };
                let head = token.text(text);
                let Some(package) = heads.get(head) else {
                    // a head of the compiler's own plugins, not imported
                    if let Some((_, module)) = first_party::HEADS.iter().find(|(h, _)| *h == head) {
                        let span = (token.start, token.end);
                        diagnostics.push(unimported(m.file, &m.parse.tree, span, head, module));
                    }
                    continue;
                };
                let Some(source) = BlockSource::new(Arc::clone(&shared), m.file, node) else {
                    continue;
                };
                if source.closed() {
                    found.push(Found {
                        module: i,
                        node: node.clone(),
                        source,
                        package: package.clone(),
                    });
                } else if let Some(Ok(plugin)) = plugins.get(package) {
                    editing.push((i, EditorBlock::new(&source, &[], plugin)));
                }
            }
        }
        let rendered = self.expand_found(&plugins, &mut found, &mut diagnostics, &mut editing);
        for (i, block) in editing {
            self.modules[i].editing.push(block);
        }
        let mut by_module: BTreeMap<usize, Vec<Expanded>> = BTreeMap::new();
        for (f, rendered) in found.into_iter().zip(rendered) {
            if let Some(rendered) = rendered {
                by_module.entry(f.module).or_default().push(Expanded {
                    node: f.node,
                    rendered,
                    source: f.source,
                    plugin: f.package,
                });
            }
        }
        for (i, mut blocks) in by_module {
            blocks.sort_by_key(|b| b.node.range());
            let m = &self.modules[i];
            let text = &self.sources.get(m.file).text;
            let (expansion, failed) = Expansion::build(text, m.file, &m.parse.tree, blocks);
            diagnostics.extend(failed);
            self.modules[i].expansion = expansion;
        }
        self.diagnostics.extend(diagnostics);
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
            let package = match (root, saved) {
                (Some(Root::Package(name)), _) => name.clone(),
                (None, Some(name)) => name.to_owned(),
                (None, None) if types::is_std_module(module) => "std".to_owned(),
                (Some(_), _) => {
                    diagnostics.extend(refuse(format!(
                        "`{module}` is a module of this project: only a package provides a plugin"
                    )));
                    continue;
                }
                (None, None) if types::builtins().iter().any(|b| b.module == *module) => {
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
    /// modules' names and places, then expands each. A call whose result
    /// this or an earlier run already has is not made again.
    fn expand_found(
        &mut self,
        plugins: &Plugins,
        found: &mut [Found],
        diagnostics: &mut Vec<Diagnostic>,
        editing: &mut Vec<(usize, EditorBlock)>,
    ) -> Vec<Option<Rendered>> {
        let mut groups: BTreeMap<(String, String), Vec<(String, usize)>> = BTreeMap::new();
        for (j, f) in found.iter().enumerate() {
            let m = &self.modules[f.module];
            let entry = groups
                .entry((group(&m.root), f.package.clone()))
                .or_default();
            entry.push((m.name.clone(), j));
        }
        let previous = self.cache.as_mut().map(|c| mem::take(&mut c.plugins));
        let mut results = Results {
            host: self.host,
            previous: previous.unwrap_or_default(),
            kept: HashMap::new(),
        };
        let mut rendered: Vec<Option<Rendered>> = found.iter().map(|_| None).collect();
        for ((_, package), mut members) in groups {
            let Some(Ok(plugin)) = plugins.get(&package) else {
                continue;
            };
            members.sort_by_key(|(name, j)| (name.clone(), found[*j].node.range()));
            let members: Vec<usize> = members.into_iter().map(|(_, j)| j).collect();
            let blocks: Vec<Block> = members.iter().map(|&j| found[j].source.block()).collect();
            let state = match results.collect(plugin.as_ref(), &blocks) {
                Ok(state) => state,
                Err(why) => {
                    for &j in &members {
                        diagnostics.push(failure(&found[j], &package, &format!("failed: {why}")));
                    }
                    continue;
                }
            };
            for &j in &members {
                let editor = EditorBlock::new(&found[j].source, &state, plugin);
                editing.push((found[j].module, editor));
            }
            for (&j, block) in members.iter().zip(&blocks) {
                let f = &mut found[j];
                match results.expand(plugin.as_ref(), block, &state, &mut f.source) {
                    Ok(arena) => {
                        let len = u32::try_from(block.text.len()).expect("source fits in u32");
                        match render(&arena, len) {
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
        if let Some(cache) = &mut self.cache {
            cache.plugins = results.kept;
        }
        rendered
    }
}

/// What plugins gave for the calls of a run: those the last run kept in
/// memory first, then those the host saved, and the ones this run made.
struct Results<'h> {
    host: &'h dyn Host,
    previous: HashMap<Vec<u8>, Vec<u8>>,
    /// What this run used, for the next.
    kept: HashMap<Vec<u8>, Vec<u8>>,
}

impl Results<'_> {
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        let value = (self.previous.remove(key))
            .or_else(|| self.kept.get(key).cloned())
            .or_else(|| self.host.cached(key))?;
        self.kept.insert(key.to_vec(), value.clone());
        Some(value)
    }

    fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.host.save(&key, &value);
        self.kept.insert(key, value);
    }

    /// What `plugin` collects from `blocks`, their heads and texts in order.
    fn collect(&mut self, plugin: &dyn Expander, blocks: &[Block]) -> Result<Vec<u8>, String> {
        let shown = blocks
            .iter()
            .flat_map(|b| [b.head.as_bytes(), b.text.as_bytes()]);
        let id = [
            b"collect".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
        ];
        let key = key(id.into_iter().chain(shown));
        if let Some(state) = self.get(&key) {
            return Ok(state);
        }
        let state = plugin.collect(blocks)?;
        self.put(key, state.clone());
        Ok(state)
    }

    /// The code `plugin` gives for `block`, keyed by the block's text, where
    /// its holes stand and the state collected. An expansion whose holes did
    /// not all parse is not kept: a cached one would not report them.
    fn expand(
        &mut self,
        plugin: &dyn Expander,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        let (column, indent) = source.layout();
        let (column, indent) = (column.to_le_bytes(), indent.to_le_bytes());
        let key = key([
            b"expand".as_slice(),
            VERSION.as_bytes(),
            plugin.id().as_bytes(),
            block.head.as_bytes(),
            block.text.as_bytes(),
            &column,
            &indent,
            state,
        ]);
        // a value that does not read as an arena is not one: made again
        if let Some(arena) = self.get(&key).and_then(|v| serde_json::from_slice(&v).ok()) {
            return Ok(arena);
        }
        let arena = plugin.expand(block, state, source)?;
        if source.diagnostics.is_empty()
            && let Ok(value) = serde_json::to_vec(&arena)
        {
            self.put(key, value);
        }
        Ok(arena)
    }
}

/// The parts, each after its length, as one key.
fn key<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        let len = u64::try_from(part.len()).expect("a part fits in u64");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(part);
    }
    out
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
    let (start, end) = found.node.range();
    Diagnostic::error(
        codes::P036,
        format!("plugin `{package}` {what}"),
        Span::new(found.source.file(), start, end),
    )
}

/// What the plugin `package` found wrong at a span of the block of `found`,
/// the whole of its frame meaning the whole block: under the plugin's own
/// code, or `P036` without one.
fn refusal(found: &Found, package: &str, d: &flick_plugin::Diagnostic) -> Diagnostic {
    let (file, start) = (found.source.file(), found.source.start());
    let (from, to) = if (d.span.start, d.span.end) == (0, found.source.end() - start) {
        found.node.range()
    } else {
        (start + d.span.start, start + d.span.end)
    };
    let span = Span::new(file, from, to);
    match (d.code.as_deref()).and_then(|code| codes::plugin(package, code)) {
        Some(code) => Diagnostic::error(code, d.message.clone(), span),
        None => Diagnostic::error(
            codes::P036,
            format!("plugin `{package}`: {}", d.message),
            span,
        ),
    }
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
