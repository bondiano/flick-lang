//! A plan command: its plan is checked against what the command declared
//! before anything happens, then its tool and `run-flick` steps run in order
//! and its files are published in one transaction under the project's root.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::str;

use flick_backend::command::{self, Descriptor, Invocation, Plan, ToolStep};
use flick_backend::host::runtime::Cancellation;
use flick_backend::{ArtifactKind, BuildResult, Toolchain};
use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::{FileId, SourceMap, Span};

use super::Contributor;
use super::inputs::expand;
use super::projection::Projections;
pub(super) use super::validate::validate_plan;
use super::validate::{PreparedStep, is_step_scratch};
use crate::args::GlobalArguments;
use crate::artifact::{Artifact, validate_destination_paths};
use crate::backends::{Reads, Tool, run_steps};
use crate::child;
use crate::commands::run_flick;
use crate::{component, publish, report};

/// How in-place tool steps are handled for a plan run.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InPlaceMode {
    /// Preview mode: in-place steps are refused without running or publishing.
    Forbidden,
    /// Contributed command: stream stderr under `"tool-output"`, fail with `P050`.
    Tool,
    /// Delegated dependency verb: stream stderr under `"package-manager-output"`, fail with `P017`/`P016`.
    PackageManager,
    /// Delegated client entry runner: inherit caller env, stream stderr under `"client-output"`.
    Client,
}

pub(crate) fn tool(pin: &Toolchain) -> command::Tool {
    command::Tool {
        id: pin.id.clone(),
        version: pin.version.clone(),
        sha256: pin.sha256.clone(),
    }
}

pub(super) fn run(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    tools: &[Tool],
    global: &GlobalArguments,
) -> ExitCode {
    let plan = match contributor.run(invocation) {
        Ok(plan) => plan,
        Err(diagnostics) => {
            return finish(global, invocation, SourceMap::default(), &diagnostics, &[]);
        }
    };
    let done = carry_out(
        project,
        contributor,
        descriptor,
        invocation,
        plan,
        tools,
        InPlaceMode::Tool,
        Some(global),
    );
    let _ = io::stdout().write_all(&done.stdout);
    finish(
        global,
        invocation,
        done.sources,
        &done.diagnostics,
        &done.refused,
    )
}

/// Runs `contributor` for a delegated verb and returns stdout, sources and
/// mapped diagnostics without rendering them yet.
pub(super) fn outcome(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    tools: &[Tool],
    global: &GlobalArguments,
    in_place: InPlaceMode,
) -> (Vec<u8>, SourceMap, Vec<Diagnostic>) {
    let plan = match contributor.run(invocation) {
        Ok(plan) => plan,
        Err(diagnostics) => {
            let mut sources = SourceMap::default();
            let all = map_diagnostics(&diagnostics, invocation, &mut sources);
            return (Vec::new(), sources, all);
        }
    };
    let mut done = carry_out(
        project,
        contributor,
        descriptor,
        invocation,
        plan,
        tools,
        in_place,
        Some(global),
    );
    let mut all = map_diagnostics(&done.diagnostics, invocation, &mut done.sources);
    all.extend(done.refused);
    (done.stdout, done.sources, all)
}

/// How many plans one command may chain through `resume`.
const ROUNDS: usize = 16;

/// What a plan and the plans resuming it print and report.
#[derive(Default)]
pub(super) struct Done {
    pub(super) stdout: Vec<u8>,
    pub(super) diagnostics: Vec<command::Diagnostic>,
    /// The host's own diagnostic, when it refused, a step failed, or publication failed.
    pub(super) refused: Vec<Diagnostic>,
    /// Sources referenced by host diagnostics (such as `run-flick` compiler/runtime spans).
    pub(super) sources: SourceMap,
}

/// Checks `plan` against `descriptor` and runs its steps, then those of
/// the plans `resume` continues it with, and publishes all their files;
/// an error leaves the project as it was.
#[allow(clippy::too_many_arguments)]
pub(super) fn carry_out(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    plan: Plan,
    tools: &[Tool],
    in_place: InPlaceMode,
    global: Option<&GlobalArguments>,
) -> Done {
    let mut done = Done::default();
    let call = (project, contributor, descriptor, invocation, tools);
    let published = match chain(call, plan, in_place, global, &mut done) {
        Ok(Some(files)) => publish(&project.root, &files),
        Ok(None) => Ok(()),
        Err(refused) => Err(refused),
    };
    done.refused.extend(published.err());
    done
}

