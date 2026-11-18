//! The program a declaration means, which formatting must keep: the
//! trivia-free AST, with the spellings the formatter is free to change made
//! the same.

use std::{mem, ptr};

use crate::plugin::head_token;
use crate::source::FileId;
use crate::syntax::ast::{Ast, lower};
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse_lines_at;

/// How to read the body of a plugin block whose calls take the lines under
/// them as arguments, as a `ui` block's do: its tokens alone do not say
/// which line is whose argument.
pub(super) struct Lines<'h> {
    pub file: FileId,
    /// The plugin heads of the file.
    pub heads: &'h [&'h str],
    /// The heads of the blocks whose bodies are such lines.
    pub bodies: &'h [String],
}

/// The AST of `node`, with `;` between statements and members dropped (a
/// line break separates them as well), the `|` before the first variant
/// dropped (`= | A | B` declares `A | B`), and an empty row of an
/// arrow written as nothing: `->{none; E}` is `->{; E}`, `->{none; none}` is
/// `->{}`, `->{S; none}` and `->{S;}` are `->{S}`. A body of lines is read
/// as the parser reads it, its lines under a call that call's last
/// arguments, as calls in parentheses would be: `f a (g b)` is `f a`, then
/// `g b` on a line under it. A declaration left as written then still
/// compares equal to its formatted twin.
pub(super) fn program(node: &Node, text: &str, lines: &Lines) -> Ast {
    normalize(lower(node, text, &|n| plugin_block(n, text, lines)))
}

/// The AST of a plugin block, its tokens but for the bodies of lines among
/// them, which are parsed; `None` for another node.
fn plugin_block(n: &Node, text: &str, lines: &Lines) -> Option<Ast> {
    if lines.bodies.is_empty() || !matches!(n.kind, K::PluginExpr | K::PluginDecl) {
        return None;
    }
    let own = head_token(n).is_some_and(|h| lines.bodies.iter().any(|b| b == h.text(text)));
    let mut block = n.clone();
    for child in &mut block.children {
        let Element::Node(raw) = child else {
            continue;
        };
        if !matches!(raw.kind, K::Block | K::PluginRest | K::PluginArgs) {
            continue;
        }
        match (own && raw.kind == K::Block).then(|| body(raw, text, lines)) {
            Some(Some(body)) => *raw = body,
            _ => raw.children = blocks_in(&raw.children, text, lines),
        }
    }
    let inner = |m: &Node| (!ptr::eq(m, &block)).then(|| plugin_block(m, text, lines))?;
    Some(lower(&block, text, &inner))
}

/// The tokens of a body of lines, parsed: a block of its lines.
fn body(raw: &Node, text: &str, lines: &Lines) -> Option<Node> {
    let fragment = parse_lines_at(text, lines.file, &raw.tokens(), lines.heads)?;
    if !fragment.diagnostics.is_empty() || fragment.end != raw.range().1 {
        return None;
    }
    // one line is a block of one statement
    Some(if fragment.tree.kind == K::Block {
        fragment.tree
    } else {
        Node {
            kind: K::Block,
            children: vec![Element::Node(fragment.tree)],
        }
    })
}

/// The tokens of another block, with each `head { … }` in them whose body
/// is lines made a block, as the code of a hole of that block reads it.
fn blocks_in(tokens: &[Element], text: &str, lines: &Lines) -> Vec<Element> {
    let kind = |i: usize| match tokens.get(i) {
        Some(Element::Token(t)) => Some(t.kind),
        _ => None,
    };
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let head = match &tokens[i] {
            Element::Token(t) if t.kind == K::Ident => t.text(text),
            _ => "",
        };
        if !lines.bodies.iter().any(|b| b == head) {
            out.push(tokens[i].clone());
            i += 1;
            continue;
        }
        let open = (i + 1..tokens.len()).find(|&j| !kind(j).is_some_and(|k| k.is_trivia()));
        let open = open.filter(|&j| kind(j) == Some(K::LBrace));
        let close = open.and_then(|open| {
            let mut depth = 0u32;
            (open..tokens.len()).find(|&j| {
                match kind(j) {
                    Some(K::LBrace) => depth += 1,
                    Some(K::RBrace) => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
        });
        match (open, close) {
            (Some(open), Some(close)) => {
                let mut children = tokens[i..=open].to_vec();
                children.push(Element::Node(Node {
                    kind: K::Block,
                    children: tokens[open + 1..close].to_vec(),
                }));
                children.push(tokens[close].clone());
                out.push(Element::Node(Node {
                    kind: K::PluginExpr,
                    children,
                }));
                i = close + 1;
            }
            _ => {
                out.push(tokens[i].clone());
                i += 1;
            }
        }
    }
    out
}

fn normalize(ast: Ast) -> Ast {
    let Ast::Node(kind, children) = ast else {
        return ast;
    };
    let mut children: Vec<Ast> = children.into_iter().map(normalize).collect();
    match kind {
        K::Block | K::Members => children.retain(|c| !is_token(c, K::Semi)),
        K::TypeDecl | K::KindDecl => {
            let eq = children.iter().position(|c| is_token(c, K::Eq));
            if let Some(eq) = eq
                && children.get(eq + 1).is_some_and(|c| is_token(c, K::Pipe))
            {
                children.remove(eq + 1);
            }
        }
        K::EffectAnnot => drop_empty_rows(&mut children),
        K::AppExpr => arguments(&mut children),
        _ => {}
    }
    Ast::Node(kind, children)
}

/// The lines under a call as its last arguments, and a call in parentheses
/// as an argument without them: both are only grouping.
fn arguments(children: &mut Vec<Ast>) {
    if let Some(Ast::Node(K::Children, lines)) = children.last_mut() {
        let lines = mem::take(lines);
        children.pop();
        children.extend(lines);
    }
    for argument in children.iter_mut().skip(1) {
        if let Ast::Node(K::ParenExpr, inner) = argument
            && let [open, Ast::Node(K::AppExpr, _), close] = inner.as_slice()
            && is_token(open, K::LParen)
            && is_token(close, K::RParen)
        {
            *argument = inner.swap_remove(1);
        }
    }
}

fn is_token(ast: &Ast, kind: K) -> bool {
    matches!(ast, Ast::Token(k, _) if *k == kind)
}

/// `{ row ; row }` of an arrow, as `fmt` writes it.
fn drop_empty_rows(children: &mut Vec<Ast>) {
    let empty = |row: &Ast| match row {
        Ast::Node(K::Row, items) => match items.as_slice() {
            [] => true,
            [Ast::Node(K::NameType, name)] => {
                matches!(name.as_slice(), [n] if is_token(n, K::NoneKw))
            }
            _ => false,
        },
        _ => false,
    };
    let rows: Vec<usize> = (0..children.len())
        .filter(|&i| matches!(children[i], Ast::Node(K::Row, _)))
        .collect();
    if rows.len() == 2 && rows.iter().all(|&i| empty(&children[i])) {
        children.retain(|c| matches!(c, Ast::Token(k, _) if *k != K::Semi));
    } else if let [_, errors] = rows[..]
        && empty(&children[errors])
    {
        children.remove(errors);
        children.retain(|c| !is_token(c, K::Semi));
    } else if let Some(&first) = rows.first()
        && empty(&children[first])
    {
        children[first] = Ast::Node(K::Row, Vec::new());
    }
}
