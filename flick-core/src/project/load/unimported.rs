//! A block of a plugin linked into the compiler whose head the file does
//! not import: `cli "x" { … }` before `import Cli exposing (plugin cli)`.
//! The error carries the import as a fix, which `flick check --fix` applies.

use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// `P037` on the head `head` at `span` of a block of the module whose tree
/// is `tree`, the module `module` exposing it.
pub(super) fn unimported(
    file: FileId,
    tree: &Node,
    span: (u32, u32),
    head: &str,
    module: &str,
) -> Diagnostic {
    let import = format!("import {module} exposing (plugin {head})");
    let (start, end) = span;
    let after = (tree.nodes())
        .filter(|n| n.kind == K::ImportDecl)
        .last()
        .map(|n| (n.range().1, format!("\n{import}")))
        .or_else(|| {
            (tree.nodes().find(|n| n.kind == K::ModuleHeader))
                .map(|n| (n.range().1, format!("\n\n{import}")))
        });
    let (at, text) = after.unwrap_or((0, format!("{import}\n\n")));
    Diagnostic::error(
        codes::P037,
        format!("`{head}` is the plugin of `{module}`, which this file does not import"),
        Span::new(file, start, end),
    )
    .with_safe_fix(|| {
        (
            format!("add `{import}`"),
            vec![Edit {
                span: Span::new(file, at, at),
                replacement: text,
            }],
        )
    })
}
