//! The SQL eDSL for `PostgreSQL`: `sql table` declarations and `sql { … }`
//! queries, elaborated into checked plans whose types the general checker
//! unifies with the rest of the program. Rules: `docs/src/content/docs/reference/sql.md`.

mod expr;
mod query;
mod select;
mod table;

pub(crate) use table::TYPES as SQL_TYPES;
