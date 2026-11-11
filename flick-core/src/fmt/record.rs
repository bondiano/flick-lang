//! Lists, tuples, `exposing (…)` and records of every kind: on one line, or
//! one item per line ending with its separator.

use std::{mem, ptr};

use flick_pretty::docvec;

use super::block::Value;
use super::comments::Comment;
use super::expr::Part;
use super::printer::{Doc, INDENT, Item, Printer, nodes, position};
use crate::syntax::cst::{Element, Node, Token};
use crate::syntax::kind::SyntaxKind as K;

/// The separator after an item: `(broken, flat)`.
type Separator = (&'static str, &'static str);

/// How the items of a bracket are laid out.
struct Shape {
    /// The separator a comment ending an item's line may follow.
    sep: char,
    /// What follows the opener when it stays on the line: `""` or `" "`.
    pad: &'static str,
    /// What follows the last item when it stays on the line.
    last: &'static str,
    /// Broken one item per line whatever the width: written with a trailing
    /// separator, a blank line between items, or a comment.
    force: bool,
    /// Several short items per line, as many as fit.
    fill: bool,
}

/// Items a list of constants may hold several of per line.
fn simple(n: &Node) -> bool {
    match n.kind {
        K::Literal | K::NameExpr | K::UnitExpr => true,
        K::PrefixExpr => n.nodes().all(|c| c.kind == K::Literal),
        _ => false,
    }
}

/// The closing token of `n` and the token right before it, if a token.
fn last_two(n: &Node) -> (Option<Token>, Option<Token>) {
    let mut solid = n
        .children
        .iter()
        .rev()
        .filter(|c| !matches!(c, Element::Token(t) if t.kind.is_trivia()));
    let mut next_token = || match solid.next() {
        Some(Element::Token(t)) => Some(*t),
        _ => None,
    };
    let close = next_token();
    (next_token(), close)
}

impl<'a> Printer<'a> {
    /// `[a, b]`, `(a, b)`, `exposing (a, b)` and their patterns and types.
    pub fn bracket(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let outer = mem::replace(&mut self.in_bracket, true);
        let doc = self.bracketed(n, suffix);
        self.in_bracket = outer;
        doc
    }

