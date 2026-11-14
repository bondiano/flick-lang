//! `flick build --emit-ir`: the IR of a build as a file.

use flick_core::build;
use flick_core::ir::lang_items;
use flick_core::project::{Compilation, Project};
use flick_core::source::SourceMap;
use flick_ir::Program;

use super::build::BuildPlan;
use crate::args::IrFormat;
use crate::artifact::Artifact;
use crate::toolchain;
use flick_backend::ArtifactKind;

/// `--emit-ir`: the IR of the build's modules, as the backend gets it, in
/// `program.ir.json` or `program.ir.cbor` of the output directory.
pub(super) fn artifact(
    compilation: &Compilation,
    sources: &mut SourceMap,
    project: Option<&Project>,
    output: &BuildPlan,
    format: IrFormat,
) -> Artifact {
    let program = output
        .backend
        .reference_ir(sources, project, compilation, output.extras.rt)
        .unwrap_or_else(|| native_program(compilation, sources, output));
    let (name, bytes) = match format {
        IrFormat::Json => ("program.ir.json", program.to_json().into_bytes()),
        IrFormat::Cbor => ("program.ir.cbor", program.to_cbor()),
    };
    Artifact {
        path: name.into(),
        contents: bytes,
        kind: ArtifactKind::Metadata,
    }
}

fn native_program(compilation: &Compilation, sources: &SourceMap, output: &BuildPlan) -> Program {
    let modules = &compilation.modules;
    let ir = modules
        .iter()
        .map(|m| {
            if output.release {
                build::optimized(modules, m, output.extras.rt)
            } else {
                m.ir.clone()
            }
        })
        .collect();
    let compiler = format!("flick {}", toolchain::COMPILER);
    Program::new(&compiler, ir, lang_items(), &|f| {
        sources.get(f).path.clone()
    })
}
