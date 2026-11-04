//! An expansion in the tree: the rendered code, placed after the module's
//! own text, parsed like any code, its placeholders replaced by the user's
//! code they stand for, and the whole wrapped in parentheses that take the
//! place of the block.
//!
//! Ranges never mix: the parentheses are the block's head and closing brace,
//! so the block's range is unchanged; every other generated token lies past
//! the module's text; the user's code keeps its own ranges, and each hole
//! sits in generated parentheses. The checker's tables, keyed by range, see
//! no two nodes with the same range.

use crate::source::FileId;
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::lexer::lex;
use crate::syntax::parse_expr_at;

use super::render::Rendered;
use super::source::BlockSource;

/// The tree that replaces the block `block`: `rendered`, placed at `at` in
/// `text`, the module text followed by its expansions.
pub fn splice(
    text: &str,
    file: FileId,
    at: u32,
    rendered: &Rendered,
    source: &BlockSource,
    block: &Node,
) -> Result<Node, String> {
    let len = u32::try_from(rendered.text.len()).expect("generated code fits in u32");
    let end = at + len;
    if text.get(at as usize..end as usize) != Some(rendered.text.as_str()) {
        return Err("the generated code is not where it was placed".into());
    }
    let (raw, errors) = lex(&rendered.text, file);
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
    let parsed = parse_expr_at(text, file, &tokens).ok_or("the generated code does not parse")?;
    if !parsed.diagnostics.is_empty() || parsed.end != end {
        return Err("the generated code does not parse".into());
    }
    let mut tree = parsed.tree;
    for &(offset, hole) in &rendered.holes {
        let user = source
            .hole_tree(hole)
            .ok_or("a hole is no longer the code it was")?;
        let placeholder = (at + offset, at + offset + 4);
        if !replace(&mut tree, placeholder, &mut Some(user)) {
            return Err("a placeholder of a hole is missing".into());
        }
    }
    let head = block.child_tokens().find(|t| t.kind == K::Ident);
    let close = block.child_tokens().find(|t| t.kind == K::RBrace);
    let (Some(head), Some(close)) = (head, close) else {
        return Err("the block is not closed".into());
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

/// Replaces the name node at `range` under `node` by `with`.
fn replace(node: &mut Node, range: (u32, u32), with: &mut Option<Node>) -> bool {
    for child in &mut node.children {
        let Element::Node(n) = child else {
            continue;
        };
        if matches!(n.kind, K::NameExpr | K::NameType)
            && n.range() == range
            && let Some(user) = with.take()
        {
            *n = user;
            return true;
        }
        if replace(n, range, with) {
            return true;
        }
    }
    false
}
