//! Editor queries for CLI clauses; their values remain ordinary Flick expressions.

use super::complete::item;
use super::{Completion, CompletionKind, Ide, tree};
use crate::doc;
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;

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

impl Ide<'_> {
    pub(super) fn cli_complete(
        &self,
        file: FileId,
        offset: u32,
        dotted: bool,
    ) -> Option<Vec<Completion>> {
        if dotted {
            return None;
        }
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let at = path.iter().rposition(|n| n.kind == K::CliExpr)?;
        let cli = path[at];
        let open = cli.child_tokens().find(|t| t.kind == K::LBrace)?;
        if offset < open.end {
            return None;
        }
        if let Some(clause) = path[at + 1..].iter().rfind(|n| n.kind == K::CliClause) {
            let word = clause.child_tokens().next()?;
            if offset > word.end {
                return None;
            }
        } else {
            // Only a clause head, not a type, default or handler expression.
            let before = &text[open.end as usize..offset as usize];
            if !before.rsplit(['\n', ';']).next()?.trim().is_empty() {
                return None;
            }
        }
        Some(
            CLAUSES
                .iter()
                .map(|(name, syntax, snippet, help)| Completion {
                    documentation: Some((*help).to_owned()),
                    snippet: Some((*snippet).to_owned()),
                    ..item(name, CompletionKind::Keyword, Some((*syntax).to_owned()))
                })
                .collect(),
        )
    }

    pub(super) fn cli_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let at = path.iter().rposition(|n| n.kind == K::CliClause)?;
        let clause = path[at];
        let mut tokens = clause.child_tokens();
        let word = tokens.next()?;
        if word.start <= offset && offset < word.end {
            let (_, syntax, _, help) =
                CLAUSES.iter().find(|(name, ..)| *name == word.text(text))?;
            return Some((
                format!("```flick\n{syntax}\n```\n\n{help}"),
                (word.start, word.end),
            ));
        }
        if !matches!(word.text(text), "flag" | "arg") {
            return None;
        }
        let name = tokens.next().filter(|t| t.kind == K::Ident)?;
        if !(name.start <= offset && offset < name.end) {
            return None;
        }
        let ty = clause.nodes().next()?;
        let ty = ty
            .text(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut markdown = format!("```flick\n{} : {ty}\n```", name.text(text));
        if let Some(help) = doc::leading_doc(path[at.checked_sub(1)?], clause, text) {
            markdown.push_str("\n\n");
            markdown.push_str(&help);
        }
        Some((markdown, (name.start, name.end)))
    }
}
