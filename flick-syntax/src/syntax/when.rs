//! Environment condition expressions for `@when`, `[package] when`, and
//! `[host] requires`.
//!
//! Grammar:
//! ```text
//! expr := or
//! or   := and ("||" and)*
//! and  := not ("&&" not)*
//! not  := "!" not | atom
//! atom := "(" expr ")" | tag
//! ```
//!
//! Tag names are lowercase ASCII identifiers `[a-z][a-z0-9_-]*` (for example
//! `js`, `go`, `rt`, `effect`, `effect-adapter`, `node`, `browser`, `luau`).

use std::{fmt, mem};

use super::cst::Node;
use super::kind::SyntaxKind as K;
use crate::source::Span;

/// Maximum nesting depth for parentheses and `!` operators in a `when`
/// condition, matching the main Flick parser's `MAX_DEPTH`.
const MAX_DEPTH: usize = 200;

/// The attribute name for module environment conditions.
pub const TAG: &str = "when";

/// A parsed `@when "<condition>"` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenAttr {
    pub condition: String,
    /// Byte range of the string literal token in source.
    pub literal_range: (u32, u32),
}

/// A parsed boolean condition over environment tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Tag {
        name: String,
        /// Byte range of the tag name within the condition string.
        range: (usize, usize),
    },
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

/// A syntax error in a `when` condition string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Byte range within the condition string.
    pub range: (usize, usize),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Expr {
    /// Evaluates this condition against the set of active tags without
    /// recursion, preserving short-circuiting left-to-right semantics.
    pub fn eval(&self, active: &impl Fn(&str) -> bool) -> bool {
        enum Frame<'a> {
            Not,
            AndRhs(&'a Expr),
            OrRhs(&'a Expr),
        }

        let mut cur = self;
        let mut stack: Vec<Frame<'_>> = Vec::new();
        'eval: loop {
            let mut val = match cur {
                Self::Tag { name, .. } => active(name),
                Self::Not(inner) => {
                    stack.push(Frame::Not);
                    cur = inner;
                    continue;
                }
                Self::And(left, right) => {
                    stack.push(Frame::AndRhs(right));
                    cur = left;
                    continue;
                }
                Self::Or(left, right) => {
                    stack.push(Frame::OrRhs(right));
                    cur = left;
                    continue;
                }
            };
            while let Some(frame) = stack.pop() {
                match frame {
                    Frame::Not => val = !val,
                    Frame::AndRhs(rhs) if val => {
                        cur = rhs;
                        continue 'eval;
                    }
                    Frame::AndRhs(_) => val = false,
                    Frame::OrRhs(rhs) if !val => {
                        cur = rhs;
                        continue 'eval;
                    }
                    Frame::OrRhs(_) => val = true,
                }
            }
            return val;
        }
    }

    /// Every tag mentioned in this condition, in source order, with its byte
    /// range inside the condition string.
    pub fn tags(&self) -> Vec<(&str, (usize, usize))> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(expr) = stack.pop() {
            match expr {
                Self::Tag { name, range } => out.push((name.as_str(), *range)),
                Self::Not(inner) => stack.push(inner),
                Self::And(left, right) | Self::Or(left, right) => {
                    stack.push(right);
                    stack.push(left);
                }
            }
        }
        out
    }
}

impl Drop for Expr {
    fn drop(&mut self) {
        fn take_children(node: &mut Expr, stack: &mut Vec<Expr>) {
            let empty = || Expr::Tag {
                name: String::new(),
                range: (0, 0),
            };
            match node {
                Expr::Tag { .. } => {}
                Expr::Not(inner) => stack.push(mem::replace(inner.as_mut(), empty())),
                Expr::And(left, right) | Expr::Or(left, right) => {
                    stack.push(mem::replace(left.as_mut(), empty()));
                    stack.push(mem::replace(right.as_mut(), empty()));
                }
            }
        }
        let mut stack = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut node) = stack.pop() {
            take_children(&mut node, &mut stack);
        }
    }
}

