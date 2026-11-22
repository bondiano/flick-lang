//! Backend-neutral execution of generated or overlaid Flick programs.
//!
//! Shared by `flick:command@0.1.0` `step::run-flick` (including `std`'s
//! `schema export`) and `flick doc --check-examples`. Synthetic modules live
//! only in memory (`OverlayHost`) and compile into an isolated temporary
//! generation emitted and launched through the project's selected backend.

mod overlay;
mod process;
#[cfg(test)]
mod tests;

pub(crate) use overlay::OverlayHost;
pub(crate) use process::{RunOutput, RunStatus};

use std::collections::BTreeMap;
#[cfg(feature = "wasm-backends")]
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
#[cfg(feature = "wasm-backends")]
use std::process::Stdio;
use std::slice;
#[cfg(feature = "wasm-backends")]
use std::str;

use flick_backend::Verb;
#[cfg(feature = "wasm-backends")]
use flick_backend::command::{Invocation, RunFlickStep, Span as CommandSpan};
#[cfg(feature = "wasm-backends")]
use flick_backend::host::runtime::Cancellation;
#[cfg(feature = "wasm-backends")]
use flick_backend::{LaunchKind, Limits};
use flick_core::build::Extras;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "wasm-backends")]
use flick_core::project::Host;
use flick_core::project::{self, Compilation, Project};
#[cfg(feature = "wasm-backends")]
use flick_core::source::Span;
use flick_core::source::{FileId, SourceMap};

#[cfg(feature = "wasm-backends")]
use process::supervise;
use process::{TempGeneration, fresh_entry_name};

use super::build::{BuildPlan, Purpose, emit, hooks};
use crate::backends::options::{self, ResolvedOptions, TargetChoice};
use crate::backends::{self, Selected};
use crate::sources::FsHost;
use crate::{builtins, package_cache, stdlib};

/// Backend and resolved `Verb::Run` options prepared once for one or more
/// `run-flick` executions.
pub(crate) struct PreparedTarget {
    pub(crate) backend: Selected,
    pub(crate) choice: TargetChoice,
    pub(crate) target_overridden: bool,
    pub(crate) options: ResolvedOptions,
    pub(crate) stages_by_class: bool,
}

/// Resolves the target backend, validates `Verb::Run` options, and lazily
/// prepares `std` and built-in module interfaces.
pub(crate) fn prepare_target(
    host: &FsHost,
    project: Option<&Project>,
    target_override: Option<&str>,
) -> Result<PreparedTarget, (SourceMap, Vec<Diagnostic>)> {
    let explicit = project.map(|p| p.root.to_string_lossy().into_owned());
    if let Err(message) = stdlib::prepare(explicit.as_deref()) {
        return Err((
            SourceMap::default(),
            vec![Diagnostic::new(codes::P034, Severity::Error, message)],
        ));
    }
    let (backend, choice, target_overridden) =
        Selected::load_target(host, project, target_override)
            .map_err(|d| (SourceMap::default(), vec![d]))?;
    let descriptor = backend.descriptor();
    let mut resolved = match &descriptor {
        Some(descriptor) => options::resolve(descriptor, Verb::Run, project, &BTreeMap::new())
            .map_err(|diagnostics| {
                let mut sources = SourceMap::default();
                if let Some(p) = project {
                    let _ = project::discover(host, &mut sources, &p.root, Some(&p.root));
                }
                (sources, diagnostics)
            })?,
        None => ResolvedOptions::default(),
    };
    // Internal generated programs never connect to a DevTools server.
    resolved.values.remove("devtools");
    let stages_by_class = options::stages_by_class(descriptor.as_ref(), &resolved);
    if let Some(problem) = project.and_then(|p| backends::check_runtime(p, &backend, &resolved)) {
        return Err((SourceMap::default(), vec![problem]));
    }
    if let Err(problem) = builtins::prepare_in(stages_by_class) {
        return Err((SourceMap::default(), vec![problem]));
    }
    Ok(PreparedTarget {
        backend,
        choice,
        target_overridden,
        options: resolved,
        stages_by_class,
    })
}

