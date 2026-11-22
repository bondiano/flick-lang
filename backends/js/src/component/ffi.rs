//! `ffi import` and `ffi check`: Flick bindings to npm packages from their
//! `.d.ts`, which the importer `@flick-lang/dts-importer` the project
//! installs reads in a step of the pinned `node`. The bindings and their
//! check are computed here, from the step's output.

use std::path::Path;
use std::slice;

use flick_backend::command::{
    Descriptor, Diagnostic, File, Invocation, Parameter, Plan, Severity, Span, Step, Style, Tool,
    ToolStep, ValueKind,
};
use flick_diagnostics::diagnostics;
use flick_ffi::ffi::check::{self, Reviewed};
use flick_ffi::ffi::import::{self, Status};
use flick_ffi::ffi::{Policy, Surface, module_part};
use flick_ir::FileId;
use serde::{Deserialize, Serialize};

const HELPER: &str = "node_modules/@flick-lang/dts-importer/index.mjs";
const GENERATED: &str = "Generated.fl";
const POLICY: &str = "policy.toml";

fn parameter(name: &str, kind: ValueKind, multiple: bool, required: bool, help: &str) -> Parameter {
    Parameter {
        name: name.into(),
        kind,
        multiple,
        required,
        help: help.into(),
    }
}

pub(super) fn descriptors() -> Vec<Descriptor> {
    let import = Descriptor {
        name: "ffi import".into(),
        summary: "Writes `<out>/<Pkg>/Generated.fl`, bindings to an installed npm package".into(),
        positionals: vec![parameter(
            "package",
            ValueKind::Text,
            false,
            true,
            "The npm package, installed in the project",
        )],
        options: vec![
            parameter(
                "out",
                ValueKind::Path,
                false,
                true,
                "Directory that gets `<Pkg>/Generated.fl`",
            ),
            parameter(
                "select",
                ValueKind::Text,
                false,
                false,
                "Import only these exports, comma-separated",
            ),
        ],
        inputs: vec!["$out".into(), HELPER.into()],
        reads: vec!["node_modules".into()],
        writes: vec!["$out".into()],
        style: Style::Plan,
    };
    let check = Descriptor {
        name: "ffi check".into(),
        summary: "Checks generated and reviewed bindings against the installed packages".into(),
        positionals: vec![parameter(
            "paths",
            ValueKind::Path,
            true,
            false,
            "Files or directories to check; by default the project's source root",
        )],
        options: vec![],
        inputs: vec!["$paths".into(), "$sources".into(), HELPER.into()],
        reads: vec!["node_modules".into()],
        writes: vec![],
        style: Style::Plan,
    };
    vec![import, check]
}

/// What a plan of `ffi` hands its resumption: the importer's outputs are
/// the surfaces of `packages`, in order.
#[derive(Serialize, Deserialize)]
enum State {
    Import {
        package: String,
        dir: String,
        module: String,
        select: Option<Vec<String>>,
        policy: Option<String>,
    },
    Check {
        packages: Vec<String>,
        /// The checked files: path and text.
        files: Vec<(String, String)>,
        /// The policy files beside generated bindings: path and text.
        policies: Vec<(String, String)>,
    },
}

fn error(message: String) -> Diagnostic {
    Diagnostic::error("F008", message)
}

fn values<'a>(invocation: &'a Invocation, name: &str) -> &'a [String] {
    (invocation.arguments.iter())
        .find(|a| a.name == name)
        .map_or(&[], |a| a.values.as_slice())
}

fn input<'a>(invocation: &'a Invocation, path: &str) -> Option<&'a File> {
    invocation.inputs.iter().find(|f| f.path == path)
}

fn text(file: &File) -> String {
    String::from_utf8_lossy(&file.bytes).into_owned()
}

/// `a/b` of `a` and `b`, either of which may be the root, empty.
fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a}/{b}"),
    }
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if input(invocation, HELPER).is_none() {
        let mut d = error(format!("no `.d.ts` reader at `{HELPER}`"));
        d.help = Some("install it: `pnpm add -D @flick-lang/dts-importer`".into());
        return Err(vec![d]);
    }
    let node = node(invocation)?;
    match invocation.name.as_str() {
        "ffi import" => import(invocation, node),
        _ => check(invocation, node),
    }
}

/// The pinned `node` the project allows this backend.
fn node(invocation: &Invocation) -> Result<&Tool, Vec<Diagnostic>> {
    let found = invocation.tools.iter().find(|t| t.id == "node");
    if found.is_some_and(|t| t.sha256.is_empty()) {
        return Err(vec![Diagnostic::error(
            "P015",
            format!("`{}` runs `node`, which is not on PATH", invocation.name),
        )]);
    }
    found.ok_or_else(|| {
        let mut d = Diagnostic::error(
            "P051",
            format!(
                "`{}` runs `node`, which `[tools]` does not allow `flick/js`",
                invocation.name
            ),
        );
        d.help = Some("allow it in `flick.toml`: `[tools]` `\"flick/js\" = [\"node\"]`".into());
        vec![d]
    })
}

