//! Foreign adapter word validation and metadata report emission for the
//! literal test backend.

use flick_backend::{Artifact, ArtifactKind, BuildRequest, Diagnostic, ModuleRole, Severity, Span};
use flick_ir::{AdapterArg as IrAdapterArg, ItemKind, Program};
use serde_json::{Value, json};

pub(crate) fn validate_foreign_adapters(
    program: &Program,
    request: &BuildRequest,
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
            let ItemKind::Foreign(foreign) = &item.kind else {
                continue;
            };
            for word in &foreign.adapter.words {
                let span = Some(Span {
                    file: word.span.file.index(),
                    start: word.span.start,
                    end: word.span.end,
                });
                match word.name.as_str() {
                    "slot" => match word.args.as_slice() {
                        [IrAdapterArg::Index(index)] => {
                            if *index >= foreign.arity {
                                diagnostics.push(Diagnostic {
                                    code: "F001".into(),
                                    severity: Severity::Error,
                                    message: format!(
                                        "`slot {index}` is out of range for arity {}",
                                        foreign.arity
                                    ),
                                    span,
                                    notes: vec![],
                                });
                            }
                        }
                        _ => {
                            diagnostics.push(Diagnostic {
                                code: "F001".into(),
                                severity: Severity::Error,
                                message: "`slot` takes one non-negative slot index".into(),
                                span,
                                notes: vec![],
                            });
                        }
                    },
                    other => {
                        diagnostics.push(Diagnostic {
                            code: "S011".into(),
                            severity: Severity::Error,
                            message: format!(
                                "unknown foreign metadata `{other}`; expected one of: arity, catch, retention, nullable, optional, reviewed, owned, slot"
                            ),
                            span,
                            notes: vec![],
                        });
                    }
                }
            }
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

pub(crate) fn report_artifact(program: &Program, request: &BuildRequest) -> Artifact {
    let mut foreigns = Vec::new();
    for module in &program.modules {
        if !request
            .modules
            .iter()
            .any(|m| m.name == module.name && m.role == ModuleRole::Own)
        {
            continue;
        }
        for item in &module.items {
            let ItemKind::Foreign(foreign) = &item.kind else {
                continue;
            };
            let words: Vec<Value> = foreign
                .adapter
                .words
                .iter()
                .map(|w| {
                    let args: Vec<Value> = w
                        .args
                        .iter()
                        .map(|a| match a {
                            IrAdapterArg::Ident(s) => json!({ "ident": s }),
                            IrAdapterArg::Index(i) => json!({ "index": i }),
                            IrAdapterArg::Text(s) => json!({ "text": s }),
                        })
                        .collect();
                    json!({
                        "name": w.name,
                        "args": args,
                        "start": w.span.start,
                        "end": w.span.end,
                    })
                })
                .collect();
            foreigns.push(json!({
                "module": module.name,
                "name": foreign.name,
                "arity": foreign.arity,
                "owned": foreign.adapter.owned,
                "words": words,
            }));
        }
    }
    let bytes = serde_json::to_vec_pretty(&json!({
        "ir_version": program.ir_version,
        "foreigns": foreigns,
    }))
    .unwrap_or_default();
    Artifact {
        id: "foreign-report".into(),
        path: "foreign-report.json".into(),
        kind: ArtifactKind::Metadata,
        bytes,
    }
}
