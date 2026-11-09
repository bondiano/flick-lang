//! The program a declaration means, which formatting must keep: the
//! trivia-free AST, with the spellings the formatter is free to change made
//! the same.

use crate::syntax::ast::{Ast, lower};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The AST of `node`, with `;` between statements and members dropped (a
/// line break separates them as well), the `|` before the first variant
/// dropped (`= | A | B` declares `A | B`), and an empty service row of an
/// arrow written as nothing: `->{none; E}` is `->{; E}`, `->{none; none}` is
/// `->{}`, `->{S;}` is `->{S}`. A declaration left as written then still
/// compares equal to its formatted twin.
pub(super) fn program(node: &Node, text: &str) -> Ast {
    normalize(lower(node, text))
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
        _ => {}
    }
    Ast::Node(kind, children)
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
        && matches!(&children[errors], Ast::Node(_, items) if items.is_empty())
    {
        children.remove(errors);
        children.retain(|c| !is_token(c, K::Semi));
    } else if let Some(&first) = rows.first()
        && empty(&children[first])
    {
        children[first] = Ast::Node(K::Row, Vec::new());
    }
}
