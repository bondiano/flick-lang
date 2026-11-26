//! Validation and merging of `flick:backend@0.1.0` component results in the browser host.

#[cfg(not(target_arch = "wasm32"))]
use flick_backend::handshake;
use flick_backend::{
    ArtifactKind, BackendPin, DeclaredInputs, Diagnostic as BackendDiagnostic, Limits, Policy,
    Severity as BackendSeverity, validate_result,
};
use flick_core::build::Artifact as FrontendArtifact;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::source::{FileId, Span};

use super::support::base64_decode;
use super::{ComponentReply, PreparedEmit};

/// Validate a component's reply against the `flick:backend@0.1.0` contract and merge
/// validated JS artifacts with shared `.d.ts` files.
#[allow(clippy::too_many_lines)]
pub(crate) fn finish(
    prepared: &PreparedEmit,
    reply: ComponentReply,
) -> (Vec<FrontendArtifact>, Vec<Diagnostic>) {
    let mut diagnostics = prepared.diagnostics.clone();
    let pin = &prepared.request.backend;
    let map_diag = |d: &BackendDiagnostic| map_backend_diagnostic(pin, d, &prepared.files);
    let protocol_error = |why: String| {
        Diagnostic::new(
            codes::P013,
            Severity::Error,
            format!(
                "backend {}@{}#{}: {why}",
                pin.identity.id, pin.identity.version, pin.sha256
            ),
        )
    };

    let mut result = match reply {
        ComponentReply::TransportError { message } => {
            diagnostics.push(protocol_error(message));
            return (Vec::new(), diagnostics);
        }
        ComponentReply::Err {
            #[cfg(not(target_arch = "wasm32"))]
            descriptor,
            diagnostics: guest_diags,
        } => {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(desc) = descriptor
                && let Err(why) = handshake(pin, &desc, &prepared.request.target)
            {
                diagnostics.push(protocol_error(why));
                return (Vec::new(), diagnostics);
            }
            for d in &guest_diags {
                diagnostics.push(map_diag(d));
            }
            if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
                diagnostics.push(protocol_error("backend refused the build".into()));
            }
            return (Vec::new(), diagnostics);
        }
        ComponentReply::Ok {
            #[cfg(not(target_arch = "wasm32"))]
            descriptor,
            mut result,
            artifact_bytes_b64,
        } => {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(desc) = descriptor
                && let Err(why) = handshake(pin, &desc, &prepared.request.target)
            {
                diagnostics.push(protocol_error(why));
                return (Vec::new(), diagnostics);
            }
            if let Some(encoded) = artifact_bytes_b64 {
                if encoded.len() != result.artifacts.len() {
                    diagnostics.push(protocol_error(
                        "artifact byte table length differs from artifact count".into(),
                    ));
                    return (Vec::new(), diagnostics);
                }
                for (artifact, b64) in result.artifacts.iter_mut().zip(encoded) {
                    match base64_decode(&b64) {
                        Ok(bytes) => artifact.bytes = bytes,
                        Err(why) => {
                            diagnostics.push(protocol_error(format!(
                                "invalid base64 payload for `{}`: {why}",
                                artifact.path
                            )));
                            return (Vec::new(), diagnostics);
                        }
                    }
                }
            }
            result
        }
    };

    let reserved_paths: Vec<String> = prepared.shared_dts.iter().map(|a| a.path.clone()).collect();
    let deny_step = |_step: &flick_backend::ToolchainStep| {
        Err("browser host executes no toolchain steps".to_owned())
    };
    let deny_launch = |_launch: &flick_backend::Launch| {
        Err("browser host executes no native launches".to_owned())
    };
    let policy = Policy {
        toolchains: &[],
        environment: &[],
        reserved_paths: &reserved_paths,
        authorize_step: &deny_step,
        authorize_launch: &deny_launch,
        limits: Limits::default(),
    };
    let mut declared = match DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.inputs.get(id).cloned()
    }) {
        Ok(d) => d,
        Err(why) => {
            diagnostics.push(protocol_error(why));
            return (Vec::new(), diagnostics);
        }
    };

    if result
        .diagnostics
        .iter()
        .any(|d| d.severity == BackendSeverity::Error)
    {
        for d in &result.diagnostics {
            diagnostics.push(map_diag(d));
        }
        return (Vec::new(), diagnostics);
    }

    if let Err(why) = validate_result(&prepared.request, &result, &policy, &mut declared) {
        diagnostics.push(protocol_error(why));
        return (Vec::new(), diagnostics);
    }

    for d in &result.diagnostics {
        diagnostics.push(map_diag(d));
    }

    let mut artifacts = prepared.shared_dts.clone();
    for artifact in result.artifacts.drain(..) {
        if matches!(
            artifact.kind,
            ArtifactKind::Source | ArtifactKind::SourceMap | ArtifactKind::Declaration
        ) {
            if let Ok(contents) = String::from_utf8(artifact.bytes) {
                artifacts.push(FrontendArtifact {
                    path: artifact.path,
                    contents,
                });
            } else {
                diagnostics.push(protocol_error(format!(
                    "backend artifact `{}` is not valid UTF-8",
                    artifact.path
                )));
                return (Vec::new(), diagnostics);
            }
        }
    }
    (artifacts, diagnostics)
}

fn map_backend_diagnostic(
    pin: &BackendPin,
    diagnostic: &BackendDiagnostic,
    files: &[FileId],
) -> Diagnostic {
    let code =
        codes::lookup(&format!("FLICK-{}", diagnostic.code)).map_or(codes::P013, |info| info.code);
    let severity = match diagnostic.severity {
        BackendSeverity::Error => Severity::Error,
        BackendSeverity::Warning => Severity::Warning,
        BackendSeverity::Note => Severity::Note,
    };
    let mut message = format!(
        "{}\nbackend {}@{}#{}",
        diagnostic.message, pin.identity.id, pin.identity.version, pin.sha256
    );
    for note in &diagnostic.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    let mut mapped = Diagnostic::new(code, severity, message);
    if let Some(span) = diagnostic.span
        && let Some(&file) = files.get(span.file as usize)
    {
        mapped.span = Some(Span::new(file, span.start, span.end));
    }
    mapped
}
