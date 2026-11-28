//! `go:ffi import` and `go:ffi check`: Flick bindings to Go packages from
//! `go/types`, run in a pinned `go` step using the embedded `inspect.go` helper.

mod check;
mod convert;
mod generate;
mod plan;
mod surface;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::slice;

use flick_backend::command::{Diagnostic, File, Invocation, Plan, Severity, Span, Tool};
use flick_diagnostics::diagnostics;
use flick_ffi::ffi::check::{self as ffi_check, Reviewed};
use flick_ffi::ffi::import::Status;
use flick_ir::FileId;
use serde::{Deserialize, Serialize};

pub(super) use plan::descriptors;
use plan::{go_tool, planned, surface_path, validate_moddir};
use surface::{GoPolicy, GoSurface, go_module_part};

const GENERATED: &str = "Generated.fl";
const POLICY: &str = "policy.toml";

#[derive(Serialize, Deserialize)]
pub(super) enum State {
    Import {
        package: String,
        dir: String,
        module: String,
        select: Option<Vec<String>>,
        policy: Option<String>,
    },
    Check {
        packages: Vec<String>,
        files: Vec<(String, String)>,
        policies: Vec<(String, String)>,
    },
}

pub(super) fn error(message: String) -> Diagnostic {
    Diagnostic::error("F008", message)
}

fn values<'a>(invocation: &'a Invocation, name: &str) -> &'a [String] {
    (invocation.arguments.iter())
        .find(|a| a.name == name)
        .map_or(&[], |a| a.values.as_slice())
}

pub(super) fn input<'a>(invocation: &'a Invocation, path: &str) -> Option<&'a File> {
    invocation.inputs.iter().find(|f| f.path == path)
}

pub(super) fn text(file: &File) -> String {
    String::from_utf8_lossy(&file.bytes).into_owned()
}

fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a}/{b}"),
    }
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    let go = go_tool(invocation)?;
    match invocation.name.as_str() {
        "ffi import" => import(invocation, go),
        _ => check(invocation, go),
    }
}

fn import(invocation: &Invocation, go: &Tool) -> Result<Plan, Vec<Diagnostic>> {
    let package = values(invocation, "package")
        .first()
        .cloned()
        .unwrap_or_default();
    let out = values(invocation, "out")
        .first()
        .cloned()
        .unwrap_or_default();
    let moddir = validate_moddir(values(invocation, "dir").first().map(String::as_str))?;
    let part = go_module_part(&package);
    let dir = join(&out, &part);
    let prefix = module_prefix(&invocation.sources, &out)?;
    let module = [prefix.as_str(), &part, "Generated"]
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
    Ok(planned(invocation, go, &[package], moddir, &state))
}

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

fn checked_files(invocation: &Invocation) -> Vec<(String, String)> {
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
                || (under && Path::new(&f.path).extension().is_some_and(|e| e == "fl"))
            {
                files.push((f.path.clone(), text(f)));
            }
        }
    }
    files
}

fn check(invocation: &Invocation, go: &Tool) -> Result<Plan, Vec<Diagnostic>> {
    let files = checked_files(invocation);
    let moddir = validate_moddir(values(invocation, "dir").first().map(String::as_str))?;
    let mut packages: Vec<String> = Vec::new();
    let mut policies = Vec::new();
    for (path, body) in &files {
        let generated = ffi_check::header(body).filter(|_| path.ends_with(GENERATED));
        let found: Vec<String> = match generated {
            Some(header) => {
                let policy = join(parent(path), POLICY);
                policies.extend(input(invocation, &policy).map(|f| (policy, text(f))));
                vec![header.package]
            }
            None => (ffi_check::reviewed(body, FileId::new(0)).into_iter())
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
        return resume_state(&state, &[]);
    }
    Ok(planned(invocation, go, &packages, moddir, &state))
}

pub(super) fn resume(state: &[u8], outputs: &[File]) -> Result<Plan, Vec<Diagnostic>> {
    let state: State = serde_json::from_slice(state)
        .map_err(|e| vec![error(format!("not a state of `go:ffi`: {e}"))])?;
    resume_state(&state, outputs)
}

fn read_surface(outputs: &[File], i: usize) -> Result<GoSurface, Vec<Diagnostic>> {
    let name = surface_path(i);
    let file = (outputs.iter().find(|f| f.path == name))
        .ok_or_else(|| vec![error(format!("the Go inspector wrote no `{name}`"))])?;
    GoSurface::parse(&text(file)).map_err(|e| vec![error(e)])
}

fn read_policy(path: &str, body: Option<&String>) -> Result<GoPolicy, Vec<Diagnostic>> {
    body.map_or_else(
        || Ok(GoPolicy::default()),
        |t| GoPolicy::parse(t).map_err(|e| vec![error(format!("`{path}`: {e}"))]),
    )
}

fn unknown_policies(policy: &GoPolicy, surface: &GoSurface, path: &str) -> Vec<Diagnostic> {
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

fn resume_state(state: &State, outputs: &[File]) -> Result<Plan, Vec<Diagnostic>> {
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
            let generated = generate::generate(&surface, &policy_, select.as_deref(), module);
            plan.files.push(File {
                path: join(dir, GENERATED),
                bytes: generated.text.clone().into_bytes(),
                executable: false,
            });
            plan.stdout = generate::report(&generated, &surface).into_bytes();
            plan.diagnostics = unknown_policies(&policy_, &surface, &path);
            for name in &generated.missing {
                plan.diagnostics.push(error(format!(
                    "`{}` does not export `{name}`",
                    surface.provenance()
                )));
            }
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
            for (i, (path, body)) in files.iter().enumerate() {
                let file = FileId::new(u32::try_from(i).expect("files fit a file id"));
                match ffi_check::header(body) {
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
                        let module = (body.lines())
                            .find_map(|l| l.strip_prefix("module "))
                            .and_then(|l| l.split_whitespace().next())
                            .unwrap_or_default();
                        let fresh =
                            generate::generate(surface, &policy, header.select.as_deref(), module);
                        if &fresh.text != body {
                            found.push(wire(&check::stale(file, path, surface), files));
                        }
                    }
                    _ => reviewed.extend(ffi_check::reviewed(body, file)),
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
