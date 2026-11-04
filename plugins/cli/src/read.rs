//! The clauses of a block, read from its tokens, the user's types and
//! expressions in them parsed by the compiler.

use flick_plugin::{Block, Compiler, Diagnostic, Span, Token, TokenKind};

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

impl Command {
    /// Its `flag` and `arg` clauses, in order.
    pub fn params(&self) -> impl Iterator<Item = &Param> {
        self.clauses.iter().filter_map(|c| match c {
            Clause::Param(p) => Some(p),
            _ => None,
        })
    }
}

/// The words a clause starts with.
pub const WORDS: [&str; 5] = ["description", "flag", "arg", "subcommand", "handle"];

/// A block that is not clauses of a command: the code of its errors.
pub const MALFORMED: &str = "L009";

pub fn command(block: &Block, compiler: &mut dyn Compiler) -> Result<Command, Vec<Diagnostic>> {
    let name = match block.arguments() {
        [t] if t.kind == TokenKind::String => t.span,
        _ => {
            return Err(malformed(
                block.whole(),
                "a `cli` block names its command: `cli \"name\" { … }`",
            ));
        }
    };
    let mut c = block.body();
    let mut clauses = Vec::new();
    while let Some(word) = c.next() {
        let w = c.text(word);
        if w == ";" {
            continue;
        }
        if word.kind != TokenKind::Ident || !WORDS.contains(&w) {
            return Err(malformed(
                word.span,
                "expected a clause: `description`, `flag`, `arg`, `subcommand` or `handle`",
            ));
        }
        let help = block.docs_above(word.span.start, " ");
        let clause = if let "flag" | "arg" = w {
            let Some(name) = c.ident() else {
                return Err(malformed(
                    word.span,
                    &format!("`{w}` takes a name: `{w} name : T`"),
                ));
            };
            let aliases = c.strings();
            if c.eat(":").is_none() {
                let message = format!("expected `: T` after `{w} {}`", c.text(name));
                return Err(malformed(name.span, &message));
            }
            let ty = c.ty(compiler).map_err(|d| vec![d])?;
            let default = match c.eat("=") {
                Some(_) => Some(c.expr(compiler).map_err(|d| vec![d])?),
                None => None,
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
            let value = c.expr(compiler).map_err(|d| vec![d])?;
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
    let text = |t: &Token| block.text_of(t.span);
    let named = |t: &Token| matches!(t.kind, TokenKind::Con | TokenKind::Ident);
    if tokens.iter().any(|t| text(t) == "->") || !tokens.first().is_some_and(|t| named(t)) {
        return (None, false);
    }
    let mut i = 0;
    while i + 2 < tokens.len() && text(tokens[i + 1]) == "." && named(tokens[i + 2]) {
        i += 2;
    }
    (Some(text(tokens[i])), i + 1 < tokens.len())
}
