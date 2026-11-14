//! Compiler adapters for portable optimizer passes; semantic rules live in support.

use crate::codegen::Known;
use crate::ir::{self, Expr, Module};
use crate::types;
use flick_backend_support::opt as portable;
pub use flick_backend_support::opt::{Arity, arity, js_number, json_string, names_only, rename};

pub use portable::Generics;

pub fn optimize(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    generics: Generics,
    arity: Arity,
) {
    let table = ir::lang_items();
    let known = Known::of(&table);
    let builtin = |module: &str| types::builtins().iter().any(|i| i.module == module);
    portable::optimize(module, exported, generics, arity, &known, &builtin);
}

pub fn simplify(expr: &mut Expr) {
    let table = ir::lang_items();
    let known = Known::of(&table);
    portable::simplify(expr, &known);
}

#[cfg(test)]
mod compose_tests;
#[cfg(test)]
mod gadt_tests;
#[cfg(test)]
mod inline_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod devirt_tests;
