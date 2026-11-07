//! Semantic tokens, which say what each name is, and folding ranges.

use super::{Ide, hints::strip, tree};
use crate::iface::{Decl, Namespace};
use crate::plugin::blocks_under;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::{FOREIGN_MODES, SyntaxKind as K};
use crate::types::{Ref, Type};
use flick_plugin::HighlightKind;

/// The token types, in the order of the legend the server announces.
pub const TOKEN_TYPES: [&str; 9] = [
    "namespace",
    "type",
    "enumMember",
    "function",
    "variable",
    "method",
    "parameter",
    "keyword",
    "macro",
];

const NAMESPACE: u32 = 0;
const TYPE: u32 = 1;
const CONSTRUCTOR: u32 = 2;
const FUNCTION: u32 = 3;
const VARIABLE: u32 = 4;
const METHOD: u32 = 5;
const PARAMETER: u32 = 6;
/// A word that is a keyword only where it stands.
const KEYWORD: u32 = 7;
/// The head of a plugin's blocks.
const MACRO: u32 = 8;

/// Nodes worth folding when they span several lines.
const FOLDED: [K; 15] = [
    K::LetDecl,
    K::TypeDecl,
    K::ServiceDecl,
    K::ClassDecl,
    K::InstanceDecl,
    K::MatchExpr,
    K::RecordExpr,
    K::RecordType,
    K::ListExpr,
    K::TaskExpr,
    K::DoExpr,
    K::UiExpr,
    K::PluginExpr,
    K::PluginDecl,
    K::RecGroup,
];

impl Ide<'_> {
    /// `(start, end, type)` of each name of `file` the checker resolved, by
    /// [`TOKEN_TYPES`], in source order.
    pub fn semantic_tokens(&self, file: FileId) -> Vec<(u32, u32, u32)> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = self.text(file);
        let mut out = Vec::new();
        for (range, r) in &m.typing.names {
            let Some(node) = tree::find(&m.tree, *range) else {
                continue;
            };
            qualifiers(node, &mut out);
            let Some(token) = tree::last_name(node) else {
                continue;
            };
            let kind = match r {
                Ref::Value { .. } if is_function(m.typing.types.get(range)) => FUNCTION,
                Ref::Local | Ref::Value { .. } => VARIABLE,
                Ref::Method { .. } | Ref::Operation { .. } => METHOD,
                Ref::Constructor(_) => CONSTRUCTOR,
                Ref::ServiceTag(_) => TYPE,
            };
            out.push((token.start, token.end, kind));
        }
        for range in m.typing.type_names.keys() {
            if let Some(node) = tree::find(&m.tree, *range) {
                qualifiers(node, &mut out);
                if let Some(token) = tree::last_name(node) {
                    out.push((token.start, token.end, TYPE));
                }
            }
        }
        for d in declarations(&m.tree, text, file) {
            let kind = match (d.namespace, d.decl) {
                (_, Decl::Constructor | Decl::KindMember) => CONSTRUCTOR,
                (_, Decl::Method) => METHOD,
                (Namespace::Type, _) => TYPE,
                _ => FUNCTION,
            };
            out.push((d.span.start, d.span.end, kind));
        }
        binders(&m.tree, &mut out);
        for b in &m.editing {
            let len = u32::try_from(b.block.text.len()).unwrap_or(u32::MAX);
            let found = (b.plugin.highlight(&b.block, &b.state))
                .unwrap_or_else(|why| super::plugins::failed(b.plugin.id(), "highlight", &why));
            for h in found {
                // a plugin's span outside its block is none
                if h.span.start <= h.span.end && h.span.end <= len {
                    let kind = match h.kind {
                        HighlightKind::Keyword => KEYWORD,
                        HighlightKind::Parameter => PARAMETER,
                    };
                    out.push((b.start + h.span.start, b.start + h.span.end, kind));
                }
            }
        }
        foreign_modes(&m.tree, text, &mut out);
        contextual_keywords(&m.tree, &mut out);
        heads(&m.tree, &mut out);
        // of tokens at one place the head of a plugin's blocks wins
        out.sort_unstable_by_key(|&(start, end, kind)| (start, kind != MACRO, end, kind));
        out.dedup_by_key(|t| t.0);
        out
    }

    /// Byte ranges of the declarations and blocks of `file` that span
    /// several lines, and of its run of imports.
    pub fn folding_ranges(&self, file: FileId) -> Vec<(u32, u32)> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = self.text(file);
        let mut out = Vec::new();
        let imports: Vec<(u32, u32)> = m
            .tree
            .nodes()
            .filter(|n| n.kind == K::ImportDecl)
            .map(Node::range)
            .collect();
        if let (Some(first), Some(last)) = (imports.first(), imports.last()) {
            out.push((first.0, last.1));
        }
        fold(&m.tree, &mut out);
        out.retain(|(s, e)| text[*s as usize..*e as usize].contains('\n'));
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn is_function(ty: Option<&Type>) -> bool {
    ty.is_some_and(|ty| matches!(strip(ty), Type::Fun { .. }))
}