/// Compiles `entry` with `overlays` without writing synthetic modules into the
/// disk module cache.
pub(crate) fn compile_overlay(
    host: &FsHost,
    project: Option<&Project>,
    rt: bool,
    entry: &str,
    overlays: &[(String, String)],
) -> (SourceMap, Vec<Diagnostic>, Compilation, Vec<PathBuf>) {
    let root = project.map_or_else(|| host.cwd.clone(), Project::source_root);
    let overlay_host = OverlayHost::new(host, &root, overlays.iter().cloned());
    let overlay_paths: Vec<PathBuf> = overlay_host.paths().cloned().collect();
    let entry_path = project::normalize(&root.join(project::module_path(entry)));
    let mut sources = SourceMap::default();
    let discovered = match project {
        Some(p) => match project::discover(host, &mut sources, &p.root, Some(&p.root)) {
            Ok(found) => found,
            Err(diagnostic) => {
                return (
                    sources,
                    vec![diagnostic],
                    Compilation::default(),
                    overlay_paths,
                );
            }
        },
        None => None,
    };
    let effective = discovered.as_ref().or(project);
    let (mut diagnostics, mut compilation) = package_cache::with_saved(|saved| {
        let (diagnostics, compilation, fresh) = project::compile_with(
            &overlay_host,
            &mut sources,
            effective,
            slice::from_ref(&entry_path),
            saved,
            Some(rt),
            None,
        );
        ((diagnostics, compilation), fresh)
    });
    if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        compilation.modules.retain(|c| !c.test);
        hooks::ir(effective, &mut compilation, &mut diagnostics);
    }
    (sources, diagnostics, compilation, overlay_paths)
}

/// Emits an already-compiled program into an isolated temporary generation and
/// executes it through the selected backend's `LaunchKind::Run` launch.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_compiled(
    host: &FsHost,
    project: Option<&Project>,
    prepared: &PreparedTarget,
    compilation: &Compilation,
    sources: &mut SourceMap,
    entry: &str,
    arguments: &[String],
    #[cfg(feature = "wasm-backends")] cancel: Option<&Cancellation>,
) -> Result<RunOutput, Vec<Diagnostic>> {
    let gen_dir = TempGeneration::reserve(host, project).map_err(|err| {
        vec![Diagnostic::new(
            codes::P013,
            Severity::Error,
            format!("cannot reserve a temporary run generation: {err}"),
        )]
    })?;
    let entry_name = fresh_entry_name(project);
    let plan = BuildPlan {
        purpose: Purpose::Run,
        backend: prepared.backend.clone(),
        target: prepared.choice.short().to_owned(),
        target_overridden: prepared.target_overridden,
        options: prepared.options.clone(),
        inspect_module: None,
        out_dir: gen_dir.path().to_path_buf(),
        entries: vec![(entry_name.clone(), entry.to_owned())],
        typescript: false,
        stages_by_class: prepared.stages_by_class,
        suites: Vec::new(),
        extras: Extras {
            rt: prepared.stages_by_class,
            ..Extras::default()
        },
        release: false,
    };
    let emission = emit(host, sources, project, compilation, &plan);
    if emission
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(emission.diagnostics);
    }

    #[cfg(feature = "wasm-backends")]
    let mut command = if let Some(pin) = prepared.backend.pin() {
        backends::launch::command(gen_dir.path(), &pin, LaunchKind::Run, &entry_name)
            .map_err(|why| vec![backends::failure(why)])?
    } else {
        return Err(vec![backends::failure(
            "running requires WebAssembly backend support",
        )]);
    };
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = (arguments, gen_dir, entry_name);
        return Err(vec![backends::failure(
            "running requires WebAssembly backend support",
        )]);
    }

    #[cfg(feature = "wasm-backends")]
    {
        let work_dir = project.map_or(host.cwd.as_path(), |p| p.root.as_path());
        command
            .args(arguments)
            .current_dir(work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                return Ok(RunOutput {
                    stdout: Vec::new(),
                    stderr: String::new(),
                    status: RunStatus::LaunchError(err.to_string()),
                });
            }
        };

        let output = supervise(child, Limits::default().artifact_bytes, cancel);
        Ok(output)
    }
}

