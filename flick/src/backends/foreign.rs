//! What the backend of a project reports of its ecosystem's packages, for
//! the compiler, which finds the Flick modules they ship through it. The
//! backend is taken without the network: a project's own pin, or an
//! official one a checkout or the store holds; without one there is none.

use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_backend::command::Dependency;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{Backend, BackendId, BackendPin, Limits, OptionValue};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::target::Wanted;
use flick_core::project::{Foreign, ForeignPackage, Project, store};
use flick_core::source::{FileId, Span};

use super::options::{self, TargetChoice};
use super::{official, packages, runtimes};
use crate::{cache, component, saved, toolchain};

pub(crate) fn foreign(project: &Project) -> Foreign {
    let Some((pin, bytes)) = selected_bytes(project, None) else {
        return Foreign::default();
    };
    // the backend's patterns, kept by its digest: a project with none of
    // those files needs no backend loaded
    if let Some(patterns) = cached(&pin.sha256)
        && !packages::any_match(&project.root, &patterns)
    {
        return Foreign::default();
    }
    let Some(backend) = load(&pin, &bytes) else {
        return Foreign::default();
    };
    keep(&pin.sha256, &backend.package_files());
    let options = [
        OptionValue {
            key: "root".into(),
            value: real_path(&project.root),
        },
        OptionValue {
            key: "skip-runtime".into(),
            value: "true".into(),
        },
    ];
    match packages::report(&*backend, &project.root, &[], &options) {
        Ok(report) => Foreign {
            name: report.name,
            version: report.version,
            declared: report.declared,
            packages: (report.sources.into_iter())
                .map(|p| ForeignPackage {
                    name: p.package,
                    version: Some(p.version).filter(|v| !v.is_empty()),
                    source_dir: Some(p.root)
                        .filter(|r| packages::is_safe_relative(r))
                        .map(PathBuf::from),
                })
                .collect(),
            diagnostics: (report.diagnostics.iter())
                .map(|d| {
                    let code =
                        codes::lookup(&format!("FLICK-{}", d.code)).map_or(codes::P013, |i| i.code);
                    Diagnostic::new(code, Severity::Error, d.message.clone())
                })
                .collect(),
        },
        Err(why) => Foreign {
            diagnostics: vec![Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("the backend's packages: {why}"),
            )],
            ..Foreign::default()
        },
    }
}

/// The files a build of `project` depends on through its backend's
/// packages: those the backend reports in `inputs` (or matches via its
/// patterns), and the modules of the packages that ship Flick sources.
pub(crate) fn package_inputs(project: &Project, effect_override: Option<bool>) -> Vec<PathBuf> {
    let Some((pin, bytes)) = selected_bytes(project, None) else {
        return Vec::new();
    };
    if let Some(patterns) = cached(&pin.sha256)
        && patterns.is_empty()
    {
        return Vec::new();
    }
    let Some(backend) = load(&pin, &bytes) else {
        return Vec::new();
    };
    let patterns = backend.package_files();
    keep(&pin.sha256, &patterns);
    if patterns.is_empty() {
        return Vec::new();
    }
    let effect = effect_override.unwrap_or_else(|| !project.config.js_is_rt());
    let options = [
        OptionValue {
            key: "root".into(),
            value: real_path(&project.root),
        },
        OptionValue {
            key: "skip-runtime".into(),
            value: "true".into(),
        },
        OptionValue {
            key: "runtime".into(),
            value: if effect { "effect" } else { "rt" }.into(),
        },
    ];
    let Ok(report) = packages::report(&*backend, &project.root, &[], &options) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = if report.inputs.is_empty() {
        packages::named(&project.root, &patterns)
            .into_iter()
            .map(|p| project.root.join(p))
            .collect()
    } else {
        report
            .inputs
            .into_iter()
            .map(|p| project.root.join(p))
            .collect()
    };
    for package in report.sources {
        if packages::is_safe_relative(&package.root) {
            let mut sources = Vec::new();
            cache::files_under(&project.root.join(&package.root), &mut sources);
            files.extend(
                (sources.into_iter())
                    .filter(|p| p.extension().is_some_and(|e| e == "fl" || e == "go")),
            );
        }
    }
    files
}

