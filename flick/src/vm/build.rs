//! `flick build --target vm`: compiles modules to standalone `.fvm` VM images.

use std::slice;

use flick_backend::ArtifactKind;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::{Decl, Interface, Namespace};
use flick_core::ir::memory::Name as MemoryName;
use flick_core::project::{self, Compilation, Project};
use flick_vm::image::build_static_image;

use super::lower;
use crate::artifact::Artifact;
use crate::commands::build::{BuildPlan, Emission};
use crate::sources::FsHost;

/// Emits standalone `.fvm` VM image artifacts for `compilation`.
pub(crate) fn artifacts(
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Emission {
    let interfaces: Vec<Interface> = compilation
        .modules
        .iter()
        .map(|m| m.interface.clone())
        .collect();

    if !output.entries.is_empty() {
        let mut built = Vec::with_capacity(output.entries.len());
        for (entry_name, entry_module) in &output.entries {
            let entry = MemoryName::new(entry_module, "main");
            match emit_one(
                compilation,
                &interfaces,
                slice::from_ref(&entry),
                Some(entry.clone()),
                output.release,
                &format!("{entry_name}.fvm"),
            ) {
                Ok(artifact) => built.push(artifact),
                Err(diagnostic) => {
                    return Emission {
                        artifacts: Vec::new(),
                        diagnostics: vec![diagnostic],
                    };
                }
            }
        }
        return Emission {
            artifacts: built,
            diagnostics: Vec::new(),
        };
    }

    // Library project or standalone file without `main`: emit one `.fvm` image
    // containing all compiled user module interfaces and exported value roots.
    let mut roots = Vec::new();
    for module in &compilation.modules {
        for export in &module.interface.exports {
            if export.entity.namespace == Namespace::Value
                && export.entity.module == module.name
                && matches!(export.decl, Decl::Let)
            {
                roots.push(MemoryName::new(&module.name, &export.entity.name));
            }
        }
    }
    let file_stem = library_image_stem(project, compilation);
    match emit_one(
        compilation,
        &interfaces,
        &roots,
        None,
        output.release,
        &format!("{file_stem}.fvm"),
    ) {
        Ok(artifact) => Emission {
            artifacts: vec![artifact],
            diagnostics: Vec::new(),
        },
        Err(diagnostic) => Emission {
            artifacts: Vec::new(),
            diagnostics: vec![diagnostic],
        },
    }
}

fn emit_one(
    compilation: &Compilation,
    interfaces: &[Interface],
    roots: &[MemoryName],
    entry: Option<MemoryName>,
    release: bool,
    path: &str,
) -> Result<Artifact, Diagnostic> {
    let lowered = lower::lower(compilation, roots, release).map_err(|err| {
        Diagnostic::new(
            codes::P012,
            Severity::Error,
            format!("VM lowering failed: {err}"),
        )
    })?;
    let image = build_static_image(lowered.program, interfaces.to_vec(), entry, lowered.root)
        .map_err(|err| {
            Diagnostic::new(
                codes::P012,
                Severity::Error,
                format!("VM image build failed: {err}"),
            )
        })?;
    let bytes = image.encode().map_err(|err| {
        Diagnostic::new(
            codes::P012,
            Severity::Error,
            format!("VM image encoding failed: {err}"),
        )
    })?;
    Ok(Artifact {
        path: path.to_owned(),
        contents: bytes,
        kind: ArtifactKind::Object,
    })
}

fn library_image_stem(project: Option<&Project>, compilation: &Compilation) -> String {
    let raw = project
        .and_then(|p| project::project_name(&FsHost::new(), p))
        .or_else(|| (compilation.modules.len() == 1).then(|| compilation.modules[0].name.clone()))
        .unwrap_or_else(|| "library".to_owned());
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized.starts_with('.') {
        "library".to_owned()
    } else {
        sanitized
    }
}
