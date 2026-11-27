//! Compiler adapters for portable optimizer passes; semantic rules live in support.

pub mod perceus;

use crate::codegen::Known;
use crate::ir::{self, Expr, Module};
use crate::types;
use flick_backend_support::opt as portable;
pub use flick_backend_support::opt::tuple_match;
pub use flick_backend_support::opt::{
    Arity, Generic, arity, js_log, js_number, json_string, names_only, rename,
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

#[cfg(all(test, feature = "native-js"))]
mod compose_tests;
#[cfg(all(test, feature = "native-js"))]
mod eval_primitives_tests;
#[cfg(all(test, feature = "native-js"))]
mod eval_tests;
#[cfg(test)]
mod gadt_tests;
#[cfg(all(test, feature = "native-js"))]
mod inline_tests;
#[cfg(all(test, feature = "native-js"))]
mod lazy_local_tests;
#[cfg(all(test, feature = "native-js"))]
mod lazy_tests;
#[cfg(all(test, feature = "native-js"))]
mod list_shape_tests;
#[cfg(all(test, feature = "native-js"))]
mod projection_tests;
#[cfg(all(test, feature = "native-js"))]
mod record_shape_tests;
#[cfg(all(test, feature = "native-js"))]
mod shape_tests;
#[cfg(all(test, feature = "native-js"))]
mod tests;

#[cfg(all(test, feature = "native-js"))]
mod devirt_tests;