/// Top-level package directories the backend of `project` reads (e.g. `node_modules`).
pub(crate) fn package_dirs(project: &Project) -> Vec<String> {
    let Some((pin, bytes)) = selected_bytes(project, None) else {
        return Vec::new();
    };
    let patterns = if let Some(patterns) = cached(&pin.sha256) {
        patterns
    } else {
        let Some(backend) = load(&pin, &bytes) else {
            return Vec::new();
        };
        let patterns = backend.package_files();
        keep(&pin.sha256, &patterns);
        patterns
    };
    packages::top_dirs(&patterns)
}

/// The name of the `[target]` of the backend `project` selects: the last
/// part of its id.
pub(crate) fn target_name(project: &Project) -> Option<String> {
    selected_choice(project, None).map(|c| c.short().to_owned())
}

/// What the closure asks of the ecosystem of the backend `project` selects,
/// as that backend takes it.
pub(crate) fn asked(project: &Project, wanted: &[Wanted]) -> Vec<Dependency> {
    let target = target_name(project);
    asked_for(target.as_deref(), wanted)
}

fn asked_for(target: Option<&str>, wanted: &[Wanted]) -> Vec<Dependency> {
    (wanted.iter())
        .filter(|w| Some(w.target.as_str()) == target)
        .map(|w| Dependency {
            name: w.name.clone(),
            version: w.version.clone(),
            by: w.by.clone(),
        })
        .collect()
}

/// What the backend of `project` finds wrong with what `wanted` asks of its
/// ecosystem and, with `installed`, what of it is not installed.
pub(crate) fn verify(project: &Project, wanted: &[Wanted], installed: bool) -> Vec<Diagnostic> {
    verify_for_target(project, None, wanted, installed)
}

/// Like [`verify`], for an explicit `--target` override when present.
pub(crate) fn verify_for_target(
    project: &Project,
    target_override: Option<&str>,
    wanted: &[Wanted],
    installed: bool,
) -> Vec<Diagnostic> {
    let target = selected_choice(project, target_override).map(|c| c.short().to_owned());
    let mine: Vec<&Wanted> = (wanted.iter())
        .filter(|w| Some(&w.target) == target.as_ref())
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let unavailable = |why: String| {
        vec![Diagnostic::new(
            codes::P013,
            Severity::Error,
            format!("cannot check the packages `[target]` asks for: {why}"),
        )]
    };
    let Some(backend) =
        selected_bytes(project, target_override).and_then(|(pin, bytes)| load(&pin, &bytes))
    else {
        return unavailable("the project's backend is not available offline".into());
    };
    let mut options = vec![
        OptionValue {
            key: "root".into(),
            value: real_path(&project.root),
        },
        OptionValue {
            key: "skip-runtime".into(),
            value: "true".into(),
        },
    ];
    if installed {
        options.push(OptionValue {
            key: "verify".into(),
            value: "installed".into(),
        });
    }
    let dependencies = asked_for(target.as_deref(), wanted);
    match packages::report(&*backend, &project.root, &dependencies, &options) {
        Ok(report) => (report.diagnostics.iter())
            .map(|d| {
                let code =
                    codes::lookup(&format!("FLICK-{}", d.code)).map_or(codes::P013, |i| i.code);
                let mut mapped = Diagnostic::new(code, Severity::Error, d.message.clone());
                // a diagnostic about one of them names it by its index
                let at = d.span.and_then(|s| mine.get(s.file as usize));
                if let Some(w) = at.filter(|w| w.span != Span::new(FileId::default(), 0, 0)) {
                    mapped = mapped.with_span(w.span);
                }
                for note in &d.notes {
                    mapped = mapped.with_fix(|| (note.clone(), Vec::new()));
                }
                mapped
            })
            .collect(),
        Err(why) => unavailable(why),
    }
}

/// Checks the runtime ABI and adapter contract through `backend`'s `packages` export.
pub(crate) fn check_runtime_with(
    backend: &WasmBackend,
    root: &Path,
    rt: bool,
) -> Option<Diagnostic> {
    let options = [
        OptionValue {
            key: "check-runtime".into(),
            value: if rt { "rt" } else { "effect" }.into(),
        },
        OptionValue {
            key: "compiler".into(),
            value: toolchain::COMPILER.into(),
        },
        OptionValue {
            key: "root".into(),
            value: real_path(root),
        },
    ];
    let report = packages::report(backend, root, &[], &options).ok()?;
    report
        .diagnostics
        .into_iter()
        .find(|d| d.severity == flick_backend::Severity::Error)
        .map(|d| {
            let code = codes::lookup(&format!("FLICK-{}", d.code)).map_or(codes::P015, |i| i.code);
            Diagnostic::new(code, Severity::Error, d.message)
        })
}

