//! Typed IR: a checked module with everything code generation must not
//! guess made explicit: each application stage and whether it runs through
//! the runtime, the dictionaries of class constraints, lifts of pure
//! functions into runtime positions, what every name refers to, and the
//! source range of every node. Evaluation order is the order of the nodes:
//! children left to right, each application stage after its argument.
//!
//! Built from the syntax tree and the checker's [`Typing`](crate::types::Typing)
//! by [`lower`]; printed for tests by [`print`].

mod exports;
mod lower;
mod print;
#[cfg(any(not(target_arch = "wasm32"), feature = "native-project"))]
pub(crate) use flick_backend_support::reach;

pub use exports::exports;
pub use flick_ir::{
    Adapter, Arm, BinOp, Binding, Copies, Expr, ExprKind, Foreign, ForeignMode, Handled, Handler,
    Item, ItemKind, Label, LangItem, Lit, Module, OpArm, Param, Pat, ResultAdapter, Stage, Stmt,
    TypeName, operation_arity,
};
pub use lower::lower;
pub use print::print;

use flick_ir::Namespace as IrNamespace;

use crate::iface::Namespace;
use crate::types::lang::{self, Lang};

/// The lang items of the built-in modules, as a program gives them to its
/// backend: every item they declare.
pub fn lang_items() -> Vec<LangItem> {
    let table = lang::table();
    (Lang::ALL.iter())
        .filter_map(|&item| {
            let found = table.find(item)?;
            Some(LangItem {
                lang: item.name().to_owned(),
                module: found.module.clone(),
                namespace: match found.namespace {
                    Namespace::Value => IrNamespace::Value,
                    Namespace::Type => IrNamespace::Type,
                    Namespace::Constructor => IrNamespace::Constructor,
                },
                name: found.name.clone(),
            })
        })
        .collect()
}

/// What the compiler names through lang items in the IR.
pub trait ExprLang {
    /// The built-in value the lang item `item` names.
    fn lang(item: Lang) -> Self;
    /// Whether this is the built-in value the lang item `item` names.
    fn is_lang(&self, item: Lang) -> bool;
}

impl ExprLang for ExprKind {
    fn lang(item: Lang) -> ExprKind {
        let item = lang::item(item);
        ExprKind::Global {
            module: item.module.clone(),
            name: item.name.clone(),
        }
    }

    fn is_lang(&self, item: Lang) -> bool {
        matches!(self, ExprKind::Global { module, name } if lang::is(item, module, name))
    }
}

#[cfg(test)]
mod lang_tests;
#[cfg(test)]
pub(crate) mod tests;