    fn bracketed(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let a = self.a;
        let Some(open) = n.children.iter().position(
            |c| matches!(c, Element::Token(t) if matches!(t.kind, K::LParen | K::LBracket)),
        ) else {
            return self.generic(n);
        };
        let (separator, close) = last_two(n);
        let Some(close) = close.filter(|c| c.kind.is_closer()) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..open]);
        let Element::Token(opener) = n.children[open] else {
            unreachable!("a token position")
        };
        let opener = self.tok(opener);
        let items = nodes(n);
        // `(a,)` is a tuple, `(a)` is not: that `,` is no choice of the author
        let single =
            items.len() == 1 && matches!(n.kind, K::TupleExpr | K::TuplePat | K::TupleType);
        let trailing =
            separator.is_some_and(|s| s.kind == K::Comma) && !items.is_empty() && !single;
        let fill = trailing
            && n.kind == K::ListExpr
            && items.len() > 1
            && items.iter().all(|i| simple(i))
            && items.windows(2).any(|w| {
                !self.text[w[0].range().1 as usize..w[1].range().0 as usize].contains('\n')
            });
        let mut shape = Shape {
            sep: ',',
            pad: "",
            last: if single { "," } else { "" },
            // a filled list stays on one line when it fits, as in Gleam
            force: trailing && !fill,
            fill,
        };
        let count = items.len();
        let mut index = 0;
        // a list broken whatever the width ends its last item with `,` too
        let last_comma = trailing && !shape.fill && shape.last.is_empty();
        let doc = self.seq(opener, &items, close, suffix, &mut shape, &mut |p, item| {
            index += 1;
            // a `,` may follow the item when the bracket breaks
            p.tail_inline = matches!(
                item.kind,
                K::IfExpr | K::MatchExpr | K::TryExpr | K::FunExpr
            );
            if index == count && !last_comma {
                return (p.node(item), (",", ", "));
            }
            // the comma inside the item's group, which then makes room for it
            (p.with_suffix(item, &[Part::Text(",")]), ("", " "))
        });
        if head.is_empty() {
            doc
        } else {
            docvec![a, head, " ", doc]
        }
    }

    /// `{ a = 1; b = 2 }`, `{ r with a = 1 }`, `{ a : T | r }`, `{ a; b = p }`.
    pub fn record(&mut self, n: &'a Node, suffix: &[Part]) -> Doc<'a> {
        let (Some(open), (separator, Some(close))) = (position(n, K::LBrace), last_two(n)) else {
            return self.generic(n);
        };
        let Element::Token(opener) = n.children[open] else {
            unreachable!("a token position")
        };
        let opener = self.tok(opener);
        let items = nodes(n);
        let update = n.kind == K::RecordUpdate;
        let base = if update { items.first().copied() } else { None };
        let with = n.children.iter().find_map(|c| match c {
            Element::Token(t) if t.kind == K::WithKw => Some(*t),
            _ => None,
        });
        let mut shape = Shape {
            sep: ';',
            pad: " ",
            last: " ",
            force: separator.is_some_and(|s| s.kind == K::Semi) && !items.is_empty(),
            fill: false,
        };
        let tails: Vec<bool> = items.iter().map(|i| i.kind == K::RowTail).collect();
        let mut index = 0;
        self.seq(opener, &items, close, suffix, &mut shape, &mut |p, item| {
            let i = index;
            index += 1;
            if base.is_some_and(|b| ptr::eq(b, item)) {
                let doc = p.node(item);
                let with = with.map_or(p.a.empty(), |w| {
                    let w = p.tok(w);
                    docvec![p.a, " ", w]
                });
                return (docvec![p.a, doc, with], ("", " "));
            }
            let doc = p.node(item);
            let sep = if tails[i] {
                ("", " ")
            } else if tails.get(i + 1).copied().unwrap_or(false) {
                (";", " ")
            } else {
                (";", "; ")
            };
            (doc, sep)
        })
    }

    /// `name = value` of a record or a codec.
    pub fn field(&mut self, n: &'a Node) -> Doc<'a> {
        let (Some(eq), Some(block)) = (position(n, K::Eq), n.nodes().next()) else {
            return self.generic(n);
        };
        let head = self.elements(n, &n.children[..=eq]);
        self.body(head, Value::Block(block), n.range().1, &[])
    }

    /// The items between `opener` and the `close` token, with the comments
    /// around them; `build` gives each item's document and separator.
    fn seq(
        &mut self,
        opener: Doc<'a>,
        nodes: &[&'a Node],
        close: Token,
        suffix: &[Part],
        shape: &mut Shape,
        build: &mut dyn FnMut(&mut Self, &'a Node) -> (Doc<'a>, Separator),
    ) -> Doc<'a> {
        let a = self.a;
        let first = nodes.first().map_or(close.start, |n| n.range().0);
        let after = self.comments.trailing(first, None);
        let mut seps = Vec::with_capacity(nodes.len());
        let mut items = self.items(nodes, close.start, Some(shape.sep), &mut |p, n| {
            let (doc, sep) = build(p, n);
            seps.push(sep);
            doc
        });
        for (item, sep) in items.iter_mut().zip(seps) {
            item.sep = sep;
        }
        let dangling = self.comments.leading(close.start);
        let close = self.tok(close);
        let suffix = self.suffix(suffix);
        let close = docvec![a, close, suffix];
        if items.is_empty() && after.is_none() && dangling.is_empty() {
            return docvec![a, opener, close];
        }
        let commented = after.is_some()
            || !dangling.is_empty()
            || items
                .iter()
                .any(|i| !i.comments.is_empty() || i.trailing.is_some());
        let blank = items.iter().any(|i| i.blank);
        shape.force |= commented || blank;
        shape.fill &= !blank;
        let first_break = match after {
            Some(c) => docvec![a, " ", c, a.line()],
            None => a.break_("", shape.pad),
        };
        let mut inner = vec![first_break];
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                inner.push(self.separator(&items[i - 1], item, shape.fill));
            }
            inner.push(self.leading(item));
            inner.push(item.doc);
        }
        let Some(last) = items.last() else {
            let inner = docvec![a, a.line(), self.comment_block(&dangling)].nest(a, INDENT);
            return docvec![a, opener, inner, a.line(), close]
                .force_break(a)
                .group(a);
        };
        let doc = if dangling.is_empty() {
            let end = match last.trailing {
                Some(c) => docvec![a, self.str(&format!("{} {c}", last.sep.0)), a.line()],
                None => a.break_(last.sep.0, shape.last),
            };
            docvec![a, opener, a.concat(inner).nest(a, INDENT), end, close]
        } else {
            inner.push(match last.trailing {
                Some(c) => self.str(&format!("{} {c}", last.sep.0)),
                None => a.text(last.sep.0),
            });
            inner.push(self.dangling(&dangling));
            docvec![a, opener, a.concat(inner).nest(a, INDENT), a.line(), close]
        };
        if shape.force {
            doc.force_break(a).group(a)
        } else {
            doc.group(a)
        }
    }

    /// The comments before a closer, on their own lines after the items.
    fn dangling(&self, comments: &[Comment<'a>]) -> Doc<'a> {
        let a = self.a;
        let blank = comments.first().is_some_and(|c| c.blank);
        docvec![
            a,
            a.lines(1 + usize::from(blank)),
            self.comment_block(comments)
        ]
    }

    /// What goes between two items.
    fn separator(&self, previous: &Item<'a>, item: &Item<'a>, fill: bool) -> Doc<'a> {
        let a = self.a;
        let (broken, flat) = previous.sep;
        let blank = a.lines(usize::from(item.blank));
        if let Some(c) = previous.trailing {
            return docvec![a, self.str(&format!("{broken} {c}")), a.line(), blank];
        }
        if !item.comments.is_empty() || item.blank {
            return docvec![a, broken, a.line(), blank];
        }
        if fill {
            a.flex_break(broken, flat)
        } else {
            a.break_(broken, flat)
        }
    }
}
