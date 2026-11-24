//! Host export validation and header projection reporting for the
//! `test/literal` fixture backend.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use flick_backend::{Artifact, ArtifactKind, BuildRequest, Diagnostic, Severity, Span};
use flick_ir::{IrType, ItemKind, Program};

const RESERVED: &[&str] = &[
    "auto", "break", "case", "char", "class", "const", "continue", "default", "do", "double",
    "else", "enum", "export", "extern", "float", "for", "goto", "if", "import", "int", "long",
    "register", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
    "union", "unsigned", "void", "volatile", "while",
];

fn is_valid_c_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !RESERVED.contains(&name)
}

fn export_error(code: &str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        severity: Severity::Error,
        message: message.into(),
        span: Some(span),
        notes: vec![],
    }
}

pub(crate) fn is_library(request: &BuildRequest) -> bool {
    request
        .target
        .options
        .iter()
        .any(|o| o.key == "option.kind" && o.value == "library")
}

/// Validates every `@export "name"` entry in `request.exports`: valid C
/// identifier, package-wide uniqueness, and no generic/dictionary functions.
pub(crate) fn validate_exports(
    program: &Program,
    request: &BuildRequest,
) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut seen: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for export in &request.exports {
        if !is_valid_c_ident(&export.name) {
            diagnostics.push(export_error(
                "P012",
                format!(
                    "invalid literal export name `{}` on `{}.{}`: must be a non-reserved C identifier",
                    export.name, export.symbol.module, export.symbol.name
                ),
                export.span,
            ));
            continue;
        }
        if let Some((prev_mod, prev_sym)) = seen.insert(
            export.name.as_str(),
            (export.symbol.module.as_str(), export.symbol.name.as_str()),
        ) {
            let mut diag = export_error(
                "P012",
                format!(
                    "duplicate `@export \"{}\"` on `{}.{}`: host export names must be unique per package",
                    export.name, export.symbol.module, export.symbol.name
                ),
                export.span,
            );
            diag.notes
                .push(format!("already exported by `{prev_mod}.{prev_sym}`"));
            diagnostics.push(diag);
            continue;
        }
        let Some(module) = program
            .modules
            .iter()
            .find(|m| m.name == export.symbol.module)
        else {
            continue;
        };
        let Some(item) = module.items.iter().find(|item| match &item.kind {
            ItemKind::Value { name, .. } => name == &export.symbol.name,
            ItemKind::Foreign(f) => f.name == export.symbol.name,
            _ => false,
        }) else {
            continue;
        };
        let (type_params, has_dicts, ty) = match &item.kind {
            ItemKind::Value {
                type_params,
                dicts,
                ty,
                ..
            } => (type_params.as_slice(), !dicts.is_empty(), ty.as_deref()),
            ItemKind::Foreign(f) => (f.type_params.as_slice(), f.dicts > 0, f.ty.as_deref()),
            _ => continue,
        };
        if has_dicts {
            diagnostics.push(export_error(
                "P012",
                format!(
                    "cannot `@export \"{}\"` on constrained function `{}.{}`: takes dictionaries",
                    export.name, export.symbol.module, export.symbol.name
                ),
                export.span,
            ));
        } else if !type_params.is_empty() {
            diagnostics.push(export_error(
                "P012",
                format!(
                    "cannot `@export \"{}\"` on generic function `{}.{}` (type parameters: {})",
                    export.name,
                    export.symbol.module,
                    export.symbol.name,
                    type_params.join(", ")
                ),
                export.span,
            ));
        } else if request.headers.as_ref().is_some_and(|h| h.enabled)
            && let Some(ty) = ty
            && let Err(reason) = c_signature(&export.name, ty)
        {
            diagnostics.push(export_error(
                "F004",
                format!(
                    "`@export \"{}\"` (`{}.{}`) has no header form: {reason}",
                    export.name, export.symbol.module, export.symbol.name
                ),
                export.span,
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Emits a JSON report of the exact `exports` list and `headers` request
/// received across the WIT boundary.
pub(crate) fn exports_report(request: &BuildRequest) -> Artifact {
    let exports: Vec<_> = request
        .exports
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "symbol": {
                    "module": entry.symbol.module,
                    "name": entry.symbol.name,
                },
                "span": {
                    "file": entry.span.file,
                    "start": entry.span.start,
                    "end": entry.span.end,
                },
            })
        })
        .collect();
    let headers = request.headers.as_ref().map(|h| {
        serde_json::json!({
            "enabled": h.enabled,
            "format": h.format,
        })
    });
    let payload = serde_json::json!({
        "kind": if is_library(request) { "library" } else { "app" },
        "exports": exports,
        "headers": headers,
    });
    Artifact {
        id: "exports-report".into(),
        path: "exports-report.json".into(),
        kind: ArtifactKind::Metadata,
        bytes: serde_json::to_vec_pretty(&payload).expect("exports report serializes"),
    }
}

