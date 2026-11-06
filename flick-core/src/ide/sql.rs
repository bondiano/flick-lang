//! SQL editor queries use the checked alias/column types and the query's CST
//! scope; the functions offered are the module's dialect's.

use std::collections::BTreeMap;

use super::complete::item;
use super::{Completion, CompletionKind, Ide, tree};
use crate::iface::Interface;
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Type, Typing, show_type};

const CLAUSES: &[(&str, &str)] = &[
    ("from", "Introduces a table alias: `from alias in Table`."),
    ("join", "Joins a table; followed by `on condition`."),
    (
        "left_join",
        "Left join: the joined columns may be NULL and read as Option values.",
    ),
    (
        "on",
        "The join condition, checked against the aliases introduced so far.",
    ),
    ("where", "Filters rows before grouping."),
    (
        "select",
        "The result row: `select { label = alias.column }`, columns, or a whole alias.",
    ),
    ("group_by", "Groups rows by these expressions."),
    ("having", "Filters groups; aggregate functions are allowed."),
    (
        "order_by",
        "Orders results; use `asc` or `desc` before an expression.",
    ),
    ("limit", "Limits the number of result rows."),
    ("offset", "Skips result rows."),
    (
        "union",
        "Combines queries with the same result columns, removing duplicates.",
    ),
    (
        "union_all",
        "Combines queries with the same result columns, keeping duplicates.",
    ),
    (
        "insert_into",
        "Inserts into a table with `values { column = value }`.",
    ),
    ("values", "The column values to insert."),
    ("update", "Updates a table with `set { column = value }`."),
    ("set", "The column values to update."),
    ("delete_from", "Deletes rows from a table."),
    (
        "returning",
        "The result row of an insert, update or delete.",
    ),
];

fn clauses(query: &Node) -> impl Iterator<Item = &Node> {
    query
        .nodes()
        .filter(|n| n.kind == K::Block)
        .flat_map(Node::nodes)
        .filter(|n| n.kind == K::SqlClause)
}

/// Whether `path` ends inside a `$…` capture, whose names are the program's.
fn captured(path: &[&Node]) -> bool {
    path.iter()
        .rev()
        .find(|n| matches!(n.kind, K::SqlExpr | K::SqlCapture))
        .is_some_and(|n| n.kind == K::SqlCapture)
}

fn bindings<'a>(
    path: &[&Node],
    typing: &'a Typing,
    text: &'a str,
    offset: u32,
) -> Vec<(&'a str, &'a Type)> {
    let mut out = Vec::new();
    if let Some(query) = path.iter().rev().find(|n| n.kind == K::SqlExpr) {
        let in_on = path
            .iter()
            .any(|n| n.kind == K::SqlClause && tree::name_of(n, K::Ident, text) == Some("on"));
        for clause in clauses(query) {
            if in_on && clause.range().0 > offset {
                continue;
            }
            let Some(word) = clause.child_tokens().next() else {
                continue;
            };
            if !matches!(word.text(text), "from" | "join" | "left_join") {
                continue;
            }
            if let Some(token) = clause.child_tokens().skip(1).find(|t| t.kind == K::Ident)
                && let Some(ty) = typing.types.get(&(token.start, token.end))
            {
                out.push((token.text(text), ty));
            }
        }
    }
    if let Some(token) = path
        .iter()
        .rev()
        .filter(|n| n.kind == K::SqlExpr)
        .find_map(|n| n.child_tokens().find(|t| t.kind == K::Ident))
        && let Some(ty) = typing.types.get(&(token.start, token.end))
    {
        out.retain(|(name, _)| *name != token.text(text));
        out.push((token.text(text), ty));
    }
    out
}

fn tables(interface: &Interface) -> impl Iterator<Item = Completion> + '_ {
    interface.exports.iter().filter_map(|e| {
        let name = e.entity.name.strip_suffix("$table")?;
        Some(item(
            name,
            CompletionKind::Type,
            Some(format!("sql table {name}")),
        ))
    })
}