/// A step that writes the surface of each of `packages` to `surface-<i>.json`.
fn steps(node: &Tool, packages: &[String]) -> Vec<Step> {
    (packages.iter().enumerate())
        .map(|(i, package)| {
            Step::Tool(ToolStep {
                id: format!("import {package}"),
                tool: node.clone(),
                arguments: [HELPER, "import", "--root", ".", package, &surface(i)]
                    .map(str::to_owned)
                    .to_vec(),
                environment: vec![],
                inputs: vec![],
                reads: vec!["node_modules".into()],
                outputs: vec![surface(i)],
                in_place: false,
            })
        })
        .collect()
}

fn surface(i: usize) -> String {
    format!("surface-{i}.json")
}

fn planned(steps: Vec<Step>, state: &State) -> Plan {
    let mut plan = Plan::empty();
    plan.steps = steps;
    plan.resume = Some(serde_json::to_vec(state).expect("the state serializes"));
    plan
}

fn import(invocation: &Invocation, node: &Tool) -> Result<Plan, Vec<Diagnostic>> {
    let package = values(invocation, "package")
        .first()
        .cloned()
        .unwrap_or_default();
    let out = values(invocation, "out")
        .first()
        .cloned()
        .unwrap_or_default();
    let dir = join(&out, &module_part(&package));
    let prefix = module_prefix(&invocation.sources, &out)?;
    let module = [prefix.as_str(), &module_part(&package), "Generated"]
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(".");
    let select = (values(invocation, "select").first()).map(|s| {
        (s.split(',').map(str::trim))
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .collect()
    });
    let policy = input(invocation, &join(&dir, POLICY)).map(text);
    let state = State::Import {
        package: package.clone(),
        dir,
        module,
        select,
        policy,
    };
    Ok(planned(steps(node, &[package]), &state))
}

