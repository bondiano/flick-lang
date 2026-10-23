//! The clauses of a `cli "name" { … }` block as the checker and the lowering
//! read them: both elaborate the block into the same calls of `Cli.*`.

use super::cst::{Node, Token};
use super::kind::SyntaxKind as K;
use super::literal::string_value;

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
    /// The name of its value in `Cli`.
    pub fn function(self) -> &'static str {
        match self {
            Multi::Switch => "switch",
            Multi::Required => "required",
            Multi::Default => "default",
            Multi::Optional => "optional",
            Multi::Many => "many",
            Multi::Some => "some",
        }
    }
}

/// A `flag` or an `arg`.
#[derive(Debug, Clone)]
pub struct Param<'n> {
    pub flag: bool,
    pub word: Token,
    pub name: Token,
    pub aliases: Vec<Token>,
    pub ty: &'n Node,
    pub default: Option<&'n Node>,
    pub multi: Multi,
    pub help: String,
}

#[derive(Debug, Clone)]
pub enum Clause<'n> {
    Description(Token, &'n Node),
    Param(Param<'n>),
    Subcommand(Token, &'n Node, String),
    Handle(Token, &'n Node),
}

impl Clause<'_> {
    pub fn word(&self) -> Token {
        match self {
            Clause::Description(w, _) | Clause::Subcommand(w, _, _) | Clause::Handle(w, _) => *w,
            Clause::Param(p) => p.word,
        }
    }
}

/// The block: the name token and the clauses, in the order written.
pub struct Block<'n> {
    pub keyword: Token,
    pub name: Option<Token>,
    pub clauses: Vec<Clause<'n>>,
}

pub fn block<'n>(node: &'n Node, text: &str) -> Block<'n> {
    let mut tokens = node.child_tokens();
    let keyword = tokens.next().unwrap_or(Token {
        kind: K::CliKw,
        start: 0,
        end: 0,
    });
    let name = tokens.find(|t| t.kind == K::String);
    let clauses = node
        .nodes()
        .filter(|n| n.kind == K::Block)
        .flat_map(Node::nodes)
        .filter(|n| n.kind == K::CliClause)
        .filter_map(|c| clause(c, text))
        .collect();
    Block {
        keyword,
        name,
        clauses,
    }
}

fn clause<'n>(node: &'n Node, text: &str) -> Option<Clause<'n>> {
    let mut tokens = node.child_tokens();
    let word = tokens.next()?;
    let expr = node.nodes().next();
    match word.text(text) {
        "description" => Some(Clause::Description(word, expr?)),
        "handle" => Some(Clause::Handle(word, expr?)),
        "subcommand" => Some(Clause::Subcommand(word, expr?, help(text, word.start))),
        w @ ("flag" | "arg") => {
            let name = tokens.next().filter(|t| t.kind == K::Ident)?;
            let aliases = node
                .child_tokens()
                .filter(|t| t.kind == K::String)
                .collect();
            let mut nodes = node.nodes();
            let ty = nodes.next()?;
            let default = nodes.next();
            Some(Clause::Param(Param {
                flag: w == "flag",
                word,
                name,
                aliases,
                ty,
                default,
                multi: multi(ty, default.is_some(), text),
                help: help(text, word.start),
            }))
        }
        _ => None,
    }
}

/// The multiplicity the written type asks for.
fn multi(ty: &Node, default: bool, text: &str) -> Multi {
    let head = |n: &Node| {
        n.child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident))
            .last()
            .map(|t| t.text(text).to_owned())
    };
    let name = match ty.kind {
        K::AppType => ty.nodes().next().and_then(head),
        K::NameType => head(ty).map(|n| format!("{n}/0")),
        _ => None,
    };
    match name.as_deref() {
        Some("Option") => Multi::Optional,
        Some("List") => Multi::Many,
        Some("NonEmpty") => Multi::Some,
        _ if default => Multi::Default,
        Some("Bool/0") => Multi::Switch,
        _ => Multi::Required,
    }
}

/// The `///` lines right above a clause, joined: its help text.
fn help(text: &str, start: u32) -> String {
    let line_start = text[..start as usize].rfind('\n').unwrap_or(0);
    let mut lines: Vec<&str> = Vec::new();
    for line in text[..line_start].lines().rev() {
        match line.trim_start().strip_prefix("///") {
            Some(doc) => lines.push(doc.trim()),
            None => break,
        }
    }
    lines.reverse();
    lines.join(" ")
}

/// A string literal's value.
pub fn string(token: Token, text: &str) -> String {
    string_value(token.text(text))
}

/// The long name of a field on the command line: `dry_run` is `dry-run`.
pub fn long_name(field: &str) -> String {
    field.replace('_', "-")
}
