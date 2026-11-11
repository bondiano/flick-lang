//! What a pattern is made of and what it binds: one reading of it for the
//! resolver, the checker and the editor.

use super::cst::{Node, Token};
use super::kind::SyntaxKind as K;

/// Whether a node of `kind` is a pattern.
pub fn is_pattern(kind: K) -> bool {
    (K::WildcardPat..=K::OrPat).contains(&kind)
}

/// Whether a node of `kind` is a pattern or a field of a record pattern,
/// which holds one.
pub fn is_part(kind: K) -> bool {
    is_pattern(kind) || kind == K::RecordPatField
}

/// The name `node` itself binds: that of a variable pattern, or of a field a
/// record pattern names alone, `{ field }`.
pub fn binder(node: &Node) -> Option<Token> {
    let binds =
        node.kind == K::VarPat || (node.kind == K::RecordPatField && !node.has_token(K::Eq));
    binds
        .then(|| node.child_tokens().find(|t| t.kind == K::Ident))
        .flatten()
}

/// The names `pattern` binds, in source order. A type written inside it
/// binds none.
pub fn binders(pattern: &Node, out: &mut Vec<Token>) {
    if let Some(name) = binder(pattern) {
        out.push(name);
    }
    for part in pattern.nodes().filter(|n| is_part(n.kind)) {
        binders(part, out);
    }
}
