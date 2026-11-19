//! What the backend of a project reports of its ecosystem's packages, for
//! the compiler, which finds the Flick modules they ship through it. The
//! backend is taken without the network: a project's own pin, or an
//! official one a checkout or the store holds; without one there is none.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use flick_backend::command::Dependency;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{Backend, BackendId, BackendPin, OptionValue};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::Emit;
use flick_core::project::target::Wanted;
use flick_core::project::{Foreign, ForeignPackage, Project, store};
use flick_core::source::{FileId, Span};

use super::{official, packages, runtimes};
use crate::{cache, component, saved};

pub(crate) fn foreign(project: &Project) -> Foreign {
    let Some((pin, bytes)) = selected(project) else {
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
    let options = [OptionValue {
        key: "root".into(),
        value: real_root(project),
    }];
    match packages::report(&*backend, &project.root, &[], &options) {
        Ok(report) => Foreign {
            name: report.name,
            packages: (report.sources.into_iter())
                .map(|p| ForeignPackage {
                    name: p.package,
                    version: Some(p.version).filter(|v| !v.is_empty()),
                    source_dir: Some(PathBuf::from(p.root)).filter(|r| !r.as_os_str().is_empty()),
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
/// packages: those the backend reads, and the modules of the packages
/// that ship Flick sources.
pub(crate) fn package_inputs(project: &Project) -> Vec<PathBuf> {
    let Some((pin, bytes)) = selected(project) else {
        return Vec::new();
    };
    let patterns = match cached(&pin.sha256) {
        Some(patterns) => patterns,
        None => {
            let Some(backend) = load(&pin, &bytes) else {
                return Vec::new();
            };
            let patterns = backend.package_files();
            keep(&pin.sha256, &patterns);
            patterns
        }
    };
    let mut files: Vec<PathBuf> = (packages::named(&project.root, &patterns).into_iter())
        .map(|p| project.root.join(p))
        .collect();
    for package in foreign(project).packages {
        if let Some(dir) = package.source_dir {
            let mut sources = Vec::new();
            cache::files_under(&project.root.join(dir), &mut sources);
            files.extend(
                (sources.into_iter())
                    .filter(|p| p.extension().is_some_and(|e| e == "fl" || e == "go")),
            );
        }
    }
    files
}

/// The name of the `[target]` of the backend `project` selects: the last
/// part of its id.
pub(crate) fn target_name(project: &Project) -> Option<String> {
    let (id, _) = super::selected_pin(project)?;
    Some(id.rsplit('/').next().unwrap_or(&id).to_owned())
}

/// What the closure asks of the ecosystem of the backend `project` selects,
/// as that backend takes it.
pub(crate) fn asked(project: &Project, wanted: &[Wanted]) -> Vec<Dependency> {
    let target = target_name(project);
    (wanted.iter())
        .filter(|w| Some(&w.target) == target.as_ref())
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
    let target = target_name(project);
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
    let Some(backend) = selected(project).and_then(|(pin, bytes)| load(&pin, &bytes)) else {
        return unavailable("the project's backend is not available offline".into());
    };
    let mut options = vec![OptionValue {
        key: "root".into(),
        value: real_root(project),
    }];
    if installed {
        options.push(OptionValue {
            key: "verify".into(),
            value: "installed".into(),
        });
    }
    let dependencies = asked(project, wanted);
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

fn real_root(project: &Project) -> String {
    let real = fs::canonicalize(&project.root).unwrap_or_else(|_| project.root.clone());
    real.to_string_lossy().replace('\\', "/")
}

/// The component `project` selects and its pin, without the network.
fn selected(project: &Project) -> Option<(BackendPin, Vec<u8>)> {
    if let Some(package) = &project.config.build.backend {
        let pin = BackendPin {
            identity: BackendId {
                id: package.id.clone(),
                version: package.version.clone(),
            },
            sha256: package.sha256.clone(),
        };
        return Some((pin, fs::read(project.root.join(&package.component)).ok()?));
    }
    let id = if project.config.build.emit == Emit::Go {
        official::GO
    } else {
        official::JS
    };
    official::local(id, project)
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