/// Executes a `flick:command@0.1.0` `run-flick` step on the project's selected
/// backend and returns its captured stdout bytes, or diagnostics with their
/// `SourceMap`.
#[cfg(feature = "wasm-backends")]
pub(crate) fn execute_step(
    project: &Project,
    step: &RunFlickStep,
    invocation: &Invocation,
    cancel: Option<&Cancellation>,
) -> Result<Vec<u8>, (SourceMap, Vec<Diagnostic>)> {
    let host = FsHost::new();
    let prepared = prepare_target(&host, Some(project), None)?;
    let overlays: Vec<(String, String)> = step
        .modules
        .iter()
        .map(|m| (m.name.clone(), m.source.clone()))
        .collect();
    let (mut sources, diagnostics, compilation, overlay_paths) = compile_overlay(
        &host,
        Some(project),
        prepared.stages_by_class,
        &step.entry,
        &overlays,
    );
    let overlay_files: Vec<FileId> = overlay_file_ids(&host, &sources, &overlay_paths);

    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        let origin = load_origin_span(&mut sources, invocation, step.origin.as_ref());
        let remapped = remap_step_diagnostics(diagnostics, &overlay_files, origin, &step.id);
        return Err((sources, remapped));
    }

    let output = match run_compiled(
        &host,
        Some(project),
        &prepared,
        &compilation,
        &mut sources,
        &step.entry,
        &step.arguments,
        cancel,
    ) {
        Ok(output) => output,
        Err(diagnostics) => {
            let origin = load_origin_span(&mut sources, invocation, step.origin.as_ref());
            let remapped = remap_step_diagnostics(diagnostics, &overlay_files, origin, &step.id);
            return Err((sources, remapped));
        }
    };

    match output.status {
        RunStatus::Success => Ok(output.stdout),
        status => {
            let origin = load_origin_span(&mut sources, invocation, step.origin.as_ref());
            let span = runtime_span(&sources, &output.stderr, &overlay_files).or(origin);
            let message = runtime_message(&format!("step `{}`", step.id), &status, &output.stderr);
            let mut diag = Diagnostic::new(codes::P061, Severity::Error, message);
            if let Some(span) = span {
                diag = diag.with_span(span);
            }
            Err((sources, vec![diag]))
        }
    }
}

pub(crate) fn runtime_message(subject: &str, status: &RunStatus, stderr: &str) -> String {
    let detail = stderr.trim();
    match status {
        RunStatus::Success => String::new(),
        RunStatus::TimedOut => format!("{subject} timed out after 60 seconds"),
        RunStatus::OutputTooLarge => format!("{subject} exceeded the stdout size limit"),
        RunStatus::Cancelled => format!("{subject} was cancelled"),
        RunStatus::LaunchError(err) => format!("{subject} could not be launched: {err}"),
        RunStatus::Failed(code) => {
            if detail.is_empty() {
                format!("{subject} failed at runtime: {code}")
            } else {
                format!("{subject} failed at runtime: {detail}")
            }
        }
    }
}

/// Merges diagnostics from `from` into `target`, adding any referenced source
/// files to `target` and remapping every `FileId`.
pub(crate) fn merge_diagnostics(
    target: &mut SourceMap,
    from: &SourceMap,
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let mut mapped: BTreeMap<FileId, FileId> = BTreeMap::new();
    let mut remap = |file: FileId| -> FileId {
        if let Some(&id) = mapped.get(&file) {
            return id;
        }
        let source = from.get(file);
        let existing = target
            .files()
            .position(|f| f.path == source.path && f.text == source.text)
            .map(|i| FileId::new(u32::try_from(i).expect("source count fits u32")));
        let id = existing.unwrap_or_else(|| target.add(source.path.clone(), source.text.clone()));
        mapped.insert(file, id);
        id
    };
    diagnostics
        .into_iter()
        .map(|mut d| {
            if let Some(span) = &mut d.span {
                span.file = remap(span.file);
            }
            for related in &mut d.related {
                related.span.file = remap(related.span.file);
            }
            for edit in d.fixes.iter_mut().flat_map(|f| &mut f.edits) {
                edit.span.file = remap(edit.span.file);
            }
            d
        })
        .collect()
}

