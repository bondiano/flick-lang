//! What the checker records of `sql` queries for the lowering.

use super::typing::Range;

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
