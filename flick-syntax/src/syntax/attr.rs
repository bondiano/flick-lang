//! Shared extraction of `K::Attribute` CST nodes (`@name args...` and `@ns.name args...`).

use super::cst::Node;
use super::kind::SyntaxKind as K;

/// The parsed header of a `K::Attribute` node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttrParts<'a> {
    /// The attribute name without `@` (e.g. `"rename"` or `"meta.rename"`).
    pub name: &'a str,
    /// Trimmed argument slice following the name, empty when none.
    pub args: &'a str,
    /// Byte span covering `@name` (including `@` and optional `.segment`).
    pub name_span: (u32, u32),
    /// End byte offset of the name token(s).
    pub name_end: u32,
    /// Whether the name is namespaced (`@pkg.attr`).
    pub namespaced: bool,
}

/// Extracts the name, arguments, and `@name` span from a `K::Attribute` node.
#[must_use]
pub fn attr_parts<'a>(node: &Node, src: &'a str) -> Option<AttrParts<'a>> {
    let mut tokens = node.child_tokens();
    let at = tokens.next().filter(|t| t.kind == K::At)?;
    let first = tokens.next().filter(|t| t.kind == K::Ident)?;
    let (name_end, namespaced) = match (tokens.next(), tokens.next()) {
        (Some(dot), Some(second))
            if dot.kind == K::Dot
                && second.kind == K::Ident
                && dot.start == first.end
                && second.start == dot.end =>
        {
            (second.end, true)
        }
        _ => (first.end, false),
    };
    let name = &src[first.start as usize..name_end as usize];
    let (_, node_end) = node.range();
    let args = src[name_end as usize..node_end as usize].trim();
    Some(AttrParts {
        name,
        args,
        name_span: (at.start, name_end),
        name_end,
        namespaced,
    })
}
