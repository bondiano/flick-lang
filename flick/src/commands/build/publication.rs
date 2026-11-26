//! File validation and publication of the shared build driver.

use std::path::Path;
use std::time::Instant;
use std::{fs, io};

use flick_core::diagnostics::{Diagnostic, Severity, codes};

use super::{BuildPlan, Emission, unwritten};
use crate::artifact::{self, Artifact};
use crate::cache;
use crate::progress::Progress;
use crate::publish;
use crate::timings;

/// Fresh and restored build/test outputs share stale ownership preflight and
/// reconciliation. Run generations pass no root because their lease owns them.
/// The files, the removal of stale ones and the manifest are one publication:
/// a failure anywhere keeps the last complete one.
pub(in crate::commands) fn publish_artifacts(
    output: &BuildPlan,
    emitted: &mut Emission,
    progress: &Progress,
    root: Option<&Path>,
) {
    if emitted
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return;
    }
    if let Err(diagnostic) = publish(output, &emitted.artifacts, progress, root) {
        emitted.diagnostics.push(*diagnostic);
    }
}

/// Publishes a build without recording it in a project's manifest.
pub(crate) fn write_artifacts(output: &BuildPlan, emitted: &mut Emission, progress: &Progress) {
    publish_artifacts(output, emitted, progress, None);
}

fn publish(
    output: &BuildPlan,
    artifacts: &[Artifact],
    progress: &Progress,
    root: Option<&Path>,
) -> Result<(), Box<Diagnostic>> {
    let failed = |why: String| Box::new(Diagnostic::new(codes::P013, Severity::Error, why));
    let out_dir = output.out_dir.as_path();
    let paths = paths(artifacts);
    artifact::validate(artifacts)
        .and_then(|()| {
            artifact::validate_destination_paths(out_dir, paths.iter().map(String::as_str))
        })
        .map_err(failed)?;
    let mut removes = unwritten::stale(out_dir, artifacts);
    let manifest = match root {
        Some(root) => {
            let (stale, path, manifest) = cache::recorded(root, out_dir, paths)
                .map_err(|e| failed(format!("cannot validate earlier build outputs: {e}")))?;
            removes.extend(stale);
            Some((path, manifest))
        }
        None => None,
    };
    progress.stage("Writing artifacts");
    let t_pub = timings::enabled().then(Instant::now);
    let committed = publish::commit(out_dir, artifacts, &removes, manifest);
    if let Some(t) = t_pub {
        timings::phase("atomic output publication", t.elapsed());
    }
    committed.map_err(|e| failed(format!("cannot publish to `{}`: {e}", out_dir.display())))
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)
}

/// The complete owned output set.
pub(in crate::commands) fn paths(files: &[Artifact]) -> Vec<String> {
    files.iter().map(|file| file.path.clone()).collect()
}
