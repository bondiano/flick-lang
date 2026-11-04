//! The types of a declaration, read from its tokens: as much of Flick's
//! types as a derive needs to tell what a field holds.

use std::fmt::{self, Display, Formatter};

use flick_plugin::{Block, Span, Token, TokenKind};

/// A type as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// A named type, maybe qualified (`Shapes.Shape`), and its arguments.
    Con {
        name: String,
        args: Vec<Ty>,
    },
    /// A type variable and its arguments: `a`, `f String`.
    Var {
        name: String,
        args: Vec<Ty>,
    },
    /// A record; `open` with a row `| r`.
    Record {
        fields: Vec<(String, Ty)>,
        open: bool,
    },
    Tuple(Vec<Ty>),
    Unit,
    /// A function, as written: no wire form.
    Fun(String),
    /// A type with a capture qualifier, as written: what it holds is a
    /// scope's.
    Captured(String),
}

impl Display for Ty {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let atom = |t: &Ty| match t {
            Ty::Con { args, .. } | Ty::Var { args, .. } if !args.is_empty() => format!("({t})"),
            Ty::Fun(_) => format!("({t})"),
            _ => t.to_string(),
        };
        match self {
            Ty::Con { name, args } | Ty::Var { name, args } => {
                write!(f, "{name}")?;
                for a in args {
                    write!(f, " {}", atom(a))?;
                }
                Ok(())
            }
            Ty::Record { fields, open } => {
                let shown: Vec<String> = fields.iter().map(|(l, t)| format!("{l} : {t}")).collect();
                let tail = if *open { " | r" } else { "" };
                write!(f, "{{ {}{tail} }}", shown.join("; "))
            }
            Ty::Tuple(items) => {
                let shown: Vec<String> = items.iter().map(ToString::to_string).collect();
                write!(f, "({})", shown.join(", "))
            }
            Ty::Unit => write!(f, "()"),
            Ty::Fun(text) | Ty::Captured(text) => write!(f, "{text}"),
        }
    }
}

impl Ty {
    /// The type `self`, its variables replaced by `with`.
    pub fn substitute(&self, with: &[(String, Ty)]) -> Ty {
        let each = |args: &[Ty]| args.iter().map(|a| a.substitute(with)).collect::<Vec<_>>();
        match self {
            Ty::Var { name, args } => match with.iter().find(|(n, _)| n == name) {
                Some((_, Ty::Con { name, args: before })) => Ty::Con {
                    name: name.clone(),
                    args: before.iter().cloned().chain(each(args)).collect(),
                },
                Some((_, Ty::Var { name, args: before })) => Ty::Var {
                    name: name.clone(),
                    args: before.iter().cloned().chain(each(args)).collect(),
                },
                Some((_, other)) if args.is_empty() => other.clone(),
                _ => Ty::Var {
                    name: name.clone(),
                    args: each(args),
                },
            },
            Ty::Con { name, args } => Ty::Con {
                name: name.clone(),
                args: each(args),
            },
            Ty::Record { fields, open } => Ty::Record {
                fields: (fields.iter())
                    .map(|(l, t)| (l.clone(), t.substitute(with)))
                    .collect(),
                open: *open,
            },
            Ty::Tuple(items) => Ty::Tuple(each(items)),
            other => other.clone(),
        }
    }
}

/// The type `block` writes at `span`, or why it does not read as one.
pub fn read(block: &Block, span: Span) -> Result<Ty, String> {
    let tokens: Vec<Token> = (block.tokens.iter())
        .filter(|t| span.start <= t.span.start && t.span.end <= span.end)
        .copied()
        .collect();
    let mut p = Reader {
        block,
        tokens: &tokens,
        at: 0,
    };
    let ty = p.ty()?;
    if p.at < tokens.len() {
        return Err(format!("`{}` is not a type", block.text_of(span)));
    }
    Ok(ty)
}

