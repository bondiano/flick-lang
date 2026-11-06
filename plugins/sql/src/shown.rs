//! What the plugin publishes of each table, for the queries of every module
//! and group: its name in the database, and each column's runtime kind, the
//! Flick type of its SQL type, the type it holds, whether it may be `NULL`,
//! what an `INSERT` must give it, and whether it holds another type than
//! its SQL type's, as a codec or an enum makes it.

use flick_plugin::{Block, Place};
use serde::{Deserialize, Serialize};

use crate::migration::is_migration;
use crate::table;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    pub sql: String,
    pub columns: Vec<ShownColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownColumn {
    pub name: String,
    pub kind: String,
    pub base: String,
    pub ty: String,
    pub nullable: bool,
    pub write: String,
    pub via: bool,
}

/// A table of a module, by the name of its row type.
pub type Named = (String, String, Shown);

/// The tables the `sql table` blocks of `blocks` declare.
pub fn tables(blocks: &[Block]) -> Vec<Named> {
    let declared = (blocks.iter()).filter(|b| b.place == Place::Declaration);
    let mut out = Vec::new();
    for block in declared.filter(|b| !is_migration(b)) {
        let (Some(table), _) = table::read(block, None) else {
            continue;
        };
        let columns = (table.columns.iter())
            .map(|c| {
                let ty = block.text_of(c.ty).to_owned();
                let base = ty == c.sql.flick || ty == c.sql.exact;
                ShownColumn {
                    name: c.name.clone(),
                    kind: c.sql.kind.to_owned(),
                    base: c.sql.flick.to_owned(),
                    ty,
                    nullable: c.nullable,
                    write: c.write.to_owned(),
                    via: c.via.is_some() || !base,
                }
            })
            .collect();
        let shown = Shown {
            sql: table.sql,
            columns,
        };
        out.push((block.module.clone(), table.name, shown));
    }
    out
}
