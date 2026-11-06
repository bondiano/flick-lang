//! The tables of `sql migration`s: the type `Name` of a row and the
//! companion value `Name$table` of type `Sql.Table { column : Sql.Column T
//! n w, … }`, whose indices say whether the column may be `NULL` and what an
//! `INSERT` must give it. A `sql table` block is the plugin of std's.

use super::super::Checker;
use super::super::ty::{Row, Type};
use super::super::typing::SqlTable;
use super::dialect::Dialect;
use crate::iface::Namespace;
use crate::types::lang::{self, Lang};

pub const SQL: &str = "Sql";

/// The Flick type of a value of a runtime kind, without a codec.
pub fn base_type(dialect: &Dialect, kind: &str) -> Type {
    match dialect.flick_type(kind) {
        "BigDecimal" => Type::con("BigDecimal", "BigDecimal"),
        // the dialect names prelude types
        other => Type::con(&lang::item(Lang::Prelude).module, other),
    }
}

/// The record of a table's rows: each column's type, in `Option` when nullable.
fn row_type(table: &SqlTable) -> Type {
    Type::Record(Row::closed(
        table
            .columns
            .iter()
            .map(|c| {
                let ty = if c.nullable {
                    Type::app(Type::lang(Lang::Option), c.ty.clone())
                } else {
                    c.ty.clone()
                };
                (c.name.clone(), ty)
            })
            .collect(),
    ))
}

/// The type of the companion `Name$table`.
fn table_type(table: &SqlTable) -> Type {
    let columns = table.columns.iter().map(|c| {
        let null = index(if c.nullable { "Nullable" } else { "NonNull" });
        let column = Type::apps(
            Type::con(SQL, "Column"),
            [c.ty.clone(), null, index(&c.write)],
        );
        (c.name.clone(), column)
    });
    Type::apps(
        Type::con(SQL, "Table"),
        [
            Type::Symbol(table.table.clone()),
            Type::Record(Row::closed(columns.collect())),
        ],
    )
}

/// `Sql.NonNull`, `Sql.Generated`, ….
pub fn index(name: &str) -> Type {
    Type::con(SQL, name)
}

impl Checker<'_> {
    /// The row type of a table; defines its companion `Name$table` and
    /// records the table for lowering.
    pub(in super::super) fn declare_table(&mut self, info: SqlTable) -> Type {
        let name = info.name.clone();
        let entity = self.env.entity(
            Namespace::Value,
            &super::super::companion_name(&name, "table"),
        );
        self.env.set_value(entity, table_type(&info));
        let row = row_type(&info);
        self.typing.sql_tables.push(info);
        row
    }
}
