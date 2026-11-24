//! Compiler adapters for portable optimizer passes; semantic rules live in support.

pub mod perceus;

use crate::codegen::Known;
use crate::ir::{self, Expr, Module};
use crate::types;
use flick_backend_support::opt as portable;
pub use flick_backend_support::opt::tuple_match;
pub use flick_backend_support::opt::{
    Arity, Generic, arity, js_number, json_string, names_only, rename,
};
#[cfg(feature = "native-project")]
pub use flick_backend_support::opt::{strip_legacy_bare_generic, strip_legacy_bare_types};

pub use portable::Generics;

pub fn optimize(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    generics: Generics,
    arity: Arity,
) {
    optimize_with_primitives(
        module,
        exported,
        generics,
        arity,
        &portable::eval::NoPrimitives,
    );
}

pub fn optimize_with_primitives(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    generics: Generics,
    arity: Arity,
    primitives: &dyn portable::eval::PrimitiveEval,
) {
    let table = ir::lang_items();
    let known = Known::of(&table);
    let builtin = |module: &str| {
        types::builtin_sources()
            .iter()
            .any(|(name, _)| *name == module)
    };
    let config = portable::eval::EvalConfig::default();
    let _ = portable::optimize_with_eval(
        module, exported, generics, arity, &known, &builtin, &config, primitives,
    );
}

#[cfg(feature = "native-project")]
pub fn optimize_legacy_bare(
    module: &mut Module,
    exported: &dyn Fn(&str) -> bool,
    generics: Generics,
    arity: Arity,
    primitives: &dyn portable::eval::PrimitiveEval,
) {
    let table = ir::lang_items();
    let known = Known::of(&table);
    let builtin = |module: &str| {
        types::builtin_sources()
            .iter()
            .any(|(name, _)| *name == module)
    };
    let config = portable::eval::EvalConfig::default();
    let _ = portable::optimize_legacy_bare(
        module, exported, generics, arity, &known, &builtin, &config, primitives,
    );
}

pub fn simplify(expr: &mut Expr) {
    let table = ir::lang_items();
    let known = Known::of(&table);
    portable::simplify(expr, &known);
}

#[cfg(test)]
mod compose_tests;
#[cfg(test)]
mod eval_primitives_tests;
#[cfg(test)]
mod eval_tests;
#[cfg(test)]
mod gadt_tests;
#[cfg(test)]
mod inline_tests;
#[cfg(test)]
mod lazy_local_tests;
#[cfg(test)]
mod lazy_tests;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod devirt_tests;
