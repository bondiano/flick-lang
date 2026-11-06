//! The SQL eDSL of the dialect `flick.toml` selects: `sql table`
//! declarations and `sql { … }` queries, elaborated into checked plans whose types the general checker
//! unifies with the rest of the program. Rules: `docs/src/content/docs/reference/sql.md`.

pub mod dialect;
mod expr;
pub(super) mod migration;
mod placeholder;
mod query;
mod select;
mod table;
