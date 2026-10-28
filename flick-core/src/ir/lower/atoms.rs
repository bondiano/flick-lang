//! Calls of `Atom` the lowering completes: a serializable atom gets its
//! key, a write its place in the source.

use super::Lower;
use crate::ir::{Expr, ExprKind, Lit, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;

/// Whether `Atom.name` is a call the lowering completes.
pub(super) fn completed(name: &str) -> bool {
    matches!(
        name,
        "serializable"
            | "serializable_family"
            | "serializable_task"
            | "serializable_result"
            | "set"
            | "update"
            | "modify"
    )
}

impl Lower<'_> {
    /// `Atom.name` at `node` given its key or its place.
    pub(super) fn atom_call(&self, name: &str, node: &Node, span: Span) -> ExprKind {
        let (module, name, arg) = match name {
            // a write names its place, which a debug log of it shows
            "set" | "update" | "modify" => ("Atom", format!("{name}_at"), self.location(node)),
            // the key is the binding the check let it define
            "serializable_task" | "serializable_result" => (
                "Atom",
                format!("{name}_at"),
                format!("{}.{}", self.module, self.definition),
            ),
            _ => (
                "Reactivity.Atom",
                name.to_owned(),
                format!("{}.{}", self.module, self.definition),
            ),
        };
        ExprKind::App {
            func: Box::new(Expr::new(
                ExprKind::Global {
                    module: module.to_owned(),
                    name,
                },
                span,
            )),
            arg: Box::new(Expr::new(ExprKind::Lit(Lit::String(arg)), span)),
            stage: Stage::Pure,
            arg_first: false,
        }
    }
}