/// The module path before a qualified name: `List` of `List.map`.
fn qualifiers(node: &Node, out: &mut Vec<(u32, u32, u32)>) {
    let names: Vec<_> = node
        .child_tokens()
        .filter(|t| matches!(t.kind, K::Con | K::Ident))
        .collect();
    if let [first, .., before_last, _] = names[..] {
        out.push((first.start, before_last.end, NAMESPACE));
    } else if let [only, _] = names[..] {
        out.push((only.start, only.end, NAMESPACE));
    }
}

/// Names that patterns bind.
fn binders(node: &Node, out: &mut Vec<(u32, u32, u32)>) {
    for child in node.nodes() {
        if child.kind == K::VarPat
            && let Some(t) = child.child_tokens().find(|t| t.kind == K::Ident)
        {
            out.push((t.start, t.end, PARAMETER));
        }
        binders(child, out);
    }
}

/// `pure` of `foreign pure f from …`, and `global` of `from global`.
fn foreign_modes(tree: &Node, text: &str, out: &mut Vec<(u32, u32, u32)>) {
    for decl in tree.nodes().filter(|n| n.kind == K::ForeignDecl) {
        let names: Vec<_> = decl.child_tokens().filter(|t| t.kind == K::Ident).collect();
        if let [mode, _, ..] = names[..]
            && FOREIGN_MODES.contains(&mode.text(text))
        {
            out.push((mode.start, mode.end, KEYWORD));
        }
        let global = decl
            .nodes()
            .flat_map(Node::child_tokens)
            .find(|t| t.kind == K::Ident && t.text(text) == "global");
        if let Some(t) = global {
            out.push((t.start, t.end, KEYWORD));
        }
    }
}

/// Contextual keywords.
fn contextual_keywords(node: &Node, out: &mut Vec<(u32, u32, u32)>) {
    for t in node.child_tokens() {
        if matches!(
            t.kind,
            K::AndKw
                | K::ComponentKw
                | K::PluginKw
                | K::EffectKw
                | K::TailKw
                | K::AbortKw
                | K::ControlKw
                | K::ReturnKw
        ) {
            out.push((t.start, t.end, KEYWORD));
        }
    }
    for child in node.nodes() {
        contextual_keywords(child, out);
    }
}

/// The head of each plugin block, and each head an import exposes.
fn heads(tree: &Node, out: &mut Vec<(u32, u32, u32)>) {
    let mut blocks = Vec::new();
    blocks_under(tree, &mut blocks);
    let exposed = (tree.nodes())
        .filter(|n| n.kind == K::ImportDecl)
        .flat_map(|i| i.nodes().filter(|n| n.kind == K::Exposing))
        .flat_map(Node::nodes)
        .filter(|item| item.has_token(K::PluginKw));
    for node in blocks.into_iter().chain(exposed) {
        if let Some(head) = node.child_tokens().find(|t| t.kind == K::Ident) {
            out.push((head.start, head.end, MACRO));
        }
    }
}

fn fold(node: &Node, out: &mut Vec<(u32, u32)>) {
    for child in node.nodes() {
        if FOLDED.contains(&child.kind) {
            out.push(child.range());
        }
        fold(child, out);
    }
}
