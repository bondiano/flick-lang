//! `std`'s contributed `schema export` command: exports a `PureCodec Json a`
//! as JSON Schema 2020-12 or an `HttpApi.Api` as `OpenAPI` 3.1 by running a
//! synthetic Flick program through the project's selected backend via a
//! `run-flick` plan step.

use std::str;

use flick_plugin::command::{
    self, Descriptor, Diagnostic, File, FlickModule, Invocation, Parameter, Plan, RunFlickStep,
    Severity, Span, Step, Style, ValueKind,
};
use serde::{Deserialize, Serialize};

use crate::source::FileId;
use crate::syntax::{self, cst::Node, kind::SyntaxKind as K, lexer};

pub(super) const COMMAND_NAME: &str = "schema export";
const STEP_ID: &str = "export";
const OUTPUT_PATH: &str = ".flick-schema-export.out";
const ENTRY_PREFIX: &str = "FlickSchemaExport";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SchemaTarget {
    JsonSchema,
    OpenApi,
}

impl SchemaTarget {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "json-schema-2020-12" => Some(Self::JsonSchema),
            "openapi-3.1" => Some(Self::OpenApi),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::JsonSchema => "JSON Schema",
            Self::OpenApi => "OpenAPI document",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ResumeState {
    codec: String,
    target: SchemaTarget,
    out: String,
    origin: Option<OriginSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OriginSpan {
    path: String,
    start: u32,
    end: u32,
}

impl From<&Span> for OriginSpan {
    fn from(span: &Span) -> Self {
        Self {
            path: span.path.clone(),
            start: span.start,
            end: span.end,
        }
    }
}

impl From<OriginSpan> for Span {
    fn from(span: OriginSpan) -> Self {
        Self {
            path: span.path,
            start: span.start,
            end: span.end,
        }
    }
}

pub(super) fn descriptors() -> Vec<Descriptor> {
    vec![Descriptor {
        name: COMMAND_NAME.into(),
        summary: "Export the static wire form of a JSON codec or HttpApi.".into(),
        positionals: vec![Parameter {
            name: "codec".into(),
            kind: ValueKind::Text,
            multiple: false,
            required: true,
            help: "`Module.value`: a `PureCodec Json a` for JSON Schema, an `HttpApi.Api gs` for OpenAPI.".into(),
        }],
        options: vec![
            Parameter {
                name: "target".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: true,
                help: "Schema format: `json-schema-2020-12` or `openapi-3.1`.".into(),
            },
            Parameter {
                name: "out".into(),
                kind: ValueKind::Path,
                multiple: false,
                required: true,
                help: "The file to write.".into(),
            },
        ],
        inputs: vec!["$sources".into()],
        reads: vec![],
        writes: vec!["$out".into()],
        style: Style::Plan,
    }]
}

fn argument<'a>(invocation: &'a Invocation, name: &str) -> Option<&'a str> {
    invocation
        .arguments
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| a.values.first())
        .map(String::as_str)
}

fn is_con(part: &str) -> bool {
    let mut chars = part.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_module_name(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_con)
}

fn is_value_ident(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let (tokens, errors) = lexer::lex(name, FileId::default());
    errors.is_empty()
        && tokens.len() == 1
        && tokens[0].kind == K::Ident
        && tokens[0].start == 0
        && tokens[0].end as usize == name.len()
}

fn rel_module_path(sources: &str, module: &str) -> String {
    let rel = format!("{}.fl", module.replace('.', "/"));
    if sources.is_empty() {
        rel
    } else {
        format!("{sources}/{rel}")
    }
}

fn fresh_entry_name(invocation: &Invocation) -> String {
    for suffix in 0usize.. {
        let candidate = if suffix == 0 {
            ENTRY_PREFIX.to_owned()
        } else {
            format!("{ENTRY_PREFIX}{suffix}")
        };
        let path = rel_module_path(&invocation.sources, &candidate);
        if !invocation.inputs.iter().any(|f| f.path == path) {
            return candidate;
        }
    }
    unreachable!("unbounded suffix range")
}

