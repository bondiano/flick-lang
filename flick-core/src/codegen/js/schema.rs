//! Refined types and `codec` blocks: runtime calls that build the inert
//! schema and codec trees.

use super::{Gen, names};
use crate::ir::Expr;
use crate::types::companion_name;

impl Gen<'_> {
    /// `T$check`, `T$parse` and `T$value` of a refined type.
    pub(super) fn refined(&mut self, name: &str, check: &Expr) -> String {
        let (mut stmts, value) = self.block(|g| g.function(&[], check, None));
        let check_name = companion_name(name, "check");
        stmts.push(self.export(&check_name, &value));
        let parse = format!(
            "{}({}, {check_name})",
            self.rt("refinedParse"),
            names::string(name)
        );
        stmts.push(self.export(&companion_name(name, "parse"), &parse));
        let identity = format!("{} $x", self.arrow("$x"));
        stmts.push(self.export(&companion_name(name, "value"), &identity));
        stmts.join("\n")
    }

    /// A `codec` block: its fields, evaluated in order.
    pub(super) fn codec_block(&mut self, name: &str, fields: &[(String, Expr)]) -> String {
        let values: Vec<&Expr> = fields.iter().map(|(_, v)| v).collect();
        let values = self.ordered(&values);
        let entries: Vec<String> = fields
            .iter()
            .zip(values)
            .map(|((l, _), v)| format!("[{}, {v}]", names::string(l)))
            .collect();
        format!(
            "{}({}, [{}])",
            self.rt("codecBlock"),
            names::string(name),
            entries.join(", ")
        )
    }
}