struct Reader<'b> {
    block: &'b Block,
    tokens: &'b [Token],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(|t| self.block.text_of(t.span))
    }

    fn kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.at).map(|t| t.kind)
    }

    fn eat(&mut self, text: &str) -> bool {
        let at = self.peek() == Some(text);
        if at {
            self.at += 1;
        }
        at
    }

    fn next_text(&mut self) -> Option<String> {
        let text = self.peek()?.to_owned();
        self.at += 1;
        Some(text)
    }

    /// `app -> ty`, or `app`; a qualifier `^…` after an atom marks it.
    fn ty(&mut self) -> Result<Ty, String> {
        let start = self.at;
        if self.peek() == Some("forall") {
            self.skip();
            return Ok(Ty::Fun(self.text_from(start)));
        }
        let head = self.app()?;
        if self.peek().is_some_and(|t| t.starts_with("->")) {
            // the rest of a function type does not matter
            self.skip();
            return Ok(Ty::Fun(self.text_from(start)));
        }
        Ok(head)
    }

    /// The text of the tokens from `start` to here, its spaces collapsed.
    fn text_from(&self, start: usize) -> String {
        let (Some(first), Some(last)) = (
            self.tokens.get(start),
            self.tokens.get(self.at.max(start + 1) - 1),
        ) else {
            return String::new();
        };
        let span = Span {
            start: first.span.start,
            end: last.span.end,
        };
        self.block
            .text_of(span)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Past the rest of this type: to a `,`, `;`, `|` or closing bracket of
    /// what holds it.
    fn skip(&mut self) {
        let mut depth = 0usize;
        while let Some(t) = self.peek() {
            match t {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" | "," | ";" | "|" if depth == 0 => return,
                ")" | "}" | "]" => depth -= 1,
                _ => {}
            }
            self.at += 1;
        }
    }

    fn app(&mut self) -> Result<Ty, String> {
        let head = self.atom()?;
        let mut args = Vec::new();
        while self.at_atom() {
            args.push(self.atom()?);
        }
        if args.is_empty() {
            return Ok(head);
        }
        match head {
            Ty::Con { name, args: none } if none.is_empty() => Ok(Ty::Con { name, args }),
            Ty::Var { name, args: none } if none.is_empty() => Ok(Ty::Var { name, args }),
            other => Ok(other),
        }
    }

    fn at_atom(&self) -> bool {
        matches!(self.kind(), Some(TokenKind::Con | TokenKind::Ident))
            || matches!(self.peek(), Some("(" | "{" | "_"))
    }

    fn atom(&mut self) -> Result<Ty, String> {
        let start = self.at;
        let ty = self.bare()?;
        if self.eat("^") {
            // `^c` or `^{s, t}`
            if self.eat("{") {
                while self.peek().is_some() && !self.eat("}") {
                    self.at += 1;
                }
            } else {
                self.at += 1;
            }
            return Ok(Ty::Captured(self.text_from(start)));
        }
        Ok(ty)
    }

    fn bare(&mut self) -> Result<Ty, String> {
        match (self.kind(), self.peek()) {
            (Some(TokenKind::Con), _) => {
                let mut name = self.next_text().unwrap_or_default();
                // `M.N.T`
                while self.peek() == Some(".")
                    && self
                        .tokens
                        .get(self.at + 1)
                        .is_some_and(|t| t.kind == TokenKind::Con)
                {
                    self.at += 1;
                    name.push('.');
                    name.push_str(&self.next_text().unwrap_or_default());
                }
                Ok(Ty::Con {
                    name,
                    args: Vec::new(),
                })
            }
            (Some(TokenKind::Ident), _) => Ok(Ty::Var {
                name: self.next_text().unwrap_or_default(),
                args: Vec::new(),
            }),
            (_, Some("(")) => {
                self.at += 1;
                if self.eat(")") {
                    return Ok(Ty::Unit);
                }
                let mut items = vec![self.ty()?];
                while self.eat(",") {
                    if self.peek() == Some(")") {
                        break;
                    }
                    items.push(self.ty()?);
                }
                if !self.eat(")") {
                    return Err("expected `)`".into());
                }
                Ok(if items.len() == 1 {
                    items.remove(0)
                } else {
                    Ty::Tuple(items)
                })
            }
            (_, Some("{")) => {
                self.at += 1;
                let mut fields = Vec::new();
                let mut open = false;
                while !self.eat("}") {
                    if self.eat("|") {
                        open = true;
                        self.at += 1;
                        continue;
                    }
                    let label = self.next_text().ok_or("expected a field")?;
                    if !self.eat(":") {
                        return Err(format!("expected `:` after `{label}`"));
                    }
                    let ty = self.ty()?;
                    fields.push((label, ty));
                    self.eat(";");
                }
                fields.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(Ty::Record { fields, open })
            }
            _ => Err("expected a type".into()),
        }
    }
}
