//! Host-named `@export "name"` collection and header projection requests for
//! `BuildRequest`.

use std::collections::HashSet;

use flick_backend::{Descriptor, HeaderRequest, HostExport, OptionValue, Span, Symbol};
use flick_core::project::Compilation;
use flick_ir::Program;

use crate::commands::build::BuildPlan;

/// Collects every `@export "name"` entry from own modules in `compilation`.
pub(crate) fn collect_own_host_exports(
    program: &Program,
    compilation: &Compilation,
) -> Vec<HostExport> {
    let own: HashSet<&str> = compilation
        .modules
        .iter()
        .filter(|m| m.package.is_none())
        .map(|m| m.name.as_str())
        .collect();
    collect_host_exports(program, &own)
}

/// Collects every `@export "name"` entry from own modules in `program` (whose
/// spans have already been remapped to `Program.files`).
pub(crate) fn collect_host_exports(
    program: &Program,
    own_modules: &HashSet<&str>,
) -> Vec<HostExport> {
    let mut out = Vec::new();
    for module in &program.modules {
        if !own_modules.contains(module.name.as_str()) {
            continue;
        }
        for export in &module.exports.host {
            out.push(HostExport {
                name: export.export_name.clone(),
                symbol: Symbol {
                    module: module.name.clone(),
                    name: export.name.clone(),
                },
                span: Span {
                    file: export.span.file.index(),
                    start: export.span.start,
                    end: export.span.end,
                },
            });
        }
    }
    out
}

/// Computes the `BuildRequest.headers` projection request from resolved target
/// options and the backend descriptor.
pub(super) fn header_request(output: &BuildPlan, descriptor: &Descriptor) -> Option<HeaderRequest> {
    if let Some(value) = output.options.get("headers") {
        return match value {
            "false" | "none" => None,
            "true" => Some(HeaderRequest {
                enabled: true,
                format: None,
            }),
            fmt => Some(HeaderRequest {
                enabled: true,
                format: Some(fmt.to_owned()),
            }),
        };
    }
    if output.extras.dts {
        return Some(HeaderRequest {
            enabled: true,
            format: Some("dts".into()),
        });
    }
    if output.options.get("kind") == Some("library") {
        let format = (descriptor.identity.id == "flick/js").then(|| "dts".into());
        return Some(HeaderRequest {
            enabled: true,
            format,
        });
    }
    None
}

/// Computes `BuildRequest.headers` for `check-target` from wire options.
pub(crate) fn check_header_request(
    options: &[OptionValue],
    backend_id: &str,
) -> Option<HeaderRequest> {
    let get = |key: &str| {
        options
            .iter()
            .find(|o| o.key == key)
            .map(|o| o.value.as_str())
    };
    if let Some(value) = get("option.headers") {
        return match value {
            "false" | "none" => None,
            "true" => Some(HeaderRequest {
                enabled: true,
                format: None,
            }),
            fmt => Some(HeaderRequest {
                enabled: true,
                format: Some(fmt.to_owned()),
            }),
        };
    }
    if get("option.declarations") == Some("true") {
        return Some(HeaderRequest {
            enabled: true,
            format: Some("dts".into()),
        });
    }
    if get("option.kind") == Some("library") {
        let format = (backend_id == "flick/js").then(|| "dts".into());
        return Some(HeaderRequest {
            enabled: true,
            format,
        });
    }
    None
}
