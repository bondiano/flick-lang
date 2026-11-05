//! What the checker records of `sql` tables and queries for the lowering.

use super::ty::Type;
use super::typing::Range;

/// A `sql table` declaration.
#[derive(Debug, Clone)]
pub struct SqlTable {
    pub decl: Range,
    /// The row type; the table is the companion `Name$table`.
    pub name: String,
    /// The table's SQL name.
    pub table: String,
    pub columns: Vec<SqlColumn>,
}

#[derive(Debug, Clone)]
pub struct SqlColumn {
    pub name: String,
    /// How the runtime passes and reads values: `bigint`, `number`, `text`, ….
    pub kind: String,
    pub nullable: bool,
    /// `Required`, `Defaulted` or `Generated`.
    pub write: String,
    /// The `via` codec.
    pub via: Option<Range>,
    pub ty: Type,
    /// The enum the column holds without a `via`, its module and name: its
    /// values are read through its `parse`.
    pub enumerates: Option<(String, String)>,
}

/// A checked query: its SQL text with `$n` placeholders, the expressions
/// giving its tables, and how each parameter and result crosses the driver.
#[derive(Debug, Clone, Default)]
pub struct SqlPlan {
    pub text: String,
    pub tables: Vec<Range>,
    /// In placeholder order.
    pub params: Vec<SqlValue>,
    pub columns: Vec<SqlValue>,
}

/// A parameter or a result column: named, or a literal of the query.
#[derive(Debug, Clone)]
pub struct SqlValue {
    pub name: String,
    pub literal: Option<Range>,
    pub form: SqlForm,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlForm {
    /// A runtime kind of a SQL base type.
    Kind(String),
    /// Like the column of the table at this index, with its codec.
    Column(usize, String),
}
