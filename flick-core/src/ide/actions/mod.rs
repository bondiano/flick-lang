//! Code actions that rewrite code without a diagnostic asking for it:
//! adding a signature, matching on a variable, qualifying a name, and
//! extracting or inlining a variable.

mod extract;
mod import_class;
mod match_on;
mod qualify;
mod signature;

use super::Ide;
use crate::diagnostics::Edit;
use crate::source::{FileId, Span};

/// A rewrite: what it does, its LSP kind and its edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub title: String,
    pub kind: &'static str,
    pub edits: Vec<Edit>,
}

impl Ide<'_> {
    /// The rewrites offered for the range `start..end` of `file`.
    pub fn actions(&self, file: FileId, start: u32, end: u32) -> Vec<Action> {
        let mut out: Vec<Action> = Vec::new();
        out.extend(self.add_signature(file, start));
        out.extend(self.infer_todo_signature(file, start));
        out.extend(self.import_class(file, start));
        out.extend(self.match_on(file, start));
        out.extend(self.qualify(file, start));
        out.extend(self.extract(file, start, end));
        out.extend(self.inline(file, start));
        out
    }
}

/// The offset of the start of the line `offset` is on.
fn line_start(text: &str, offset: u32) -> u32 {
    text[..offset as usize]
        .rfind('\n')
        .map_or(0, |i| to_u32(i + 1))
}

/// The whitespace that starts the line `offset` is on.
fn indent_at(text: &str, offset: u32) -> &str {
    let line = &text[line_start(text, offset) as usize..];
    &line[..line.len() - line.trim_start_matches(' ').len()]
}

/// Whether only spaces come before `offset` on its line.
fn starts_line(text: &str, offset: u32) -> bool {
    text[line_start(text, offset) as usize..offset as usize]
        .chars()
        .all(|c| c == ' ')
}

fn edit(file: FileId, start: u32, end: u32, replacement: impl Into<String>) -> Edit {
    Edit {
        span: Span::new(file, start, end),
        replacement: replacement.into(),
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("source offsets fit in u32")
}

#[cfg(test)]
mod tests;
