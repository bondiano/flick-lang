//! Parser events and their conversion into the lossless tree.

use std::num::NonZeroU32;

use super::super::cst::{Element, Node, Token};
use super::super::kind::SyntaxKind;
use super::super::lexer::RawToken;

#[derive(Debug, Clone, Copy)]
pub(super) enum Event {
    Start {
        kind: Option<SyntaxKind>,
        forward_parent: Option<NonZeroU32>,
    },
    /// The next token, as the given kind when the parser read a word as a
    /// keyword only there (`and` of a `do` group).
    Token(Option<SyntaxKind>),
    Finish,
}

#[inline]
fn attach_trivia(raw: &[RawToken], stack: &mut [Node], next: &mut usize) {
    let mut i = *next;
    let len = raw.len();
    if i >= len || !raw[i].kind.is_trivia() {
        return;
    }
    if let Some(top) = stack.last_mut() {
        while i < len && raw[i].kind.is_trivia() {
            let t = raw[i];
            top.children.push(Element::Token(Token {
                kind: t.kind,
                start: t.start,
                end: t.end,
            }));
            i += 1;
        }
    } else {
        while i < len && raw[i].kind.is_trivia() {
            i += 1;
        }
    }
    *next = i;
}

/// Builds the tree from parser events, placing trivia before the node that
/// follows it so comments stay between declarations.
pub(super) fn build(raw: &[RawToken], mut events: Vec<Event>) -> Node {
    let mut stack: Vec<Node> = Vec::with_capacity(32);
    let mut next = 0_usize;
    let mut kinds: Vec<SyntaxKind> = Vec::with_capacity(8);
    let tombstone = Event::Start {
        kind: None,
        forward_parent: None,
    };
    for i in 0..events.len() {
        let ev = events[i];
        events[i] = tombstone;
        match ev {
            Event::Start {
                kind,
                forward_parent,
            } => {
                if forward_parent.is_none() {
                    if let Some(kind) = kind {
                        if !stack.is_empty() {
                            attach_trivia(raw, &mut stack, &mut next);
                        }
                        stack.push(Node {
                            kind,
                            children: Vec::new(),
                        });
                    }
                    continue;
                }
                kinds.clear();
                kinds.extend(kind);
                let mut parent = forward_parent;
                while let Some(at) = parent {
                    let idx = at.get() as usize;
                    let pev = events[idx];
                    events[idx] = tombstone;
                    match pev {
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
                        attach_trivia(raw, &mut stack, &mut next);
                    }
                    stack.push(Node {
                        kind,
                        children: Vec::new(),
                    });
                }
            }
            Event::Token(kind) => {
                attach_trivia(raw, &mut stack, &mut next);
                let t = raw[next];
                stack
                    .last_mut()
                    .expect("token inside a node")
                    .children
                    .push(Element::Token(Token {
                        kind: kind.unwrap_or(t.kind),
                        start: t.start,
                        end: t.end,
                    }));
                next += 1;
            }
            Event::Finish => {
                if stack.len() == 1 {
                    attach_trivia(raw, &mut stack, &mut next);
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
