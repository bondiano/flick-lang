//! File validation, publication and host tool steps of the shared driver.

use std::path::Path;
use std::{fs, io};

use flick_backend::ArtifactKind;
use flick_core::build;
use flick_core::diagnostics::{Diagnostic, Severity, codes};

use super::schemas::compile_schemas;
use super::{BuildPlan, Emission, Purpose, unwritten};
use crate::artifact::{self, Artifact};
use crate::progress::Progress;
use crate::publish::{self, Beside};
use crate::{cache, toolchain};

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
    let paths = paths(output, artifacts);
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
    let made = host_outputs(output, artifacts, progress)?;
    progress.stage("Writing artifacts");
    publish::commit(out_dir, artifacts.iter().chain(&made), &removes, manifest)
        .map_err(|e| failed(format!("cannot publish to `{}`: {e}", out_dir.display())))
}

/// The files host tools make from a build's own: Go executables and the
/// compiled schema decoders. The tools run on a copy of the build beside
/// the output, so their results are published with it or not at all.
fn host_outputs(
    output: &BuildPlan,
    artifacts: &[Artifact],
    progress: &Progress,
) -> Result<Vec<Artifact>, Box<Diagnostic>> {
    let failed = |why: String| Box::new(Diagnostic::new(codes::P013, Severity::Error, why));
    let go = output.go && !output.backend.is_component();
    if !go && !output.extras.schema_aot {
        return Ok(Vec::new());
    }
    let copy = Beside::new(&output.out_dir).map_err(|e| failed(e.to_string()))?;
    let dir = copy.path();
    for artifact in artifacts {
        let path = dir.join(&artifact.path);
        write(&path, &artifact.contents)
            .and_then(|()| artifact.restore_permissions(&path))
            .map_err(|e| failed(format!("cannot write `{}`: {e}", path.display())))?;
    }
    let mut made = Vec::new();
    let read = |path: String, kind| {
        fs::read(dir.join(&path))
            .map(|contents| Artifact {
                path,
                contents,
                kind,
            })
            .map_err(|e| failed(e.to_string()))
    };
    if go {
        let tests = [(build::go::TEST.to_owned(), String::new())];
        let built = if output.purpose == Purpose::Test {
            &tests[..]
        } else {
            &output.entries[..]
        };
        for (name, _) in built {
            progress.stage("Building Go executable");
            if let Some(problem) = toolchain::go_build(dir, name) {
                return Err(Box::new(problem));
            }
            made.push(read(build::go::executable(name), ArtifactKind::Executable)?);
        }
    }
    let aot = if output.extras.schema_aot {
        output.entries.as_slice()
    } else {
        &[]
    };
    for (name, entry) in aot {
        let ext = if output.typescript { "ts" } else { "js" };
        let module = dir.join(format!("{}.{ext}", entry.replace('.', "/")));
        let file = build::schema_aot_file(name);
        progress.stage("Compiling schema decoders");
        compile_schemas(dir, &module, &file, &output.backend).map_err(|why| {
            failed(format!(
                "cannot write the compiled decoders `{file}`: {why}"
            ))
        })?;
        made.push(read(file, ArtifactKind::Source)?);
    }
    Ok(made)
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)
}

/// The complete owned output set, including host tool results.
pub(in crate::commands) fn paths(output: &BuildPlan, files: &[Artifact]) -> Vec<String> {
    let mut paths: Vec<_> = files.iter().map(|file| file.path.clone()).collect();
    if output.go && !output.backend.is_component() {
        if output.purpose == Purpose::Test {
            paths.push(build::go::executable(build::go::TEST));
        } else {
            paths.extend(
                output
                    .entries
                    .iter()
                    .map(|(name, _)| build::go::executable(name)),
            );
        }
    }
    if output.extras.schema_aot {
        paths.extend(
            output
                .entries
                .iter()
                .map(|(name, _)| build::schema_aot_file(name)),
        );
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Selected;

    #[test]
    fn an_empty_go_test_still_reserves_its_executable() {
        let output = BuildPlan {
            purpose: Purpose::Test,
            backend: Selected::Native,
            out_dir: "unused-output".into(),
            entries: vec![],
            typescript: false,
            go: true,
            suites: vec![],
            extras: build::Extras::default(),
            release: false,
        };
        let files = [Artifact::source(
            "go/test/main.go".into(),
            "package main".into(),
        )];
        let executable = if cfg!(windows) {
            "flick-test.exe"
        } else {
            "flick-test"
        };
        assert_eq!(paths(&output, &files), ["go/test/main.go", executable]);
    }
}
