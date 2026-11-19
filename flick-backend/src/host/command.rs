//! The records of the command interface between Wasmtime's bindings and
//! the native ones, for the backend host and the plugin host alike.

use super::component::bindings::flick::command::types as w;
use crate::command as n;

pub fn invocation(i: &n::Invocation) -> w::Invocation {
    w::Invocation {
        name: i.name.clone(),
        arguments: (i.arguments.iter())
            .map(|a| w::Argument {
                name: a.name.clone(),
                values: a.values.clone(),
            })
            .collect(),
        sources: i.sources.clone(),
        inputs: i.inputs.iter().map(file_wire).collect(),
        tools: i.tools.iter().map(|t| tool_wire(t.clone())).collect(),
    }
}

pub fn file_wire(f: &n::File) -> w::File {
    w::File {
        path: f.path.clone(),
        bytes: f.bytes.clone(),
        executable: f.executable,
    }
}

fn tool_wire(t: n::Tool) -> w::Tool {
    w::Tool {
        id: t.id,
        version: t.version,
        sha256: t.sha256,
    }
}

fn tool(t: w::Tool) -> n::Tool {
    n::Tool {
        id: t.id,
        version: t.version,
        sha256: t.sha256,
    }
}

fn parameter(p: w::Parameter) -> n::Parameter {
    n::Parameter {
        name: p.name,
        kind: match p.kind {
            w::ValueKind::Flag => n::ValueKind::Flag,
            w::ValueKind::Text => n::ValueKind::Text,
            w::ValueKind::Path => n::ValueKind::Path,
        },
        multiple: p.multiple,
        required: p.required,
        help: p.help,
    }
}

pub fn descriptors(ds: Vec<w::Descriptor>) -> Vec<n::Descriptor> {
    (ds.into_iter())
        .map(|d| n::Descriptor {
            name: d.name,
            summary: d.summary,
            positionals: d.positionals.into_iter().map(parameter).collect(),
            options: d.options.into_iter().map(parameter).collect(),
            inputs: d.inputs,
            reads: d.reads,
            writes: d.writes,
            style: match d.style {
                w::Style::Plan => n::Style::Plan,
                w::Style::Session => n::Style::Session,
            },
        })
        .collect()
}

pub fn diagnostics(ds: Vec<w::Diagnostic>) -> Vec<n::Diagnostic> {
    (ds.into_iter())
        .map(|d| n::Diagnostic {
            code: d.code,
            severity: match d.severity {
                w::Severity::Error => n::Severity::Error,
                w::Severity::Warning => n::Severity::Warning,
                w::Severity::Note => n::Severity::Note,
            },
            message: d.message,
            notes: d.notes,
            span: d.span.map(|s| n::Span {
                path: s.path,
                start: s.start,
                end: s.end,
            }),
            help: d.help,
        })
        .collect()
}

pub fn plan(p: w::Plan) -> n::Plan {
    n::Plan {
        files: (p.files.into_iter())
            .map(|f| n::File {
                path: f.path,
                bytes: f.bytes,
                executable: f.executable,
            })
            .collect(),
        steps: (p.steps.into_iter())
            .map(|s| match s {
                w::Step::Tool(t) => n::Step::Tool(n::ToolStep {
                    id: t.id,
                    tool: tool(t.tool),
                    arguments: t.arguments,
                    environment: (t.environment.into_iter())
                        .map(|v| n::Variable {
                            name: v.name,
                            value: v.value,
                        })
                        .collect(),
                    inputs: t.inputs,
                    reads: t.reads,
                    outputs: t.outputs,
                }),
            })
            .collect(),
        diagnostics: diagnostics(p.diagnostics),
        stdout: p.stdout,
        resume: p.resume,
    }
}

pub fn handled(h: w::Handled) -> n::Handled {
    n::Handled {
        state: h.state,
        response: h.response,
    }
}
