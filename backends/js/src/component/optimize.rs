//! Release passes operate on explicit IR, visibility and lang-item facts.

use super::data::Data;
use crate::codegen::{self, Known};
use flick_backend::{BuildRequest, ModuleRole, Profile};
use flick_backend_support::opt;
use flick_ir::Module;
use std::collections::{HashMap, HashSet};

pub(super) type Exports = (
    HashMap<(String, String), usize>,
    HashSet<(String, String, String)>,
);

pub(super) fn exports(request: &BuildRequest, data: &Data) -> Exports {
    let mut exports = Exports::default();
    if request.target.profile != Profile::Release {
        return exports;
    }
    for module in &data.program.modules {
        let facts = &data.facts[&module.name];
        for (name, arity) in module.items.iter().filter_map(codegen::exported_worker) {
            if facts.exports.iter().any(|e| e == name)
                || module.exports.host.iter().any(|h| h.name == name)
                || request
                    .exports
                    .iter()
                    .any(|e| e.symbol.module == module.name && e.symbol.name == name)
            {
                exports.0.insert((module.name.clone(), name.into()), arity);
            }
        }
        for (instance, method) in module.items.iter().flat_map(codegen::direct_methods) {
            exports
                .1
                .insert((module.name.clone(), instance.into(), method.into()));
        }
    }
    exports
}

pub(super) fn primitives(
    request: &BuildRequest,
    data: &Data,
    lang: &Known<'_>,
) -> opt::eval::ReviewedPrimitives {
    let builtin = |owner: &str| {
        request
            .modules
            .iter()
            .any(|m| m.name == owner && m.role == ModuleRole::Builtin)
    };
    opt::eval::ReviewedPrimitives::for_js(
        &data.program.modules,
        &builtin,
        lang,
        data.rt,
        data.bridge,
        &|module, spec| {
            data.foreign
                .iter()
                .find(|r| r.module == module && r.spec == spec)
                .map(|r| r.specifier.as_str())
        },
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
    let exported = |name: &str| {
        data.facts[&module.name].exports.iter().any(|e| e == name)
            || module.exports.host.iter().any(|h| h.name == name)
            || request
                .exports
                .iter()
                .any(|e| e.symbol.module == module.name && e.symbol.name == name)
            || module
                .initialization
                .prefixes
                .iter()
                .any(|p| p.bindings.iter().any(|n| n == name))
    };
    let generics = |owner: &str, name: &str| {
        let module = data.program.modules.iter().find(|m| m.name == owner)?;
        let facts = data.facts.get(owner)?;
        opt::names_only(module, name, &|name| {
            facts.exports.iter().any(|e| e == name)
        })
    };
    let arity = |owner: &str, name: &str| {
        let module = data.program.modules.iter().find(|m| m.name == owner)?;
        module
            .items
            .iter()
            .filter_map(opt::arity)
            .find_map(|(n, arity)| (n == name).then_some(arity))
    };
    let builtin = |owner: &str| {
        request
            .modules
            .iter()
            .any(|m| m.name == owner && m.role == ModuleRole::Builtin)
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
