//! Backend `check-target` execution during `flick check` and LSP analysis so
//! non-official/pinned backends validate their own foreign adapter words at
//! exact source spans without waiting for `flick build`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use flick_backend::{
    Backend, BackendPin, BuildRequest, DeclaredInputs, Environment, Input, InputKind, Mode,
    ModuleInput, ModuleRole, OptionValue, Profile, Source, TargetRequest, sha256,
};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::ir;
use flick_core::project::{Compilation, Project, ResolvedEnvironment};
use flick_core::source::{FileId, SourceMap, Span};
use flick_ir::{ItemKind, Program};

use super::foreign::{load, selected_bytes};
use super::{options, request};
use crate::toolchain;

/// Runs a non-official/pinned backend component's `check-target` export during
/// `flick check` and IDE analysis so backend-declared foreign adapter words are
/// validated at their exact source spans without waiting for `flick build`.
///
/// Official `flick/js` and `flick/go` foreign adapters are validated in-process
/// by `flick-core` to keep `flick check` and LSP latency free of WASM startup.
pub(crate) fn check_target_for_project(
    project: &Project,
    compilation: &Compilation,
    sources: &SourceMap,
    environment: &ResolvedEnvironment,
    stages_by_class: bool,
) -> Vec<Diagnostic> {
    if matches!(
        environment.backend_id.as_str(),
        options::OFFICIAL_JS | options::OFFICIAL_GO
    ) {
        return Vec::new();
    }
    let has_foreign = compilation.modules.iter().any(|m| {
        m.package.is_none()
            && compilation.module_available(&m.name)
            && (!m.ir.preambles.is_empty()
                || m.ir
                    .items
                    .iter()
                    .any(|item| matches!(item.kind, ItemKind::Foreign(_))))
    });
    if !has_foreign {
        return Vec::new();
    }
    let Some((pin, bytes)) = selected_bytes(project, Some(&environment.backend_id)) else {
        return Vec::new();
    };
    let Some(backend) = load(&pin, &bytes) else {
        return Vec::new();
    };
    let (request, files, input_bytes) = build_check_request(
        project,
        compilation,
        sources,
        environment,
        stages_by_class,
        &pin,
    );
    let Ok(mut declared) = DeclaredInputs::new(&request.inputs, |id| input_bytes.get(id).cloned())
    else {
        return Vec::new();
    };
    match backend.check_target(&request, &mut declared) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .into_iter()
            .map(|d| map_check_diagnostic(d, &pin.identity.id, &files, sources))
            .collect(),
    }
}

fn build_check_request(
    project: &Project,
    compilation: &Compilation,
    sources: &SourceMap,
    environment: &ResolvedEnvironment,
    stages_by_class: bool,
    pin: &BackendPin,
) -> (BuildRequest, Vec<FileId>, BTreeMap<String, Vec<u8>>) {
    let pruned = compilation.prune_for_environment(sources, stages_by_class);
    let files = RefCell::new(Vec::new());
    let ir_modules: Vec<_> = pruned.modules.iter().map(|m| m.ir.clone()).collect();
    let program = Program::new(
        &format!("flick {}", toolchain::COMPILER),
        ir_modules,
        ir::lang_items(),
        &|file| {
            files.borrow_mut().push(file);
            sources.get(file).path.clone()
        },
    );
    let files = files.into_inner();
    let inline_foreign = super::request::collect_inline_foreign(&program);
    let mut input_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut req_sources = Vec::new();
    for (i, name) in program.files.iter().enumerate() {
        let file = sources.get(files[i]);
        let id = format!("source/{i}");
        let raw = file.text.as_bytes().to_vec();
        inputs.push(Input {
            id: id.clone(),
            kind: InputKind::Source,
            sha256: sha256(&raw),
            size: u64::try_from(raw.len()).unwrap_or(u64::MAX),
        });
        input_bytes.insert(id.clone(), raw);
        req_sources.push(Source {
            name: name.clone(),
            input: id,
            line_starts: file.line_starts().to_vec(),
        });
    }
    let modules = pruned
        .modules
        .iter()
        .map(|m| ModuleInput {
            name: m.name.clone(),
            role: ModuleRole::Own,
            package: m.ir.package.clone().or_else(|| m.package.clone()),
            output: format!("{}.out", m.name.replace('.', "/")),
            interface: None,
        })
        .collect();
    let mut req_options = vec![
        OptionValue {
            key: "command".into(),
            value: "check".into(),
        },
        OptionValue {
            key: "labels".into(),
            value: "none".into(),
        },
    ];
    req_options.extend(options::project_wire_options(
        project,
        Some(&environment.backend_id),
    ));
    let request = BuildRequest {
        target: TargetRequest {
            backend_api: flick_backend::VERSION.into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: "0.1.0".into(),
            runtime_abis: request::runtime_abis("0.1.0"),
            environment: Environment {
                backend: environment.backend.clone(),
                platform: environment.platform.clone(),
            },
            tags: environment.tags.iter().cloned().collect(),
            host_package: environment.host_package.clone(),
            profile: Profile::Debug,
            mode: Mode::WholeProgram,
            required_capabilities: Vec::new(),
            options: req_options,
        },
        backend: pin.clone(),
        program: program.to_cbor(),
        module_root: None,
        modules,
        sources: req_sources,
        entries: Vec::new(),
        test_roots: Vec::new(),
        foreign: Vec::new(),
        inline_foreign,
        projections: Vec::new(),
        package_targets: Vec::new(),
        root_handlers: Vec::new(),
        inputs,
        toolchains: Vec::new(),
        runtime_dependencies: Vec::new(),
        declared_dependencies: None,
    };
    (request, files, input_bytes)
}

fn map_check_diagnostic(
    d: flick_backend::Diagnostic,
    backend_id: &str,
    files: &[FileId],
    sources: &SourceMap,
) -> Diagnostic {
    let code = codes::lookup(&format!("FLICK-{}", d.code))
        .map(|info| info.code)
        .or_else(|| codes::plugin(backend_id, &d.code))
        .unwrap_or(codes::P013);
    let severity = match d.severity {
        flick_backend::Severity::Error => Severity::Error,
        flick_backend::Severity::Warning => Severity::Warning,
        flick_backend::Severity::Note => Severity::Note,
    };
    let mut message = d.message;
    for note in &d.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    let mut mapped = Diagnostic::new(code, severity, message);
    if let Some(span) = d.span
        && let Some(&file_id) = files.get(span.file as usize)
    {
        let text = &sources.get(file_id).text;
        let start = span.start as usize;
        let end = span.end as usize;
        if start <= end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end)
        {
            mapped.span = Some(Span::new(file_id, span.start, span.end));
        }
    }
    mapped
}
