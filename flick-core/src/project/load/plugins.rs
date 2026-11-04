//! The plugins the loaded modules import and the blocks they expand. Each
//! plugin learns from all its blocks of a group, the project's sources, its
//! tests or a package, then gives the code of each block; the module is
//! checked with that code in place of its blocks.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use flick_plugin::Block;

use super::{Loader, Root};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::expansion::{Expanded, Expansion};
use crate::plugin::render::{Rendered, render};
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal::string_value;

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
        for (i, m) in self.modules.iter().enumerate() {
            let text = &self.sources.get(m.file).text;
            let mut heads: HashMap<String, String> = HashMap::new();
            for import in m.parse.tree.nodes().filter(|n| n.kind == K::PluginImport) {
                let Some(name) = import.child_tokens().find(|t| t.kind == K::String) else {
                    continue;
                };
                let package = string_value(name.text(text));
                let plugin = plugins
                    .entry(package.clone())
                    .or_insert_with(|| self.host.plugin(&package));
                if let Err(why) = plugin {
                    diagnostics.push(Diagnostic::error(
                        codes::P035,
                        format!("plugin \"{package}\" cannot be loaded: {why}"),
                        Span::new(m.file, name.start, name.end),
                    ));
                    continue;
                }
                for head in exposed(import, text) {
                    heads.insert(head, package.clone());
                }
            }
            if heads.is_empty() {
                continue;
            }
            let shared: Arc<str> = Arc::from(text.as_str());
            let mut blocks = Vec::new();
            blocks_under(&m.parse.tree, &mut blocks);
            for node in blocks {
                let head = node.child_tokens().find(|t| t.kind == K::Ident);
                let Some(package) = head.and_then(|h| heads.get(h.text(text))) else {
                    continue;
                };
                if let Some(source) = BlockSource::new(Arc::clone(&shared), m.file, node) {
                    found.push(Found {
                        module: i,
                        node: node.clone(),
                        source,
                        package: package.clone(),
                    });
                }
            }
        }
        let rendered = self.expand_found(&plugins, &mut found, &mut diagnostics);
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

    /// The code each block of `found` stands for, from its plugin: each
    /// plugin collects from its blocks of a group, in the order of their
    /// modules' names and places, then expands each.
    fn expand_found(
        &self,
        plugins: &Plugins,
        found: &mut [Found],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Option<Rendered>> {
        let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
        for (j, f) in found.iter().enumerate() {
            let group = group(&self.modules[f.module].root);
            groups
                .entry((group, f.package.clone()))
                .or_default()
                .push(j);
        }
        let mut rendered: Vec<Option<Rendered>> = found.iter().map(|_| None).collect();
        for ((_, package), mut members) in groups {
            let Some(Ok(plugin)) = plugins.get(&package) else {
                continue;
            };
            members.sort_by_key(|&j| (&self.modules[found[j].module].name, found[j].node.range()));
            let blocks: Vec<Block> = members.iter().map(|&j| found[j].source.block()).collect();
            let state = match plugin.collect(&blocks) {
                Ok(state) => state,
                Err(why) => {
                    for &j in &members {
                        diagnostics.push(failure(&found[j], &package, &format!("failed: {why}")));
                    }
                    continue;
                }
            };
            for (&j, block) in members.iter().zip(&blocks) {
                let f = &mut found[j];
                match plugin.expand(block, &state, &mut f.source) {
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
                        let (file, start) = (f.source.file(), f.source.start());
                        diagnostics
                            .extend(refused.iter().map(|d| refusal(file, start, &package, d)));
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

/// The group a module's blocks are collected with.
fn group(root: &Root) -> String {
    match root {
        Root::Source => "source".into(),
        Root::Test => "test".into(),
        Root::Package(name) => format!("package {name}"),
        Root::Other(_) => "other".into(),
    }
}

/// The names a plugin import exposes.
fn exposed(import: &Node, text: &str) -> Vec<String> {
    import
        .nodes()
        .filter(|n| n.kind == K::Exposing)
        .flat_map(Node::tokens)
        .filter(|t| t.kind == K::Ident)
        .map(|t| t.text(text).to_owned())
        .collect()
}

/// The plugin blocks under `node`.
fn blocks_under<'n>(node: &'n Node, out: &mut Vec<&'n Node>) {
    for child in node.nodes() {
        if child.kind == K::PluginExpr {
            out.push(child);
        } else {
            blocks_under(child, out);
        }
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

/// `P036` for what the plugin `package` found wrong, at a span of the body
/// that starts at `start`.
fn refusal(file: FileId, start: u32, package: &str, d: &flick_plugin::Diagnostic) -> Diagnostic {
    Diagnostic::error(
        codes::P036,
        format!("plugin `{package}`: {}", d.message),
        Span::new(file, start + d.span.start, start + d.span.end),
    )
}
