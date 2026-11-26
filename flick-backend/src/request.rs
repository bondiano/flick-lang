use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{FileId, Program};

use crate::validate::{Budget, Limits, id, runtime_dependency, string};
use crate::{BuildRequest, InputKind, Inputs, Mode, ModuleInput, ModuleRole, VERSION};

mod graph;
mod sources;

pub use sources::validate_source_spans;

/// Validate the closed graph and all referenced side inputs before target
/// preflight. This is structural validation, not a proof of IR semantics.
pub fn validate_request(
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
    limits: Limits,
) -> Result<Program, String> {
    validate_size(request, limits)?;
    if request.target.backend_api != VERSION {
        return Err("request is for another backend API".into());
    }
    if request.backend.sha256.len() != 64
        || !request
            .backend
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err("backend component needs a lowercase SHA-256 pin".into());
    }
    let program = Program::from_cbor(&request.program).map_err(|e| e.to_string())?;
    if request.target.ir_version != program.ir_version {
        return Err("request IR version differs from its program".into());
    }
    let mut ids = BTreeSet::new();
    let mut manifest = BTreeMap::new();
    let mut size = 0_u64;
    for input in &request.inputs {
        id(&input.id, &mut ids, limits)?;
        size = size
            .checked_add(input.size)
            .ok_or("side input size overflow")?;
        if size > u64::try_from(limits.request_bytes).unwrap_or(u64::MAX) {
            return Err("backend side inputs exceed their total input limit".into());
        }
        manifest.insert(input.id.as_str(), input.kind);
    }
    let texts = sources::validate(request, &program, &manifest, inputs)?;
    let modules = graph::modules(request, &program, &manifest, limits)?;
    let files: Result<BTreeSet<_>, _> = (0..program.files.len())
        .map(|i| u32::try_from(i).map(FileId::new))
        .collect();
    let files = files.map_err(|_| "too many program source files")?;
    let problems = flick_ir::validate(
        &program,
        &|name| {
            modules
                .get(name)
                .is_some_and(|m| m.role == ModuleRole::External && m.interface.is_some())
        },
        &files,
    );
    if !problems.is_empty() {
        return Err(format!("invalid backend IR: {}", problems.join("; ")));
    }
    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    validate_source_spans(&program, &text_refs)?;
    sources::validate_inline_spans(request, &text_refs)?;
    sources::validate_export_spans(request, &text_refs)?;
    match (&request.target.mode, &request.module_root) {
        (Mode::WholeProgram, None) => {}
        (Mode::PerModule, Some(root))
            if modules
                .get(root.as_str())
                .is_some_and(|m| m.role == ModuleRole::Own) => {}
        (Mode::WholeProgram, Some(_)) => {
            return Err("whole-program mode cannot set a module root".into());
        }
        (Mode::PerModule, None) => {
            return Err("per-module mode requires a module root".into());
        }
        (Mode::PerModule, Some(root)) => {
            return Err(format!(
                "per-module root `{root}` must name an own module in the graph"
            ));
        }
    }
    graph::roots(request, &modules)?;
    side_data(request, &manifest, &modules, limits)?;
    Ok(program)
}

pub(super) fn validate_size(request: &BuildRequest, limits: Limits) -> Result<(), String> {
    ciborium::into_writer(request, Budget(limits.request_bytes))
        .map_err(|_| "backend request exceeds its total input limit".into())
}

fn side_data(
    request: &BuildRequest,
    manifest: &BTreeMap<&str, InputKind>,
    modules: &BTreeMap<&str, &ModuleInput>,
    limits: Limits,
) -> Result<(), String> {
    let mut projections = BTreeSet::new();
    for projection in &request.projections {
        if !modules.contains_key(projection.module.as_str())
            || manifest.get(projection.input.as_str()) != Some(&InputKind::TargetFacts)
            || !projections.insert((&projection.module, &projection.schema.name))
        {
            return Err("invalid or duplicate compiler projection input".into());
        }
    }
    for foreign in &request.foreign {
        if !modules.contains_key(foreign.module.as_str())
            || foreign
                .inputs
                .iter()
                .any(|i| manifest.get(i.as_str()) != Some(&InputKind::Foreign))
        {
            return Err("foreign binding references an undeclared foreign input".into());
        }
    }
    for inline in &request.inline_foreign {
        if !modules.contains_key(inline.module.as_str()) {
            return Err("inline foreign entry references an unknown module".into());
        }
        if let Some(name) = &inline.name {
            string(name, limits)?;
            if name.is_empty() {
                return Err("empty inline foreign binding name".into());
            }
        }
        if inline.code.contains('\0') {
            return Err("inline foreign code contains NUL".into());
        }
    }
    let mut runtimes = BTreeSet::new();
    for dependency in &request.runtime_dependencies {
        runtime_dependency(dependency, limits)?;
        if !runtimes.insert(&dependency.package) {
            return Err("duplicate backend runtime dependency".into());
        }
    }
    for export in &request.exports {
        if !modules.contains_key(export.symbol.module.as_str()) {
            return Err("host export references an unknown module".into());
        }
        string(&export.name, limits)?;
        string(&export.symbol.name, limits)?;
        if export.symbol.name.is_empty() {
            return Err("empty host export symbol name".into());
        }
    }
    if let Some(headers) = &request.headers
        && let Some(format) = &headers.format
    {
        string(format, limits)?;
        if format.is_empty() {
            return Err("empty header projection format".into());
        }
    }
    if let Some(declared) = &request.declared_dependencies {
        if declared.len() > limits.files {
            return Err("declared dependency count exceeds its limit".into());
        }
        for name in declared {
            string(name, limits)?;
            if name.is_empty() {
                return Err("empty declared dependency name".into());
            }
        }
    }
    Ok(())
}
