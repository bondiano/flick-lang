//! `sql migration N "name" { DDL }`: the module's tables are the schema its
//! migrations build, each with its row type and `Name$table` as `sql table`
//! declares them, and the module has the value `migrations`, the list
//! `Database.Migrate.run` applies. A statement that does not apply is
//! `FLICK-Q013`, one outside the DDL subset `FLICK-Q010`.

use super::super::Checker;
use super::super::ty::Type;
use super::super::typing::{SqlColumn, SqlTable};
use super::table::base_type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::ddl::{self, Migration, Table};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

/// The module of `Migration`.
const MIGRATE: &str = "Database.Migrate";

/// The migrations of these declarations.
pub fn migrations(decls: &[&Node], text: &str) -> Vec<Migration> {
    decls
        .iter()
        .filter(|d| d.kind == K::SqlMigrationDecl)
        .map(|d| ddl::migration(d, text))
        .collect()
}

impl Checker<'_> {
    /// The tables the migrations leave, the problems on the way reported,
    /// and the value `migrations` declared.
    pub(in super::super) fn migrated_tables(&mut self, decls: &[&Node]) -> Vec<Table> {
        let migrations = migrations(decls, self.text);
        if migrations.is_empty() {
            return Vec::new();
        }
        let (tables, problems) = ddl::schema(&migrations);
        for p in problems {
            let code = if p.unsupported {
                codes::Q010
            } else {
                codes::Q013
            };
            self.error(code, Span::new(self.file, p.range.0, p.range.1), p.message);
        }
        let list = Type::app(Type::lang(Lang::List), Type::con(MIGRATE, "Migration"));
        let entity = self.env.entity(Namespace::Value, "migrations");
        self.env.set_value(entity, list);
        self.env.declare(Namespace::Value, "migrations");
        tables
    }

    /// The row of a migrated table, as `sql table` would declare it.
    pub(in super::super) fn migrated_row(&mut self, table: &Table, name: &str) -> Type {
        let dialect = self.options.sql;
        let mut columns = Vec::new();
        for c in &table.columns {
            let words: Vec<&str> = c.ty.iter().map(String::as_str).collect();
            let Some(sql) = dialect
                .column_type(&words)
                .filter(|t| t.words.split(' ').count() == words.len())
            else {
                let known: Vec<&str> = dialect.types.iter().map(|t| t.words).collect();
                self.error(
                    codes::Q010,
                    Span::new(self.file, c.range.0, c.range.1),
                    format!(
                        "unknown SQL type `{}`; known: {}",
                        c.ty.join(" "),
                        known.join(", ")
                    ),
                );
                continue;
            };
            columns.push(SqlColumn {
                name: c.name.clone(),
                kind: sql.kind.to_owned(),
                nullable: !c.not_null,
                write: c.write().index().to_owned(),
                via: None,
                ty: base_type(dialect, sql.kind),
            });
        }
        self.declare_table(SqlTable {
            decl: table.range,
            name: name.to_owned(),
            table: table.name.clone(),
            columns,
        })
    }
}
