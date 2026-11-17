//! Collecting the blocks of a round into those to expand and those only
//! editors see while they are not closed.

use std::collections::HashMap;

use super::super::Loader;
use super::{Found, Plugins};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::source::BlockSource;
use crate::plugin::{EditorBlock, head_token};
use crate::source::Span;
use crate::syntax::cst::Node;

/// Where the blocks of a round go: those to expand, and those only editors
/// see while they are not closed.
pub(super) struct Sink<'a> {
    pub(super) plugins: &'a Plugins,
    /// The heads of the module the blocks are in, by the package of each.
    pub(super) heads: &'a HashMap<String, String>,
    pub(super) found: &'a mut Vec<Found>,
    pub(super) editing: &'a mut Vec<(usize, EditorBlock)>,
}

impl Sink<'_> {
    /// The block `node` of the module `i`, read as `source`.
    pub(super) fn take(
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
            return;
        };
        if let Some(off) = loader.disabled(package, head, Span::new(m.file, token.start, token.end))
        {
            diagnostics.push(off);
            return;
        }
        if let Some(Err(_)) = self.plugins.get(package) {
            let import = (m.syntax.imports.iter())
                .flat_map(|i| &i.plugins)
                .find(|item| item.name == head);
            let (start, end) = node.range();
            let mut d = Diagnostic::error(
                codes::P036,
                format!("plugin `{package}` is not loaded, so this block has no code"),
                Span::new(m.file, start, end),
            );
            if let Some(item) = import {
                d = d.with_related(item.span, "the import says why it does not load");
            }
            diagnostics.push(d);
            return;
        }
        let Some(source) = source else {
            return;
        };
        if source.closed() {
            let (start, end) = node.range();
            let plugin_id = self
                .plugins
                .get(package)
                .and_then(|p| p.as_ref().ok())
                .map(|p| p.id().to_owned())
                .unwrap_or_default();
            self.found.push(Found {
                module: i,
                node: node.clone(),
                whole: Span::new(source.file(), start, end),
                source,
                package: package.clone(),
                plugin_id,
                keep: false,
                declared: Vec::new(),
            });
        } else if let Some(Ok(plugin)) = self.plugins.get(package) {
            self.editing
                .push((i, EditorBlock::new(&source, &[], plugin)));
        }
    }
}
