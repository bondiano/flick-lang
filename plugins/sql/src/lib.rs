//! The plugin of `std` for `sql table Name "table" { column : type as T }`:
//! a table of the database as its row type and the companion `Name.table`
//! queries read it from. What the types cannot state is checked here: the
//! SQL types and attributes of the columns (`std/Q010`), a column given
//! twice (`std/Q001`) and a column of another type than its SQL type's
//! without a codec (`std/Q011`).

mod code;
mod dialect;
mod editor;
mod table;

use flick_plugin::{Arena, Block, Compiler, Completion, Diagnostic, Fact, Hover, Piece, Plugin};

pub struct Sql;

/// A column whose type is not the one of its SQL type, and has no codec.
pub const MISMATCH: &str = "Q011";

impl Plugin for Sql {
    fn collect(&self, _: &[Block], _: &mut dyn Compiler) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        match table::read(block, Some(&mut *compiler)) {
            (Some(table), _) => Ok(code::arena(block, &table, compiler)),
            (None, wrong) => Err(wrong),
        }
    }

    fn check(&self, block: &Block, _: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
        let (table, mut wrong) = table::read(block, None);
        let Some(table) = table else {
            return wrong;
        };
        for c in &table.columns {
            let Some(fact) = facts.iter().find(|f| f.span == c.ty) else {
                continue;
            };
            let Some(exact) = &fact.ty.exact else {
                continue;
            };
            if *exact != c.sql.exact {
                let message = format!(
                    "a `{}` column holds `{}`, not `{}`: give a codec with `via`",
                    c.sql.kind, c.sql.flick, fact.ty.shown
                );
                wrong.push(Diagnostic::error(message, c.ty).with_code(MISMATCH));
            }
        }
        wrong
    }

    fn format(&self, block: &Block, _: &[u8], compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
        editor::format(block, compiler)
    }

    fn hover(&self, block: &Block, _: &[u8], offset: u32, _: &[Fact]) -> Option<Hover> {
        editor::hover(block, offset)
    }

    fn complete(&self, block: &Block, _: &[u8], offset: u32) -> Vec<Completion> {
        editor::complete(block, offset)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::MISMATCH;
    use crate::table::{DUPLICATE, MALFORMED};

    #[test]
    fn std_explains_every_code_of_the_plugin() {
        let explain = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/std/explain");
        for code in [DUPLICATE, MALFORMED, MISMATCH] {
            let path = explain.join(format!("{code}.md"));
            assert!(path.exists(), "no {}", path.display());
        }
    }
}
