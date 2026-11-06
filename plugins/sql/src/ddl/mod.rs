//! The statements of `sql migration N "name" { … }`: SQL DDL read from the
//! tokens of the block. The schema of a module is built from all its
//! migrations at once, so a word's place names its block too: the block's
//! index in the high 32 bits, the offset in the block in the low ones.

mod alter;
mod schema;

use std::mem;

pub use schema::schema;

use flick_plugin::{Block, Span, TokenKind, string_value};

/// A range in one of a module's migrations, its start and end each a
/// [`place`].
pub type Range = (u64, u64);

/// The place of `offset` in the block `index`, every offset of a block
/// being a `u32`.
pub fn place(index: usize, offset: u32) -> u64 {
    let index = u64::from(u32::try_from(index).expect("a module has fewer than 2^32 migrations"));
    (index << 32) | u64::from(offset)
}

/// The block and the span in it of the range `range`.
pub fn located(range: Range) -> (usize, Span) {
    let index = usize::try_from(range.0 >> 32).expect("a migration's index fits in usize");
    // the low 32 bits are the offset in the block
    let offset = |at: u64| u32::try_from(at & u64::from(u32::MAX)).unwrap_or(u32::MAX);
    (
        index,
        Span {
            start: offset(range.0),
            end: offset(range.1),
        },
    )
}

/// A word of a statement: its text and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub range: Range,
}

impl Word {
    /// The word in lower case, as SQL compares keywords.
    pub fn lower(&self) -> String {
        self.text.to_ascii_lowercase()
    }
}

/// One statement, up to its `;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub words: Vec<Word>,
}

impl Statement {
    /// The statement as SQL: its words, spaced as a person writes them; a
    /// string, `"cat"`, is a text literal, `'cat'`, as in `sql { … }`.
    pub fn sql(&self) -> String {
        let mut out = String::new();
        for (i, w) in self.words.iter().enumerate() {
            let tight = i == 0
                || matches!(w.text.as_str(), "," | ")")
                || self.words[i - 1].text == "("
                || w.text == "(" && self.words[i - 1].range.1 == w.range.0;
            if !tight {
                out.push(' ');
            }
            if w.text.starts_with('"') {
                out.push('\'');
                out.push_str(&string_value(&w.text).replace('\'', "''"));
                out.push('\'');
            } else {
                out.push_str(&w.text);
            }
        }
        out
    }
}

/// A migration of the module: its version, name and statements.
#[derive(Debug, Clone)]
pub struct Migration {
    pub range: Range,
    /// `None` for a version that is no whole number.
    pub version: Option<i64>,
    pub name: String,
    pub statements: Vec<Statement>,
}

/// The migration of `block`, the `index`th of its module.
pub fn migration(block: &Block, index: usize) -> Migration {
    let text = |span: Span| block.text_of(span);
    let args = block.arguments();
    let version = (args.iter())
        .find(|t| t.kind == TokenKind::Number)
        .and_then(|t| text(t.span).replace('_', "").parse().ok());
    let name = (args.iter())
        .find(|t| t.kind == TokenKind::String)
        .map(|t| string_value(text(t.span)))
        .unwrap_or_default();
    let mut statements = Vec::new();
    let mut words = Vec::new();
    for t in block.body().rest() {
        if text(t.span) == ";" {
            if !words.is_empty() {
                statements.push(Statement {
                    words: mem::take(&mut words),
                });
            }
            continue;
        }
        words.push(Word {
            text: text(t.span).to_owned(),
            range: (place(index, t.span.start), place(index, t.span.end)),
        });
    }
    if !words.is_empty() {
        statements.push(Statement { words });
    }
    let whole = block.whole();
    Migration {
        range: (place(index, whole.start), place(index, whole.end)),
        version,
        name,
        statements,
    }
}

/// The type of a table's rows: `article_tags` is `ArticleTags`.
pub fn type_name(table: &str) -> String {
    table
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
