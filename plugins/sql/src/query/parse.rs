//! The clauses of `sql [params] { … }` read from the tokens of the block:
//! a clause starts at its word, `from x in T` or `where e`, and runs to the
//! next word of a clause or `;` outside brackets; its expressions are the
//! Flick a query writes, parsed with the binding powers of Flick's own
//! operators, so the SQL of each is the one its tree gives.

use flick_plugin::{Block, Diagnostic, Span, Token, TokenKind};

use crate::table::MALFORMED;

/// The words that start a clause.
pub const CLAUSES: [&str; 19] = [
    "from",
    "join",
    "left_join",
    "on",
    "where",
    "group_by",
    "having",
    "order_by",
    "limit",
    "offset",
    "select",
    "union",
    "union_all",
    "insert_into",
    "values",
    "update",
    "set",
    "delete_from",
    "returning",
];

/// A token of the body: its text and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    pub text: String,
    pub span: Span,
    pub kind: TokenKind,
}

/// A clause: its word, the alias of `from x in T`, and its expressions.
#[derive(Debug, Clone)]
pub struct Clause {
    pub word: Tok,
    pub alias: Option<Tok>,
    pub items: Vec<E>,
    pub span: Span,
}

/// A query: its clauses, the name of its parameters, and where it is.
#[derive(Debug, Clone)]
pub struct Query {
    pub binder: Option<Tok>,
    pub clauses: Vec<Clause>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Op {
    fn of(text: &str) -> Option<Op> {
        Some(match text {
            "||" => Op::Or,
            "&&" => Op::And,
            "==" => Op::Eq,
            "!=" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "++" => Op::Concat,
            "+" => Op::Add,
            "-" => Op::Sub,
            "*" => Op::Mul,
            "/" => Op::Div,
            "%" => Op::Rem,
            _ => return None,
        })
    }

    /// The binding powers of Flick's table: `||`, `&&` and `++` group to
    /// the right, the others to the left.
    fn powers(self) -> (u8, u8) {
        match self {
            Op::Or => (4, 4),
            Op::And => (6, 6),
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => (8, 9),
            Op::Concat => (10, 10),
            Op::Add | Op::Sub => (12, 13),
            Op::Mul | Op::Div | Op::Rem => (14, 15),
        }
    }

    pub fn comparison(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge)
    }

    pub fn text(self) -> &'static str {
        match self {
            Op::Or => "||",
            Op::And => "&&",
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Concat => "++",
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Rem => "%",
        }
    }
}

/// The operand of a prefix operator binds tighter than `*`.
const PREFIX: u8 = 18;

/// An expression of a clause.
#[derive(Debug, Clone)]
pub enum E {
    /// `name`, `alias.column`, `params.x`, `Users.column`.
    Path(Vec<Tok>, Span),
    /// `$name`, `$name.field`, `$(e)`: the Flick expression after `$`.
    Capture {
        inner: Span,
        span: Span,
    },
    Literal(Tok),
    Paren(Box<E>, Span),
    Apply(Tok, Vec<E>, Span),
    Not(Box<E>, Span),
    Neg(Box<E>, Span),
    Binary(Op, Box<E>, Box<E>, Span),
    Record(Vec<(Tok, E)>, Span),
    Sub(Box<Query>),
}

impl E {
    pub fn span(&self) -> Span {
        match self {
            E::Path(_, s)
            | E::Capture { span: s, .. }
            | E::Paren(_, s)
            | E::Apply(_, _, s)
            | E::Not(_, s)
            | E::Neg(_, s)
            | E::Binary(_, _, _, s)
            | E::Record(_, s) => *s,
            E::Literal(t) => t.span,
            E::Sub(q) => q.span,
        }
    }
}

/// A parse that failed: what is wrong, where.
pub type Wrong = Diagnostic;

fn wrong(message: impl Into<String>, span: Span) -> Wrong {
    Diagnostic::error(message, span).with_code(MALFORMED)
}

fn join(a: Span, b: Span) -> Span {
    Span {
        start: a.start,
        end: b.end,
    }
}

/// The tokens of `tokens`, with their texts.
pub fn tokens(block: &Block, tokens: &[Token]) -> Vec<Tok> {
    (tokens.iter())
        .map(|t| Tok {
            text: block.text_of(t.span).to_owned(),
            span: t.span,
            kind: t.kind,
        })
        .collect()
}

