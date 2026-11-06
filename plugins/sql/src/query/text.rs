//! The SQL text of a query as it is built: pieces of text, the placeholder
//! of each value, and the casts the type of a value decides, which the code
//! writes as `Sql.cast_open (Sql.Derive.like px) ++ "$1" ++ …`.

use std::fmt::Write;

/// A piece of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// The placeholder of a slot, `$n`, cast as its type says.
    Slot(usize),
    /// `Sql.sum_cast` or `Sql.avg_cast` of a proxy.
    Cast(&'static str, usize),
}

/// A SQL text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sql(pub Vec<Piece>);

impl Sql {
    pub fn text(s: impl Into<String>) -> Sql {
        Sql(vec![Piece::Text(s.into())])
    }

    pub fn slot(slot: usize) -> Sql {
        Sql(vec![Piece::Slot(slot)])
    }

    pub fn push(&mut self, s: &str) {
        if let Some(Piece::Text(last)) = self.0.last_mut() {
            last.push_str(s);
        } else {
            self.0.push(Piece::Text(s.to_owned()));
        }
    }

    pub fn append(&mut self, other: Sql) {
        for piece in other.0 {
            match piece {
                Piece::Text(s) => self.push(&s),
                other => self.0.push(other),
            }
        }
    }

    /// `prefix`, `inner` and `suffix`.
    pub fn wrap(prefix: &str, inner: Sql, suffix: &str) -> Sql {
        let mut out = Sql::text(prefix);
        out.append(inner);
        out.push(suffix);
        out
    }

    /// The texts joined by `sep`.
    pub fn join(items: impl IntoIterator<Item = Sql>, sep: &str) -> Sql {
        let mut out = Sql::default();
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                out.push(sep);
            }
            out.append(item);
        }
        out
    }

    /// The text with each placeholder and cast by name: what two values
    /// are told apart by, as a key of `group_by`.
    pub fn key(&self) -> String {
        let mut out = String::new();
        for piece in &self.0 {
            match piece {
                Piece::Text(s) => out.push_str(s),
                Piece::Slot(n) => {
                    let _ = write!(out, "${}", n + 1);
                }
                Piece::Cast(name, proxy) => {
                    let _ = write!(out, "<{name} {proxy}>");
                }
            }
        }
        out
    }
}

/// `"name"` with inner quotes doubled.
pub fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
