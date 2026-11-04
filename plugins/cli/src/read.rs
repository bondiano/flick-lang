//! The clauses of a block, read from its tokens, the user's types and
//! expressions in them parsed by the compiler.

use flick_plugin::{Block, Compiler, Diagnostic, Hole, Span, Token, TokenKind};

use crate::{len, text};

/// How the values of a `flag` or `arg` make its field, from the written type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multi {
    /// `Bool` without a default: `Cli.switch`.
    Switch,
    /// `T` without a default: `Cli.required`.
    Required,
    /// `T = default`: `Cli.default`.
    Default,
    /// `Option T`: `Cli.optional`.
    Optional,
    /// `List T`: `Cli.many`.
    Many,
    /// `NonEmpty T`: `Cli.some`.
    Some,
}

impl Multi {
    /// Its value in `Cli`.
    pub fn function(self) -> &'static str {
        match self {
            Multi::Switch => "Cli.switch",
            Multi::Required => "Cli.required",
            Multi::Default => "Cli.default",
            Multi::Optional => "Cli.optional",
            Multi::Many => "Cli.many",
            Multi::Some => "Cli.some",
        }
    }
}

/// A `flag` or an `arg`, at spans of the block.
pub struct Param {
    pub flag: bool,
    pub word: Span,
    pub name: Span,
    /// The string tokens of its other names.
    pub aliases: Vec<Span>,
    pub ty: Span,
    pub default: Option<Span>,
    pub multi: Multi,
    pub help: String,
}

pub enum Clause {
    Description {
        word: Span,
        value: Span,
    },
    Param(Param),
    Subcommand {
        word: Span,
        value: Span,
        help: String,
    },
    Handle {
        word: Span,
        value: Span,
    },
}

impl Clause {
    pub fn word(&self) -> Span {
        match self {
            Clause::Description { word, .. }
            | Clause::Subcommand { word, .. }
            | Clause::Handle { word, .. } => *word,
            Clause::Param(p) => p.word,
        }
    }
}

/// The block: the string token naming the command, and the clauses in the
/// order written.
pub struct Command {
    pub name: Span,
    pub clauses: Vec<Clause>,
}

/// The words a clause starts with.
pub const WORDS: [&str; 5] = ["description", "flag", "arg", "subcommand", "handle"];

/// A block that is not clauses of a command: the code of its errors.
pub const MALFORMED: &str = "L009";

pub fn command(block: &Block, compiler: &mut dyn Compiler) -> Result<Command, Vec<Diagnostic>> {
    let end = len(block);
    let arguments = (block.tokens.iter())
        .take_while(|t| t.span.start < block.body)
        .count();
    let (head, mut rest) = block.tokens.split_at(arguments);
    let name = match head {
        [t] if t.kind == TokenKind::String => t.span,
        _ => {
            return Err(malformed(
                Span { start: 0, end },
                "a `cli` block names its command: `cli \"name\" { … }`",
            ));
        }
    };
    let mut clauses = Vec::new();
    while let Some((word, after)) = rest.split_first() {
        rest = after;
        if is(block, Some(word), ";") {
            continue;
        }
        let w = text(block, word.span);
        if word.kind != TokenKind::Ident || !WORDS.contains(&w) {
            return Err(malformed(
                word.span,
                "expected a clause: `description`, `flag`, `arg`, `subcommand` or `handle`",
            ));
        }
        let help = help(&block.text, word.span.start, " ");
        let clause = if let "flag" | "arg" = w {
            let Some((name, after)) = rest
                .split_first()
                .filter(|(t, _)| t.kind == TokenKind::Ident)
            else {
                return Err(malformed(
                    word.span,
                    &format!("`{w}` takes a name: `{w} name : T`"),
                ));
            };
            rest = after;
            let aliases: Vec<Span> = (rest.iter())
                .take_while(|t| t.kind == TokenKind::String)
                .map(|t| t.span)
                .collect();
            rest = &rest[aliases.len()..];
            if !is(block, rest.first(), ":") {
                return Err(malformed(
                    name.span,
                    &format!("expected `: T` after `{w} {}`", text(block, name.span)),
                ));
            }
            let ty = span(compiler.parse_type(start(rest.get(1), end), end))?;
            rest = past(rest, ty.end);
            let default = if is(block, rest.first(), "=") {
                let value = span(compiler.parse_expr(start(rest.get(1), end), end))?;
                rest = past(rest, value.end);
                Some(value)
            } else {
                None
            };
            Clause::Param(Param {
                flag: w == "flag",
                word: word.span,
                name: name.span,
                aliases,
                ty,
                default,
                multi: multi(block, ty, default.is_some()),
                help,
            })
        } else {
            let value = span(compiler.parse_expr(start(rest.first(), end), end))?;
            rest = past(rest, value.end);
            match w {
                "description" => Clause::Description {
                    word: word.span,
                    value,
                },
                "handle" => Clause::Handle {
                    word: word.span,
                    value,
                },
                _ => Clause::Subcommand {
                    word: word.span,
                    value,
                    help,
                },
            }
        };
        clauses.push(clause);
    }
    Ok(Command { name, clauses })
}

