//! A plan command: its plan is checked against what the command declared
//! before anything happens, then its tool steps run and its files are
//! published in one transaction under the project's root.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::{fs, mem};

use flick_backend::command::{self, Descriptor, Invocation, Plan, Step, ToolStep};
use flick_backend::{
    ArtifactKind, BuildResult, InputRef, Limits, OptionValue, StepOutput, Toolchain, ToolchainStep,
    validate_output_paths,
};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::{FileId, SourceMap, Span};

use super::Contributor;
use super::inputs::{expand, normal, under};
use super::projection::Projections;
use crate::args::GlobalArguments;
use crate::artifact::{Artifact, validate_destination_paths};
use crate::backends::{Reads, Tool, authorize, run_steps};
use crate::{child, component, publish, report};

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
        Err(diagnostics) => return finish(global, invocation, &diagnostics, &[]),
    };
    let done = carry_out(
        project,
        contributor,
        descriptor,
        invocation,
        plan,
        tools,
        global,
    );
    let _ = io::stdout().write_all(&done.stdout);
    finish(global, invocation, &done.diagnostics, &done.refused)
}

/// What the plan of `invocation` printed and reported, carried out, for a
/// verb of flick that reports it with its own.
pub(super) fn outcome(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    tools: &[Tool],
    global: &GlobalArguments,
) -> (Vec<u8>, SourceMap, Vec<Diagnostic>) {
    let (stdout, diagnostics, refused) = match contributor.run(invocation) {
        Ok(plan) => {
            let done = carry_out(
                project,
                contributor,
                descriptor,
                invocation,
                plan,
                tools,
                global,
            );
            (done.stdout, done.diagnostics, done.refused)
        }
        Err(diagnostics) => (Vec::new(), diagnostics, Vec::new()),
    };
    let (sources, mut all) = mapped(invocation, &diagnostics);
    all.extend(refused);
    (stdout, sources, all)
}

/// How many plans one command may chain through `resume`.
const ROUNDS: usize = 16;

/// What a plan and the plans resuming it print and report.
pub(super) struct Done {
    pub(super) stdout: Vec<u8>,
    pub(super) diagnostics: Vec<command::Diagnostic>,
    /// The host's own diagnostic, when it refused or could not publish.
    pub(super) refused: Vec<Diagnostic>,
}

/// Checks `plan` against `descriptor` and runs its steps, then those of
/// the plans `resume` continues it with, and publishes all their files;
/// an error leaves the project as it was.
pub(super) fn carry_out(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    plan: Plan,
    tools: &[Tool],
    global: &GlobalArguments,
) -> Done {
    let mut done = Done {
        stdout: Vec::new(),
        diagnostics: Vec::new(),
        refused: Vec::new(),
    };
    let call = (
        project,
        contributor,
        descriptor,
        invocation,
        tools,
        Some(global),
    );
    let published = match chain(call, plan, &mut done) {
        Ok(Some(files)) => publish(&project.root, &files),
        Ok(None) => Ok(()),
        Err(refused) => Err(refused),
    };
    done.refused.extend(published.err());
    done
}

/// What a plan runs for; without the arguments of `flick`, as a preview,
/// it may change nothing in place.
pub(super) type Call<'a> = (
    &'a Project,
    &'a dyn Contributor,
    &'a Descriptor,
    &'a Invocation,
    &'a [Tool],
    Option<&'a GlobalArguments>,
);