/// Emits a simple C-like header projection for `request.exports` when
/// `request.headers` is enabled.
pub(crate) fn header_artifact(
    program: &Program,
    request: &BuildRequest,
) -> Result<Option<Artifact>, Vec<Diagnostic>> {
    if !request.headers.as_ref().is_some_and(|h| h.enabled) {
        return Ok(None);
    }
    let mut text = String::from("/* Generated by test/literal */\nvoid flick_init(void);\n");
    for export in &request.exports {
        let Some(module) = program
            .modules
            .iter()
            .find(|m| m.name == export.symbol.module)
        else {
            continue;
        };
        let Some(ty) = module.items.iter().find_map(|i| match &i.kind {
            ItemKind::Value { name, ty, .. } if name == &export.symbol.name => ty.as_deref(),
            ItemKind::Foreign(f) if f.name == export.symbol.name => f.ty.as_deref(),
            _ => None,
        }) else {
            continue;
        };
        match c_signature(&export.name, ty) {
            Ok(decl) => {
                let _ = writeln!(text, "{decl};");
            }
            Err(reason) => {
                return Err(vec![export_error(
                    "F004",
                    format!(
                        "`@export \"{}\"` (`{}.{}`) has no header form: {reason}",
                        export.name, export.symbol.module, export.symbol.name
                    ),
                    export.span,
                )]);
            }
        }
    }
    Ok(Some(Artifact {
        id: "library-header".into(),
        path: "library.h".into(),
        kind: ArtifactKind::Declaration,
        bytes: text.into_bytes(),
    }))
}

fn c_signature(export_name: &str, ty: &IrType) -> Result<String, String> {
    let mut params = Vec::new();
    let mut cur = ty;
    while let IrType::Fun { param, result, .. } = cur {
        params.push(c_type(param)?);
        cur = result;
    }
    let ret = c_type(cur)?;
    let args = if params.is_empty() {
        "void".to_owned()
    } else {
        params.join(", ")
    };
    Ok(format!("{ret} {export_name}({args})"))
}

fn c_type(ty: &IrType) -> Result<&'static str, String> {
    match ty {
        IrType::Named { module, name, args } if module == "Prelude" && args.is_empty() => {
            match name.as_str() {
                "Unit" => Ok("void"),
                "Bool" => Ok("bool"),
                "Int" => Ok("int32_t"),
                "Float" => Ok("double"),
                "String" => Ok("const char *"),
                other => Err(format!("`{other}` has no C header type in literal backend")),
            }
        }
        IrType::Named { name, .. } => {
            Err(format!("`{name}` has no C header type in literal backend"))
        }
        IrType::Record { .. } => Err("records have no C header type in literal backend".into()),
        IrType::Tuple(_) => Err("tuples have no C header type in literal backend".into()),
        _ => Err("type has no C header form in literal backend".into()),
    }
}