/// Whether `name` is a valid environment tag (`[a-z][a-z0-9_-]*`).
pub fn is_valid_tag(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// Whether `node` is a `@when` attribute (regardless of whether its arguments
/// are valid).
pub fn is_when(node: &Node, text: &str) -> bool {
    node.child_tokens()
        .find(|t| t.kind == K::Ident)
        .is_some_and(|t| t.text(text) == TAG)
}

const USAGE: &str = "`@when` takes one condition string, as `@when \"js && !browser\"`";

/// Reads a `@when "<cond>"` attribute node; returns `None` if `node` is not a
/// `@when` attribute, or `Some(Err(..))` if its argument is missing, malformed,
/// or fails to parse as a condition.
pub fn read(node: &Node, text: &str) -> Option<Result<WhenAttr, String>> {
    let mut tokens = node
        .child_tokens()
        .filter(|t| matches!(t.kind, K::Ident | K::String));
    let head = tokens.next()?;
    if head.kind != K::Ident || head.text(text) != TAG {
        return None;
    }
    let rest: Vec<_> = tokens.collect();
    let [arg] = rest.as_slice() else {
        return Some(Err(USAGE.to_owned()));
    };
    if arg.kind != K::String {
        return Some(Err(USAGE.to_owned()));
    }
    let Some(condition) = plain_string(arg.text(text)) else {
        return Some(Err(USAGE.to_owned()));
    };
    if let Err(err) = parse(&condition) {
        return Some(Err(format!("invalid `@when` condition: {err}")));
    }
    Some(Ok(WhenAttr {
        condition,
        literal_range: (arg.start, arg.end),
    }))
}

fn plain_string(literal: &str) -> Option<String> {
    let inner = literal.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains(['\\', '\n', '"'])).then(|| inner.to_owned())
}

/// Maps a byte range inside a parsed condition string back to a span inside
/// `literal` when `literal` is a single-line plain quoted string in `source`,
/// falling back to `literal` otherwise.
pub fn subspan(source: &str, literal: Span, range: (usize, usize)) -> Span {
    let Some(raw) = source.get(literal.range()) else {
        return literal;
    };
    if raw.starts_with("\"\"\"") || raw.starts_with("'''") {
        return literal;
    }
    let quoted = (raw.starts_with('"') && raw.ends_with('"') && !raw.contains(['\\', '\n']))
        || (raw.starts_with('\'') && raw.ends_with('\'') && !raw.contains('\n'));
    if !quoted || raw.len() < 2 {
        return literal;
    }
    let inner_len = raw.len() - 2;
    if range.0 > range.1 || range.1 > inner_len {
        return literal;
    }
    let Ok(start_off) = u32::try_from(range.0 + 1) else {
        return literal;
    };
    let Ok(end_off) = u32::try_from(range.1 + 1) else {
        return literal;
    };
    Span::new(
        literal.file,
        literal.start + start_off,
        literal.start + end_off,
    )
}