/// What `plan` and the plans resuming it write, carried out up to
/// publishing; none when the contributor stopped with diagnostics.
pub(super) fn chain(
    call: Call,
    mut plan: Plan,
    done: &mut Done,
) -> Result<Option<Vec<Artifact>>, Diagnostic> {
    let (project, contributor, descriptor, invocation, tools, global) = call;
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
    for round in 1..=ROUNDS {
        done.stdout.append(&mut plan.stdout);
        done.diagnostics.append(&mut plan.diagnostics);
        let in_place: Vec<ToolStep> = (plan.steps.extract_if(.., |Step::Tool(s)| s.in_place))
            .map(|Step::Tool(s)| s)
            .collect();
        let (result, order) = graph(&plan, &invocation.tools).map_err(refused)?;
        let outputs = (result.steps.iter()).flat_map(|s| s.outputs.iter().map(|o| o.path.as_str()));
        let paths: Vec<&str> = (result.artifacts.iter().map(|a| a.path.as_str()))
            .chain(outputs)
            .collect();
        validate_output_paths(paths.iter().copied(), Limits::default()).map_err(refused)?;
        // what `resume` gets is never written, so it need not be declared
        let written = result.artifacts.len()
            + if plan.resume.is_some() {
                0
            } else {
                paths.len()
            };
        if let Some(path) = paths.iter().take(written).find(|p| !under(p, &writes)) {
            return Err(refused(format!(
                "it writes `{path}`, which the command does not declare"
            )));
        }
        for step in &result.steps {
            authorize(step).map_err(refused)?;
        }
        if !in_place.is_empty() && global.is_none() {
            return Err(refused(
                "it changes the project in place, which a preview may not".into(),
            ));
        }
        for step in &in_place {
            check_in_place(step, &invocation.tools, &writes).map_err(refused)?;
        }
        let links = links(&plan, &reads, root).map_err(refused)?;
        // the step directories go beside `.flick/commands`, inside the project
        let anchor = root.join(".flick").join("commands");
        let outputs = run_steps(
            &result,
            &order,
            &BTreeMap::new(),
            &links,
            tools,
            &anchor,
            cancel,
        )
        .map_err(|why| Diagnostic::new(codes::P050, Severity::Error, why))?;
        files.extend(result.artifacts.into_iter().map(Artifact::from));
        let state = plan.resume.take();
        if state.is_none() && !plan.needs.is_empty() {
            return Err(refused("it needs what it does not resume with".into()));
        }
        let outputs = if state.is_some() {
            outputs
        } else {
            files.extend(outputs);
            Vec::new()
        };
        if let Some(global) = global.filter(|_| !in_place.is_empty()) {
            // what runs in place sees the files so far, which are published first
            publish(root, &mem::take(&mut files))?;
            for step in &in_place {
                run_in_place(step, tools, root, global)?;
            }
        }
        let Some(state) = state else {
            return Ok(Some(files));
        };
        let answers = projections.answer(root, &plan.needs);
        if round == ROUNDS {
            break;
        }
        let outputs = (outputs.into_iter())
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

/// An in-place step reads and inputs nothing of the plan, runs a tool the
/// host found, and changes only what the command declares it writes.
fn check_in_place(
    step: &ToolStep,
    granted: &[command::Tool],
    writes: &[String],
) -> Result<(), String> {
    if !step.inputs.is_empty() || !step.reads.is_empty() {
        return Err(format!(
            "step `{}` runs in place, so it inputs and reads nothing of the plan",
            step.id
        ));
    }
    pinned(step, granted)?;
    authorize(&toolchain_step(0, step, Vec::new(), Vec::new())).map_err(|why| why.to_string())?;
    match step
        .outputs
        .iter()
        .find(|o| !normal(o) || !under(o, writes))
    {
        Some(path) => Err(format!(
            "step `{}` changes `{path}`, which the command does not declare",
            step.id
        )),
        None => Ok(()),
    }
}

/// Runs `step` in the project's root, its output passing through: to
/// stderr with the rest of the host's when it changes the project.
fn run_in_place(
    step: &ToolStep,
    tools: &[Tool],
    root: &Path,
    global: &GlobalArguments,
) -> Result<(), Diagnostic> {
    let failed = |why: String| {
        Diagnostic::new(
            codes::P050,
            Severity::Error,
            format!("step `{}` (`{}`) failed: {why}", step.id, step.tool.id),
        )
    };
    let pin = Toolchain {
        id: step.tool.id.clone(),
        version: step.tool.version.clone(),
        sha256: step.tool.sha256.clone(),
    };
    // the binary is checked again: it may have changed since it was pinned
    let tool = (tools.iter())
        .find(|t| t.pin == pin)
        .ok_or_else(|| "the host resolved no such tool".to_owned())
        .and_then(Tool::recheck)
        .map_err(failed)?;
    let mut process = tool.step(root);
    process
        .args(&step.arguments)
        .envs(step.environment.iter().map(|v| (&v.name, &v.value)));
    let running = if step.outputs.is_empty() {
        child::spawn(&mut process, global, "tool-output")
    } else {
        child::spawn_tool(&mut process, global, "tool-output")
    };
    match running.and_then(|r| r.wait(&child::signals())) {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(failed(format!(
            "it exited with code {}",
            child::code(status)
        ))),
        Err(e) => Err(failed(e.to_string())),
    }
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

/// What each step of `plan` reads in place, within `reads`, linked to the
/// project's files.
fn links(plan: &Plan, reads: &[String], root: &Path) -> Result<Reads, String> {
    let mut links = Reads::new();
    for (i, Step::Tool(step)) in plan.steps.iter().enumerate() {
        for read in &step.reads {
            if !normal(read) || !under(read, reads) {
                return Err(format!(
                    "step `{}` reads `{read}`, which the command does not declare",
                    step.id
                ));
            }
            let target = root.join(read);
            if fs::symlink_metadata(&target).is_err() {
                return Err(format!(
                    "step `{}` reads `{read}`, which the project has not",
                    step.id
                ));
            }
            links.entry(i).or_default().push((read.clone(), target));
        }
    }
    Ok(links)
}

/// The plan as a build result the host's step executor runs: its files as
/// artifacts, each step reading files and the outputs of earlier steps.
fn graph(plan: &Plan, granted: &[command::Tool]) -> Result<(BuildResult, Vec<usize>), String> {
    let mut produced: BTreeMap<&str, usize> = BTreeMap::new();
    let mut steps = Vec::new();
    for (i, Step::Tool(step)) in plan.steps.iter().enumerate() {
        pinned(step, granted)?;
        let mut dependencies = Vec::new();
        let mut inputs = Vec::new();
        for input in &step.inputs {
            if let Some(&j) = produced.get(input.as_str()) {
                dependencies.push(format!("step:{j}"));
                inputs.push(InputRef::Artifact(format!("out:{input}")));
            } else if plan.files.iter().any(|f| &f.path == input) {
                inputs.push(InputRef::Artifact(format!("file:{input}")));
            } else {
                return Err(format!(
                    "step `{}` reads `{input}`, which the plan has not",
                    step.id
                ));
            }
        }
        dependencies.dedup();
        for output in &step.outputs {
            produced.insert(output, i);
        }
        steps.push(toolchain_step(i, step, dependencies, inputs));
    }
    let artifacts = (plan.files.iter())
        .map(|f| flick_backend::Artifact {
            id: format!("file:{}", f.path),
            path: f.path.clone(),
            kind: if f.executable {
                ArtifactKind::Executable
            } else {
                ArtifactKind::Resource
            },
            bytes: f.bytes.clone(),
        })
        .collect();
    let order = (0..steps.len()).collect();
    let result = BuildResult {
        artifacts,
        steps,
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    };
    Ok((result, order))
}

/// A step runs only a tool the host pinned for this call, and found.
fn pinned(step: &ToolStep, granted: &[command::Tool]) -> Result<(), String> {
    if step.tool.sha256.is_empty() || !granted.contains(&step.tool) {
        return Err(format!(
            "step `{}` runs `{}` {} #{}, which the host did not pin for it",
            step.id, step.tool.id, step.tool.version, step.tool.sha256
        ));
    }
    Ok(())
}

/// `step`, the `i`th of its plan, as the host's step executor runs it.
fn toolchain_step(
    i: usize,
    step: &ToolStep,
    dependencies: Vec<String>,
    inputs: Vec<InputRef>,
) -> ToolchainStep {
    ToolchainStep {
        id: format!("step:{i}"),
        dependencies,
        toolchain: Toolchain {
            id: step.tool.id.clone(),
            version: step.tool.version.clone(),
            sha256: step.tool.sha256.clone(),
        },
        arguments: step.arguments.clone(),
        environment: (step.environment.iter())
            .map(|v| OptionValue {
                key: v.name.clone(),
                value: v.value.clone(),
            })
            .collect(),
        inputs,
        outputs: (step.outputs.iter())
            .map(|o| StepOutput {
                id: format!("out:{o}"),
                path: o.clone(),
                kind: ArtifactKind::Resource,
            })
            .collect(),
    }
}

/// Reports the contributor's diagnostics and the host's own, with the exit
/// code an error gives.
pub(super) fn finish(
    global: &GlobalArguments,
    invocation: &Invocation,
    diagnostics: &[command::Diagnostic],
    host: &[Diagnostic],
) -> ExitCode {
    let (sources, mut all) = mapped(invocation, diagnostics);
    all.extend(host.iter().cloned());
    report::render(global, &sources, &all);
    report::exit_code(&all)
}

/// The compiler's form of the contributor's `diagnostics`, with the inputs
/// their spans point into.
fn mapped(
    invocation: &Invocation,
    diagnostics: &[command::Diagnostic],
) -> (SourceMap, Vec<Diagnostic>) {
    let mut sources = SourceMap::default();
    let mut files = BTreeMap::new();
    let all = (diagnostics.iter())
        .map(|d| diagnostic(d, invocation, &mut sources, &mut files))
        .collect();
    (sources, all)
}

/// The compiler's form of `d`, its span into the input it names.
fn diagnostic(
    d: &command::Diagnostic,
    invocation: &Invocation,
    sources: &mut SourceMap,
    files: &mut BTreeMap<String, FileId>,
) -> Diagnostic {
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
    if let Some(span) = &d.span {
        let file = files.get(&span.path).copied().or_else(|| {
            let input = invocation.inputs.iter().find(|f| f.path == span.path)?;
            let text = String::from_utf8_lossy(&input.bytes).into_owned();
            let id = sources.add(span.path.clone(), text);
            files.insert(span.path.clone(), id);
            Some(id)
        });
        let len = file.map_or(0, |f| sources.get(f).text.len());
        if let Some(file) = file
            && span.start <= span.end
            && span.end as usize <= len
        {
            out = out.with_span(Span::new(file, span.start, span.end));
        }
    }
    if let Some(help) = &d.help {
        out = out.with_fix(|| (help.clone(), Vec::new()));
    }
    out
}
