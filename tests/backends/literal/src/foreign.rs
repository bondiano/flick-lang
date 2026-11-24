//! Foreign adapter declarations and `check-target` validation for the
//! `test/literal` fixture backend.

use flick_backend::{
    AdapterArg, AdapterArgKind, AdapterWord, Artifact, ArtifactKind, BuildRequest, Diagnostic,
    Inputs, ModuleRole, Severity, Span, TypeFormAttribute,
};
use flick_backend_support::inline_span::map_byte_range;
use flick_ir::{AdapterArg as IrAdapterArg, FileId, Foreign, ItemKind, Program};

pub(crate) fn foreign_adapters() -> Vec<AdapterWord> {
    vec![
        AdapterWord {
            name: "callback".into(),
            args: vec![
                AdapterArg {
                    name: "fn".into(),
                    kind: AdapterArgKind::ParamIndex,
                },
                AdapterArg {
                    name: "data".into(),
                    kind: AdapterArgKind::Keyword(vec!["data".into()]),
                },
                AdapterArg {
                    name: "user_data".into(),
                    kind: AdapterArgKind::ParamIndex,
                },
            ],
            repeatable: true,
            doc: "Pair callback parameter `fn` with its opaque userdata parameter `user_data`."
                .into(),
        },
        AdapterWord {
            name: "method".into(),
            args: vec![],
            repeatable: false,
            doc: "Call the foreign binding as a method on its first argument.".into(),
        },
    ]
}

pub(crate) fn type_form_attributes() -> Vec<TypeFormAttribute> {
    vec![TypeFormAttribute {
        name: "literal".into(),
        doc: "Declare the literal-backend foreign type form.".into(),
    }]
}

fn foreign_error(code: &str, message: impl Into<String>, span: flick_ir::Span) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        severity: Severity::Error,
        message: message.into(),
        span: Some(Span {
            file: span.file.index(),
            start: span.start,
            end: span.end,
        }),
        notes: vec![],
    }
}

fn validate_foreign(foreign: &Foreign, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen_method = false;
    for word in &foreign.adapter.words {
        match word.name.as_str() {
            "callback" => {
                let Some((fn_idx, data_idx)) = (match word.args.as_slice() {
                    [
                        IrAdapterArg::Int(fn_i),
                        IrAdapterArg::Word(kw),
                        IrAdapterArg::Int(data_i),
                    ] if kw == "data" => usize::try_from(*fn_i)
                        .ok()
                        .zip(usize::try_from(*data_i).ok()),
                    _ => None,
                }) else {
                    diagnostics.push(foreign_error(
                        "F001",
                        "write `callback <fn> data <user_data>` with parameter indices",
                        word.span,
                    ));
                    continue;
                };
                if fn_idx >= foreign.arity || data_idx >= foreign.arity {
                    diagnostics.push(foreign_error(
                        "F001",
                        format!(
                            "`callback {fn_idx} data {data_idx}` parameter index is out of range for arity {}",
                            foreign.arity
                        ),
                        word.span,
                    ));
                } else if fn_idx == data_idx {
                    diagnostics.push(foreign_error(
                        "F001",
                        format!(
                            "`callback {fn_idx} data {data_idx}` requires distinct callback and userdata parameters"
                        ),
                        word.span,
                    ));
                } else if foreign
                    .adapter
                    .callbacks
                    .get(fn_idx)
                    .copied()
                    .flatten()
                    .is_none()
                {
                    diagnostics.push(foreign_error(
                        "F001",
                        format!(
                            "`callback {fn_idx} data {data_idx}` requires parameter {fn_idx} to be a callback function"
                        ),
                        word.span,
                    ));
                }
            }
            "method" => {
                if !word.args.is_empty() {
                    diagnostics.push(foreign_error(
                        "F001",
                        "`method` takes no arguments",
                        word.span,
                    ));
                } else if seen_method {
                    diagnostics.push(foreign_error(
                        "F001",
                        "duplicate foreign metadata `method`",
                        word.span,
                    ));
                } else {
                    seen_method = true;
                    if foreign.arity == 0 {
                        diagnostics.push(foreign_error(
                            "F001",
                            "`method` needs at least one argument to be the receiver",
                            word.span,
                        ));
                    }
                }
            }
            other => {
                diagnostics.push(foreign_error(
                    "S011",
                    format!("unknown foreign metadata `{other}`"),
                    word.span,
                ));
            }
        }
    }
}

pub(crate) fn validate_foreigns(
    program: &Program,
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for module in &program.modules {
        if !request
            .modules
            .iter()
            .any(|m| m.name == module.name && m.role == ModuleRole::Own)
        {
            continue;
        }
        for item in &module.items {
            if let ItemKind::Foreign(foreign) = &item.kind {
                validate_foreign(foreign, &mut diagnostics);
            }
        }
    }
    for entry in &request.inline_foreign {
        if let Some(pos) = entry.code.find("SYNTAX_ERROR") {
            let source = request
                .sources
                .get(entry.span.file as usize)
                .and_then(|s| inputs.read(&s.input).ok())
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default();
            let ir_span = flick_ir::Span::new(
                FileId::new(entry.span.file),
                entry.span.start,
                entry.span.end,
            );
            let mapped = if source.is_empty() {
                ir_span
            } else {
                map_byte_range(&source, ir_span, pos, pos + "SYNTAX_ERROR".len())
            };
            diagnostics.push(foreign_error(
                "F011",
                "invalid inline foreign code in literal backend",
                mapped,
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

pub(crate) fn inline_report(request: &BuildRequest) -> Artifact {
    let entries: Vec<_> = request
        .inline_foreign
        .iter()
        .map(|entry| {
            serde_json::json!({
                "module": entry.module,
                "name": entry.name,
                "code": entry.code,
                "span": {
                    "file": entry.span.file,
                    "start": entry.span.start,
                    "end": entry.span.end,
                }
            })
        })
        .collect();
    Artifact {
        id: "inline-foreign".into(),
        path: "inline-foreign.json".into(),
        kind: ArtifactKind::Metadata,
        bytes: serde_json::to_vec_pretty(&entries).expect("inline report serializes"),
    }
}