/// Parses a `when` condition string.
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(ParseError {
            message: "condition cannot be empty".to_owned(),
            range: (0, input.len()),
        });
    }
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        end: input.len(),
        depth: 0,
    };
    let expr = parser.parse_or()?;
    if let Some(extra) = parser.peek() {
        return Err(ParseError {
            message: format!("unexpected {} in condition", extra.kind.describe()),
            range: extra.range,
        });
    }
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokKind<'a> {
    Tag(&'a str),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

impl TokKind<'_> {
    fn describe(&self) -> String {
        match self {
            Self::Tag(name) => format!("tag `{name}`"),
            Self::And => "`&&`".to_owned(),
            Self::Or => "`||`".to_owned(),
            Self::Not => "`!`".to_owned(),
            Self::LParen => "`(`".to_owned(),
            Self::RParen => "`)`".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
struct Tok<'a> {
    kind: TokKind<'a>,
    range: (usize, usize),
}

fn lex(input: &str) -> Result<Vec<Tok<'_>>, ParseError> {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match b {
            b'(' => {
                out.push(Tok {
                    kind: TokKind::LParen,
                    range: (i, i + 1),
                });
                i += 1;
            }
            b')' => {
                out.push(Tok {
                    kind: TokKind::RParen,
                    range: (i, i + 1),
                });
                i += 1;
            }
            b'!' => {
                out.push(Tok {
                    kind: TokKind::Not,
                    range: (i, i + 1),
                });
                i += 1;
            }
            b'&' => {
                if bytes.get(i + 1) == Some(&b'&') {
                    out.push(Tok {
                        kind: TokKind::And,
                        range: (i, i + 2),
                    });
                    i += 2;
                } else {
                    return Err(ParseError {
                        message: "expected `&&` instead of `&`".to_owned(),
                        range: (i, i + 1),
                    });
                }
            }
            b'|' => {
                if bytes.get(i + 1) == Some(&b'|') {
                    out.push(Tok {
                        kind: TokKind::Or,
                        range: (i, i + 2),
                    });
                    i += 2;
                } else {
                    return Err(ParseError {
                        message: "expected `||` instead of `|`".to_owned(),
                        range: (i, i + 1),
                    });
                }
            }
            b if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_') => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_'))
                {
                    i += 1;
                }
                let word = &input[start..i];
                if !is_valid_tag(word) {
                    return Err(ParseError {
                        message: format!(
                            "`{word}` is not a valid environment tag: tags start with a lowercase letter and use lowercase letters, digits, `-`, or `_`"
                        ),
                        range: (start, i),
                    });
                }
                out.push(Tok {
                    kind: TokKind::Tag(word),
                    range: (start, i),
                });
            }
            _ => {
                let ch = input[i..].chars().next().unwrap_or('?');
                let end = i + ch.len_utf8();
                return Err(ParseError {
                    message: format!("unexpected character `{ch}` in condition"),
                    range: (i, end),
                });
            }
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Tok<'a>],
    pos: usize,
    end: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Tok<'a>> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<&'a Tok<'a>> {
        let tok = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(tok)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and()?;
        while self.peek().is_some_and(|t| matches!(t.kind, TokKind::Or)) {
            self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_not()?;
        while self.peek().is_some_and(|t| matches!(t.kind, TokKind::And)) {
            self.bump();
            let rhs = self.parse_not()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        let mut negations = 0usize;
        while let Some(tok) = self
            .peek()
            .filter(|t| matches!(t.kind, TokKind::Not))
            .cloned()
        {
            self.bump();
            if self.depth + negations >= MAX_DEPTH {
                return Err(ParseError {
                    message: format!("condition nesting exceeds maximum depth of {MAX_DEPTH}"),
                    range: tok.range,
                });
            }
            negations += 1;
        }
        let saved = self.depth;
        self.depth += negations;
        let mut expr = self.parse_atom()?;
        self.depth = saved;
        for _ in 0..negations {
            expr = Expr::Not(Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let Some(tok) = self.bump() else {
            return Err(ParseError {
                message: "expected an environment tag, `!`, or `(`".to_owned(),
                range: (self.end, self.end),
            });
        };
        match tok.kind {
            TokKind::Tag(name) => Ok(Expr::Tag {
                name: name.to_owned(),
                range: tok.range,
            }),
            TokKind::LParen => {
                let open = tok.range;
                if self.depth >= MAX_DEPTH {
                    return Err(ParseError {
                        message: format!("condition nesting exceeds maximum depth of {MAX_DEPTH}"),
                        range: open,
                    });
                }
                self.depth += 1;
                let inner = self.parse_or();
                self.depth -= 1;
                let inner = inner?;
                match self.bump() {
                    Some(Tok {
                        kind: TokKind::RParen,
                        ..
                    }) => Ok(inner),
                    Some(other) => Err(ParseError {
                        message: format!(
                            "expected `)` to close `(`, found {}",
                            other.kind.describe()
                        ),
                        range: other.range,
                    }),
                    None => Err(ParseError {
                        message: "unclosed `(` in condition".to_owned(),
                        range: open,
                    }),
                }
            }
            _ => Err(ParseError {
                message: format!(
                    "expected an environment tag, `!`, or `(`, found {}",
                    tok.kind.describe()
                ),
                range: tok.range,
            }),
        }
    }
}

#[cfg(test)]
#[path = "when_tests.rs"]
mod tests;
