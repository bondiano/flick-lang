//! Parser events and their conversion into the lossless tree.

use std::mem;

use super::super::cst::{Element, Node, Token};
use super::super::kind::SyntaxKind;
use super::super::lexer::RawToken;

#[derive(Debug)]
pub(super) enum Event {
    Start {
        kind: Option<SyntaxKind>,
        forward_parent: Option<usize>,
    },
    Token,
    Finish,
}

/// Builds the tree from parser events, placing trivia before the node that
/// follows it so comments stay between declarations.
pub(super) fn build(raw: &[RawToken], mut events: Vec<Event>) -> Node {
    let mut stack: Vec<Node> = Vec::new();
    let mut next = 0;
    let trivia = |stack: &mut Vec<Node>, next: &mut usize| {
        while *next < raw.len() && raw[*next].kind.is_trivia() {
            let t = raw[*next];
            if let Some(top) = stack.last_mut() {
                top.children.push(Element::Token(Token {
                    kind: t.kind,
                    start: t.start,
                    end: t.end,
                }));
            }
            *next += 1;
        }
    };
    let mut kinds = Vec::new();
    for i in 0..events.len() {
        match mem::replace(
            &mut events[i],
            Event::Start {
                kind: None,
                forward_parent: None,
            },
        ) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                kinds.clear();
                kinds.extend(kind);
                let mut parent = forward_parent;
                while let Some(at) = parent {
                    match mem::replace(
                        &mut events[at],
                        Event::Start {
                            kind: None,
                            forward_parent: None,
                        },
                    ) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            kinds.extend(kind);
                            parent = forward_parent;
                        }
                        _ => unreachable!("forward parent is a start event"),
                    }
                }
                for &kind in kinds.iter().rev() {
                    if !stack.is_empty() {
                        trivia(&mut stack, &mut next);
                    }
                    stack.push(Node {
                        kind,
                        children: Vec::new(),
                    });
                }
            }
            Event::Token => {
                trivia(&mut stack, &mut next);
                let t = raw[next];
                stack
                    .last_mut()
                    .expect("token inside a node")
                    .children
                    .push(Element::Token(Token {
                        kind: t.kind,
                        start: t.start,
                        end: t.end,
                    }));
                next += 1;
            }
            Event::Finish => {
                if stack.len() == 1 {
                    trivia(&mut stack, &mut next);
                }
                let node = stack.pop().expect("balanced events");
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Element::Node(node)),
                    None => return node,
                }
            }
        }
    }
    unreachable!("the root node is finished last")
}
