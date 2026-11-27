//! In-process `flick run --target vm` execution on `flick_vm::{tier0, tier1}`.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::slice;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::ir::memory::Name as MemoryName;
use flick_core::project::{self, Compiled, Project};
use flick_core::source::{FileId, SourceMap, Span};
use flick_vm::tier0::{
    ExecutionReport, Heap, HostMode, ProgramOutcome, Reason,
    execute_main_program_outcome_with_host, execute_rooted_main_program_outcome_with_host,
};
use flick_vm::tier1;

use super::{VmTier, lower, resolve_tier, target_override};
use crate::args::{GlobalArguments, RunArguments};
use crate::backends::options;
use crate::commands::build;
use crate::commands::check::select;
use crate::sources::FsHost;
use crate::{builtins, package_cache, report};

enum RunSource {
    Standalone(PathBuf),
    Project {
        project: Box<Project>,
        roots: Vec<PathBuf>,
        entry_module: String,
    },
}

fn is_standalone_path(cwd: &Path, name: &str) -> bool {
    let candidate = cwd.join(name);
    candidate.is_file()
        || Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("fl"))
}

fn resolve_entry_module(project: &Project, options: &RunArguments) -> Result<String, Diagnostic> {
    if let Some(module) = &options.entry {
        return Ok(module.clone());
    }
    let entries = &project.config.entries;
    let listed = || {
        entries
            .iter()
            .map(|e| format!("`{}`", e.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (&options.name, entries.as_slice()) {
        (_, []) => Err(Diagnostic::new(
            codes::P007,
            Severity::Error,
            "a library has no `main` to run: give `--entry App.Main`",
        )),
        (Some(name), _) => entries
            .iter()
            .find(|e| &e.name == name)
            .map(|e| e.module.clone())
            .ok_or_else(|| {
                Diagnostic::new(
                    codes::P007,
                    Severity::Error,
                    format!(
                        "the project has no entry `{name}`; its entries: {}",
                        listed()
                    ),
                )
            }),
        (None, [only]) => Ok(only.module.clone()),
        (None, _) => Err(Diagnostic::new(
            codes::P007,
            Severity::Error,
            format!(
                "the project has several entries; name the one to run: {}",
                listed()
            ),
        )),
    }
}

fn resolve_source(
    host: &FsHost,
    sources: &mut SourceMap,
    options: &RunArguments,
    global: &GlobalArguments,
) -> Result<RunSource, Vec<Diagnostic>> {
    let explicit = global.project.as_deref().map(Path::new);
    let discovered = project::discover(host, sources, &host.cwd, explicit).map_err(|d| vec![d])?;
    if options.entry.is_none()
        && let Some(name) = &options.name
    {
        let is_project_entry = discovered
            .as_ref()
            .is_some_and(|p| p.config.entries.iter().any(|e| &e.name == name));
        if !is_project_entry && is_standalone_path(&host.cwd, name) {
            let path = project::normalize(&host.cwd.join(name));
            return Ok(RunSource::Standalone(path));
        }
    }
    let Some(project) = discovered else {
        let message = format!(
            "no `{}` in the working directory or any parent (pass a `.fl` file path to run standalone)",
            project::CONFIG_FILE
        );
        return Err(vec![project::no_project(message)]);
    };
    let entry_module = resolve_entry_module(&project, options).map_err(|d| vec![d])?;
    let (_, roots) =
        select(host, sources, Some(&project.root.to_string_lossy()), &[]).map_err(|d| vec![d])?;
    Ok(RunSource::Project {
        project: Box::new(project),
        roots,
        entry_module,
    })
}

pub(super) fn run(options: &RunArguments, global: &GlobalArguments) -> ExitCode {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let run_source = match resolve_source(&host, &mut sources, options, global) {
        Ok(source) => source,
        Err(diagnostics) => {
            report::render(global, &sources, &diagnostics);
            return report::exit_code(&diagnostics);
        }
    };
    let project_ref = match &run_source {
        RunSource::Standalone(_) => None,
        RunSource::Project { project, .. } => Some(project.as_ref()),
    };
    let tier = match resolve_tier(options.vm_tier.as_deref(), project_ref) {
        Ok(tier) => tier,
        Err(diagnostics) => {
            report::render(global, &sources, &diagnostics);
            return report::exit_code(&diagnostics);
        }
    };
    let target_overridden = options::resolve_target(options.target.as_deref(), project_ref)
        .is_ok_and(|(_, overridden)| overridden);
    if let Err(diagnostic) = builtins::prepare_in(false) {
        report::render(global, &sources, &[diagnostic]);
        return ExitCode::from(2);
    }
    let (roots, explicit_entry) = match &run_source {
        RunSource::Standalone(path) => (vec![path.clone()], None),
        RunSource::Project {
            roots,
            entry_module,
            ..
        } => (roots.clone(), Some(entry_module.as_str())),
    };
    let override_cfg = target_override(
        target_overridden,
        false,
        Vec::new(),
        options.no_native_overrides,
    );
    let (mut diagnostics, mut compilation) = package_cache::compile_for(
        &host,
        &mut sources,
        project_ref,
        &roots,
        Some(&override_cfg),
        None,
    );
    if global.warnings_as_errors {
        diagnostics = report::promote(&diagnostics);
    }
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        report::render(global, &sources, &diagnostics);
        return report::exit_code(&diagnostics);
    }
    compilation.modules.retain(|c| !c.test);
    build::hooks::ir(project_ref, &mut compilation, &mut diagnostics);
    if global.warnings_as_errors {
        diagnostics = report::promote(&diagnostics);
    }
    if !diagnostics.is_empty() {
        report::render(global, &sources, &diagnostics);
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return report::exit_code(&diagnostics);
        }
    }
    let entry_module = match find_entry_module(&compilation.modules, &roots, explicit_entry) {
        Ok(name) => name,
        Err(diagnostic) => {
            report::render(global, &sources, slice::from_ref(&diagnostic));
            return report::exit_code(slice::from_ref(&diagnostic));
        }
    };
    let release = build::release(options.profile, project_ref.map(|p| &p.config.build));
    let entry = MemoryName::new(&entry_module, "main");
    let lowered = match lower::lower(&compilation, slice::from_ref(&entry), release) {
        Ok(lowered) => lowered,
        Err(err) => {
            let diagnostic = Diagnostic::new(
                codes::P012,
                Severity::Error,
                format!("VM lowering failed: {err}"),
            );
            report::render(global, &sources, &[diagnostic]);
            return ExitCode::from(1);
        }
    };
    match tier {
        VmTier::Tier0 => execute_tier0(&lowered, &entry),
        VmTier::Tier1 => execute_tier1(&lowered, &entry, &sources, global),
    }
}

fn find_entry_module(
    modules: &[Compiled],
    roots: &[PathBuf],
    explicit: Option<&str>,
) -> Result<String, Diagnostic> {
    if let Some(name) = explicit {
        if modules
            .iter()
            .any(|m| m.name == name && m.interface.exports_value("main"))
        {
            return Ok(name.to_owned());
        }
        return Err(Diagnostic::new(
            codes::P011,
            Severity::Error,
            format!("entry module `{name}` does not export `main`"),
        ));
    }
    let mains: Vec<&Compiled> = modules
        .iter()
        .filter(|m| roots.contains(&m.path) && m.interface.exports_value("main"))
        .collect();
    match mains.as_slice() {
        [only] => Ok(only.name.clone()),
        [] => Err(Diagnostic::new(
            codes::P011,
            Severity::Error,
            "no module exports `main`",
        )),
        _ => Err(Diagnostic::new(
            codes::P011,
            Severity::Error,
            "multiple modules export `main`",
        )),
    }
}

fn execute_tier0(lowered: &lower::LoweredProgram, entry: &MemoryName) -> ExitCode {
    let mut heap = Heap::new();
    let report = match &lowered.root {
        Some(root) => execute_rooted_main_program_outcome_with_host(
            &mut heap,
            &lowered.program,
            entry,
            root,
            u64::MAX,
            HostMode::LiveOs,
        ),
        None => execute_main_program_outcome_with_host(
            &mut heap,
            &lowered.program,
            entry,
            u64::MAX,
            HostMode::LiveOs,
        ),
    };
    finish_report(&mut heap, report)
}

fn execute_tier1(
    lowered: &lower::LoweredProgram,
    entry: &MemoryName,
    sources: &SourceMap,
    global: &GlobalArguments,
) -> ExitCode {
    let mut heap = Heap::new();
    let report = match tier1::execute_main_program_outcome_with_host(
        &mut heap,
        &lowered.program,
        entry,
        u64::MAX,
        HostMode::LiveOs,
    ) {
        Ok(report) => report,
        Err(err) => {
            let diagnostic = match err.span() {
                Some(mem_span) if (mem_span.file as usize) < sources.files().count() => {
                    Diagnostic::error(
                        codes::P012,
                        err.to_string(),
                        Span::new(FileId::new(mem_span.file), mem_span.start, mem_span.end),
                    )
                }
                _ => Diagnostic::new(codes::P012, Severity::Error, err.to_string()),
            };
            report::render(global, sources, &[diagnostic]);
            return ExitCode::from(1);
        }
    };
    finish_report(&mut heap, report)
}

fn finish_report(heap: &mut Heap, report: ExecutionReport) -> ExitCode {
    if !report.stdout.is_empty() {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(report.stdout.as_bytes());
        let _ = stdout.flush();
    }
    match report.outcome {
        ProgramOutcome::Returned(value) => {
            if let Err(err) = heap.drop_value(value) {
                eprintln!("vm error: failed to drop return value: {err}");
                return ExitCode::from(1);
            }
            if let Err(leak) = heap.finish() {
                eprintln!("vm error: memory leak on exit: {leak}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        ProgramOutcome::Raised(value) => {
            eprintln!("error: unhandled failure");
            let _ = heap.drop_value(value);
            let _ = heap.finish();
            ExitCode::from(1)
        }
        ProgramOutcome::Failed(cause) => {
            let interrupted = heap.cause_reasons(&cause).is_ok_and(|mut reasons| {
                reasons.len() > 0 && reasons.all(|r| matches!(r, Reason::Interrupt { .. }))
            });
            let message = match heap.cause_error(&cause) {
                Ok(err) | Err(err) => err.to_string(),
            };
            let _ = heap.drop_cause(cause);
            let _ = heap.finish();
            if interrupted {
                eprintln!("interrupted");
                ExitCode::from(130)
            } else {
                eprintln!("error: {message}");
                ExitCode::from(1)
            }
        }
        ProgramOutcome::Fault(err) => {
            let _ = heap.finish();
            eprintln!("vm error: {err}");
            ExitCode::from(1)
        }
    }
}
