//! The SQL eDSL of the dialect `flick.toml` selects: `sql { … }` queries,
//! elaborated into checked plans whose types the general checker unifies
//! with the rest of the program. Rules: `docs/src/content/docs/reference/sql.md`.

pub mod dialect;
mod expr;
mod placeholder;
mod query;
mod select;
