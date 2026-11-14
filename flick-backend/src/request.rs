use std::collections::{BTreeMap, BTreeSet};
use std::{iter, str};

use flick_ir::{FileId, Program};

use crate::validate::{Budget, Limits, id, runtime_dependency};
use crate::{BuildRequest, InputKind, Inputs, Mode, ModuleInput, ModuleRole, VERSION};

/// Validate the closed graph and all referenced side inputs before target
/// preflight. This is structural validation, not a proof of IR semantics.
pub fn validate_request(
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
    limits: Limits,
) -> Result<Program, String> {
    ciborium::into_writer(request, Budget(limits.request_bytes))
        .map_err(|_| "backend request exceeds its total input limit")?;
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
    sources(request, &program, &manifest, inputs)?;
    let mut modules = BTreeMap::new();
    for module in &request.modules {
        if modules.insert(module.name.as_str(), module).is_some() {
            return Err("duplicate module metadata".into());
        }
        if let Some(interface) = &module.interface
            && manifest.get(interface.as_str()) != Some(&InputKind::Interface)
        {
            return Err("module interface is not a declared interface input".into());
        }
    }
    if program
        .modules
        .iter()
        .any(|m| !modules.contains_key(m.name.as_str()))
        || modules.values().any(|m| {
            m.role != ModuleRole::External && !program.modules.iter().any(|ir| ir.name == m.name)
        })
    {
        return Err("request module metadata differs from its IR graph".into());
    }
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
    match (&request.target.mode, &request.module_root) {
        (Mode::WholeProgram, None) => {}
        (Mode::PerModule, Some(root)) if program.modules.iter().any(|m| m.name == *root) => {}
        _ => return Err("compilation mode has no valid module root".into()),
    }
    for module in &request.modules {
        if module.role == ModuleRole::External && module.interface.is_none() {
            return Err("external module has no explicit interface ABI".into());
        }
    }
    for entry in &request.entries {
        if !modules.contains_key(entry.symbol.module.as_str()) {
            return Err("entry references a module outside the graph".into());
        }
    }
    for root in &request.test_roots {
        if !modules.contains_key(root.module.as_str()) {
            return Err("test root references a module outside the graph".into());
        }
    }
    side_data(request, &manifest, &modules, limits)?;
    Ok(program)
}

fn sources(
    request: &BuildRequest,
    program: &Program,
    manifest: &BTreeMap<&str, InputKind>,
    inputs: &mut dyn Inputs,
) -> Result<(), String> {
    if request.sources.len() != program.files.len() {
        return Err("request sources differ from Program.files".into());
    }
    for (source, name) in request.sources.iter().zip(&program.files) {
        if source.name != *name || manifest.get(source.input.as_str()) != Some(&InputKind::Source) {
            return Err("request source has no matching declared source input".into());
        }
        let bytes = inputs
            .read(&source.input)
            .map_err(|_| "declared source is unavailable")?;
        let text = str::from_utf8(&bytes).map_err(|_| "request source is not UTF-8")?;
        let lines: Result<Vec<u32>, _> = iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .map(u32::try_from)
            .collect();
        if lines.map_err(|_| "source exceeds the span address space")? != source.line_starts {
            return Err("request source has an invalid line index".into());
        }
    }
    Ok(())
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
