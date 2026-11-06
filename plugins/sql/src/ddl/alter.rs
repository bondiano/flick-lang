//! The changes of a schema after its tables are made: `alter table`,
//! `drop table` and `drop index`.

use super::Word;
use super::schema::{Schema, ident, items, lower, span};

impl Schema {
    pub(super) fn alter_table(&mut self, words: &[Word]) {
        let mut i = 2;
        if lower(&words[i..words.len().min(i + 2)]) == ["if", "exists"] {
            i += 2;
        }
        let Some(target) = words.get(i) else { return };
        let Some(t) = self.known(target) else { return };
        let table = self.tables[t].name.clone();
        for action in items(&words[i + 1..]) {
            self.alter(&table, action);
        }
    }

    fn alter(&mut self, table: &str, action: &[Word]) {
        let lowered = lower(action);
        let word = |i: usize| lowered.get(i).map_or("", String::as_str);
        let skip = usize::from(word(1) == "column");
        let named = |i: usize| action.get(i).map(ident).unwrap_or_default();
        match word(0) {
            "add" if word(1) != "constraint" => {
                let mut at = 1 + skip;
                if lowered[at..lowered.len().min(at + 3)] == ["if", "not", "exists"] {
                    at += 3;
                }
                if let Some(column) = self.column(table, &action[at..]) {
                    self.add_column(table, column, action);
                }
            }
            "drop" if word(1) != "constraint" => {
                let mut at = 1 + skip;
                if lowered[at..lowered.len().min(at + 2)] == ["if", "exists"] {
                    at += 2;
                }
                let name = named(at);
                let Some(t) = self.table(table) else { return };
                let before = t.columns.len();
                t.columns.retain(|c| c.name != name);
                if t.columns.len() == before {
                    self.problem(span(action), format!("no column `{name}` in `{table}`"));
                }
            }
            "rename" if word(1) == "to" => {
                let name = named(2);
                if let Some(t) = self.table(table) {
                    t.name = name;
                }
            }
            "rename" => {
                let (from, to) = (named(1 + skip), named(3 + skip));
                match self
                    .table(table)
                    .and_then(|t| t.columns.iter_mut().find(|c| c.name == from))
                {
                    Some(c) => c.name = to,
                    None => self.problem(span(action), format!("no column `{from}` in `{table}`")),
                }
            }
            "alter" => {
                let name = named(1 + skip);
                let rest: Vec<&str> = lowered[2 + skip..].iter().map(String::as_str).collect();
                let Some(c) = self
                    .table(table)
                    .and_then(|t| t.columns.iter_mut().find(|c| c.name == name))
                else {
                    return self.problem(span(action), format!("no column `{name}` in `{table}`"));
                };
                match rest.as_slice() {
                    ["set", "not", "null"] => c.not_null = true,
                    ["drop", "not", "null"] => c.not_null = false,
                    ["set", "default", ..] => c.default = true,
                    ["drop", "default"] => c.default = false,
                    ["type", ty @ ..] | ["set", "data", "type", ty @ ..] => {
                        c.ty = ty
                            .iter()
                            .take_while(|w| **w != "using")
                            .map(|w| (*w).to_owned())
                            .collect();
                    }
                    _ => self.unsupported(action, "this change of a column"),
                }
            }
            _ => self.unsupported(action, "this change of a table"),
        }
    }

    pub(super) fn drop_table(&mut self, words: &[Word]) {
        let mut i = 2;
        if lower(&words[i..words.len().min(i + 2)]) == ["if", "exists"] {
            i += 2;
        }
        for item in items(&words[i..]) {
            let Some(w) = item
                .first()
                .filter(|w| !matches!(w.lower().as_str(), "cascade" | "restrict"))
            else {
                continue;
            };
            if let Some(t) = self.known(w) {
                self.tables.remove(t);
            }
        }
    }

    pub(super) fn drop_index(&mut self, words: &[Word]) {
        let lowered = lower(words);
        let exists = lowered.get(2).map(String::as_str) == Some("if");
        for name in lowered[if exists { 4 } else { 2 }..]
            .iter()
            .filter(|w| !matches!(w.as_str(), "," | "cascade" | "restrict"))
        {
            let before = self.indexes.len();
            self.indexes.retain(|i| i != name);
            if self.indexes.len() == before && !exists {
                self.problem(span(words), format!("no index `{name}` here"));
            }
        }
    }
}
