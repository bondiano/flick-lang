//! Signatures for hover as the source writes them.

use std::collections::HashSet;

use super::{Ide, tree};
use crate::fmt::foralls;
use crate::project::Analyzed;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::keeps_for_owner;

/// A signature as written, without an outer `forall` it does not need.
pub(super) fn signature_as_written(node: &Node, text: &str, outer: &HashSet<String>) -> String {
    as_written(
        foralls::implicit_body(node, text, outer).unwrap_or(node),
        text,
    )
}

/// `node` as its source lays it out, one written line per line, its
/// continuation lines dedented by the indentation of its first line.
pub(super) fn as_written(node: &Node, text: &str) -> String {
    let (start, end) = node.range();
    let line_start = text[..start as usize].rfind('\n').map_or(0, |i| i + 1);
    let indent = text[line_start..start as usize]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    let mut lines = text[start as usize..end as usize].lines();
    let first = lines.next().unwrap_or_default().trim_end().to_owned();
    lines.fold(first, |mut acc, line| {
        let cut = line.len() - line.trim_start().len();
        acc.push('\n');
        acc.push_str(line[cut.min(indent)..].trim_end());
        acc
    })
}

impl Ide<'_> {
    /// The signature of a top-level value as its source writes it.
    pub(super) fn signature(&self, module: &str, name: &str) -> Option<String> {
        let source = self.source(module)?;
        let mut nodes = source.tree.nodes();
        let sig = nodes.find(|d| {
            d.kind == K::Signature
                && d.child_tokens()
                    .next()
                    .is_some_and(|t| t.text(&source.text) == name)
        })?;
        // the constraints of `retention owner` are in no text: show the scheme
        if nodes
            .next()
            .is_some_and(|decl| keeps_for_owner(decl, &source.text))
        {
            return None;
        }
        Some(signature_as_written(
            sig.nodes().next()?,
            &source.text,
            &HashSet::new(),
        ))
    }

    /// The signature of `method` as the instance `instance` of `m` writes it.
    pub(super) fn instance_signature(
        &self,
        m: &Analyzed,
        instance: &str,
        method: &str,
    ) -> Option<String> {
        let info = m.typing.instances.iter().find(|i| i.name == instance)?;
        let text = self.text(m.file);
        let decl = tree::find(&m.tree, info.span)?;
        let sig = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .find(|n| n.kind == K::Signature && tree::name_of(n, K::Ident, text) == Some(method))?;
        // a variable of the instance is not the method's own
        let head = decl
            .nodes()
            .filter(|n| n.kind != K::Members)
            .flat_map(|n| foralls::type_names(n, text))
            .collect();
        Some(signature_as_written(sig.nodes().next()?, text, &head))
    }
}

#[cfg(test)]
#[path = "written_tests.rs"]
mod tests;