/// Finds an identifier token bound by a `LetDecl` (its first pattern child, e.g. `VarPat`).
fn find_let_binder(decl: &Node, text: &str, value: &str) -> Option<(u32, u32)> {
    fn search_pat(node: &Node, text: &str, value: &str) -> Option<(u32, u32)> {
        if node.kind == K::VarPat {
            return node
                .child_tokens()
                .find(|t| t.kind == K::Ident && t.text(text) == value)
                .map(|t| (t.start, t.end));
        }
        for child in node.nodes() {
            if let Some(span) = search_pat(child, text, value) {
                return Some(span);
            }
        }
        None
    }
    if let Some(tok) = decl
        .child_tokens()
        .find(|t| t.kind == K::Ident && t.text(text) == value)
    {
        return Some((tok.start, tok.end));
    }
    decl.nodes()
        .next()
        .and_then(|pat| search_pat(pat, text, value))
}

/// Finds the byte range of the value `value` (or module header fallback) in `text`.
fn find_value_span(text: &str, value: &str) -> Option<(u32, u32)> {
    let parsed = syntax::parse(text, FileId::default());
    let mut let_span = None;
    let mut sig_span = None;
    let mut header_span = None;
    let mut expose_span = None;

    for node in parsed.tree.nodes() {
        match node.kind {
            K::ModuleHeader => {
                header_span = Some(node.range());
                if let Some(exposing) = node.nodes().find(|n| n.kind == K::Exposing) {
                    for item in exposing.nodes().filter(|n| n.kind == K::ExposeItem) {
                        if let Some(tok) = item
                            .child_tokens()
                            .find(|t| t.kind == K::Ident && t.text(text) == value)
                        {
                            expose_span = Some((tok.start, tok.end));
                        }
                    }
                }
            }
            K::LetDecl => {
                if let Some(span) = find_let_binder(node, text, value) {
                    let_span.get_or_insert(span);
                }
            }
            K::RecGroup => {
                for decl in node.nodes().filter(|n| n.kind == K::LetDecl) {
                    if let Some(span) = find_let_binder(decl, text, value) {
                        let_span.get_or_insert(span);
                    }
                }
            }
            K::Signature | K::ForeignDecl => {
                if let Some(tok) = node
                    .child_tokens()
                    .find(|t| t.kind == K::Ident && t.text(text) == value)
                {
                    sig_span.get_or_insert((tok.start, tok.end));
                }
            }
            _ => {}
        }
    }

    let_span.or(sig_span).or(expose_span).or(header_span)
}

/// Finds the byte range of an inline module or type declaration `child` in `text`.
fn find_member_owner_span(text: &str, child: &str) -> Option<(u32, u32)> {
    fn search(node: &Node, text: &str, child: &str) -> Option<(u32, u32)> {
        for decl in node.nodes() {
            match decl.kind {
                K::TypeDecl | K::InlineModuleDecl | K::PluginDecl => {
                    if let Some(tok) = decl
                        .child_tokens()
                        .find(|t| t.kind == K::Con && t.text(text) == child)
                    {
                        return Some((tok.start, tok.end));
                    }
                    if let Some(path) = decl.nodes().find(|n| n.kind == K::ModulePath)
                        && path.text(text).trim() == child
                    {
                        return Some(path.range());
                    }
                }
                _ => {}
            }
        }
        None
    }
    let parsed = syntax::parse(text, FileId::default());
    search(&parsed.tree, text, child).or_else(|| {
        parsed
            .tree
            .nodes()
            .find(|n| n.kind == K::ModuleHeader)
            .map(Node::range)
    })
}

fn find_origin(invocation: &Invocation, module: &str, value: &str) -> Option<Span> {
    let exact_path = rel_module_path(&invocation.sources, module);
    if let Some(file) = invocation.inputs.iter().find(|f| f.path == exact_path) {
        let text = str::from_utf8(&file.bytes).ok()?;
        let (start, end) = find_value_span(text, value).unwrap_or((0, 0));
        return Some(Span {
            path: file.path.clone(),
            start,
            end,
        });
    }
    let mut prefix = module;
    while let Some((parent, child)) = prefix.rsplit_once('.') {
        let parent_path = rel_module_path(&invocation.sources, parent);
        if let Some(file) = invocation.inputs.iter().find(|f| f.path == parent_path) {
            let text = str::from_utf8(&file.bytes).ok()?;
            let (start, end) = find_member_owner_span(text, child).unwrap_or((0, 0));
            return Some(Span {
                path: file.path.clone(),
                start,
                end,
            });
        }
        prefix = parent;
    }
    None
}

