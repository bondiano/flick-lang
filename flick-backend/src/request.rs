use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{FileId, Program};

use crate::validate::{Budget, Limits, id, runtime_dependency};
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
    validate_source_spans(
        &program,
        &texts.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    )?;
    match (&request.target.mode, &request.module_root) {
        (Mode::WholeProgram, None) => {}
        (Mode::PerModule, Some(root)) if program.modules.iter().any(|m| m.name == *root) => {}
        _ => return Err("compilation mode has no valid module root".into()),
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
    let mut runtimes = BTreeSet::new();
    for dependency in &request.runtime_dependencies {
        runtime_dependency(dependency, limits)?;
        if !runtimes.insert(&dependency.package) {
            return Err("duplicate backend runtime dependency".into());
        }
    }
    Ok(())
}