fn malformed(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(message, span).with_code(MALFORMED)]
}

fn span(hole: Result<Hole, Diagnostic>) -> Result<Span, Vec<Diagnostic>> {
    hole.map(|h| h.span).map_err(|d| vec![d])
}

/// Where `token` starts, or `end` without one.
fn start(token: Option<&Token>, end: u32) -> u32 {
    token.map_or(end, |t| t.span.start)
}

pub fn is(block: &Block, token: Option<&Token>, punct: &str) -> bool {
    token.is_some_and(|t| t.kind == TokenKind::Punct && text(block, t.span) == punct)
}

/// The tokens of `rest` after `end`.
fn past(rest: &[Token], end: u32) -> &[Token] {
    let skip = rest.iter().take_while(|t| t.span.end <= end).count();
    &rest[skip..]
}

/// The multiplicity the type written at `ty` asks for.
fn multi(block: &Block, ty: Span, default: bool) -> Multi {
    let tokens: Vec<&Token> = (block.tokens.iter())
        .filter(|t| ty.start <= t.span.start && t.span.end <= ty.end)
        .collect();
    let (head, applied) = head(block, &tokens);
    match head {
        Some("Option") if applied => Multi::Optional,
        Some("List") if applied => Multi::Many,
        Some("NonEmpty") if applied => Multi::Some,
        _ if default => Multi::Default,
        Some("Bool") if !applied => Multi::Switch,
        _ => Multi::Required,
    }
}

/// The last part of the name a type starts with, and whether more follows
/// it: `NonEmpty.NonEmpty a` is `NonEmpty`, applied. A function type has
/// none.
fn head<'b>(block: &'b Block, tokens: &[&Token]) -> (Option<&'b str>, bool) {
    let named = |t: &Token| matches!(t.kind, TokenKind::Con | TokenKind::Ident);
    if tokens.iter().any(|t| text(block, t.span) == "->")
        || !tokens.first().is_some_and(|t| named(t))
    {
        return (None, false);
    }
    let mut i = 0;
    while i + 2 < tokens.len() && text(block, tokens[i + 1].span) == "." && named(tokens[i + 2]) {
        i += 2;
    }
    (Some(text(block, tokens[i].span)), i + 1 < tokens.len())
}

/// The `///` lines right above the clause at `start`, joined by `sep`.
pub fn help(text: &str, start: u32, sep: &str) -> String {
    let start = (start as usize).min(text.len());
    let line_start = text[..start].rfind('\n').unwrap_or(0);
    let mut lines: Vec<&str> = Vec::new();
    for line in text[..line_start].lines().rev() {
        match line.trim_start().strip_prefix("///") {
            Some(doc) => lines.push(doc.trim()),
            None => break,
        }
    }
    lines.reverse();
    lines.join(sep)
}
