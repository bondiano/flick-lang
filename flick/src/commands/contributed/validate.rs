//! Upfront host validation of contributed command plans (`Step::Tool` and
//! `Step::RunFlick`) before any step executes.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str;

use flick_backend::command::{Invocation, Plan, RunFlickStep, Step, ToolStep};
use flick_backend::{
    ArtifactKind, InputRef, Limits, OptionValue, StepOutput, Toolchain, ToolchainStep,
    validate_output_paths,
};
use flick_core::project;

use super::inputs::{normal, under};
use crate::backends::authorize;

/// Validated metadata for one staged step in a plan, ready for sequential execution.
pub(super) enum PreparedStep<'a> {
    Tool {
        step: &'a ToolStep,
        toolchain: Box<ToolchainStep>,
        reads: Vec<(String, PathBuf)>,
    },
    RunFlick(&'a RunFlickStep),
}

/// Validated steps of a plan: staged steps run before publishing; in-place
/// tool steps run directly in the project root after publishing.
pub(super) struct ValidatedPlan<'a> {
    pub(super) regular: Vec<PreparedStep<'a>>,
    pub(super) in_place: Vec<&'a ToolStep>,
}

fn check_bounded_string(label: &str, value: &str, limits: Limits) -> Result<(), String> {
    if value.len() > limits.string_bytes || value.contains('\0') {
        return Err(format!("{label} exceeds its limit or contains NUL"));
    }
    Ok(())
}

/// Validates every step, path, tool grant, overlay module, and origin span in
/// `plan` BEFORE any step executes.
pub(super) fn validate_plan<'a>(
    plan: &'a Plan,
    invocation: &Invocation,
    writes: &[String],
    reads: &[String],
    root: &Path,
    allow_in_place: bool,
) -> Result<ValidatedPlan<'a>, String> {
    let limits = Limits::default();
    if plan.steps.len() > limits.steps {
        return Err(format!("it declares more than {} steps", limits.steps));
    }

    let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
    let mut available: BTreeSet<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    let mut all_paths: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    let mut regular: Vec<PreparedStep<'a>> = Vec::with_capacity(plan.steps.len());
    let mut in_place: Vec<&'a ToolStep> = Vec::new();

    for (i, step) in plan.steps.iter().enumerate() {
        match step {
            Step::Tool(t) if t.in_place => {
                validate_in_place_step(
                    i,
                    t,
                    plan,
                    invocation,
                    writes,
                    limits,
                    allow_in_place,
                    &mut seen_ids,
                )?;
                in_place.push(t);
            }
            Step::Tool(t) => {
                let entry = validate_tool_step(
                    i,
                    t,
                    invocation,
                    reads,
                    root,
                    limits,
                    &mut seen_ids,
                    &mut available,
                    &mut all_paths,
                )?;
                regular.push(entry);
            }
            Step::RunFlick(r) => {
                validate_run_flick_step(
                    r,
                    invocation,
                    limits,
                    &mut seen_ids,
                    &mut available,
                    &mut all_paths,
                )?;
                regular.push(PreparedStep::RunFlick(r));
            }
        }
    }

    validate_output_paths(all_paths.iter().copied(), limits)?;
    // What `resume` gets is never written, so step outputs need not be declared in `writes`.
    let written = if plan.resume.is_some() {
        plan.files.len()
    } else {
        all_paths.len()
    };
    if let Some(path) = all_paths.iter().take(written).find(|p| !under(p, writes)) {
        return Err(format!(
            "it writes `{path}`, which the command does not declare"
        ));
    }

    Ok(ValidatedPlan { regular, in_place })
}

