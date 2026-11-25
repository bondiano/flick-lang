//! Library entry `.d.ts` header projection (`index.d.ts`) for `@export "name"`
//! host-named functions. An export that TypeScript cannot faithfully state is
//! reported as `FLICK-F004` at the `@export` attribute span, never weakened.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::Projection;
use super::dts::jsdoc;
use crate::iface::{Interface, Namespace};
use crate::source::Span;

/// One host-named export to project into a JS library's `index.d.ts`.
pub struct HostExportSpec<'a> {
    pub export_name: &'a str,
    pub module: &'a str,
    pub symbol: &'a str,
    pub span: Span,
}

/// A host export that TypeScript cannot faithfully express.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedExport {
    pub export_name: String,
    pub module: String,
    pub symbol: String,
    pub span: Span,
    pub reason: String,
}

/// Projects `index.d.ts` for all `@export "name"` functions of a JS library,
/// returning `(contents, referenced_modules, refused_exports)`.
///
/// When `global_name` is `Some(name)` (for `format = "iife"`), also emits a
/// `declare global { var <name>: { ... }; }` augmentation.
#[allow(clippy::too_many_arguments)]
pub fn library_declaration_file(
    exports: &[HostExportSpec<'_>],
    interfaces: &dyn Fn(&str) -> Option<Interface>,
    declarations: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    path: &dyn Fn(&str) -> String,
    foreign: &dyn Fn(&str, &str) -> String,
    rt: bool,
    global_name: Option<&str>,
) -> (String, BTreeSet<String>, Vec<RefusedExport>) {
    let projection = Projection::for_runtime("$index", interfaces, package, rt).shipping(foreign);
    let mut lines = Vec::new();
    let mut global_fields = Vec::new();
    let mut refused = Vec::new();
    for export in exports {
        let decl_iface = declarations(export.module).or_else(|| interfaces(export.module));
        let scheme = decl_iface
            .as_ref()
            .and_then(|iface| iface.find(Namespace::Value, export.symbol))
            .and_then(|entry| entry.scheme.as_ref());
        let Some(scheme) = scheme else {
            refused.push(RefusedExport {
                export_name: export.export_name.to_owned(),
                module: export.module.to_owned(),
                symbol: export.symbol.to_owned(),
                span: export.span,
                reason: "exported function has no checked type".to_owned(),
            });
            continue;
        };
        match projection.value(scheme) {
            Ok(ty) => {
                let doc = interfaces(export.module)
                    .map(|iface| jsdoc(&iface, export.symbol))
                    .filter(|d| !d.is_empty())
                    .or_else(|| decl_iface.as_ref().map(|iface| jsdoc(iface, export.symbol)))
                    .unwrap_or_default();
                lines.push(format!(
                    "{doc}export declare const {}: {ty};",
                    export.export_name
                ));
                if global_name.is_some() {
                    global_fields.push(format!("    readonly {}: {ty};", export.export_name));
                }
            }
            Err(reason) => refused.push(RefusedExport {
                export_name: export.export_name.to_owned(),
                module: export.module.to_owned(),
                symbol: export.symbol.to_owned(),
                span: export.span,
                reason,
            }),
        }
    }
    let mut out = projection.imports(path);
    out.push('\n');
    if lines.is_empty() {
        out.push_str("export {};\n");
    } else {
        for line in &lines {
            out.push_str(line);
            out.push('\n');
        }
    }
    if let Some(global) = global_name {
        let _ = writeln!(out, "\ndeclare global {{");
        let _ = writeln!(out, "  var {global}: {{");
        for field in &global_fields {
            let _ = writeln!(out, "{field}");
        }
        let _ = writeln!(out, "  }};");
        let _ = writeln!(out, "}}");
    }
    let used = projection.used.borrow().clone();
    (out, used, refused)
}
