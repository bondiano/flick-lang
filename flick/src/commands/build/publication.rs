//! File validation, publication and host tool steps of the shared driver.

use std::path::Path;
use std::{fs, io};

use flick_core::build;
use flick_core::diagnostics::{Diagnostic, Severity, codes};

use super::schemas::compile_schemas;
use super::{BuildPlan, Emission, unwritten};
use crate::artifact;
use crate::progress::Progress;
use crate::toolchain;

pub(crate) fn write_artifacts(output: &BuildPlan, emitted: &mut Emission, progress: &Progress) {
    if emitted
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return;
    }
    let out_dir = output.out_dir.as_path();
    if let Err(why) = artifact::validate_destination(out_dir, &emitted.artifacts) {
        emitted
            .diagnostics
            .push(Diagnostic::new(codes::P013, Severity::Error, why));
        return;
    }
    progress.stage("Writing artifacts");
    if let Err(err) = unwritten::remove(out_dir, &emitted.artifacts) {
        let message = format!("cannot remove an earlier build's files: {err}");
        (emitted.diagnostics).push(Diagnostic::new(codes::P013, Severity::Error, message));
        return;
    }
    for artifact in &emitted.artifacts {
        let path = out_dir.join(&artifact.path);
        if let Err(err) = write_if_changed(&path, &artifact.contents)
            .and_then(|()| artifact.restore_permissions(&path))
        {
            emitted.diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot write `{}`: {err}", path.display()),
            ));
            return;
        }
    }
    if output.go {
        let tests = [(build::go::TEST.to_owned(), String::new())];
        let built = if output.suites.is_empty() {
            &output.entries[..]
        } else {
            &tests[..]
        };
        for (name, _) in built {
            progress.stage("Building Go executable");
            if let Some(problem) = toolchain::go_build(out_dir, name) {
                emitted.diagnostics.push(problem);
                return;
            }
        }
    }
    let aot = if output.extras.schema_aot {
        output.entries.as_slice()
    } else {
        &[]
    };
    for (name, entry) in aot {
        let ext = if output.typescript { "ts" } else { "js" };
        let module = out_dir.join(format!("{}.{ext}", entry.replace('.', "/")));
        let file = build::schema_aot_file(name);
        progress.stage("Compiling schema decoders");
        if let Err(reason) = compile_schemas(out_dir, &module, &file) {
            emitted.diagnostics.push(Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot write the compiled decoders `{file}`: {reason}"),
            ));
            break;
        }
    }
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)
}

fn write_if_changed(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if fs::read(path).is_ok_and(|bytes| bytes == contents.as_ref()) {
        return Ok(());
    }
    write(path, contents)
}
