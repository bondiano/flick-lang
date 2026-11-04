//! What an editor shows of a block: the clauses to insert where one starts,
//! what a clause word means and the type and help of a parameter.

use flick_plugin::{Block, Completion, Highlight, HighlightKind, Hover, Token, TokenKind};

// Name, syntax, insertion snippet, and help shared by completion and hover.
const CLAUSES: &[(&str, &str, &str, &str)] = &[
    (
        "description",
        "description text",
        "description \"${1:Help text}\"",
        "Sets the command's help text (`description text`, where `text : String`). May appear once.",
    ),
    (
        "flag",
        "flag name \"alias\" : T = default",
        "flag ${1:name} : ${2:Bool}",
        "Declares `flag name : T`, a named option `--name` and a field of type `T` in the handler's record. Optional aliases precede `:`; `= default` supplies an absent value. A `///` comment supplies its help text.",
    ),
    (
        "arg",
        "arg name : T = default",
        "arg ${1:name} : ${2:String}",
        "Declares `arg name : T`, a positional argument and a field of type `T` in the handler's record. `= default` supplies an absent value. A `List T` argument consumes the remaining arguments and must be last; a `///` comment supplies its help text.",
    ),
    (
        "subcommand",
        "subcommand command",
        "subcommand ${1:command}",
        "Adds a nested command (`subcommand command`, where `command : Cli.Command r e`). A `///` comment supplies its help text. A command with subcommands cannot also declare positional arguments.",
    ),
    (
        "handle",
        "handle (fun options -> …)",
        "handle fun ${1:options} -> ${0:()}",
        "Sets the handler (`handle f`, where `f : Options ->{r; e} Unit`). `Options` is the exact record of this block's flags and arguments. May appear once; runs only after the command line has been parsed.",
    ),
];

pub fn complete(block: &Block, offset: u32) -> Vec<Completion> {
    if !at_clause(block, offset) {
        return Vec::new();
    }
    (CLAUSES.iter())
        .map(|(name, syntax, snippet, help)| Completion {
            label: (*name).to_owned(),
            detail: Some((*syntax).to_owned()),
            documentation: Some((*help).to_owned()),
            snippet: Some((*snippet).to_owned()),
        })
        .collect()
}

pub fn hover(block: &Block, offset: u32) -> Option<Hover> {
    let i = (block.tokens.iter()).position(|t| {
        t.span.start >= block.body && t.span.start <= offset && offset < t.span.end
    })?;
    let token = block.tokens[i];
    if token.kind != TokenKind::Ident {
        return None;
    }
    let word = block.text_of(token.span);
    if at_clause(block, token.span.start)
        && let Some((_, syntax, _, help)) = CLAUSES.iter().find(|(name, ..)| *name == word)
    {
        return Some(Hover {
            markdown: format!("```flick\n{syntax}\n```\n\n{help}"),
            span: token.span,
        });
    }
    // the name of a `flag` or an `arg`
    let clause = block.tokens[i.checked_sub(1)?];
    if !matches!(block.text_of(clause.span), "flag" | "arg") || !at_clause(block, clause.span.start)
    {
        return None;
    }
    let mut markdown = format!("```flick\n{word} : {}\n```", written_type(block, i)?);
    let docs = block.docs_above(clause.span.start, "\n");
    if !docs.is_empty() {
        markdown.push_str("\n\n");
        markdown.push_str(&docs);
    }
    Some(Hover {
        markdown,
        span: token.span,
    })
}

/// Each clause word as a keyword, and the name of each `flag` and `arg` as
/// a parameter.
pub fn highlight(block: &Block) -> Vec<Highlight> {
    let mut out = Vec::new();
    let body = block
        .tokens
        .iter()
        .skip_while(|t| t.span.start < block.body);
    let mut tokens = body.peekable();
    while let Some(t) = tokens.next() {
        let word = block.text_of(t.span);
        if t.kind != TokenKind::Ident || !CLAUSES.iter().any(|(name, ..)| *name == word) {
            continue;
        }
        if !at_clause(block, t.span.start) {
            continue;
        }
        out.push(Highlight {
            span: t.span,
            kind: HighlightKind::Keyword,
        });
        if let ("flag" | "arg", Some(name)) = (word, tokens.peek())
            && name.kind == TokenKind::Ident
        {
            out.push(Highlight {
                span: name.span,
                kind: HighlightKind::Parameter,
            });
        }
    }
    out
}

/// Whether a clause can start at `offset`: at the start of the body, after
/// `;`, on a line no deeper than the clauses before, or after the `{` of a
/// block named by a string, such as one being written inside another. The
/// word being written there may already stand before `offset`.
fn at_clause(block: &Block, offset: u32) -> bool {
    let Some(before) = block.text.get(block.body as usize..offset as usize) else {
        return false;
    };
    let cut = before.rfind(['\n', ';', '{']);
    let line = &before[cut.map_or(0, |i| i + 1)..];
    let word = line.trim_start();
    if !word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    match cut.map(|i| (i, before.as_bytes()[i])) {
        None | Some((_, b';')) => true,
        Some((i, b'{')) => before[..i].trim_end().ends_with('"'),
        Some(_) => indent(block).is_none_or(|clauses| line.len() - word.len() <= clauses),
    }
}

/// The indent of the first clause that starts a line of the body.
fn indent(block: &Block) -> Option<usize> {
    let first = block.tokens.iter().find(|t| t.span.start >= block.body)?;
    let before = &block.text[block.body as usize..first.span.start as usize];
    let line = before.rfind('\n')?;
    Some(before.len() - line - 1)
}

/// The type written after the name at token `name`, up to `=` or `;` or
/// the end of its line, its spaces collapsed.
fn written_type(block: &Block, name: usize) -> Option<String> {
    let rest = &block.tokens[name + 1..];
    let colon = rest.iter().position(|t| block.text_of(t.span) == ":")?;
    let mut end: Option<&Token> = None;
    let mut last_end = rest[colon].span.end;
    for t in &rest[colon + 1..] {
        let between = &block.text[last_end as usize..t.span.start as usize];
        if between.contains('\n') || matches!(block.text_of(t.span), "=" | ";") {
            break;
        }
        end = Some(t);
        last_end = t.span.end;
    }
    let start = rest.get(colon + 1)?.span.start;
    let written = block.text.get(start as usize..end?.span.end as usize)?;
    Some(written.split_whitespace().collect::<Vec<_>>().join(" "))
}