#[allow(clippy::too_many_arguments)]
fn validate_in_place_step<'a>(
    index: usize,
    t: &'a ToolStep,
    plan: &Plan,
    invocation: &Invocation,
    writes: &[String],
    limits: Limits,
    allow_in_place: bool,
    seen_ids: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    if !allow_in_place {
        return Err(format!(
            "step `{}` runs in place, which cannot be previewed",
            t.id
        ));
    }
    if plan.resume.is_some() {
        return Err(format!("step `{}` runs in place and cannot resume", t.id));
    }
    if t.id.is_empty() {
        return Err("step has an empty id".into());
    }
    check_bounded_string("step id", &t.id, limits)?;
    if !seen_ids.insert(t.id.as_str()) {
        return Err(format!("duplicate step id `{}`", t.id));
    }
    if t.tool.sha256.is_empty() {
        return Err(format!(
            "step `{}` runs `{}`, which is not on PATH",
            t.id, t.tool.id
        ));
    }
    if !invocation.tools.contains(&t.tool) {
        return Err(format!(
            "step `{}` runs `{}` {} #{}, which the host did not pin for it",
            t.id, t.tool.id, t.tool.version, t.tool.sha256
        ));
    }
    if !t.inputs.is_empty() || !t.reads.is_empty() {
        return Err(format!(
            "step `{}` runs in place and takes no `inputs` or `reads`",
            t.id
        ));
    }
    for output in &t.outputs {
        if !normal(output) || !under(output, writes) {
            return Err(format!(
                "step `{}` writes `{output}` in place, which the command does not declare",
                t.id
            ));
        }
    }
    let toolchain = ToolchainStep {
        id: format!("step:{index}"),
        dependencies: vec![],
        toolchain: Toolchain {
            id: t.tool.id.clone(),
            version: t.tool.version.clone(),
            sha256: t.tool.sha256.clone(),
        },
        arguments: t.arguments.clone(),
        environment: (t.environment.iter())
            .map(|v| OptionValue {
                key: v.name.clone(),
                value: v.value.clone(),
            })
            .collect(),
        inputs: vec![],
        outputs: vec![],
    };
    authorize(&toolchain)
}

#[allow(clippy::too_many_arguments)]
fn validate_tool_step<'a>(
    index: usize,
    t: &'a ToolStep,
    invocation: &Invocation,
    reads: &[String],
    root: &Path,
    limits: Limits,
    seen_ids: &mut BTreeSet<&'a str>,
    available: &mut BTreeSet<&'a str>,
    all_paths: &mut Vec<&'a str>,
) -> Result<PreparedStep<'a>, String> {
    if t.id.is_empty() {
        return Err("step has an empty id".into());
    }
    check_bounded_string("step id", &t.id, limits)?;
    if !seen_ids.insert(t.id.as_str()) {
        return Err(format!("duplicate step id `{}`", t.id));
    }
    if t.tool.sha256.is_empty() {
        return Err(format!(
            "step `{}` runs `{}`, which is not on PATH",
            t.id, t.tool.id
        ));
    }
    if !invocation.tools.contains(&t.tool) {
        return Err(format!(
            "step `{}` runs `{}` {} #{}, which the host did not pin for it",
            t.id, t.tool.id, t.tool.version, t.tool.sha256
        ));
    }
    let mut inputs = Vec::with_capacity(t.inputs.len());
    for input in &t.inputs {
        if !available.contains(input.as_str()) {
            return Err(format!(
                "step `{}` reads `{input}`, which the plan has not",
                t.id
            ));
        }
        inputs.push(InputRef::Artifact(format!("in:{input}")));
    }
    let mut step_reads = Vec::with_capacity(t.reads.len());
    for read in &t.reads {
        if !normal(read) || !under(read, reads) {
            return Err(format!(
                "step `{}` reads `{read}`, which the command does not declare",
                t.id
            ));
        }
        let target = root.join(read);
        if fs::symlink_metadata(&target).is_err() {
            return Err(format!(
                "step `{}` reads `{read}`, which the project has not",
                t.id
            ));
        }
        step_reads.push((read.clone(), target));
    }
    let toolchain = ToolchainStep {
        id: format!("step:{index}"),
        dependencies: vec![],
        toolchain: Toolchain {
            id: t.tool.id.clone(),
            version: t.tool.version.clone(),
            sha256: t.tool.sha256.clone(),
        },
        arguments: t.arguments.clone(),
        environment: (t.environment.iter())
            .map(|v| OptionValue {
                key: v.name.clone(),
                value: v.value.clone(),
            })
            .collect(),
        inputs,
        outputs: (t.outputs.iter())
            .map(|o| StepOutput {
                id: format!("out:{o}"),
                path: o.clone(),
                kind: ArtifactKind::Resource,
            })
            .collect(),
    };
    authorize(&toolchain)?;
    for output in &t.outputs {
        available.insert(output.as_str());
        all_paths.push(output.as_str());
    }
    Ok(PreparedStep::Tool {
        step: t,
        toolchain: Box::new(toolchain),
        reads: step_reads,
    })
}

