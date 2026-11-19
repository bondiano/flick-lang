//! What the backend of a project reports of its ecosystem's packages, for
//! the compiler, which finds the Flick modules they ship through it. The
//! backend is taken without the network: a project's own pin, or an
//! official one a checkout or the store holds; without one there is none.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{Backend, BackendId, BackendPin, OptionValue};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::Emit;
use flick_core::project::{Foreign, ForeignPackage, Project, store};

use super::{official, packages, runtimes};
use crate::{component, saved};

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
    let real = fs::canonicalize(&project.root).unwrap_or_else(|_| project.root.clone());
    let options = [OptionValue {
        key: "root".into(),
        value: real.to_string_lossy().replace('\\', "/"),
    }];
    match packages::report(&*backend, &project.root, &options) {
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
