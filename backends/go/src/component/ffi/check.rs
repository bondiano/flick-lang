//! `go:ffi check`: verifies generated Go FFI modules and reviewed bindings
//! against the live `go/types` surface of each referenced Go package.

use flick_diagnostics::diagnostics::{Diagnostic, codes};
use flick_ffi::ffi::check::Reviewed;
use flick_ir::{FileId, Span};

use super::surface::{GoExport, GoExportKind, GoSurface};

/// Checks every reviewed binding referencing `surface.package`.
pub(super) fn check_reviewed(bindings: &[Reviewed], surface: &GoSurface) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for b in bindings.iter().filter(|b| b.package == surface.package) {
        let Some(export) = surface.find(&b.export) else {
            out.push(Diagnostic::error(
                codes::F007,
                format!(
                    "`{}` no longer exports `{}`, which `{}` was reviewed against",
                    surface.provenance(),
                    b.export,
                    b.name
                ),
                b.span,
            ));
            continue;
        };
        if export.fingerprint != b.fingerprint {
            out.push(
                Diagnostic::error(
                    codes::F007,
                    format!(
                        "the declaration of `{}` changed since `{}` was reviewed against {}",
                        b.export, b.name, b.provenance
                    ),
                    b.span,
                )
                .with_fix(|| {
                    (
                        format!(
                            "review the binding against {} and write `reviewed \"{} {}\"`",
                            surface.provenance(),
                            surface.provenance(),
                            export.fingerprint
                        ),
                        Vec::new(),
                    )
                }),
            );
        }
        for (meta, why) in required_metadata(export) {
            if !b.metadata.iter().any(|m| m == meta) {
                out.push(Diagnostic::error(
                    codes::F007,
                    format!(
                        "`{}` {why}: its reviewed binding `{}` needs `{meta}`",
                        b.export, b.name
                    ),
                    b.span,
                ));
            }
        }
    }
    out
}

/// Required Flick metadata words implied by a Go export's `go/types` signature.
pub(super) fn required_metadata(export: &GoExport) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    if export.kind == GoExportKind::Method {
        out.push(("this", "is a Go method"));
    }
    let Some(sig) = &export.signature else {
        return out;
    };
    if sig.results.last().is_some_and(|r| r.is_error) {
        out.push(("catch", "returns a trailing Go `error`"));
    }
    if sig.params.first().is_some_and(|p| p.ty.is_context) {
        out.push(("cancel", "takes a leading `context.Context`"));
    }
    if sig.variadic {
        out.push(("variadic", "is variadic"));
    }
    out
}

/// Reports a stale `Generated.fl` module that differs from a fresh import.
pub(super) fn stale(file: FileId, path: &str, surface: &GoSurface) -> Diagnostic {
    Diagnostic::error(
        codes::F006,
        format!(
            "`{path}` is not what `flick go:ffi import` writes for {} now",
            surface.provenance()
        ),
        Span::new(file, 0, 0),
    )
    .with_fix(|| {
        (
            format!(
                "run `flick go:ffi import {}` again, then `flick go:ffi check`",
                surface.package
            ),
            Vec::new(),
        )
    })
}
