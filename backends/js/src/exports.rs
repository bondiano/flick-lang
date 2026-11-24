//! JavaScript validation for `@export "name"` host-named function exports.
//!
//! Shared between the `flick/js` component (`check-target` / `generate`) and
//! in-process checking (`flick check` / LSP) so both report identical codes,
//! messages, and source spans at the `@export` attribute.

use std::collections::BTreeMap;

use flick_ir::{ItemKind, Module, Span};

/// A JS-backend diagnostic on an `@export "name"` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub notes: Vec<String>,
}

const JS_RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Whether `name` is a valid, non-reserved JavaScript export identifier.
pub fn is_valid_js_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
        return false;
    }
    !JS_RESERVED.contains(&name)
}

/// A host export entry to validate against a package's own modules.
pub struct ExportEntry<'a> {
    pub export_name: &'a str,
    pub module: &'a str,
    pub symbol: &'a str,
    pub span: Span,
}

/// Validates every `@export "name"` across own modules of a package.
pub fn validate_modules<'a>(
    modules: impl IntoIterator<Item = &'a Module>,
) -> Vec<ExportDiagnostic> {
    let modules: Vec<&Module> = modules.into_iter().collect();
    let entries: Vec<ExportEntry<'_>> = modules
        .iter()
        .flat_map(|module| {
            module.exports.host.iter().map(move |host| ExportEntry {
                export_name: host.export_name.as_str(),
                module: module.name.as_str(),
                symbol: host.name.as_str(),
                span: host.span,
            })
        })
        .collect();
    validate_entries(&modules, &entries)
}

/// Validates an explicit host export list (e.g. from `BuildRequest.exports`)
/// against `modules`.
pub fn validate_entries(modules: &[&Module], entries: &[ExportEntry<'_>]) -> Vec<ExportDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for entry in entries {
        if !is_valid_js_ident(entry.export_name) {
            diagnostics.push(ExportDiagnostic {
                code: "P012",
                message: format!(
                    "invalid JavaScript export name `{}` in `@export` on `{}.{}`: must be a non-reserved JavaScript identifier",
                    entry.export_name, entry.module, entry.symbol
                ),
                span: entry.span,
                notes: vec![],
            });
            continue;
        }
        if let Some((prev_mod, prev_sym)) =
            seen.insert(entry.export_name, (entry.module, entry.symbol))
        {
            diagnostics.push(ExportDiagnostic {
                code: "P012",
                message: format!(
                    "duplicate `@export \"{}\"` on `{}.{}`: host export names must be unique per package",
                    entry.export_name, entry.module, entry.symbol
                ),
                span: entry.span,
                notes: vec![format!("already exported by `{prev_mod}.{prev_sym}`")],
            });
            continue;
        }
        let Some(module) = modules.iter().find(|m| m.name == entry.module) else {
            continue;
        };
        let Some(item) = module.items.iter().find(|item| match &item.kind {
            ItemKind::Value { name, .. } => name == entry.symbol,
            ItemKind::Foreign(f) => f.name == entry.symbol,
            _ => false,
        }) else {
            continue;
        };
        let (type_params, has_dicts) = match &item.kind {
            ItemKind::Value {
                type_params, dicts, ..
            } => (type_params.as_slice(), !dicts.is_empty()),
            ItemKind::Foreign(f) => (f.type_params.as_slice(), f.dicts > 0),
            _ => continue,
        };
        if has_dicts {
            diagnostics.push(ExportDiagnostic {
                code: "P012",
                message: format!(
                    "cannot `@export \"{}\"` on constrained function `{}.{}`: functions taking type-class dictionaries cannot be exported to a host; export a concrete wrapper instead",
                    entry.export_name, entry.module, entry.symbol
                ),
                span: entry.span,
                notes: vec![],
            });
        } else if !type_params.is_empty() {
            diagnostics.push(ExportDiagnostic {
                code: "P012",
                message: format!(
                    "cannot `@export \"{}\"` on generic function `{}.{}` (type parameters: {}): export a monomorphic wrapper instead",
                    entry.export_name,
                    entry.module,
                    entry.symbol,
                    type_params.join(", ")
                ),
                span: entry.span,
                notes: vec![],
            });
        }
    }
    diagnostics.sort_by_key(|d| (d.span.file.index(), d.span.start, d.span.end));
    diagnostics
}

#[cfg(feature = "component")]
pub(crate) fn validate_request(
    program: &flick_ir::Program,
    exports: &[flick_backend::HostExport],
) -> Vec<flick_backend::Diagnostic> {
    let modules: Vec<&Module> = program.modules.iter().collect();
    let entries: Vec<ExportEntry<'_>> = exports
        .iter()
        .map(|e| ExportEntry {
            export_name: e.name.as_str(),
            module: e.symbol.module.as_str(),
            symbol: e.symbol.name.as_str(),
            span: Span::new(flick_ir::FileId::new(e.span.file), e.span.start, e.span.end),
        })
        .collect();
    validate_entries(&modules, &entries)
        .into_iter()
        .map(|d| flick_backend::Diagnostic {
            code: d.code.into(),
            severity: flick_backend::Severity::Error,
            message: d.message,
            span: Some(flick_backend::Span {
                file: d.span.file.index(),
                start: d.span.start,
                end: d.span.end,
            }),
            notes: d.notes,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_js_export_identifiers() {
        assert!(is_valid_js_ident("addNumbers"));
        assert!(is_valid_js_ident("_init"));
        assert!(is_valid_js_ident("$run1"));
        assert!(!is_valid_js_ident(""));
        assert!(!is_valid_js_ident("123bad"));
        assert!(!is_valid_js_ident("bad-name"));
        assert!(!is_valid_js_ident("bad name"));
        assert!(!is_valid_js_ident("class"));
        assert!(!is_valid_js_ident("default"));
        assert!(!is_valid_js_ident("return"));
    }
}