impl Ide<'_> {
    pub(super) fn sql_complete(
        &self,
        file: FileId,
        offset: u32,
        dotted: bool,
    ) -> Option<Vec<Completion>> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        if captured(&path) {
            return None;
        }
        if !path.iter().any(|n| n.kind == K::SqlExpr) {
            return None;
        }
        if dotted {
            let scope = bindings(&path, &m.typing, text, offset);
            let qualifier = text[..offset as usize]
                .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
                .next()?;
            if let Some((_, Type::Record(row))) = scope.iter().find(|(name, _)| *name == qualifier)
            {
                return Some(
                    row.entries
                        .iter()
                        .map(|(name, ty)| item(name, CompletionKind::Field, Some(show_type(ty))))
                        .collect(),
                );
            }
            return Some(
                self.interface(qualifier)
                    .map_or_else(Vec::new, |i| tables(&i).collect()),
            );
        }
        let table_position = path
            .iter()
            .find(|n| n.kind == K::SqlClause)
            .is_some_and(|clause| {
                let Some(word) = clause.child_tokens().next() else {
                    return false;
                };
                match word.text(text) {
                    "from" | "join" | "left_join" => clause
                        .child_tokens()
                        .any(|t| t.kind == K::InKw && t.end <= offset),
                    "insert_into" | "update" | "delete_from" => word.end <= offset,
                    _ => false,
                }
            });
        if table_position {
            return self.sql_table_names(m, offset);
        }
        let mut out = BTreeMap::new();
        let mut columns = BTreeMap::new();
        for (name, ty) in bindings(&path, &m.typing, text, offset) {
            out.insert(
                name.to_owned(),
                item(name, CompletionKind::Local, Some(show_type(ty))),
            );
            if path
                .iter()
                .filter(|n| n.kind == K::SqlExpr)
                .any(|n| tree::name_of(n, K::Ident, text) == Some(name))
            {
                continue;
            }
            if let Type::Record(row) = ty {
                for (column, ty) in &row.entries {
                    columns
                        .entry(column)
                        .and_modify(|c| *c = None)
                        .or_insert(Some(ty));
                }
            }
        }
        for (name, ty) in columns {
            if let Some(ty) = ty {
                out.entry(name.to_owned())
                    .or_insert_with(|| item(name, CompletionKind::Field, Some(show_type(ty))));
            }
        }
        for (entries, kind) in [
            (CLAUSES, CompletionKind::Keyword),
            (m.typing.sql_dialect.functions, CompletionKind::Value),
        ] {
            for (name, doc) in entries {
                out.entry((*name).to_owned())
                    .or_insert_with(|| item(name, kind, Some((*doc).to_owned())));
            }
        }
        Some(out.into_values().collect())
    }

    fn sql_table_names(&self, m: &Analyzed, offset: u32) -> Option<Vec<Completion>> {
        let text = self.text(m.file);
        let names = self.complete_names(m.file, offset);
        let mut out: BTreeMap<_, _> = m
            .typing
            .sql_tables
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    item(
                        &t.name,
                        CompletionKind::Type,
                        Some(format!("sql table {}", t.name)),
                    ),
                )
            })
            .collect();
        // the module's own tables, which the plugin of std declares
        for c in tables(&m.interface) {
            out.entry(c.label.clone()).or_insert(c);
        }
        for import in m.tree.nodes().filter(|n| n.kind == K::ImportDecl) {
            let module = import.nodes().find(|n| n.kind == K::ModulePath)?;
            let module: String = module.text(text).split_whitespace().collect();
            if let Some(interface) = self.interface(&module) {
                for c in tables(&interface).filter(|c| {
                    names
                        .iter()
                        .any(|n| n.kind == CompletionKind::Type && n.label == c.label)
                }) {
                    out.entry(c.label.clone()).or_insert(c);
                }
            }
        }
        for c in names
            .into_iter()
            .filter(|c| c.kind == CompletionKind::Module)
        {
            out.entry(c.label.clone()).or_insert(c);
        }
        Some(out.into_values().collect())
    }

    pub(super) fn sql_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        if captured(&path) || !path.iter().any(|n| n.kind == K::SqlExpr) {
            return None;
        }
        let scope = bindings(&path, &m.typing, text, offset);
        for node in path.iter().rev().filter(|n| n.kind == K::NameExpr) {
            if let Some((name, ty)) = scope.iter().find(|(name, _)| *name == node.text(text)) {
                return Some((
                    format!("```flick\n{name} : {}\n```", show_type(ty)),
                    node.range(),
                ));
            }
        }
        let node = path.last()?;
        if let Some(token) = node
            .child_tokens()
            .find(|t| t.start <= offset && offset < t.end)
        {
            let word = token.text(text);
            let keyword = node.kind == K::SqlClause && node.child_tokens().next() == Some(token);
            let function = node.kind == K::NameExpr
                && (word == "count_all"
                    || path
                        .iter()
                        .rev()
                        .nth(1)
                        .is_some_and(|n| n.kind == K::AppExpr));
            if let Some((_, doc)) = if keyword {
                CLAUSES
            } else if function {
                m.typing.sql_dialect.functions
            } else {
                &[]
            }
            .iter()
            .find(|(name, _)| *name == word)
            {
                return Some((
                    format!("```sql\n{word}\n```\n\n{doc}"),
                    (token.start, token.end),
                ));
            }
        }
        let (range, ty) = tree::innermost(
            m.typing.types.iter().filter(|(_, ty)| **ty != Type::Error),
            offset,
        )?;
        if m.typing.names.contains_key(&range) || m.typing.type_names.contains_key(&range) {
            return None;
        }
        let name = &text[range.0 as usize..range.1 as usize];
        Some((format!("```flick\n{name} : {}\n```", show_type(ty)), range))
    }
}
