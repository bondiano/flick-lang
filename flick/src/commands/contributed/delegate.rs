//! The verbs of flick that a project's backend implements for its own
//! ecosystem: flick does its part on the Flick libraries, then runs the
//! backend's plan command of the same name with the verb's arguments.

use std::env;
use std::io::{self, Write as _};
use std::path::Path;
use std::process::ExitCode;
use std::slice;
use std::sync::atomic::AtomicUsize;

use flick_backend::Descriptor;
use flick_backend::command::{self, Argument, Dependency, Invocation, Style};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::SourceMap;

use super::inputs;
use super::plan::{self, InPlaceMode};
use super::registry::{Contributor, SourceKind};
use super::{descriptors, invocation, load, sources};
use crate::args::GlobalArguments;
use crate::backends::{self, Package, Selected, Tool};
use crate::child::{self, Running};
use crate::sources::FsHost;
use crate::{report, toolchain};

/// The verbs a backend may implement, which no plugin may.
pub(super) const VERBS: [&str; 9] = [
    "install",
    "add",
    "remove",
    "update",
    "deps",
    "pack",
    "doctor",
    "run-client",
    "dev-client",
];

/// What the backend's command printed and reported.
pub(crate) struct Delegated {
    pub(crate) stdout: Vec<u8>,
    pub(crate) sources: SourceMap,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) executed: bool,
}