pub(super) type Call<'a> = (
    &'a Project,
    &'a dyn Contributor,
    &'a Descriptor,
    &'a Invocation,
    &'a [Tool],
);

/// What `plan` and the plans resuming it write, carried out up to
/// publishing; none when the contributor or a `run-flick` step stopped with
/// diagnostics. When the final round has in-place steps, accumulated files
/// are published before running those steps and an empty list is returned.
pub(super) fn chain(
    call: Call,
    mut plan: Plan,
    in_place: InPlaceMode,
    global: Option<&GlobalArguments>,
    done: &mut Done,
) -> Result<Option<Vec<Artifact>>, Diagnostic> {
    let (project, contributor, descriptor, invocation, tools) = call;
    let refused = |why: String| {
        Diagnostic::new(
            codes::P055,
            Severity::Error,
            format!("the plan of `{}` is refused: {why}", descriptor.name),
        )
    };
    let writes = expand(&descriptor.writes, invocation).map_err(refused)?;
    let reads = expand(&descriptor.reads, invocation).map_err(refused)?;
    let root = &project.root;
    let cancel = component::cancellation().map_err(refused)?;
    component::cancel_on_signals();
    let mut files: Vec<Artifact> = Vec::new();
    let mut projections = Projections::default();
    let allow_in_place = in_place != InPlaceMode::Forbidden;
    for round in 1..=ROUNDS {
        done.stdout.append(&mut plan.stdout);
        done.diagnostics.append(&mut plan.diagnostics);
        let validated = validate_plan(&plan, invocation, &writes, &reads, root, allow_in_place)
            .map_err(refused)?;
        let (plan_artifacts, mut produced) = plan_files(&plan);

        let Some(step_outputs) = execute_steps(
            project,
            invocation,
            tools,
            validated.regular,
            &mut produced,
            cancel,
            done,
        )?
        else {
            return Ok(None);
        };

        files.extend(plan_artifacts);
        if plan.resume.is_none() {
            if !plan.needs.is_empty() {
                return Err(refused("it needs what it does not resume with".into()));
            }
            files.extend(step_outputs);
            if !validated.in_place.is_empty() {
                publish(root, &files)?;
                for step in validated.in_place {
                    run_in_place(root, step, tools, in_place, global)?;
                }
                return Ok(Some(Vec::new()));
            }
            return Ok(Some(files));
        }
        let state = plan.resume.take().expect("checked");
        let answers = projections.answer(root, &plan.needs);
        if round == ROUNDS {
            break;
        }
        let outputs = (step_outputs.into_iter())
            .map(|a| command::File {
                executable: a.kind == ArtifactKind::Executable,
                path: a.path,
                bytes: a.contents,
            })
            .collect();
        match contributor.resume(state, outputs, answers) {
            Ok(next) => plan = next,
            Err(diagnostics) => {
                done.diagnostics.extend(diagnostics);
                return Ok(None);
            }
        }
    }
    Err(refused(format!("it resumes more than {ROUNDS} times")))
}

fn prepare_in_place(
    root: &Path,
    step: &ToolStep,
    tools: &[Tool],
    mode: InPlaceMode,
    spawn_code: Code,
) -> Result<Command, Diagnostic> {
    let tool = (tools.iter())
        .find(|t| {
            t.pin.id == step.tool.id
                && t.pin.version == step.tool.version
                && t.pin.sha256 == step.tool.sha256
        })
        .ok_or_else(|| {
            Diagnostic::new(
                codes::P055,
                Severity::Error,
                format!(
                    "step `{}` names an unpinned tool `{}`",
                    step.id, step.tool.id
                ),
            )
        })?;
    let verified = tool.recheck().map_err(|why| {
        Diagnostic::new(
            spawn_code,
            Severity::Error,
            format!("step `{}`: {why}", step.id),
        )
    })?;
    let mut process = if mode == InPlaceMode::Client {
        verified.client_command(root)
    } else {
        verified.step(root)
    };
    process.args(&step.arguments);
    for var in &step.environment {
        process.env(&var.name, &var.value);
    }
    Ok(process)
}

