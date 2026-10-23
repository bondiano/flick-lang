//! Breaking lines longer than [`WIDTH`]. A long line is split recursively at
//! its outermost structure, in this order:
//!
//! 1. after the `=` of a `let` or record field, or the `->` of a `fun` or
//!    match arm: the body moves to the next line, two spaces deeper;
//! 2. an `if` chain at the start of the line: branches under `then`/`else`;
//! 3. before the operators of the weakest precedence present;
//! 4. after the arrows of a type (after `->{r; e}` as a whole);
//! 5. before each argument of a function application;
//! 6. inside the widest bracket pair: one item per line, closer dedented.
//!
//! Every continuation is two spaces deeper than the piece it continues. A
//! wrapped line is kept only if the whole file still parses to the same AST;
//! otherwise it stays long.

use std::mem;

use super::{K, Line, Tok, join, space};

pub const WIDTH: usize = 100;

type Piece = (usize, Vec<Tok>);

pub(super) fn wrap_long_lines(
    text: &str,
    lines: &[Option<Line>],
    same_program: impl Fn(&str) -> bool,
) -> String {
    let mut rendered: Vec<String> = lines
        .iter()
        .map(|l| {
            l.as_ref()
                .map_or(String::new(), |l| join(text, l.indent, &l.toks))
        })
        .collect();
    for (i, line) in lines.iter().enumerate() {
        let Some(line) = line else { continue };
        let multiline = line.toks.iter().any(|t| t.token.text(text).contains('\n'));
        if multiline || rendered[i].chars().count() <= WIDTH {
            continue;
        }
        let pieces = split(text, line.indent, &line.toks, false);
        if pieces.len() < 2 {
            continue;
        }
        let wrapped: Vec<String> = pieces.iter().map(|(n, t)| join(text, *n, t)).collect();
        let long = mem::replace(&mut rendered[i], wrapped.join("\n"));
        if !same_program(&assemble(&rendered)) {
            rendered[i] = long;
        }
    }
    assemble(&rendered)
}

fn assemble(lines: &[String]) -> String {
    lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect()
}

fn width(text: &str, toks: &[Tok]) -> usize {
    toks.iter()
        .enumerate()
        .map(|(k, &t)| {
            t.token.text(text).chars().count() + usize::from(k > 0 && space(toks[k - 1], t))
        })
        .sum()
}

/// Bracket depth of each token relative to the start of the line; an opener
/// and its closer are at the depth outside them.
fn depths(toks: &[Tok]) -> Vec<i32> {
    let mut depth = 0;
    toks.iter()
        .map(|t| {
            if t.token.kind.is_closer() {
                depth -= 1;
            }
            let here = depth;
            if matches!(t.token.kind, K::LParen | K::LBracket | K::LBrace) {
                depth += 1;
            }
            here
        })
        .collect()
}

fn precedence(kind: K) -> u8 {
    match kind {
        K::PipeGt => 1,
        K::OrOr => 2,
        K::AndAnd => 3,
        K::EqEq | K::NotEq | K::Lt | K::Le | K::Gt | K::Ge => 4,
        K::PlusPlus => 5,
        K::Plus | K::Minus => 6,
        K::Star | K::Slash | K::Percent => 7,
        _ => 8,
    }
}

/// `block_start`: the piece is the first line of a block on its own line, so
/// a continuation may start at its column with `|>`.
fn split(text: &str, indent: usize, toks: &[Tok], block_start: bool) -> Vec<Piece> {
    if indent + width(text, toks) <= WIDTH || toks.len() < 2 {
        return vec![(indent, toks.to_vec())];
    }
    let line = Long {
        text,
        indent,
        toks,
        depth: depths(toks),
        block_start,
    };
    line.body()
        .or_else(|| line.if_chain())
        .or_else(|| line.operators())
        .or_else(|| line.arrows())
        .or_else(|| line.arguments())
        .or_else(|| line.brackets())
        .unwrap_or_else(|| vec![(indent, toks.to_vec())])
}

/// A line that does not fit, and the strategies that can split it.
struct Long<'a> {
    text: &'a str,
    indent: usize,
    toks: &'a [Tok],
    depth: Vec<i32>,
    block_start: bool,
}

