//! The statements of `sql migration N "name" { … }`: SQL DDL read from the
//! tokens of the block, for name resolution, the checker, which derives
//! the module's tables from the schema the migrations build, and lowering,
//! which runs the statements as written.

mod alter;
mod schema;

use std::mem;

pub use schema::{Column, Problem, Table, Write, schema};

use super::cst::{Node, Token};
use super::kind::SyntaxKind as K;
use super::literal::string_value;

/// A word of a statement: its text and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub range: (u32, u32),
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
    pub fn range(&self) -> (u32, u32) {
        let start = self.words.first().map_or(0, |w| w.range.0);
        (start, self.words.last().map_or(start, |w| w.range.1))
    }

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
    pub range: (u32, u32),
    /// `None` for a version that is no whole number.
    pub version: Option<i64>,
    pub name: String,
    pub statements: Vec<Statement>,
}

/// The migrations of a module, in the order written.
pub fn migrations(tree: &Node, text: &str) -> Vec<Migration> {
    tree.nodes()
        .filter(|n| n.kind == K::SqlMigrationDecl)
        .map(|n| migration(n, text))
        .collect()
}

/// A `sql migration` declaration.
pub fn migration(decl: &Node, text: &str) -> Migration {
    let tokens: Vec<Token> = decl.child_tokens().collect();
    let version = tokens
        .iter()
        .find(|t| t.kind == K::Number)
        .and_then(|t| t.text(text).replace('_', "").parse().ok());
    let name = tokens
        .iter()
        .find(|t| t.kind == K::String)
        .map(|t| string_value(t.text(text)))
        .unwrap_or_default();
    let mut statements = Vec::new();
    let mut words = Vec::new();
    for t in decl
        .nodes()
        .filter(|n| n.kind == K::Block)
        .flat_map(Node::child_tokens)
    {
        if t.kind == K::Semi {
            if !words.is_empty() {
                statements.push(Statement {
                    words: mem::take(&mut words),
                });
            }
            continue;
        }
        words.push(Word {
            text: t.text(text).to_owned(),
            range: (t.start, t.end),
        });
    }
    if !words.is_empty() {
        statements.push(Statement { words });
    }
    Migration {
        range: decl.range(),
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

/// The type names of the tables a module's migrations leave, for name
/// resolution: the problems are the checker's to report.
pub fn table_names(tree: &Node, text: &str) -> Vec<(String, (u32, u32))> {
    schema(&migrations(tree, text))
        .0
        .into_iter()
        .map(|t| (type_name(&t.name), t.at))
        .collect()
}

#[cfg(test)]
mod tests;