/// Spawns a single validated in-place client step in the background and
/// returns its live child handle (for `flick dev`).
pub(super) fn spawn_in_place(
    root: &Path,
    step: &ToolStep,
    tools: &[Tool],
    event_kind: &'static str,
    global: &GlobalArguments,
) -> Result<child::Running, Diagnostic> {
    let mut process = prepare_in_place(root, step, tools, InPlaceMode::Client, codes::P015)?;
    child::spawn(&mut process, global, event_kind).map_err(|err| {
        Diagnostic::new(
            codes::P015,
            Severity::Error,
            format!("cannot run `{}`: {err}", step.tool.id),
        )
    })
}

fn run_in_place(
    root: &Path,
    step: &ToolStep,
    tools: &[Tool],
    mode: InPlaceMode,
    global: Option<&GlobalArguments>,
) -> Result<(), Diagnostic> {
    let (event_kind, fail_code, spawn_code) = match mode {
        InPlaceMode::PackageManager => ("package-manager-output", codes::P017, codes::P016),
        InPlaceMode::Client => ("client-output", codes::P017, codes::P015),
        InPlaceMode::Tool | InPlaceMode::Forbidden => ("tool-output", codes::P050, codes::P050),
    };
    let mut process = prepare_in_place(root, step, tools, mode, spawn_code)?;
    let status = if let Some(global) = global {
        let running = if mode == InPlaceMode::Client || step.outputs.is_empty() {
            child::spawn(&mut process, global, event_kind)
        } else {
            child::spawn_tool(&mut process, global, event_kind)
        };
        running.and_then(|r| r.wait(&child::signals()))
    } else {
        process.status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(Diagnostic::new(
            fail_code,
            Severity::Error,
            format!(
                "`{}` failed with exit code {}",
                step.tool.id,
                child::code(s)
            ),
        )),
        Err(err) => Err(Diagnostic::new(
            spawn_code,
            Severity::Error,
            format!("cannot run `{}`: {err}", step.tool.id),
        )),
    }
}

fn plan_files(plan: &Plan) -> (Vec<Artifact>, BTreeMap<String, Artifact>) {
    let mut produced = BTreeMap::new();
    let mut artifacts = Vec::with_capacity(plan.files.len());
    let resuming = plan.resume.is_some();
    for f in &plan.files {
        let artifact = Artifact {
            path: f.path.clone(),
            contents: f.bytes.clone(),
            kind: if f.executable {
                ArtifactKind::Executable
            } else {
                ArtifactKind::Resource
            },
        };
        produced.insert(artifact.path.clone(), artifact.clone());
        if !(resuming && is_step_scratch(&f.path)) {
            artifacts.push(artifact);
        }
    }
    (artifacts, produced)
}

fn execute_steps(
    project: &Project,
    invocation: &Invocation,
    tools: &[Tool],
    prepared: Vec<PreparedStep<'_>>,
    produced: &mut BTreeMap<String, Artifact>,
    cancel: &Cancellation,
    done: &mut Done,
) -> Result<Option<Vec<Artifact>>, Diagnostic> {
    // the step directories go beside `.flick/commands`, inside the project
    let anchor = project.root.join(".flick").join("commands");
    let mut step_outputs: Vec<Artifact> = Vec::new();
    for step in prepared {
        match step {
            PreparedStep::Tool {
                step,
                toolchain,
                reads: step_reads,
            } => {
                let input_artifacts: Vec<flick_backend::Artifact> = step
                    .inputs
                    .iter()
                    .filter_map(|path| produced.get(path))
                    .map(|a| flick_backend::Artifact {
                        id: format!("in:{}", a.path),
                        path: a.path.clone(),
                        kind: a.kind,
                        bytes: a.contents.clone(),
                    })
                    .collect();
                let result = BuildResult {
                    artifacts: input_artifacts,
                    steps: vec![*toolchain],
                    runtime_dependencies: vec![],
                    launches: vec![],
                    diagnostics: vec![],
                };
                let mut links = Reads::new();
                if !step_reads.is_empty() {
                    links.insert(0, step_reads);
                }
                let outs = run_steps(
                    &result,
                    &[0],
                    &BTreeMap::new(),
                    &links,
                    tools,
                    &anchor,
                    cancel,
                )
                .map_err(|why| Diagnostic::new(codes::P050, Severity::Error, why))?;
                for artifact in outs {
                    produced.insert(artifact.path.clone(), artifact.clone());
                    step_outputs.push(artifact);
                }
            }
            PreparedStep::RunFlick(step) => {
                match run_flick::execute_step(project, step, invocation, Some(cancel)) {
                    Ok(stdout) => {
                        if let Some(path) = &step.output {
                            let artifact = Artifact {
                                path: path.clone(),
                                contents: stdout,
                                kind: ArtifactKind::Resource,
                            };
                            produced.insert(artifact.path.clone(), artifact.clone());
                            step_outputs.push(artifact);
                        }
                    }
                    Err((step_sources, step_diags)) => {
                        let merged = run_flick::merge_diagnostics(
                            &mut done.sources,
                            &step_sources,
                            step_diags,
                        );
                        done.refused.extend(merged);
                        return Ok(None);
                    }
                }
            }
        }
    }
    Ok(Some(step_outputs))
}