impl Long<'_> {
    fn top(&self, i: usize) -> bool {
        self.depth[i] == 0
    }

    fn deeper(&self) -> usize {
        self.indent + 2
    }

    fn recurse(&self, pieces: Vec<Piece>) -> Vec<Piece> {
        pieces
            .into_iter()
            .flat_map(|(n, t)| split(self.text, n, &t, false))
            .collect()
    }

    /// The body after the `=` of a `let` or field, or the `->` of a `fun` or arm.
    fn body(&self) -> Option<Vec<Piece>> {
        let toks = self.toks;
        let opens_body = |t: Tok| match t.token.kind {
            K::Eq => matches!(t.parent, K::LetDecl | K::RecordField),
            K::Arrow => matches!(t.parent, K::FunExpr | K::MatchArm),
            _ => false,
        };
        let i = (0..toks.len() - 1).find(|&i| {
            self.top(i) && opens_body(toks[i]) && !toks[i + 1].token.kind.is_comment()
        })?;
        let mut pieces = split(self.text, self.indent, &toks[..=i], false);
        pieces.extend(split(self.text, self.deeper(), &toks[i + 1..], true));
        Some(pieces)
    }

    /// `if c then a else b` starting the line: a line per head, branches deeper.
    fn if_chain(&self) -> Option<Vec<Piece>> {
        let toks = self.toks;
        if toks[0].token.kind != K::IfKw {
            return None;
        }
        let mut pieces = Vec::new();
        let mut start = 0;
        for (i, t) in toks.iter().enumerate() {
            if !self.top(i) || t.parent != K::IfExpr {
                continue;
            }
            let kind = t.token.kind;
            let else_if =
                kind == K::ElseKw && toks.get(i + 1).is_some_and(|n| n.token.kind == K::IfKw);
            if kind == K::ElseKw && start < i {
                pieces.push((self.deeper(), toks[start..i].to_vec()));
                start = i;
            }
            if kind == K::ThenKw || (kind == K::ElseKw && !else_if) {
                pieces.push((self.indent, toks[start..=i].to_vec()));
                start = i + 1;
            }
        }
        if start >= toks.len() || pieces.is_empty() {
            return None;
        }
        pieces.push((self.deeper(), toks[start..].to_vec()));
        Some(self.recurse(pieces))
    }

    /// Before each operator of the weakest precedence on the line.
    fn operators(&self) -> Option<Vec<Piece>> {
        let toks = self.toks;
        let operator = |i: usize| {
            self.top(i) && toks[i].parent == K::BinExpr && toks[i].token.kind.is_binary_op()
        };
        let weakest = (1..toks.len())
            .filter(|&i| operator(i))
            .map(|i| precedence(toks[i].token.kind))
            .min()?;
        let cuts: Vec<usize> = (1..toks.len())
            .filter(|&i| operator(i) && precedence(toks[i].token.kind) == weakest)
            .collect();
        // `|>` lines up with the start of its block, as in `xs\n|> f`
        let pipes = weakest == precedence(K::PipeGt) && self.block_start;
        let continuation = if pipes { self.indent } else { self.deeper() };
        Some(self.recurse(pieces_at(toks, &cuts, self.indent, continuation)))
    }

    /// After each arrow of a type, keeping `->{r; e}` together.
    fn arrows(&self) -> Option<Vec<Piece>> {
        let toks = self.toks;
        let mut cuts = Vec::new();
        for i in 0..toks.len() - 1 {
            if !(self.top(i) && toks[i].parent == K::ArrowType && toks[i].token.kind == K::Arrow) {
                continue;
            }
            let effect =
                toks[i + 1].parent == K::EffectAnnot && toks[i + 1].token.kind == K::LBrace;
            let end = if effect {
                (i + 1..toks.len()).find(|&j| toks[j].token.kind == K::RBrace && self.top(j))
            } else {
                Some(i)
            };
            cuts.extend(end.filter(|&e| e + 1 < toks.len()).map(|e| e + 1));
        }
        (!cuts.is_empty()).then(|| self.recurse(pieces_at(toks, &cuts, self.indent, self.deeper())))
    }

    /// Before each argument of a function application.
    fn arguments(&self) -> Option<Vec<Piece>> {
        let cuts: Vec<usize> = (1..self.toks.len())
            .filter(|&i| self.top(i) && self.toks[i].arg)
            .collect();
        (!cuts.is_empty())
            .then(|| self.recurse(pieces_at(self.toks, &cuts, self.indent, self.deeper())))
    }

    /// Inside the widest bracket pair: opener, one item per line, closer. A
    /// single expression in parentheses stays hugged by them: `(f\n  a\n  b)`.
    fn brackets(&self) -> Option<Vec<Piece>> {
        let toks = self.toks;
        let opener = |i: usize| matches!(toks[i].token.kind, K::LParen | K::LBracket | K::LBrace);
        let (open, close) = (0..toks.len())
            .filter(|&i| self.top(i) && opener(i))
            .filter_map(|open| {
                let close = (open + 1..toks.len())
                    .find(|&j| self.top(j) && toks[j].token.kind.is_closer())?;
                (close > open + 1).then_some((open, close))
            })
            .max_by_key(|(open, close)| close - open)?;
        let separator =
            |i: usize| self.depth[i] == 1 && matches!(toks[i].token.kind, K::Comma | K::Semi);
        let single = !(open + 1..close).any(separator);
        if single && toks[open].token.kind == K::LParen {
            let mut pieces = split(self.text, self.indent, &toks[open + 1..close], false);
            if pieces.len() > 1 {
                let mut first = toks[..=open].to_vec();
                first.append(&mut pieces[0].1);
                pieces[0] = (self.indent, first);
                pieces.last_mut()?.1.extend_from_slice(&toks[close..]);
                // the hugging brackets may push a piece past the width again
                return Some(self.recurse(pieces));
            }
        }
        let mut pieces = vec![(self.indent, toks[..=open].to_vec())];
        let mut start = open + 1;
        for i in open + 1..close {
            if separator(i) {
                pieces.push((self.deeper(), toks[start..=i].to_vec()));
                start = i + 1;
            }
        }
        if start < close {
            pieces.push((self.deeper(), toks[start..close].to_vec()));
        }
        pieces.push((self.indent, toks[close..].to_vec()));
        Some(self.recurse(pieces))
    }
}

/// Pieces starting at each cut; all but the first are continuations.
fn pieces_at(toks: &[Tok], cuts: &[usize], indent: usize, deeper: usize) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut start = 0;
    for &cut in cuts {
        pieces.push((
            if start == 0 { indent } else { deeper },
            toks[start..cut].to_vec(),
        ));
        start = cut;
    }
    pieces.push((deeper, toks[start..].to_vec()));
    pieces
}