/// Checks the runtime contract of `project` using its selected backend.
pub(crate) fn check_runtime_project(project: &Project, rt: bool) -> Option<Diagnostic> {
    let (pin, bytes) = selected_bytes(project, None)?;
    let backend = load(&pin, &bytes)?;
    check_runtime_with(&backend, &project.root, rt)
}

/// Backend-owned lines for `flick doctor`: `(ok, what, detail)`.
pub(crate) fn doctor(
    project: Option<&Project>,
    node: Option<&str>,
    pnpm: Option<&str>,
) -> Option<Vec<(Option<bool>, String, String)>> {
    let cwd = env::current_dir().unwrap_or_default();
    let (root, loaded) = match project {
        Some(project) => (
            project.root.as_path(),
            selected_bytes(project, None).and_then(|(pin, bytes)| load(&pin, &bytes)),
        ),
        None => (
            cwd.as_path(),
            official::local_bytes(official::JS, None).and_then(|(pin, bytes)| load(&pin, &bytes)),
        ),
    };
    let backend = loaded?;
    let options = [
        OptionValue {
            key: "inspect".into(),
            value: "doctor".into(),
        },
        OptionValue {
            key: "compiler".into(),
            value: toolchain::COMPILER.into(),
        },
        OptionValue {
            key: "has-project".into(),
            value: if project.is_some() { "true" } else { "false" }.into(),
        },
        OptionValue {
            key: "node-version".into(),
            value: node.unwrap_or_default().into(),
        },
        OptionValue {
            key: "pnpm-version".into(),
            value: pnpm.unwrap_or_default().into(),
        },
        OptionValue {
            key: "root".into(),
            value: real_path(root),
        },
    ];
    let report = packages::report(&*backend, root, &[], &options).ok()?;
    let lines = report
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
    Some(lines)
}

fn real_path(path: &Path) -> String {
    packages::canonical_slashed(path)
}

fn selected_choice(project: &Project, target_override: Option<&str>) -> Option<TargetChoice> {
    options::resolve_target(target_override, Some(project))
        .ok()
        .map(|(choice, _)| choice)
}

/// The component `project` selects and its pin, without the network.
fn selected_bytes(
    project: &Project,
    target_override: Option<&str>,
) -> Option<(BackendPin, Vec<u8>)> {
    match selected_choice(project, target_override)? {
        TargetChoice::Pinned { package, .. } => {
            let pin = BackendPin {
                identity: BackendId {
                    id: package.id.clone(),
                    version: package.version.clone(),
                },
                sha256: package.sha256.clone(),
            };
            let limit = Limits::default().artifact_bytes;
            let mut bytes = Vec::new();
            File::open(project.root.join(&package.component))
                .and_then(|f| {
                    f.take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
                        .read_to_end(&mut bytes)
                })
                .ok()?;
            (bytes.len() <= limit).then_some((pin, bytes))
        }
        TargetChoice::Official { id, .. } => official::local_bytes(id, Some(project)),
    }
}

/// The backend of `pin`, loaded once a process.
fn load(pin: &BackendPin, bytes: &[u8]) -> Option<Arc<WasmBackend>> {
    if let Some(loaded) = runtimes::find(pin) {
        return Some(loaded);
    }
    let cancel = component::cancellation().ok()?.clone();
    let engine = component::engine().ok()?;
    let backend =
        WasmBackend::load_with_cancellation(engine, pin.clone(), bytes, Budget::default(), cancel);
    let backend = Arc::new(backend.ok()?);
    runtimes::loaded(pin, &backend);
    Some(backend)
}

fn patterns_path(sha256: &str) -> Option<PathBuf> {
    store::store_dir().map(|d| d.join("backends").join(format!("{sha256}.files.json")))
}

fn cached(sha256: &str) -> Option<Vec<String>> {
    serde_json::from_slice(&fs::read(patterns_path(sha256)?).ok()?).ok()
}

fn keep(sha256: &str, patterns: &[String]) {
    if let (Some(path), Ok(bytes)) = (patterns_path(sha256), serde_json::to_vec(patterns)) {
        let _ = path.parent().map(fs::create_dir_all);
        let _ = saved::write(&path, &bytes);
    }
}