fn publish(root: &Path, files: &[Artifact]) -> Result<(), Diagnostic> {
    if files.is_empty() {
        return Ok(());
    }
    let refused = |why| Diagnostic::new(codes::P055, Severity::Error, why);
    validate_destination_paths(root, files.iter().map(|f| f.path.as_str())).map_err(refused)?;
    publish::commit(root, files, &[], None)
        .map_err(|e| Diagnostic::new(codes::P013, Severity::Error, format!("cannot publish: {e}")))
}

fn map_diagnostics(
    diagnostics: &[command::Diagnostic],
    invocation: &Invocation,
    sources: &mut SourceMap,
) -> Vec<Diagnostic> {
    let mut files = BTreeMap::new();
    diagnostics
        .iter()
        .map(|d| diagnostic(d, invocation, sources, &mut files))
        .collect()
}

/// Reports the contributor's diagnostics and the host's own, with the exit
/// code an error gives.
pub(super) fn finish(
    global: &GlobalArguments,
    invocation: &Invocation,
    mut sources: SourceMap,
    diagnostics: &[command::Diagnostic],
    host: &[Diagnostic],
) -> ExitCode {
    let mut all = map_diagnostics(diagnostics, invocation, &mut sources);
    all.extend(host.iter().cloned());
    report::render(global, &sources, &all);
    report::exit_code(&all)
}

/// The compiler's form of `d`, without an invocation's input map (used for
/// template scaffolding diagnostics).
pub(crate) fn standalone_diagnostic(d: &command::Diagnostic) -> Diagnostic {
    let code = codes::lookup(&format!("FLICK-{}", d.code)).map_or(codes::P013, |info| info.code);
    let severity = match d.severity {
        command::Severity::Error => Severity::Error,
        command::Severity::Warning => Severity::Warning,
        command::Severity::Note => Severity::Note,
    };
    let mut message = d.message.clone();
    for note in &d.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    let mut out = Diagnostic::new(code, severity, message);
    if let Some(help) = &d.help {
        out = out.with_fix(|| (help.clone(), Vec::new()));
    }
    out
}

/// The compiler's form of `d`, its span into the input it names.
fn diagnostic(
    d: &command::Diagnostic,
    invocation: &Invocation,
    sources: &mut SourceMap,
    files: &mut BTreeMap<String, FileId>,
) -> Diagnostic {
    let mut out = standalone_diagnostic(d);
    if let Some(span) = &d.span {
        let file = files.get(&span.path).copied().or_else(|| {
            let input = invocation.inputs.iter().find(|f| f.path == span.path)?;
            let text = str::from_utf8(&input.bytes).ok()?;
            let id = sources.add(span.path.clone(), text.to_owned());
            files.insert(span.path.clone(), id);
            Some(id)
        });
        if let Some(file) = file {
            let text = &sources.get(file).text;
            let start = span.start as usize;
            let end = span.end as usize;
            if span.start <= span.end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end)
            {
                out = out.with_span(Span::new(file, span.start, span.end));
            }
        }
    }
    out
}
