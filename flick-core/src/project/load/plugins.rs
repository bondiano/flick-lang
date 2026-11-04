//! The plugins the loaded modules import, which expand their blocks.

use super::Loader;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

impl Loader<'_> {
    /// `P035` for each plugin a module imports: this compiler loads none yet.
    pub(super) fn plugins(&mut self) {
        let mut found = Vec::new();
        for m in &self.modules {
            let text = &self.sources.get(m.file).text;
            for import in m.parse.tree.nodes().filter(|n| n.kind == K::PluginImport) {
                let Some(package) = package(import) else {
                    continue;
                };
                found.push(Diagnostic::error(
                    codes::P035,
                    format!("plugin {} cannot be loaded", package.text(text)),
                    Span::new(m.file, package.start, package.end),
                ));
            }
        }
        self.diagnostics.extend(found);
    }
}

/// The string naming the package of a plugin import.
fn package(import: &Node) -> Option<Token> {
    import.child_tokens().find(|t| t.kind == K::String)
}