fn exporter_source(entry: &str, module: &str, value: &str, target: SchemaTarget) -> String {
    let expr = match target {
        SchemaTarget::JsonSchema => format!("Codec.json_schema \"{value}\" {module}.{value}"),
        SchemaTarget::OpenApi => format!("HttpApi.openapi {module}.{value}"),
    };
    [
        format!("module {entry} exposing (main)"),
        String::new(),
        format!("import {module}"),
        String::new(),
        "main : Unit ->{Host.Console} Unit".to_owned(),
        "let main () =".to_owned(),
        format!("  match {expr} with"),
        "  | Ok doc -> Platform.stdout_write (\"OK\\n\" ++ doc)".to_owned(),
        "  | Err (Codec.ExportError msg) -> Platform.stdout_write (\"ERR\\n\" ++ msg)".to_owned(),
        String::new(),
    ]
    .join("\n")
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if invocation.name != COMMAND_NAME {
        return Err(command::unknown(&invocation.name));
    }
    let codec = argument(invocation, "codec").unwrap_or_default();
    let target_arg = argument(invocation, "target").unwrap_or_default();
    let out = argument(invocation, "out").unwrap_or_default();

    let Some(target) = SchemaTarget::parse(target_arg) else {
        return Err(vec![Diagnostic::error(
            "P018",
            format!(
                "unknown schema target `{target_arg}`; expected `json-schema-2020-12` or `openapi-3.1`"
            ),
        )]);
    };
    let Some((module, value)) = codec.rsplit_once('.') else {
        return Err(vec![Diagnostic::error(
            "P018",
            format!("`{codec}` is not `Module.value`"),
        )]);
    };
    if !is_module_name(module) || !is_value_ident(value) {
        return Err(vec![Diagnostic::error(
            "P018",
            format!("`{codec}` is not `Module.value`"),
        )]);
    }

    let origin = find_origin(invocation, module, value);
    let entry = fresh_entry_name(invocation);
    let source = exporter_source(&entry, module, value, target);
    let state = ResumeState {
        codec: codec.to_owned(),
        target,
        out: out.to_owned(),
        origin: origin.as_ref().map(OriginSpan::from),
    };
    let resume = serde_json::to_vec(&state).map_err(|err| {
        vec![Diagnostic::error(
            "P013",
            format!("cannot encode schema export state: {err}"),
        )]
    })?;

    let mut plan = Plan::empty();
    plan.steps.push(Step::RunFlick(RunFlickStep {
        id: STEP_ID.into(),
        entry: entry.clone(),
        modules: vec![FlickModule {
            name: entry,
            source,
        }],
        arguments: vec![],
        output: Some(OUTPUT_PATH.into()),
        origin,
    }));
    plan.resume = Some(resume);
    Ok(plan)
}

pub(super) fn resume(state: &[u8], outputs: &[File]) -> Result<Plan, Vec<Diagnostic>> {
    let state: ResumeState = serde_json::from_slice(state).map_err(|err| {
        vec![Diagnostic::error(
            "P055",
            format!("invalid schema export continuation state: {err}"),
        )]
    })?;
    let origin = state.origin.map(Span::from);
    let Some(output) = outputs.iter().find(|f| f.path == OUTPUT_PATH) else {
        return Err(vec![Diagnostic {
            code: "P061".into(),
            severity: Severity::Error,
            message: format!("exporter for `{}` produced no output", state.codec),
            notes: vec![],
            span: origin,
            help: None,
        }]);
    };
    if let Some(payload) = output.bytes.strip_prefix(b"OK\n") {
        let mut plan = Plan::empty();
        plan.files.push(File {
            path: state.out,
            bytes: payload.to_vec(),
            executable: false,
        });
        return Ok(plan);
    }
    if let Some(err_bytes) = output.bytes.strip_prefix(b"ERR\n") {
        let reason = String::from_utf8_lossy(err_bytes).trim().to_owned();
        return Err(vec![Diagnostic {
            code: "D008".into(),
            severity: Severity::Error,
            message: format!(
                "`{}` has no strict {}: {reason}",
                state.codec,
                state.target.label()
            ),
            notes: vec![],
            span: origin,
            help: None,
        }]);
    }
    Err(vec![Diagnostic {
        code: "P061".into(),
        severity: Severity::Error,
        message: format!("unexpected exporter output for `{}`", state.codec),
        notes: vec![],
        span: origin,
        help: None,
    }])
}

#[cfg(test)]
mod tests {
    use flick_plugin::command::Argument;

    use super::*;

    fn invocation(codec: &str, target: &str, out: &str, inputs: Vec<File>) -> Invocation {
        Invocation {
            name: COMMAND_NAME.into(),
            arguments: vec![
                Argument {
                    name: "codec".into(),
                    values: vec![codec.into()],
                },
                Argument {
                    name: "target".into(),
                    values: vec![target.into()],
                },
                Argument {
                    name: "out".into(),
                    values: vec![out.into()],
                },
            ],
            sources: "src".into(),
            inputs,
            tools: vec![],
        }
    }