/// The module of `dir`'s files: `src/Ffi` of a project with source root
/// `src` gives `Ffi`; empty outside the source root.
fn module_prefix(sources: &str, dir: &str) -> Result<String, Vec<Diagnostic>> {
    let relative = if sources.is_empty() {
        Some(dir)
    } else if dir == sources {
        Some("")
    } else {
        dir.strip_prefix(sources).and_then(|r| r.strip_prefix('/'))
    };
    let Some(relative) = relative else {
        return Ok(String::new());
    };
    let name = relative.replace('/', ".");
    let capitalized = |part: &str| {
        let mut chars = part.chars();
        chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if !name.is_empty() && !name.split('.').all(capitalized) {
        return Err(vec![error(format!(
            "`{dir}` is no module directory: each part must be a capitalized name"
        ))]);
    }
    Ok(name)
}

/// The files `ffi check` checks: each path named, a directory standing for
/// the modules under it, or the source root's modules.
fn checked(invocation: &Invocation) -> Vec<(String, String)> {
    let paths = values(invocation, "paths");
    let roots = if paths.is_empty() {
        slice::from_ref(&invocation.sources)
    } else {
        paths
    };
    let mut files = Vec::new();
    for root in roots {
        for f in &invocation.inputs {
            let under = root.is_empty()
                || f.path
                    .strip_prefix(root.as_str())
                    .is_some_and(|r| r.starts_with('/'));
            if f.path == *root
                || (under
                    && Path::new(&f.path).extension().is_some_and(|e| e == "fl")
                    && !f.path.starts_with("node_modules/"))
            {
                files.push((f.path.clone(), text(f)));
            }
        }
    }
    files
}

fn check(invocation: &Invocation, node: &Tool) -> Result<Plan, Vec<Diagnostic>> {
    let files = checked(invocation);
    let mut packages: Vec<String> = Vec::new();
    let mut policies = Vec::new();
    for (path, text) in &files {
        let generated = check::header(text).filter(|_| path.ends_with(GENERATED));
        let found: Vec<String> = match generated {
            Some(header) => {
                let policy = join(parent(path), POLICY);
                policies.extend(input(invocation, &policy).map(|f| (policy, self::text(f))));
                vec![header.package]
            }
            None => (check::reviewed(text, FileId::new(0)).into_iter())
                .map(|r| r.package)
                .collect(),
        };
        for package in found {
            if !packages.contains(&package) {
                packages.push(package);
            }
        }
    }
    let state = State::Check {
        packages: packages.clone(),
        files,
        policies,
    };
    if packages.is_empty() {
        return resume_check(&state, &[]);
    }
    Ok(planned(steps(node, &packages), &state))
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub(super) fn resume(state: &[u8], outputs: &[File]) -> Result<Plan, Vec<Diagnostic>> {
    let state: State = serde_json::from_slice(state)
        .map_err(|e| vec![error(format!("not a state of `ffi`: {e}"))])?;
    resume_check(&state, outputs)
}

/// The surface of the `i`th package from the importer's output.
fn read_surface(outputs: &[File], i: usize) -> Result<Surface, Vec<Diagnostic>> {
    let name = surface(i);
    let file = (outputs.iter().find(|f| f.path == name))
        .ok_or_else(|| vec![error(format!("the importer wrote no `{name}`"))])?;
    Surface::parse(&text(file)).map_err(|e| vec![error(e)])
}

fn read_policy(path: &str, text: Option<&String>) -> Result<Policy, Vec<Diagnostic>> {
    text.map_or_else(
        || Ok(Policy::default()),
        |t| Policy::parse(t).map_err(|e| vec![error(format!("`{path}`: {e}"))]),
    )
}

/// Names in `policy` the package does not export.
fn unknown_policies(policy: &Policy, surface: &Surface, path: &str) -> Vec<Diagnostic> {
    (policy.exports.keys())
        .filter(|name| surface.find(name).is_none())
        .map(|name| {
            error(format!(
                "`{path}` sets a policy for `{name}`, which `{}` does not export",
                surface.provenance()
            ))
        })
        .collect()
}

fn resume_check(state: &State, outputs: &[File]) -> Result<Plan, Vec<Diagnostic>> {
    let mut plan = Plan::empty();
    match state {
        State::Import {
            dir,
            module,
            select,
            policy,
            ..
        } => {
            let surface = read_surface(outputs, 0)?;
            let path = join(dir, POLICY);
            let policy_ = read_policy(&path, policy.as_ref())?;
            let generated = import::generate(&surface, &policy_, select.as_deref(), module);
            plan.files.push(File {
                path: join(dir, GENERATED),
                bytes: generated.text.clone().into_bytes(),
                executable: false,
            });
            plan.stdout = generated.report(&surface).into_bytes();
            plan.diagnostics = unknown_policies(&policy_, &surface, &path);
            for name in &generated.missing {
                (plan.diagnostics).push(error(format!(
                    "`{}` does not export `{name}`",
                    surface.provenance()
                )));
            }
            // asking for a blocked export by name is an error; the others stay usable
            if select.is_some() {
                let blocked = generated
                    .entries
                    .iter()
                    .filter(|e| e.status != Status::Supported);
                for entry in blocked {
                    plan.diagnostics.push(Diagnostic::error(
                        "F005",
                        format!(
                            "`{}` is blocked: {}: {}",
                            entry.export,
                            entry.status.label(),
                            entry.detail
                        ),
                    ));
                }
            }
        }
        State::Check {
            packages,
            files,
            policies,
        } => {
            let surfaces = (0..packages.len())
                .map(|i| read_surface(outputs, i))
                .collect::<Result<Vec<_>, _>>()?;
            let of =
                |package: &str| (packages.iter().position(|p| p == package)).map(|i| &surfaces[i]);
            let mut found = Vec::new();
            let mut reviewed: Vec<Reviewed> = Vec::new();
            for (i, (path, text)) in files.iter().enumerate() {
                let file = FileId::new(u32::try_from(i).expect("files fit a file id"));
                match check::header(text) {
                    Some(header) if path.ends_with(GENERATED) => {
                        let Some(surface) = of(&header.package) else {
                            continue;
                        };
                        let policy_path = join(parent(path), POLICY);
                        let policy_text = (policies.iter())
                            .find(|(p, _)| *p == policy_path)
                            .map(|(_, t)| t);
                        let policy = read_policy(&policy_path, policy_text)?;
                        found.extend(unknown_policies(&policy, surface, &policy_path));
                        let module = (text.lines())
                            .find_map(|l| l.strip_prefix("module "))
                            .and_then(|l| l.split_whitespace().next())
                            .unwrap_or_default();
                        let fresh =
                            import::generate(surface, &policy, header.select.as_deref(), module);
                        if &fresh.text != text {
                            found.push(wire(&check::stale(file, path, surface), files));
                        }
                    }
                    _ => reviewed.extend(check::reviewed(text, file)),
                }
            }
            for surface in &surfaces {
                let checked = check::check_reviewed(&reviewed, surface);
                found.extend(checked.iter().map(|d| wire(d, files)));
            }
            plan.diagnostics = found;
        }
    }
    Ok(plan)
}

/// The command form of a diagnostic of the checker, its span into the
/// checked file it points at.
fn wire(d: &diagnostics::Diagnostic, files: &[(String, String)]) -> Diagnostic {
    let code = d.code.to_string();
    let mut out = Diagnostic::error(
        code.strip_prefix("FLICK-").unwrap_or(&code),
        d.message.clone(),
    );
    out.severity = match d.severity {
        diagnostics::Severity::Error => Severity::Error,
        diagnostics::Severity::Warning => Severity::Warning,
        diagnostics::Severity::Note => Severity::Note,
    };
    out.span = (d.span).and_then(|s| {
        let (path, _) = files.get(usize::try_from(s.file.index()).ok()?)?;
        Some(Span {
            path: path.clone(),
            start: s.start,
            end: s.end,
        })
    });
    out.help = d.fixes.first().map(|f| f.message.clone());
    out
}