/// The query of a block: `binder` names its parameters, `body` is the
/// inside of its braces, `whole` the block.
pub fn query(binder: Option<Tok>, body: &[Tok], whole: Span) -> Result<Query, Wrong> {
    let closing = closing(body)?;
    let mut p = Parser {
        toks: body,
        closing: &closing,
    };
    p.clauses(binder, 0, body.len(), whole)
}

/// The index of the closing bracket of each opening one.
fn closing(toks: &[Tok]) -> Result<Vec<usize>, Wrong> {
    let mut out = vec![usize::MAX; toks.len()];
    let mut open: Vec<usize> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        match t.text.as_str() {
            "(" | "[" | "{" => open.push(i),
            ")" | "]" | "}" => {
                let Some(o) = open.pop() else {
                    return Err(wrong(format!("`{}` closes nothing", t.text), t.span));
                };
                out[o] = i;
            }
            _ => {}
        }
    }
    match open.pop() {
        Some(o) => Err(wrong(
            format!("`{}` is not closed", toks[o].text),
            toks[o].span,
        )),
        None => Ok(out),
    }
}

struct Parser<'t> {
    toks: &'t [Tok],
    closing: &'t [usize],
}

/// Whether `t` starts a clause.
fn at_clause(t: &Tok) -> bool {
    matches!(t.kind, TokenKind::Ident | TokenKind::Keyword) && CLAUSES.contains(&t.text.as_str())
}

