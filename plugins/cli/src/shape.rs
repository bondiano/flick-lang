//! The rules of a command the signatures of `Cli` cannot state: each name
//! used once, defaults, many-valued arguments last, clauses once, and a
//! handler or subcommands.

use std::collections::BTreeMap;

use flick_plugin::{Block, Diagnostic, Span};

use crate::read::{Clause, Command, Multi, Param};

/// Long and short names the CLI reserves for itself.
const RESERVED: [&str; 4] = ["help", "h", "version", "completions"];

pub fn check(block: &Block, command: &Command) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let mut error = |code: &str, span: Span, message: String| {
        found.push(Diagnostic::error(message, span).with_code(code));
    };
    let mut names: BTreeMap<String, Span> = BTreeMap::new();
    let mut taken =
        |error: &mut dyn FnMut(&str, Span, String), name: String, span: Span, what: &str| {
            if RESERVED.contains(&name.as_str()) {
                error(
                    "L001",
                    span,
                    format!("`{}` is reserved by the CLI", dashed(&name)),
                );
            } else if names.insert(name.clone(), span).is_some() {
                error(
                    "L001",
                    span,
                    format!(
                        "{what} `{}` is already taken in this command",
                        dashed(&name)
                    ),
                );
            }
        };
    let mut fields: Vec<&str> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let params: Vec<&Param> = command.params().collect();
    for clause in &command.clauses {
        let word = clause.word();
        if let Clause::Description { .. } | Clause::Handle { .. } = clause {
            let w = block.text_of(word);
            if seen.contains(&w) {
                error("L004", word, format!("`{w}` is given twice"));
            }
            seen.push(w);
        }
        let Clause::Param(p) = clause else { continue };
        let field = block.text_of(p.name);
        if fields.contains(&field) {
            error(
                "L001",
                p.name,
                format!("the options already have a field `{field}`"),
            );
            continue;
        }
        fields.push(field);
        if p.flag {
            taken(&mut error, field.replace('_', "-"), p.name, "the name");
            for &alias in &p.aliases {
                taken(
                    &mut error,
                    unquoted(block.text_of(alias)).to_owned(),
                    alias,
                    "the alias",
                );
            }
        } else if let Some(&alias) = p.aliases.first() {
            error(
                "L007",
                alias,
                "a positional `arg` has no other names".to_owned(),
            );
        }
        if let (Multi::Optional, Some(d)) = (p.multi, p.default) {
            error(
                "L002",
                d,
                "an `Option` parameter is `None` when absent: it takes no `= default`".to_owned(),
            );
        }
    }
    command_shape(
        block,
        command,
        &params,
        seen.contains(&"handle"),
        &mut error,
    );
    found
}

/// Positional arguments, subcommands and the handler of the command.
fn command_shape(
    block: &Block,
    command: &Command,
    params: &[&Param],
    handled: bool,
    error: &mut dyn FnMut(&str, Span, String),
) {
    let args: Vec<&&Param> = params.iter().filter(|p| !p.flag).collect();
    for p in args.iter().rev().skip(1) {
        if matches!(p.multi, Multi::Many | Multi::Some) {
            error(
                "L003",
                p.ty,
                "an `arg` of many values takes the rest of the line: only the last one can"
                    .to_owned(),
            );
        }
    }
    let subcommand = (command.clauses.iter()).any(|c| matches!(c, Clause::Subcommand { .. }));
    if let (true, Some(a)) = (subcommand, args.first()) {
        error(
            "L006",
            a.word,
            "a command with subcommands takes no `arg`: it could not be told from a subcommand's name"
                .to_owned(),
        );
    }
    if !subcommand && !handled {
        error(
            "L005",
            block.whole(),
            "a command needs `handle` or at least one `subcommand`".to_owned(),
        );
    }
}

/// What a string literal holds, escapes as written.
fn unquoted(literal: &str) -> &str {
    (literal.strip_prefix('"'))
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(literal)
}

fn dashed(name: &str) -> String {
    if name.chars().count() == 1 {
        format!("-{name}")
    } else {
        format!("--{name}")
    }
}
