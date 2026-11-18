//! Release passes over explicit IR and visibility facts, as the native
//! build runs them: the tuple-match fold, then the portable passes.

use flick_backend::{BuildRequest, ModuleRole};
use flick_backend_support::Known;
use flick_backend_support::opt::{self, tuple_match};
use flick_ir::Module;

use super::data::Data;

pub(super) fn module(
    request: &BuildRequest,
    data: &Data,
    module: &Module,
    lang: &Known<'_>,
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
    tuple_match::fold_tuple_matches(&mut result);
    opt::optimize(&mut result, &exported, &generics, &arity, lang, &builtin);
    result
}