    fn source_file(path: &str, text: &str) -> File {
        File {
            path: path.into(),
            bytes: text.as_bytes().to_vec(),
            executable: false,
        }
    }

    #[test]
    fn descriptor_declares_sources_input_and_out_write() {
        let ds = descriptors();
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].name, "schema export");
        assert_eq!(ds[0].inputs, ["$sources"]);
        assert_eq!(ds[0].writes, ["$out"]);
        assert_eq!(ds[0].style, Style::Plan);
    }

    #[test]
    fn run_rejects_malformed_codec_injection_and_unknown_target() {
        for bad_codec in [
            "item_json",
            "lib.Wire.item_json",
            "Lib.Wire.Item",
            "Lib.Wire.let",
            "Lib.Wire.x; import Evil",
            "Lib.Wire.x\nlet y = 1",
            "../Escape.x",
        ] {
            let err = run(&invocation(
                bad_codec,
                "json-schema-2020-12",
                "o.json",
                vec![],
            ))
            .expect_err(bad_codec);
            assert_eq!(err[0].code, "P018", "{bad_codec}: {err:?}");
        }
        let err = run(&invocation(
            "Lib.Wire.item_json",
            "unknown-format",
            "o.json",
            vec![],
        ))
        .expect_err("unknown target");
        assert_eq!(err[0].code, "P018");
    }

    #[test]
    fn run_finds_declaration_span_and_avoids_module_collision() {
        let wire = "module Lib.Wire exposing (item_json)\n\nitem_json : PureCodec Json Item\nlet item_json = Codec.json Item.schema\n";
        let inv = invocation(
            "Lib.Wire.item_json",
            "json-schema-2020-12",
            "item.json",
            vec![
                source_file("src/Lib/Wire.fl", wire),
                source_file(
                    "src/FlickSchemaExport.fl",
                    "module FlickSchemaExport exposing ()\n",
                ),
            ],
        );
        let plan = run(&inv).expect("plan");
        assert_eq!(plan.steps.len(), 1);
        let Step::RunFlick(step) = &plan.steps[0] else {
            panic!("expected RunFlick step");
        };
        assert_eq!(step.entry, "FlickSchemaExport1");
        assert_eq!(step.modules[0].name, "FlickSchemaExport1");
        assert!(
            step.modules[0]
                .source
                .contains("Codec.json_schema \"item_json\" Lib.Wire.item_json")
        );
        let parsed = syntax::parse(&step.modules[0].source, FileId::default());
        assert!(
            parsed.diagnostics.is_empty(),
            "generated exporter must parse cleanly: {:?}\n{}",
            parsed.diagnostics,
            step.modules[0].source
        );
        let origin = step.origin.as_ref().expect("origin span");
        assert_eq!(origin.path, "src/Lib/Wire.fl");
        assert_eq!(
            &wire[origin.start as usize..origin.end as usize],
            "item_json"
        );
        assert!(wire[..origin.start as usize].ends_with("let "));

        let state = plan.resume.expect("resume state");
        let ok = resume(
            &state,
            &[File {
                path: OUTPUT_PATH.into(),
                bytes: b"OK\n{\"type\":\"object\"}".to_vec(),
                executable: false,
            }],
        )
        .expect("ok resume");
        assert_eq!(ok.files.len(), 1);
        assert_eq!(ok.files[0].path, "item.json");
        assert_eq!(ok.files[0].bytes, b"{\"type\":\"object\"}");

        let refused = resume(
            &state,
            &[File {
                path: OUTPUT_PATH.into(),
                bytes: b"ERR\ndata -> JSON Schema: unsupported".to_vec(),
                executable: false,
            }],
        )
        .expect_err("D008 refusal");
        assert_eq!(refused[0].code, "D008");
        assert_eq!(refused[0].span.as_ref(), Some(origin));
        assert!(refused[0].message.contains("has no strict JSON Schema"));

        let bad = resume(
            &state,
            &[File {
                path: OUTPUT_PATH.into(),
                bytes: b"GARBAGE".to_vec(),
                executable: false,
            }],
        )
        .expect_err("malformed output");
        assert_eq!(bad[0].code, "P061");
        assert_eq!(bad[0].span.as_ref(), Some(origin));
    }
}
