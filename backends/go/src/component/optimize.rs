//! The Go release adapter supplies primitive bindings and visibility to the
//! shared type-preserving optimizer passes.

use flick_backend::{BuildRequest, ModuleRole};
use flick_backend_support::Known;
use flick_backend_support::opt;
use flick_ir::Module;

use super::data::Data;

pub(super) fn primitives(
    request: &BuildRequest,
    data: &Data,
    lang: &Known<'_>,
) -> opt::eval::ReviewedPrimitives {
    let builtin = |owner: &str| {
        (request.modules.iter()).any(|m| m.name == owner && m.role == ModuleRole::Builtin)
    };
    opt::eval::ReviewedPrimitives::build(
        &data.program.modules,
        &builtin,
        lang,
        &|module, foreign, prim| data.bindings.allows_primitive(module, foreign, prim),
    )
}

pub(super) fn module(
    request: &BuildRequest,
    data: &Data,
    module: &Module,
    lang: &Known<'_>,
    primitives: &dyn opt::eval::PrimitiveEval,
) -> Module {
    let mut result = module.clone();
    let exports = |owner: &str| data.facts.get(owner).map(|f| f.exports.as_slice());
    let exported = |name: &str| {
        exports(&module.name).is_some_and(|e| e.iter().any(|e| e == name))
            || (module.initialization.prefixes.iter()).any(|p| p.bindings.iter().any(|n| n == name))
    };
    let generics = |owner: &str, name: &str| {
        let module = data.program.modules.iter().find(|m| m.name == owner)?;
        let exports = exports(owner)?;
        opt::names_only(module, name, &|name| exports.iter().any(|e| e == name))
    };
    let arity = |owner: &str, name: &str| {
        let module = data.program.modules.iter().find(|m| m.name == owner)?;
        (module.items.iter())
            .filter_map(opt::arity)
            .find_map(|(n, arity)| (n == name).then_some(arity))
    };
    let builtin = |owner: &str| {
        (request.modules.iter()).any(|m| m.name == owner && m.role == ModuleRole::Builtin)
    };
    let config = opt::eval::EvalConfig::default();
    let _ = opt::optimize_with_eval(
        &mut result,
        &exported,
        &generics,
        &arity,
        lang,
        &builtin,
        &config,
        primitives,
    );
    result
}
