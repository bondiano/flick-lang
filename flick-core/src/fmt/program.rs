//! The program a declaration means, which formatting must keep: the
//! trivia-free AST, with the spellings the formatter is free to change made
//! the same.

use std::ptr;

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
    pub bodies: &'h [&'h str],
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
    let mut block = n.clone();
    parse_bodies(&mut block, text, lines);
    let inner = |m: &Node| (!ptr::eq(m, &raw const block)).then(|| plugin_block(m, text, lines))?;
    Some(lower(&block, text, &inner))
}

/// Parses the body of each `head { … }` among the tokens of `n`, and of its
/// parts kept as tokens, whose body is lines: the code of a hole reads it so.
fn parse_bodies(n: &mut Node, text: &str, lines: &Lines) {
    let mut i = 0;
    while i < n.children.len() {
        match &mut n.children[i] {
            Element::Node(raw) => parse_bodies(raw, text, lines),
            Element::Token(t) if lines.bodies.contains(&t.text(text)) => {
                if let Some((open, close)) = braced(&n.children, i)
                    && let Some(body) = body(&n.children[open + 1..close], text, lines)
                {
                    n.children.drain(open + 2..close);
                    i = open + 1;
                    n.children[i] = Element::Node(body);
                }
            }
            Element::Token(_) => {}
        }
        i += 1;
    }
}

/// The `{` after the head at `at` and its matching `}`.
fn braced(children: &[Element], at: usize) -> Option<(usize, usize)> {
    let mut open = None;
    let mut depth = 0u32;
    for (j, child) in children.iter().enumerate().skip(at + 1) {
        match child {
            Element::Token(t) if t.kind == K::LBrace => {
                open = open.or(Some(j));
                depth += 1;
            }
            Element::Token(t) if t.kind == K::RBrace && depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return open.map(|open| (open, j));
                }
            }
            Element::Token(t) if t.kind.is_trivia() => {}
            _ if open.is_none() => return None,
            _ => {}
        }
    }
    None
}

/// `raw`, the tokens of a body, parsed as lines: a block of statements, a
/// line one of one.
fn body(raw: &[Element], text: &str, lines: &Lines) -> Option<Node> {
    let mut tokens = Vec::new();
    for element in raw {
        match element {
            Element::Token(t) => tokens.push(*t),
            Element::Node(n) => tokens.extend(n.tokens()),
        }
    }
    let end = tokens.iter().rev().find(|t| !t.kind.is_trivia())?.end;
    let fragment = parse_lines_at(text, lines.file, &tokens, lines.heads)?;
    if !fragment.diagnostics.is_empty() || fragment.end != end {
        return None;
    }
    Some(if fragment.tree.kind == K::Block {
        fragment.tree
    } else {
        Node {
            kind: K::Block,
            children: vec![Element::Node(fragment.tree)],
        }
    })
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
        K::CatchExpr => catch_subject(&mut children),
        _ => {}
    }
    Ast::Node(kind, children)
}

/// The lines under a call as its last arguments, and a call in parentheses
/// as an argument without them: both are only grouping.
fn arguments(children: &mut Vec<Ast>) {
    if let Some(Ast::Node(K::Children, _)) = children.last()
        && let Some(Ast::Node(_, lines)) = children.pop()
    {
        children.extend(lines);
    }
    for argument in children.iter_mut().skip(1) {
        if let Ast::Node(K::ParenExpr, inner) = argument
            && let [_, Ast::Node(K::AppExpr, _), _] = inner.as_slice()
        {
            *argument = inner.swap_remove(1);
        }
    }
}

/// Explicit grouping parentheses around a pipe expression on the left of
/// `catch` do not change the AST: `catch` binds weaker than `|>` and `<|`.
fn catch_subject(children: &mut [Ast]) {
    let Some(first) = children.first_mut() else {
        return;
    };
    if let Ast::Node(K::ParenExpr, inner) = first
        && let [_, Ast::Node(K::BinExpr, bin), _] = inner.as_slice()
        && bin
            .iter()
            .any(|c| is_token(c, K::PipeGt) || is_token(c, K::LtPipe))
    {
        *first = inner.swap_remove(1);
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