/// Extracts a 1-based line number for `target_path` (matching its full path,
/// normalized path, or `/`-separated suffix) from runtime `stderr`.
pub(crate) fn extract_line_from_stderr(stderr: &str, target_path: &Path) -> Option<usize> {
    let normalized = target_path.to_string_lossy().replace('\\', "/");
    let mut suffixes = vec![normalized.as_str()];
    if let Some(pos) = normalized.find("/src/") {
        suffixes.push(&normalized[pos + 1..]);
    }
    if let Some(name) = target_path.file_name().and_then(|n| n.to_str()) {
        suffixes.push(name);
    }
    for line in stderr.lines() {
        for suffix in &suffixes {
            let mut search = line;
            while let Some(at) = search.find(suffix) {
                let bounded = at == 0
                    || matches!(
                        search.as_bytes()[at - 1],
                        b'/' | b'\\' | b' ' | b'\t' | b'(' | b'[' | b'"' | b'\'' | b':'
                    );
                let after = &search[at + suffix.len()..];
                if bounded && let Some(rest) = after.strip_prefix(':') {
                    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                    if let Ok(line_no) = digits.parse::<usize>()
                        && line_no > 0
                    {
                        return Some(line_no);
                    }
                }
                search = &search[at + suffix.len()..];
            }
        }
    }
    None
}

#[cfg(feature = "wasm-backends")]
fn overlay_file_ids(host: &FsHost, sources: &SourceMap, overlay_paths: &[PathBuf]) -> Vec<FileId> {
    let displayed: Vec<String> = overlay_paths.iter().map(|p| host.display(p)).collect();
    sources
        .files()
        .enumerate()
        .filter(|(_, f)| displayed.iter().any(|p| p == &f.path))
        .map(|(i, _)| FileId::new(u32::try_from(i).expect("source count fits u32")))
        .collect()
}

#[cfg(feature = "wasm-backends")]
fn load_origin_span(
    sources: &mut SourceMap,
    invocation: &Invocation,
    origin: Option<&CommandSpan>,
) -> Option<Span> {
    let origin = origin?;
    let input = invocation.inputs.iter().find(|f| f.path == origin.path)?;
    let text = str::from_utf8(&input.bytes).ok()?;
    let start = origin.start as usize;
    let end = origin.end as usize;
    if origin.start > origin.end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return None;
    }
    let existing = sources
        .files()
        .position(|f| f.path == origin.path && f.text == text)
        .map(|i| FileId::new(u32::try_from(i).expect("source count fits u32")));
    let file = existing.unwrap_or_else(|| sources.add(origin.path.clone(), text.to_owned()));
    Some(Span::new(file, origin.start, origin.end))
}

#[cfg(feature = "wasm-backends")]
fn remap_step_diagnostics(
    diagnostics: Vec<Diagnostic>,
    overlay_files: &[FileId],
    origin: Option<Span>,
    step_id: &str,
) -> Vec<Diagnostic> {
    let Some(origin) = origin else {
        return diagnostics;
    };
    diagnostics
        .into_iter()
        .map(|mut d| {
            let in_overlay = d
                .span
                .is_some_and(|span| overlay_files.contains(&span.file));
            if in_overlay {
                d.span = Some(origin);
                let _ = write!(d.message, "\nnote: in generated step `{step_id}`");
                // Drop related spans and edits that point into synthetic overlay files.
                d.related.retain(|r| !overlay_files.contains(&r.span.file));
                for fix in &mut d.fixes {
                    fix.edits.retain(|e| !overlay_files.contains(&e.span.file));
                }
            }
            d
        })
        .collect()
}

#[cfg(feature = "wasm-backends")]
fn runtime_span(sources: &SourceMap, stderr: &str, overlay_files: &[FileId]) -> Option<Span> {
    for (idx, file) in sources.files().enumerate() {
        let id = FileId::new(u32::try_from(idx).expect("source count fits u32"));
        if overlay_files.contains(&id) {
            continue;
        }
        if let Some(line_no) = extract_line_from_stderr(stderr, Path::new(&file.path)) {
            let starts = file.line_starts();
            if let Some(&start) = starts.get(line_no.saturating_sub(1)) {
                let end = starts
                    .get(line_no)
                    .copied()
                    .unwrap_or_else(|| u32::try_from(file.text.len()).unwrap_or(u32::MAX));
                let slice = &file.text[start as usize..end as usize];
                let trimmed_len = slice.trim_end_matches(['\r', '\n']).len();
                let line_end = start.saturating_add(u32::try_from(trimmed_len).unwrap_or(0));
                return Some(Span::new(id, start, line_end));
            }
        }
    }
    None
}
