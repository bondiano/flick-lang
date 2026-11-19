//! A plan command: its plan is checked against what the command declared
//! before anything happens, then its tool steps run and its files are
//! published in one transaction under the project's root.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Component, Path};
use std::process::ExitCode;

use flick_backend::command::{self, Descriptor, Invocation, Plan, Step};
use flick_backend::{
    ArtifactKind, BuildResult, InputRef, Limits, OptionValue, StepOutput, Toolchain, ToolchainStep,
    validate_output_paths,
};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::SourceMap;

use super::{Contributor, values};
use crate::args::GlobalArguments;
use crate::artifact::{Artifact, validate_destination_paths};
use crate::backends::{Tool, authorize, run_steps};
use crate::{component, publish, report};

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
        Err(diagnostics) => return finish(global, &diagnostics, &[]),
    };
    let refused = carry_out(project, descriptor, invocation, &plan, tools);
    let _ = io::stdout().write_all(&plan.stdout);
    finish(
        global,
        &plan.diagnostics,
        &refused.err().into_iter().collect::<Vec<_>>(),
    )
}

/// Checks `plan` against `descriptor`, runs its steps and publishes its
/// files and their outputs; an error leaves the project as it was.
pub(super) fn carry_out(
    project: &Project,
    descriptor: &Descriptor,
    invocation: &Invocation,
    plan: &Plan,
    tools: &[Tool],
) -> Result<(), Diagnostic> {
    let refused = |why: String| {
        Diagnostic::new(
            codes::P055,
            Severity::Error,
            format!("the plan of `{}` is refused: {why}", descriptor.name),
        )
    };
    let writes = writes(descriptor, invocation).map_err(refused)?;
    let (result, order) = graph(plan, &invocation.tools).map_err(refused)?;
    let paths = (result.artifacts.iter().map(|a| a.path.as_str())).chain(
        result
            .steps
            .iter()
            .flat_map(|s| s.outputs.iter().map(|o| o.path.as_str())),
    );
    let paths: Vec<&str> = paths.collect();
    validate_output_paths(paths.iter().copied(), Limits::default()).map_err(refused)?;
    if let Some(path) = paths.iter().find(|p| !under(p, &writes)) {
        return Err(refused(format!(
            "it writes `{path}`, which the command does not declare"
        )));
    }
    for step in &result.steps {
        authorize(step).map_err(refused)?;
    }
    let root = &project.root;
    let cancel = component::cancellation().map_err(refused)?;
    component::cancel_on_signals();
    // the step directories go beside `.flick/commands`, inside the project
    let anchor = root.join(".flick").join("commands");
    let outputs = run_steps(&result, &order, &BTreeMap::new(), tools, &anchor, cancel)
        .map_err(|why| Diagnostic::new(codes::P050, Severity::Error, why))?;
    let files: Vec<Artifact> = (result.artifacts.into_iter().map(Artifact::from))
        .chain(outputs)
        .collect();
    validate_destination_paths(root, files.iter().map(|f| f.path.as_str())).map_err(refused)?;
    publish::commit(root, &files, &[], None)
        .map_err(|e| Diagnostic::new(codes::P013, Severity::Error, format!("cannot publish: {e}")))
}

/// The paths the command may write, relative to the root: what
/// `descriptor` names, `$name` replaced by that path argument.
fn writes(descriptor: &Descriptor, invocation: &Invocation) -> Result<Vec<String>, String> {
    let values = values(invocation);
    let mut writes = Vec::new();
    for write in &descriptor.writes {
        match write.strip_prefix('$') {
            Some(name) => {
                writes.extend(values.get(name).into_iter().flat_map(|v| v.iter()).cloned());
            }
            None => writes.push(write.clone()),
        }
    }
    for write in &writes {
        let normal = Path::new(write)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        if !normal {
            return Err(format!("the declared path `{write}` leaves the project"));
        }
    }
    Ok(writes)
}

fn under(path: &str, writes: &[String]) -> bool {
    writes.iter().any(|w| {
        path == w
            || path
                .strip_prefix(w.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })
}

/// The plan as a build result the host's step executor runs: its files as
/// artifacts, each step reading files and the outputs of earlier steps.
fn graph(plan: &Plan, granted: &[command::Tool]) -> Result<(BuildResult, Vec<usize>), String> {
    let mut produced: BTreeMap<&str, usize> = BTreeMap::new();
    let mut steps = Vec::new();
    for (i, Step::Tool(step)) in plan.steps.iter().enumerate() {
        if !granted.contains(&step.tool) {
            return Err(format!(
                "step `{}` runs `{}` {} #{}, which the host did not pin for it",
                step.id, step.tool.id, step.tool.version, step.tool.sha256
            ));
        }
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
        steps.push(ToolchainStep {
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
        });
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

/// Reports the contributor's diagnostics and the host's own, with the exit
/// code an error gives.
pub(super) fn finish(
    global: &GlobalArguments,
    diagnostics: &[command::Diagnostic],
    host: &[Diagnostic],
) -> ExitCode {
    let mut all: Vec<Diagnostic> = diagnostics.iter().map(diagnostic).collect();
    all.extend(host.iter().cloned());
    report::render(global, &SourceMap::default(), &all);
    report::exit_code(&all)
}

fn diagnostic(d: &command::Diagnostic) -> Diagnostic {
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
    Diagnostic::new(code, severity, message)
}