/// Runs the plan command `verb` of the backend `project` selects with
/// `arguments`, by their names, and what the closure asks of its ecosystem;
/// `None` when the backend has no such command.
pub(crate) fn delegate(
    project: &Project,
    verb: &str,
    arguments: Vec<Argument>,
    dependencies: Vec<Dependency>,
    global: &GlobalArguments,
) -> Option<Delegated> {
    let failed = |code, message: String| Delegated {
        stdout: Vec::new(),
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
        executed: false,
    };
    let source = sources(project)
        .into_iter()
        .find(|s| matches!(s.kind, SourceKind::Backend { .. }))?;
    let descriptor = match descriptors(project, &source) {
        Ok(all) => all
            .into_iter()
            .find(|d| d.name == verb && d.style == Style::Plan)?,
        Err(why) => {
            let message = format!("the commands of `{}` are unavailable: {why}", source.id);
            return Some(failed(codes::P013, message));
        }
    };
    let declared = |name: &str| {
        (descriptor.positionals.iter())
            .chain(&descriptor.options)
            .any(|p| p.name == name)
    };
    if let Some(argument) = arguments.iter().find(|a| !declared(&a.name)) {
        let message = format!(
            "the backend `{}` takes no `{}` for `flick {verb}`",
            source.id, argument.name
        );
        return Some(failed(codes::P063, message));
    }
    let tools = backends::granted_tools(project, &source.id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let mut invocation = match invocation(project, &source.id, &descriptor, arguments, &tools, &cwd)
    {
        Ok(invocation) => invocation,
        Err(why) => return Some(failed(codes::P055, format!("`{verb}` cannot run: {why}"))),
    };
    invocation.dependencies = dependencies;
    let named = invocation
        .arguments
        .iter()
        .any(|a| matches!(a.name.as_str(), "packages" | "why") && !a.values.is_empty());
    if matches!(verb, "install" | "update" | "deps")
        && !descriptor.inputs.is_empty()
        && invocation.inputs.is_empty()
        && invocation.dependencies.is_empty()
        && !named
    {
        return Some(Delegated {
            stdout: Vec::new(),
            sources: SourceMap::default(),
            diagnostics: Vec::new(),
            executed: false,
        });
    }
    let contributor = match load(project, &source) {
        Ok(contributor) => contributor,
        Err(why) => {
            return Some(failed(
                codes::P013,
                format!("cannot load `{}`: {why}", source.id),
            ));
        }
    };
    let (stdout, sources, diagnostics) = plan::outcome(
        project,
        &*contributor,
        &descriptor,
        &invocation,
        &tools,
        global,
        InPlaceMode::PackageManager,
    );
    Some(Delegated {
        stdout,
        sources,
        diagnostics,
        executed: true,
    })
}

fn client_arguments(entry: Option<&str>, extra_args: &[String]) -> Vec<Argument> {
    let mut arguments = Vec::new();
    if let Some(name) = entry {
        arguments.push(Argument {
            name: "entry".into(),
            values: vec![name.to_owned()],
        });
    }
    if !extra_args.is_empty() {
        arguments.push(Argument {
            name: "args".into(),
            values: extra_args.to_vec(),
        });
    }
    arguments
}

fn no_client_runner(id: &str, verb: &str) -> Diagnostic {
    let action = verb.strip_suffix("-client").unwrap_or(verb);
    Diagnostic::new(
        codes::P067,
        Severity::Error,
        format!("the backend `{id}` has no client entry runner for `flick {action}`"),
    )
}

struct ClientPlan {
    tools: Vec<Tool>,
    plan: command::Plan,
    in_place: Vec<command::ToolStep>,
}

fn prepare_client_plan(
    project: &Project,
    verb: &str,
    target_override: Option<&str>,
    entry: Option<&str>,
    extra_args: &[String],
    max_steps: Option<usize>,
) -> Result<ClientPlan, Vec<Diagnostic>> {
    let (Selected::Component(package), choice, _) =
        Selected::load_target(&FsHost::new(), Some(project), target_override)
            .map_err(|d| vec![d])?
    else {
        return Err(vec![no_client_runner(
            project.config.default_target(),
            verb,
        )]);
    };
    let all = package
        .describe()
        .map_err(|why| vec![Diagnostic::new(codes::P013, Severity::Error, why)])?;
    let Some(descriptor) = all
        .into_iter()
        .find(|d| d.name == verb && d.style == Style::Plan)
    else {
        return Err(vec![no_client_runner(choice.id(), verb)]);
    };
    let id = package.pin().identity.id;
    let tools = backends::granted_tools(project, &id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let arguments = client_arguments(entry, extra_args);
    let invocation =
        invocation(project, &id, &descriptor, arguments, &tools, &cwd).map_err(|why| {
            vec![Diagnostic::new(
                codes::P055,
                Severity::Error,
                format!("`{verb}` cannot run: {why}"),
            )]
        })?;
    let plan = package.run(&invocation).map_err(|diagnostics| {
        if diagnostics.is_empty() {
            vec![Diagnostic::new(
                codes::P017,
                Severity::Error,
                format!("`{verb}` refused to start"),
            )]
        } else {
            diagnostics
                .iter()
                .map(plan::standalone_diagnostic)
                .collect()
        }
    })?;
    let refused = |why: String| {
        vec![Diagnostic::new(
            codes::P055,
            Severity::Error,
            format!("the plan of `{verb}` is refused: {why}"),
        )]
    };
    let writes = inputs::expand(&descriptor.writes, &invocation).map_err(refused)?;
    let reads = inputs::expand(&descriptor.reads, &invocation).map_err(refused)?;
    let validated = plan::validate_plan(&plan, &invocation, &writes, &reads, &project.root, true)
        .map_err(refused)?;
    let too_many = max_steps.is_some_and(|max| validated.in_place.len() > max);
    if !plan.files.is_empty()
        || plan.resume.is_some()
        || !plan.needs.is_empty()
        || !validated.regular.is_empty()
        || too_many
    {
        return Err(refused(format!(
            "`{verb}` may only run in-place processes with no file writes"
        )));
    }
    let in_place = validated.in_place.into_iter().cloned().collect();
    Ok(ClientPlan {
        tools,
        plan,
        in_place,
    })
}

/// Runs a backend's `run-client` verb directly for `flick run <client>`,
/// forwarding signals and preserving the child's exit code (including 130/143
/// on SIGINT/SIGTERM) without synthesizing a `FLICK-P017` failure diagnostic.
pub(crate) fn run_client(
    project: &Project,
    target_override: Option<&str>,
    entry: Option<&str>,
    extra_args: &[String],
    signals: &AtomicUsize,
    global: &GlobalArguments,
) -> ExitCode {
    let prepared = match prepare_client_plan(
        project,
        "run-client",
        target_override,
        entry,
        extra_args,
        None,
    ) {
        Ok(prepared) => prepared,
        Err(diagnostics) => {
            report::render(global, &SourceMap::default(), &diagnostics);
            return report::exit_code(&diagnostics);
        }
    };
    if !prepared.plan.diagnostics.is_empty() {
        let mapped: Vec<Diagnostic> = prepared
            .plan
            .diagnostics
            .iter()
            .map(plan::standalone_diagnostic)
            .collect();
        report::render(global, &SourceMap::default(), &mapped);
        if mapped.iter().any(|d| d.severity == Severity::Error) {
            return report::exit_code(&mapped);
        }
    }
    if !prepared.plan.stdout.is_empty() {
        let _ = io::stdout().write_all(&prepared.plan.stdout);
    }
    for step in &prepared.in_place {
        let running = match plan::spawn_in_place(
            &project.root,
            step,
            &prepared.tools,
            "client-output",
            global,
        ) {
            Ok(running) => running,
            Err(diag) => {
                let code = report::exit_code(slice::from_ref(&diag));
                report::render(global, &SourceMap::default(), &[diag]);
                return code;
            }
        };
        match running.wait(signals) {
            Ok(status) => {
                let code = child::code(status);
                if code != 0 {
                    return ExitCode::from(code);
                }
            }
            Err(_) => return ExitCode::from(1),
        }
    }
    ExitCode::SUCCESS
}

/// Spawns a backend's `dev-client` background server for `flick dev`.
/// Returns `Ok(None)` when the backend emits only a note (e.g. missing local
/// dev tool) so non-client entries can keep running.
pub(crate) fn spawn_dev_client(
    project: &Project,
    target_override: Option<&str>,
    entry: Option<&str>,
    extra_args: &[String],
    global: &GlobalArguments,
) -> Result<Option<Running>, Diagnostic> {
    let prepared = prepare_client_plan(
        project,
        "dev-client",
        target_override,
        entry,
        extra_args,
        Some(1),
    )
    .map_err(|mut ds| {
        ds.drain(..).next().unwrap_or_else(|| {
            Diagnostic::new(
                codes::P017,
                Severity::Error,
                "client dev server refused to start",
            )
        })
    })?;
    for diag in &prepared.plan.diagnostics {
        match diag.severity {
            command::Severity::Error => return Err(plan::standalone_diagnostic(diag)),
            command::Severity::Warning | command::Severity::Note => {
                report::log(global, &diag.message);
            }
        }
    }
    let Some(step) = prepared.in_place.first() else {
        return Ok(None);
    };
    plan::spawn_in_place(
        &project.root,
        step,
        &prepared.tools,
        "client-dev-stderr",
        global,
    )
    .map(Some)
}

pub(crate) type DoctorLine = (Option<bool>, String, String);

fn doctor_tools(backend_desc: &Descriptor) -> Vec<command::Tool> {
    let mut tool_ids: Vec<&str> = Vec::new();
    for id in backend_desc
        .host_tools
        .iter()
        .chain(&backend_desc.launch_runtimes)
    {
        if !tool_ids.contains(&id.as_str()) {
            tool_ids.push(id.as_str());
        }
    }
    tool_ids
        .into_iter()
        .map(|id| match Tool::find(id, None) {
            Ok(found) => plan::tool(&found.pin),
            Err(_) => command::Tool {
                id: id.to_owned(),
                version: String::new(),
                sha256: String::new(),
            },
        })
        .collect()
}

/// Runs a loaded backend's pure read-only `doctor` plan, returning
/// `(ok, what, detail)` lines; `Ok(None)` when the backend declares no `doctor`.
pub(crate) fn doctor(
    package: &Package,
    project: Option<&Project>,
    cwd: &Path,
) -> Result<Option<Vec<DoctorLine>>, Diagnostic> {
    let all = package
        .describe()
        .map_err(|why| Diagnostic::new(codes::P013, Severity::Error, why))?;
    let Some(descriptor) = all
        .into_iter()
        .find(|d| d.name == "doctor" && d.style == Style::Plan)
    else {
        return Ok(None);
    };
    let backend_desc = package.descriptor();
    let tools = doctor_tools(&backend_desc);
    let mut arguments = vec![
        Argument {
            name: "in-project".into(),
            values: vec![if project.is_some() { "true" } else { "false" }.into()],
        },
        Argument {
            name: "compiler".into(),
            values: vec![toolchain::COMPILER.into()],
        },
    ];
    if let Some(p) = project
        && let Some(granted) = p.config.tools.get(&backend_desc.identity.id)
        && !granted.is_empty()
    {
        arguments.push(Argument {
            name: "granted-tools".into(),
            values: granted.clone(),
        });
    }
    let root = project.map_or(cwd, |p| p.root.as_path());
    let sources = project
        .and_then(|p| {
            p.source_root()
                .strip_prefix(&p.root)
                .ok()
                .map(inputs::slashed)
        })
        .unwrap_or_default();
    let mut inv = Invocation {
        name: descriptor.name.clone(),
        arguments,
        sources,
        inputs: Vec::new(),
        tools,
        dependencies: Vec::new(),
    };
    if let Some(p) = project {
        let paths = inputs::expand(&descriptor.inputs, &inv)
            .map_err(|why| Diagnostic::new(codes::P055, Severity::Error, why))?;
        let skip = backends::package_dirs(p);
        inv.inputs = inputs::read(root, &paths, &skip)
            .map_err(|why| Diagnostic::new(codes::P055, Severity::Error, why))?;
    }
    let plan = match package.run(&inv) {
        Ok(plan) => plan,
        Err(diagnostics) => {
            let first = diagnostics.first().map_or_else(
                || Diagnostic::new(codes::P015, Severity::Error, "backend `doctor` failed"),
                plan::standalone_diagnostic,
            );
            return Err(first);
        }
    };
    if !plan.files.is_empty()
        || !plan.steps.is_empty()
        || plan.resume.is_some()
        || !plan.needs.is_empty()
    {
        return Err(Diagnostic::new(
            codes::P055,
            Severity::Error,
            "the backend's `doctor` command must be read-only".to_owned(),
        ));
    }
    let lines = plan
        .diagnostics
        .into_iter()
        .filter_map(|d| {
            let ok = match d.code.as_str() {
                "doctor-ok" => Some(true),
                "doctor-fail" => Some(false),
                "doctor-note" => None,
                _ => return None,
            };
            let (what, detail) = d.message.split_once('\n')?;
            Some((ok, what.to_owned(), detail.to_owned()))
        })
        .collect();
    Ok(Some(lines))
}
