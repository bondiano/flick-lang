//! `flick build --emit-ir`: the IR of a build as a file.

use flick_core::build;
use flick_core::ir::lang_items;
use flick_core::project::Compilation;
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
    sources: &SourceMap,
    output: &BuildPlan,
    format: IrFormat,
) -> Artifact {
    let modules = &compilation.modules;
    // shortcut: the built-in modules are left out, their spans point into
    // source maps of their own; a reader takes them from the same compiler.
    // Add them with their own files when a backend outside the compiler
    // needs a program that stands alone.
    let ir = modules
        .iter()
        .map(|m| {
            if output.release && !output.backend.is_component() {
                build::optimized(modules, m, output.extras.rt)
            } else {
                m.ir.clone()
            }
        })
        .collect();
    let compiler = format!("flick {}", toolchain::COMPILER);
    let program = Program::new(&compiler, ir, lang_items(), &|f| {
        sources.get(f).path.clone()
    });
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