impl Parser<'_> {
    /// The clauses of the tokens `start..end`.
    fn clauses(
        &mut self,
        binder: Option<Tok>,
        start: usize,
        end: usize,
        whole: Span,
    ) -> Result<Query, Wrong> {
        let mut clauses = Vec::new();
        let mut i = start;
        while i < end {
            let t = &self.toks[i];
            if t.text == ";" {
                i += 1;
                continue;
            }
            if !at_clause(t) {
                let message = "expected a SQL clause (`from`, `where`, `select`, …)";
                return Err(wrong(message, t.span));
            }
            let mut j = i + 1;
            while j < end && !self.starts_clause(j) && self.toks[j].text != ";" {
                j = self.skip(j);
            }
            clauses.push(self.clause(i, j)?);
            i = j;
        }
        Ok(Query {
            binder,
            clauses,
            span: whole,
        })
    }

    /// Whether a clause starts at `i`: its word, not a name right after
    /// `$` or `.`, as in `limit $limit` and `t.offset`.
    fn starts_clause(&self, i: usize) -> bool {
        let t = &self.toks[i];
        let named = i
            .checked_sub(1)
            .map(|j| &self.toks[j])
            .is_some_and(|p| matches!(p.text.as_str(), "$" | ".") && p.span.end == t.span.start);
        at_clause(t) && !named
    }

    /// The index after the token at `i`, past its brackets.
    fn skip(&self, i: usize) -> usize {
        match self.closing[i] {
            usize::MAX => i + 1,
            close => close + 1,
        }
    }

    /// The clause of the tokens `start..end`, its word first.
    fn clause(&mut self, start: usize, end: usize) -> Result<Clause, Wrong> {
        let word = self.toks[start].clone();
        let last = self.toks[end - 1].span;
        let span = join(word.span, last);
        let mut at = start + 1;
        let mut alias = None;
        if matches!(word.text.as_str(), "from" | "join" | "left_join") {
            let named = (self.toks.get(at)).filter(|t| t.kind == TokenKind::Ident && at < end);
            let Some(name) = named else {
                return Err(wrong(
                    format!("`{}` names an alias: `{} x in T`", word.text, word.text),
                    span,
                ));
            };
            alias = Some(name.clone());
            at += 1;
            if self
                .toks
                .get(at)
                .filter(|_| at < end)
                .is_none_or(|t| t.text != "in")
            {
                return Err(wrong(
                    format!("expected `in` after `{} {}`", word.text, name.text),
                    span,
                ));
            }
            at += 1;
        }
        let mut items = Vec::new();
        while at < end {
            let (e, next) = self.expr(at, end, 0)?;
            items.push(e);
            at = next;
            if at < end {
                if self.toks[at].text != "," {
                    return Err(wrong(
                        "expected `,` or the end of the clause",
                        self.toks[at].span,
                    ));
                }
                at += 1;
            }
        }
        if items.is_empty() {
            return Err(wrong(
                format!("`{}` needs an expression", word.text),
                word.span,
            ));
        }
        Ok(Clause {
            word,
            alias,
            items,
            span,
        })
    }

    /// The expression from `at` on, before `end`, its operators binding
    /// at least `min`; and where it ends.
    fn expr(&mut self, at: usize, end: usize, min: u8) -> Result<(E, usize), Wrong> {
        let (mut lhs, mut at) = self.prefix(at, end)?;
        while at < end {
            let t = &self.toks[at];
            let Some(op) = (t.kind == TokenKind::Punct)
                .then(|| Op::of(&t.text))
                .flatten()
            else {
                break;
            };
            let (left, right) = op.powers();
            if left < min {
                break;
            }
            let (rhs, next) = self.expr(at + 1, end, right)?;
            if op.comparison()
                && let Some(n) = self.toks.get(next).filter(|_| next < end)
                && Op::of(&n.text).is_some_and(Op::comparison)
            {
                let message =
                    "comparison operators do not chain; combine the comparisons with `&&`";
                return Err(wrong(message, n.span));
            }
            let span = join(lhs.span(), rhs.span());
            lhs = E::Binary(op, Box::new(lhs), Box::new(rhs), span);
            at = next;
        }
        Ok((lhs, at))
    }

    fn prefix(&mut self, at: usize, end: usize) -> Result<(E, usize), Wrong> {
        let Some(t) = self.toks.get(at).filter(|_| at < end) else {
            let near = self
                .toks
                .get(at.saturating_sub(1))
                .map_or(Span { start: 0, end: 0 }, |t| t.span);
            return Err(wrong("expected an expression", near));
        };
        if t.text == "not" || t.text == "-" {
            let (operand, next) = self.expr(at + 1, end, PREFIX)?;
            let span = join(t.span, operand.span());
            let e = if t.text == "not" {
                E::Not(Box::new(operand), span)
            } else {
                E::Neg(Box::new(operand), span)
            };
            return Ok((e, next));
        }
        self.application(at, end)
    }

    /// An atom, applied to the atoms after it when it is a name.
    fn application(&mut self, at: usize, end: usize) -> Result<(E, usize), Wrong> {
        let (head, mut next) = self.atom(at, end)?;
        let E::Path(words, _) = &head else {
            return Ok((head, next));
        };
        let [name] = &words[..] else {
            return Ok((head, next));
        };
        if name.kind != TokenKind::Ident {
            return Ok((head, next));
        }
        let name = name.clone();
        let mut args = Vec::new();
        while next < end && self.at_atom(next) {
            let (arg, after) = self.atom(next, end)?;
            args.push(arg);
            next = after;
        }
        if args.is_empty() {
            return Ok((head, next));
        }
        let span = join(name.span, args.last().map_or(name.span, E::span));
        Ok((E::Apply(name, args, span), next))
    }

    fn at_atom(&self, at: usize) -> bool {
        let t = &self.toks[at];
        match t.kind {
            TokenKind::Ident => !at_clause(t),
            TokenKind::Con | TokenKind::Number | TokenKind::String => true,
            TokenKind::Keyword => matches!(t.text.as_str(), "true" | "false" | "sql"),
            TokenKind::Punct => matches!(t.text.as_str(), "(" | "$" | "{"),
        }
    }

    fn atom(&mut self, at: usize, end: usize) -> Result<(E, usize), Wrong> {
        let t = &self.toks[at];
        match (t.kind, t.text.as_str()) {
            (TokenKind::Ident | TokenKind::Con | TokenKind::Keyword, "sql") => self.sub(at, end),
            (TokenKind::Ident | TokenKind::Con, _) => Ok(self.path(at, end)),
            (TokenKind::Number | TokenKind::String, _) | (TokenKind::Keyword, "true" | "false") => {
                Ok((E::Literal(t.clone()), at + 1))
            }
            (TokenKind::Punct, "(") => {
                let close = self.closing[at];
                let (inner, after) = self.expr(at + 1, close, 0)?;
                if after != close {
                    return Err(wrong("expected `)`", self.toks[after].span));
                }
                let span = join(t.span, self.toks[close].span);
                Ok((E::Paren(Box::new(inner), span), close + 1))
            }
            (TokenKind::Punct, "$") => self.capture(at, end),
            (TokenKind::Punct, "{") => self.record(at),
            _ => {
                let message = "this is not a SQL expression: use columns, parameters, literals, operators and SQL functions";
                Err(wrong(message, t.span))
            }
        }
    }

    /// `name`, `a.b`, `Users.id`: names joined by `.` right after them.
    fn path(&self, at: usize, end: usize) -> (E, usize) {
        let mut words = vec![self.toks[at].clone()];
        let mut i = at + 1;
        while i + 1 < end
            && self.toks[i].text == "."
            && self.toks[i].span.start == words.last().map_or(0, |w| w.span.end)
            && matches!(self.toks[i + 1].kind, TokenKind::Ident | TokenKind::Con)
        {
            words.push(self.toks[i + 1].clone());
            i += 2;
        }
        let span = join(
            words[0].span,
            words.last().map_or(words[0].span, |w| w.span),
        );
        (E::Path(words, span), i)
    }

    /// `$name`, `$name.field` or `$(e)`, the Flick expression right after
    /// `$`.
    fn capture(&self, at: usize, end: usize) -> Result<(E, usize), Wrong> {
        let dollar = &self.toks[at];
        let next = self
            .toks
            .get(at + 1)
            .filter(|n| at + 1 < end && n.span.start == dollar.span.end);
        let Some(next) = next else {
            return Err(wrong("expected a name or `(` right after `$`", dollar.span));
        };
        let after = match (next.kind, next.text.as_str()) {
            (TokenKind::Punct, "(") => self.closing[at + 1] + 1,
            (TokenKind::Ident | TokenKind::Con, _) => self.path(at + 1, end).1,
            _ => return Err(wrong("expected a name or `(` right after `$`", dollar.span)),
        };
        let inner = join(next.span, self.toks[after - 1].span);
        Ok((
            E::Capture {
                inner,
                span: join(dollar.span, inner),
            },
            after,
        ))
    }

    /// `{ label = e; … }`: a field starts at `label =`.
    fn record(&mut self, at: usize) -> Result<(E, usize), Wrong> {
        let close = self.closing[at];
        let mut fields = Vec::new();
        let mut i = at + 1;
        while i < close {
            if self.toks[i].text == ";" {
                i += 1;
                continue;
            }
            let label = &self.toks[i];
            let eq = self.toks.get(i + 1).filter(|_| i + 1 < close);
            if label.kind != TokenKind::Ident || eq.is_none_or(|e| e.text != "=") {
                return Err(wrong("expected a field `label = value`", label.span));
            }
            let mut j = i + 2;
            while j < close && self.toks[j].text != ";" && !self.field_start(j, close) {
                j = self.skip(j);
            }
            let (value, after) = self.expr(i + 2, j, 0)?;
            if after != j {
                return Err(wrong(
                    "expected `;` or the end of the record",
                    self.toks[after].span,
                ));
            }
            fields.push((label.clone(), value));
            i = j;
        }
        let span = join(self.toks[at].span, self.toks[close].span);
        Ok((E::Record(fields, span), close + 1))
    }

    /// Whether a field `label = …` starts at `i`, before `close`.
    fn field_start(&self, i: usize, close: usize) -> bool {
        self.toks[i].kind == TokenKind::Ident && i + 1 < close && self.toks[i + 1].text == "="
    }

    /// `sql { … }` or `sql name { … }` in a query: a subquery.
    fn sub(&mut self, at: usize, end: usize) -> Result<(E, usize), Wrong> {
        let mut i = at + 1;
        let binder = (self.toks.get(i))
            .filter(|t| i < end && t.kind == TokenKind::Ident)
            .cloned();
        if binder.is_some() {
            i += 1;
        }
        if self
            .toks
            .get(i)
            .filter(|_| i < end)
            .is_none_or(|t| t.text != "{")
        {
            return Err(wrong("expected `{` after `sql`", self.toks[at].span));
        }
        let close = self.closing[i];
        let span = join(self.toks[at].span, self.toks[close].span);
        let query = self.clauses(binder, i + 1, close, span)?;
        Ok((E::Sub(Box::new(query)), close + 1))
    }
}
