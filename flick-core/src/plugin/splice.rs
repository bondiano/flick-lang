//! An expansion in the tree: the rendered code, placed after the module's
//! own text, parsed like any code, its placeholders replaced by the user's
//! code they stand for, and the whole wrapped in parentheses that take the
//! place of the block.
//!
//! Ranges never mix: the parentheses are the block's head and closing brace
//! (nothing at its end, without a body), so the block's range is unchanged;
//! every other generated token lies past the module's text; the user's code
//! keeps its own ranges, and each hole sits in generated parentheses. The
//! checker's tables, keyed by range, see no two nodes with the same range.
//! A block that stands as a declaration gives declarations, which take its
//! place among the module's.

use flick_plugin::HoleKind;

use crate::source::FileId;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::lexer::lex_generated;
use crate::syntax::{Fragment, Heads, parse_decls_at, parse_expr_at};

use super::render::{DECL_PLACEHOLDER, Rendered};
use super::source::BlockSource;

/// The tree that replaces the block `block`, an expression: `rendered`,
/// placed at `at` in `text`, the module text followed by its expansions.
pub fn splice(
    text: &str,
    file: FileId,
    at: u32,
    rendered: &Rendered,
    source: &BlockSource,
    block: &Node,
) -> Result<Node, String> {
    let tree = generated(text, file, at, rendered, source, parse_expr_at)?;
    let head = block.child_tokens().find(|t| t.kind == K::Ident);
    let Some(head) = head else {
        return Err("the block has no head".into());
    };
    // `}`, or nothing at the end of a block without a body
    let (_, end) = block.range();
    let close = match block.child_tokens().find(|t| t.kind == K::RBrace) {
        Some(close) => close,
        None if source.closed() => Token {
            kind: K::RBrace,
            start: end,
            end,
        },
        None => return Err("the block is not closed".into()),
    };
    Ok(Node {
        kind: K::ParenExpr,
        children: vec![
            Element::Token(Token {
                kind: K::LParen,
                ..head
            }),
            Element::Node(tree),
            Element::Token(Token {
                kind: K::RParen,
                ..close
            }),
        ],
    })
}

/// The declarations that replace the block of `source`, a declaration of
/// the module: [`splice`]'s, in order.
pub fn splice_decls(
    text: &str,
    file: FileId,
    at: u32,
    rendered: &Rendered,
    source: &BlockSource,
) -> Result<Vec<Node>, String> {
    let tree = generated(text, file, at, rendered, source, parse_decls_at)?;
    let decls: Vec<Node> = (tree.children.into_iter())
        .filter_map(|c| match c {
            Element::Node(n) => Some(n),
            Element::Token(_) => None,
        })
        .collect();
    if decls.is_empty() {
        return Err("the generated code declares nothing".into());
    }
    Ok(decls)
}

/// `rendered`, placed at `at` in `text`, parsed by `parse` and with the
/// user's code of each hole in place of its placeholder.
fn generated(
    text: &str,
    file: FileId,
    at: u32,
    rendered: &Rendered,
    source: &BlockSource,
    parse: for<'t> fn(&'t str, FileId, &[Token], Heads<'t>) -> Option<Fragment>,
) -> Result<Node, String> {
    let len = u32::try_from(rendered.text.len()).expect("generated code fits in u32");
    let end = at + len;
    if text.get(at as usize..end as usize) != Some(rendered.text.as_str()) {
        return Err("the generated code is not where it was placed".into());
    }
    let (raw, errors) = lex_generated(&rendered.text, file);
    if !errors.is_empty() {
        return Err("the generated code does not lex".into());
    }
    let tokens: Vec<Token> = raw
        .iter()
        .map(|t| Token {
            kind: t.kind,
            start: t.start + at,
            end: t.end + at,
        })
        .collect();
    // generated code has no blocks of its own
    let parsed = parse(text, file, &tokens, &[]).ok_or("the generated code does not parse")?;
    if !parsed.diagnostics.is_empty() || parsed.end != end {
        return Err("the generated code does not parse".into());
    }
    let mut tree = parsed.tree;
    for &(offset, hole) in &rendered.holes {
        let user = source
            .hole_tree(hole)
            .ok_or("a hole is no longer the code it was")?;
        let (kind, width) = match hole.kind {
            HoleKind::Expr => (K::NameExpr, 4),
            HoleKind::Ty => (K::NameType, 4),
            HoleKind::Pat => (K::VarPat, 4),
            HoleKind::Decl => (K::LetDecl, DECL_PLACEHOLDER.len()),
        };
        let width = u32::try_from(width).expect("a short placeholder");
        let placeholder = (at + offset, at + offset + width);
        if !replace(&mut tree, kind, placeholder, &mut Some(user)) {
            return Err("a placeholder of a hole is missing".into());
        }
    }
    Ok(tree)
}

/// Replaces the node of `kind` at `range` under `node` by `with`.
fn replace(node: &mut Node, kind: K, range: (u32, u32), with: &mut Option<Node>) -> bool {
    for child in &mut node.children {
        let Element::Node(n) = child else {
            continue;
        };
        if n.kind == kind
            && n.range() == range
            && let Some(user) = with.take()
        {
            *n = user;
            return true;
        }
        if replace(n, kind, range, with) {
            return true;
        }
    }
    false
}
