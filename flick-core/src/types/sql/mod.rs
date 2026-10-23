//! The SQL eDSL for `PostgreSQL`: `sql table` declarations and `sql { … }`
//! queries, elaborated into checked plans whose types the general checker
//! unifies with the rest of the program. Rules: `docs/reference/sql.md`.

mod expr;
mod query;
mod select;
mod table;