fn validate_run_flick_step<'a>(
    r: &'a RunFlickStep,
    invocation: &Invocation,
    limits: Limits,
    seen_ids: &mut BTreeSet<&'a str>,
    available: &mut BTreeSet<&'a str>,
    all_paths: &mut Vec<&'a str>,
) -> Result<(), String> {
    if r.id.is_empty() {
        return Err("step has an empty id".into());
    }
    check_bounded_string("step id", &r.id, limits)?;
    if !seen_ids.insert(r.id.as_str()) {
        return Err(format!("duplicate step id `{}`", r.id));
    }
    check_bounded_string("run-flick entry", &r.entry, limits)?;
    if !project::is_module_name(&r.entry) {
        return Err(format!(
            "step `{}` has invalid Flick entry module `{}`",
            r.id, r.entry
        ));
    }
    if r.modules.len() > limits.files {
        return Err(format!(
            "step `{}` overlays more than {} modules",
            r.id, limits.files
        ));
    }
    let mut seen_modules: BTreeSet<&str> = BTreeSet::new();
    let mut total_bytes: usize = 0;
    for module in &r.modules {
        check_bounded_string("overlay module name", &module.name, limits)?;
        if !project::is_module_name(&module.name) {
            return Err(format!(
                "step `{}` overlays invalid module name `{}`",
                r.id, module.name
            ));
        }
        if !seen_modules.insert(module.name.as_str()) {
            return Err(format!(
                "step `{}` overlays duplicate module `{}`",
                r.id, module.name
            ));
        }
        total_bytes = total_bytes.saturating_add(module.source.len());
        if total_bytes > limits.artifact_bytes {
            return Err(format!(
                "step `{}` overlay sources exceed {} MiB",
                r.id,
                limits.artifact_bytes >> 20
            ));
        }
    }
    if r.arguments.len() > limits.files {
        return Err(format!(
            "step `{}` passes more than {} arguments",
            r.id, limits.files
        ));
    }
    for arg in &r.arguments {
        check_bounded_string("run-flick argument", arg, limits)?;
    }
    if let Some(output) = &r.output {
        if output.is_empty() {
            return Err(format!("step `{}` has an empty output path", r.id));
        }
        check_bounded_string("run-flick output", output, limits)?;
        available.insert(output.as_str());
        all_paths.push(output.as_str());
    }
    if let Some(origin) = &r.origin {
        let Some(input) = invocation.inputs.iter().find(|f| f.path == origin.path) else {
            return Err(format!(
                "step `{}` origin `{}` is not in invocation inputs",
                r.id, origin.path
            ));
        };
        let Ok(text) = str::from_utf8(&input.bytes) else {
            return Err(format!(
                "step `{}` origin `{}` is not valid UTF-8",
                r.id, origin.path
            ));
        };
        let start = origin.start as usize;
        let end = origin.end as usize;
        if origin.start > origin.end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return Err(format!(
                "step `{}` origin span {}..{} is out of bounds or splits UTF-8 in `{}`",
                r.id, origin.start, origin.end, origin.path
            ));
        }
    }
    Ok(())
}
