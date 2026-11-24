//! Foreign adapter declarations and `check-target` validation for the
//! `test/literal` fixture backend.

use flick_backend::{
    Abi, AbiConversion, AbiDirection, AdapterArg, AdapterArgKind, AdapterWord, Artifact,
    ArtifactKind, BuildRequest, Diagnostic, Inputs, ModuleRole, Severity, Span, TypeFormAttribute,
    ValueAbi, ValueAbiEntry, ValueAbiRefusal,
};
use flick_backend_support::inline_span::map_byte_range;
use flick_ir::{AdapterArg as IrAdapterArg, FileId, Foreign, ItemKind, Program};

use crate::diagnostic;

pub(crate) const ABI_NAME: &str = "test.literal-ffi";
pub(crate) const ABI_VERSION: &str = "0.1.0";

pub(crate) fn value_abi() -> ValueAbi {
    let row = |key: &str,
               flick_type: &str,
               host_type: &str,
               conversion: AbiConversion,
               direction: AbiDirection,
               notes: &str| ValueAbiEntry {
        key: key.into(),
        flick_type: flick_type.into(),
        host_type: host_type.into(),
        conversion,
        direction,
        notes: notes.into(),
    };
    ValueAbi {
        abi: Abi {
            name: ABI_NAME.into(),
            version: ABI_VERSION.into(),
        },
        opaque_type: "void *".into(),
        catch_error_type: "const char *".into(),
        catch_lang_item: Some("string".into()),
        entries: vec![
            row("unit", "Unit", "void", AbiConversion::None, AbiDirection::Both, "No value; returns void"),
            row("bool", "Bool", "bool", AbiConversion::None, AbiDirection::Both, "C99 boolean"),
            row("int", "Int", "int32_t", AbiConversion::None, AbiDirection::Both, "32-bit signed integer"),
            row("float", "Float", "double", AbiConversion::None, AbiDirection::Both, "IEEE 754 binary64"),
            row("string", "String", "const char *", AbiConversion::View, AbiDirection::Both, "Borrowed UTF-8 string view"),
            row("function", "a -> b", "void (*)(...)", AbiConversion::None, AbiDirection::In, "Inbound callback function pointer"),
            row("func_ref", "FuncRef (a -> b)", "void (*)(...)", AbiConversion::None, AbiDirection::In, "Function pointer without captures; needs no userdata"),
        ],
        unsupported: vec![
            ValueAbiRefusal {
                key: "bigint".into(),
                reason: "literal backend has no direct arbitrary-precision integer ABI; convert to Int or Float or pass an opaque handle".into(),
            },
            ValueAbiRefusal {
                key: "list".into(),
                reason: "literal backend does not marshal Flick lists directly; pass an opaque handle or copy through an adapter".into(),
            },
            ValueAbiRefusal {
                key: "record".into(),
                reason: "records have no stable ABI layout in the literal backend; pass scalar fields or an opaque handle".into(),
            },
            ValueAbiRefusal {
                key: "adt".into(),
                reason: "user-defined ADTs have no stable ABI layout in the literal backend; expose an opaque handle instead".into(),
            },
        ],
        fallback_reason: "type is outside the test.literal-ffi direct value ABI table; pass an opaque handle instead".into(),
        admits_copied_data: false,
    }
}

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

pub(crate) fn validate_foreign_abis(request: &BuildRequest) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for binding in &request.foreign {
        if binding.abi.name != ABI_NAME || binding.abi.version != ABI_VERSION {
            diagnostics.push(Diagnostic {
                code: "F014".into(),
                severity: Severity::Error,
                message: format!(
                    "foreign binding ABI mismatch for `{}` in `{}`: expected `{ABI_NAME}@{ABI_VERSION}`, got `{}@{}`",
                    binding.specifier, binding.module, binding.abi.name, binding.abi.version
                ),
                span: None,
                notes: vec![],
            });
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

pub(crate) fn validate_foreigns(
    program: &Program,
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = match validate_foreign_abis(request) {
        Ok(()) => Vec::new(),
        Err(diags) => diags,
    };
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

/// The reports of the `foreign` case: of the adapters and of inline code.
pub(crate) fn reports(
    request: &BuildRequest,
    artifacts: &mut Vec<Artifact>,
) -> Result<(), Vec<Diagnostic>> {
    let program =
        Program::from_cbor(&request.program).map_err(|e| diagnostic(&e.to_string(), None))?;
    artifacts.extend(adapter_report(&program, request));
    if !request.inline_foreign.is_empty() {
        artifacts.push(inline_report(request));
    }
    Ok(())
}

/// What the backend receives of each foreign binding of the program's own
/// modules: the host arity of each callback, the adapter words, such as
/// `callback i data j`, and the retention of what the host keeps.
fn adapter_report(program: &Program, request: &BuildRequest) -> Option<Artifact> {
    let own =
        |name: &str| (request.modules.iter()).any(|m| m.name == name && m.role == ModuleRole::Own);
    let entries: Vec<_> = (program.modules.iter())
        .filter(|module| own(&module.name))
        .flat_map(|module| module.items.iter().map(move |item| (module, item)))
        .filter_map(|(module, item)| match &item.kind {
            ItemKind::Foreign(foreign) => Some(serde_json::json!({
                "module": module.name,
                "name": foreign.name,
                "callbacks": foreign.adapter.callbacks,
                "words": foreign.adapter.words,
                "retention": foreign.adapter.retention,
            })),
            _ => None,
        })
        .collect();
    (!entries.is_empty()).then(|| Artifact {
        id: "foreign-adapters".into(),
        path: "foreign-adapters.json".into(),
        kind: ArtifactKind::Metadata,
        bytes: serde_json::to_vec_pretty(&entries).expect("adapter report serializes"),
    })
}

fn inline_report(request: &BuildRequest) -> Artifact {
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
