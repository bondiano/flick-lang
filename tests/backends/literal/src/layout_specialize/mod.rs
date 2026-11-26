//! Layout-aware polymorphic specialization fixture mode (`case = "layout-specialize"`,
//! `Mode::WholeProgram`) for the `test/literal` backend (#739, #743).
//!
//! Runs `flick_backend_support::specialize` + `LayoutEngine` on the incoming typed
//! `flick_ir::Program`, writes `layout-specialize-report.json` with deterministic
//! layout, classification, specialization, and boxing metrics, and emits a runnable
//! JavaScript module executing the specialized program directly on Node (both unboxed
//! specialized definitions and boxed fallback forms).

mod emit;
mod expr;
mod report;
mod runtime;

use std::collections::BTreeSet;

use flick_backend::{Artifact, BuildRequest, Diagnostic, Mode, ModuleRole};
use flick_backend_support::layout::PrimitiveTable;
use flick_backend_support::specialize::{DefKey, SpecializeConfig, SpecializedProgram, specialize};
use flick_ir::{ItemKind, Program};

use super::{diagnostic, typed_ir};
use emit::emit_whole_program_js;
use report::report_artifact;

fn option_usize(request: &BuildRequest, name: &str) -> Option<usize> {
    let key = format!("option.{name}");
    request
        .target
        .options
        .iter()
        .find(|o| o.key == key)
        .and_then(|o| o.value.parse::<usize>().ok())
}

fn config_from_request(request: &BuildRequest) -> SpecializeConfig {
    let mut cfg = SpecializeConfig::default();
    if let Some(max_instances) = option_usize(request, "max-instances") {
        cfg.max_instances = max_instances;
    }
    if let Some(max_output_bytes) = option_usize(request, "max-output-bytes") {
        cfg.max_output_bytes = Some(max_output_bytes);
    }
    cfg
}

fn roots_from_request(program: &Program, request: &BuildRequest) -> Vec<DefKey> {
    let mut roots: Vec<DefKey> = request
        .entries
        .iter()
        .map(|e| (e.symbol.module.clone(), e.symbol.name.clone()))
        .collect();
    if roots.is_empty() {
        let own: BTreeSet<&str> = request
            .modules
            .iter()
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| m.name.as_str())
            .collect();
        for module in &program.modules {
            if !own.contains(module.name.as_str()) {
                continue;
            }
            for item in &module.items {
                if let ItemKind::Value { name, .. } = &item.kind {
                    roots.push((module.name.clone(), name.clone()));
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn run_specialize(
    program: &Program,
    request: &BuildRequest,
) -> Result<SpecializedProgram, Vec<Diagnostic>> {
    if request.target.mode != Mode::WholeProgram {
        return Err(diagnostic(
            "layout-specialize requires whole-program compilation mode",
            None,
        ));
    }
    typed_ir::validate_program(program, request)?;
    let config = config_from_request(request);
    let roots = roots_from_request(program, request);
    specialize(program, &roots, &PrimitiveTable::target_64(), &config)
        .map_err(|e| diagnostic(&e.to_string(), None))
}

pub(super) fn check_target(
    program: &Program,
    request: &BuildRequest,
) -> Result<(), Vec<Diagnostic>> {
    let _ = run_specialize(program, request)?;
    Ok(())
}

pub(super) type GeneratedBundle = (Vec<(String, Vec<u8>)>, Artifact);

pub(super) fn generate(
    program: &Program,
    request: &BuildRequest,
) -> Result<GeneratedBundle, Vec<Diagnostic>> {
    let specialized = run_specialize(program, request)?;
    let report = report_artifact(program, &specialized, request)?;

    let own: BTreeSet<&str> = request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own)
        .map(|m| m.name.as_str())
        .collect();
    let entry_module = request
        .entries
        .first()
        .map(|e| e.symbol.module.as_str())
        .or_else(|| own.iter().next().copied());

    let mut sources = Vec::new();
    for m in &request.modules {
        if m.role != ModuleRole::Own {
            continue;
        }
        let code = if entry_module == Some(m.name.as_str()) {
            emit_whole_program_js(program, &specialized, &m.name)
        } else {
            "// included in whole-program entry bundle\n".to_owned()
        };
        sources.push((m.name.clone(), code.into_bytes()));
    }
    Ok((sources, report))
}
